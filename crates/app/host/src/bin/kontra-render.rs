//! `kontra-render`: render one instrument, through KONTRA, to a WAV, and say what came out.
//!
//! ```text
//! kontra-render --instrument "Electric Mint.nki" --key 60 --out mint.wav
//! kontra-render --instrument "Electric Mint.nki" --midi parity.mid --out mint.wav --rate 48000
//! ```
//!
//! Stdout is one JSON object (the [`Summary`] the parity tools parse) and nothing
//! else; diagnostics go to stderr, or to `--diag`. Exit status is 0 when a WAV was
//! written, 2 for a usage error, and 1 when the render failed.

use std::path::PathBuf;

use kontra_host::render::{DEFAULT_RATE, Request};

/// What the command line asked for.
#[derive(Debug)]
struct Args {
    request: Request,
    /// Where diagnostics go; `None` drops them.
    diag: Option<PathBuf>,
    /// Print the usage and stop.
    help: bool,
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse(&args) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("kontra-render: {e}\n\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };
    if parsed.help {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    let mut sink: Box<dyn std::io::Write> = match &parsed.diag {
        Some(path) => match std::fs::File::create(path) {
            Ok(file) => Box::new(file),
            Err(e) => {
                eprintln!("kontra-render: {}: {e}", path.display());
                return std::process::ExitCode::FAILURE;
            }
        },
        None => Box::new(std::io::sink()),
    };
    match kontra_host::render::run(&parsed.request, &mut *sink) {
        Ok(summary) => match serde_json::to_string(&summary) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kontra-render: summary: {e}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("kontra-render: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
usage: kontra-render --instrument <file> --out <wav> [options]

  --instrument <file>   the Kontakt instrument (or `bank.ufs/Name.uvip`) to render
  --out <wav>           where the 32-bit float stereo WAV goes (default render.wav)
  --midi <file>         play this standard MIDI file instead of a single note
  --key <0..127>        key of the single note (default 60)
  --velocity <1..127>   velocity of the single note (default 100)
  --hold <seconds>      how long the single note is held (default 1.5)
  --wheel <0..127>      move the mod wheel to this value one second in
  --tail <seconds>      silence after the last key-up (default 2)
  --rate <hz>           output rate (default 48000; comparisons require 48000)
  --live                render in real time instead of waiting for the disk
  --diag <path>         write the diagnostics there instead of stderr
  --quiet               write no diagnostics
  -h, --help            this text

stdout is one JSON object: format, instrument, wav, name, rate, channels, frames,
duration_s, peak, peak_dbfs, rms, rms_dbfs, sha256, note, midi, notes, zones,
load_s, cpu_ms, realtime_factor.";

/// Parses the command line; `Err` carries the message for a usage error.
fn parse(args: &[String]) -> Result<Args, String> {
    let mut request = Request::default();
    let mut diag = Some(PathBuf::from("/dev/stderr"));
    let mut help = false;
    let mut given_instrument = false;
    let mut given_out = false;
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        i += 1;
        let mut value = || -> Result<&String, String> {
            let v = args.get(i).ok_or_else(|| format!("{arg} needs a value"))?;
            i += 1;
            Ok(v)
        };
        match arg.as_str() {
            "--instrument" | "--nki" => {
                request.instrument = PathBuf::from(value()?);
                given_instrument = true;
            }
            "--out" | "--wav" => {
                request.out = PathBuf::from(value()?);
                given_out = true;
            }
            "--midi" => request.midi = Some(PathBuf::from(value()?)),
            "--key" => request.note.key = number(value()?, "--key", 0, 127)?,
            "--velocity" => request.note.velocity = number(value()?, "--velocity", 1, 127)?,
            "--hold" => request.note.hold_s = seconds(value()?)?,
            "--tail" => request.tail_s = seconds(value()?)?,
            "--wheel" => request.wheel = Some(number(value()?, "--wheel", 0, 127)?),
            "--rate" => request.rate = rate(value()?)?,
            "--live" => request.live = true,
            "--diag" => diag = Some(PathBuf::from(value()?)),
            "--quiet" => diag = None,
            "-h" | "--help" => help = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if !help && !given_instrument {
        return Err("--instrument is required".into());
    }
    if !help && !given_out {
        return Err("--out is required: a render without a WAV measures nothing".into());
    }
    if request.rate != DEFAULT_RATE && !help {
        eprintln!(
            "kontra-render: warning: --rate {} is not the {DEFAULT_RATE} Hz comparisons use",
            request.rate
        );
    }
    if request.tail_s < 0.0 {
        return Err("--tail cannot be negative".into());
    }
    Ok(Args {
        request,
        diag,
        help,
    })
}

/// An integer argument within `lo..=hi`.
fn number(v: &str, flag: &str, lo: u8, hi: u8) -> Result<u8, String> {
    let n: u16 = v.parse().map_err(|_| format!("{flag} {v}: not a number"))?;
    u8::try_from(n)
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| format!("{flag} {n} is outside {lo}..={hi}"))
}

/// A sample rate argument within what a WAV writer and the engine can hold.
fn rate(v: &str) -> Result<u32, String> {
    let n: u32 = v.parse().map_err(|_| format!("--rate {v}: not a number"))?;
    if (1_000..=384_000).contains(&n) {
        Ok(n)
    } else {
        Err(format!("--rate {n} is outside 1000..=384000"))
    }
}

/// A non-negative seconds argument.
fn seconds(v: &str) -> Result<f32, String> {
    let s: f32 = v
        .parse()
        .map_err(|_| format!("{v}: not a number of seconds"))?;
    if s.is_finite() && s >= 0.0 {
        Ok(s)
    } else {
        Err(format!("{v}: not a number of seconds"))
    }
}
