//! `kontra-silence`: where the chain from "the file has zones" to "the output
//! has samples" breaks, one instrument at a time.
//!
//! ```sh
//! kontra-silence --manifest list.txt [--json records.jsonl] [--key auto|60] [--all-keys]
//!                 [--cc 2=64]
//! ```
//!
//! Each manifest line is an instrument path, or a JSON object with a `path`
//! field; blank lines and `#` comments are skipped. One tab-separated record
//! per instrument goes to stdout, the same records as JSON Lines with `--json`.
//!
//! # What it measures
//!
//! The chain a note must pass for a zone to sound, and what this build reads
//! for each link:
//!
//! 1. **Translated / prepared.** Zones in the reader's model, and how many the
//!    prepare pass kept. Dropped zones are counted by reason (purged group,
//!    absent sample, sample the store could not open).
//! 2. **Selected.** For the played key and velocity: prepared zones whose key
//!    and velocity ranges contain it, then the engine's own refusal record
//!    (`Engine::rejects`, compiled into the `trace` build): trigger, group
//!    mask, purge, key range, velocity, zone slot, bus, start rule. The first
//!    condition that fails is the one reported, in the order the engine tests
//!    them.
//! 3. **Voices.** `voice_start` events from the engine trace, and the distinct
//!    zones that produced one.
//! 4. **Samples.** Source frames each voice's play cursor advanced (position
//!    sampled between 128-frame blocks), and `missing_frames` — frames a voice
//!    had to render as silence because the data was not there.
//! 5. **Gain.** The peak of each voice's gain matrix (`current_gain`: zone
//!    gain, group level, modulators and the amplitude envelope), and the peak
//!    of the voice's own audio before any group effect, so a voice loud inside
//!    and silent outside is told apart from one silent inside.
//! 6. **Output.** The peak of the final stereo mix, after groups, buses,
//!    sends, the instrument rack and the master fader.
//!
//! `broken` is the first link with no evidence behind it, as a short name;
//! `detail` carries the numbers for that link. When a script ignores the note
//! and plays its own (`ignore_event` + `play_note`), the record says so: the
//! refusals then belong to the script's note, not the host's.
//!
//! # Causal probe
//!
//! With `--probe`, a row that ends `downstream` is rendered a second time with
//! every non-bypassed effect slot whose stored output gain is 0 or less set to
//! unity, and `probe_peak` reports what the mix does then. A `probe_peak` well
//! above the first render is evidence that those slots, not something further
//! down, own the silence. The probe never changes what the engine does with the
//! file; it builds a second plan for one measurement.
//!
//! # Script probe
//!
//! `KONTRA_SCRIPT_SOURCE=<dir>` replaces script slot `n`'s source with
//! `<dir>/n.ksp` when that file exists, as `kontra-render` does, so a `message()`
//! can be added to a branch and read back from the same harness that measures
//! the row: `last_message` in the JSON is the last text the script showed. That
//! is how the deciding branch of a `note_ignored` row was named.
//!
//! # Limitations (read these before trusting a row)
//!
//! - **One note, one point in time.** Velocity is fixed (default 100), the hold
//!   is short (default 500 ms) and no controller, pitch bend or key switch is
//!   sent unless asked for (`--cc NUM=VALUE` sends a controller before the
//!   note, for an instrument whose group start criteria read one). An instrument that needs a keyswitch held, a mod
//!   wheel moved, or a UI control set to make a sound is reported as silent at
//!   the point measured, with `start_rule`/`velocity`/`key_range`/`trigger`
//!   saying why. That is a statement about this probe, not about the
//!   instrument.
//! - **The key is chosen, not given.** `--key auto` (the default) plays the
//!   candidate key that the most prepared zones cover, so a "silent" row is not
//!   the harness picking a key no zone uses. `--all-keys` emits one row per
//!   candidate key.
//! - **Envelope.** A voice with a slow attack that has not opened within the
//!   hold reads as `voice_gain_zero`; `envelope_stages` and the reported gain
//!   distinguish "attack has not arrived" from "gain is exactly zero". The
//!   tool does not wait long enough to call a 30-second pad silent.
//! - **Sample content** is probed only with `--sample-frames N`: up to N frames
//!   from each covering zone's region start are decoded through the store's own
//!   read path, so `sample_peak_min` says whether those zones hold any non-zero
//!   audio there. It reads the heads only, not the whole sample.
//! - **Effects are judged by their output, not their settings.** A closed
//!   filter, a bypassed insert or a ducking compressor shows up as a silent
//!   voice or a silent mix; `filter_hz_min` is reported for the covering
//!   groups, and `zero_output_slots` names every non-bypassed slot in the
//!   covering groups' racks and the instrument/main/bus racks whose stored
//!   output gain is 0. Whether Kontakt plays such a slot is a question this
//!   tool cannot answer on its own; `--probe` only shows what one render does
//!   without it.
//! - **One engine, fresh.** Saved script values and the instrument's own state
//!   are the load defaults; nothing is recalled from a project. An instrument
//!   silent only after a particular saved state is not covered.
//! - **Trace capacity.** The trace is bounded; `trace_dropped` reports events
//!   it lost, and `voices_started: 0` with a non-zero drop is untrustworthy.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use kontra_engine::{Engine, Event, HostNote, HostPattern, Plan, Prepared, SampleTime, Timed};
use kontra_stream::StreamClient;
use kontra_stream::{Store, StoreConfig};
use serde::Serialize;

/// The engine's rate, as every other probe and the oracle use.
const RATE: u32 = 48_000;
/// Frames per `process` call; the granularity of the between-block samples.
const BLOCK: usize = 128;
/// Below this a peak is silence: the same threshold `tools/census/keysweep.py` uses.
const SILENT: f64 = 1e-4;
/// The keys the census keysweep plays; `auto` chooses among them.
const KEYS: [u8; 9] = [36, 43, 48, 55, 60, 64, 67, 72, 84];
/// Trace buffer, in events.
const TRACE_EVENTS: usize = 1 << 20;

/// One instrument's row.
#[derive(Clone, Debug, Serialize)]
struct Record {
    path: String,
    name: String,
    error: Option<String>,
    key: u8,
    velocity: u8,
    /// Zones the reader produced.
    zones_translated: usize,
    /// Zones the prepare pass kept (the plan's zones).
    zones_prepared: usize,
    /// Prepared zones whose key and velocity ranges contain the played note.
    zones_covering: usize,
    /// Distinct groups among the covering zones.
    groups_covering: usize,
    /// Why prepared zones were dropped at load, by reason.
    unprepared: BTreeMap<String, usize>,
    /// `voice_start` events: voices the note started.
    voices_started: usize,
    /// Distinct zones that started a voice.
    zones_sounding: usize,
    /// Source frames the sounding voices advanced, summed over zones.
    frames_consumed: u64,
    /// Source frames rendered as silence because data was unavailable.
    missing_frames: u64,
    /// Largest cell of any sounding voice's gain matrix, sampled between blocks.
    voice_gain_max: f64,
    /// Largest peak of a voice's own audio before its group's effects.
    voice_peak_max: f64,
    /// Peak of the final stereo mix.
    peak: f64,
    /// Attack-pass refusals by reason, for the block the note-on is in.
    rejections: BTreeMap<String, usize>,
    /// Release-pass refusals by reason, recorded the same way.
    rejections_release: BTreeMap<String, usize>,
    /// Covering zones by trigger: `attack`, `first`, `legato`, `release`.
    triggers: BTreeMap<String, usize>,
    /// `play_note` calls a script made while the note was being decided.
    script_play_note: usize,
    /// `ignore_event` calls a script made while the note was being decided.
    script_ignore_event: usize,
    /// Lowest and highest key any prepared zone spans.
    keys_min: u8,
    keys_max: u8,
    /// Notes a script ignored (their zones never started).
    ignored_notes: usize,
    /// Smallest group gain among the covering zones' groups.
    group_gain_min: f64,
    /// Smallest bus gain among the covering zones' groups.
    bus_gain_min: f64,
    /// Lowest cutoff among per-voice filters on the covering groups, when any.
    filter_hz_min: Option<f64>,
    /// Lowest decoded sample peak among the covering zones, when probed.
    sample_peak_min: Option<f64>,
    /// Fewest source frames among the covering zones' playable regions.
    region_frames_min: u64,
    /// Covering zones read as wavetables rather than as a region of frames.
    wavetable_zones: usize,
    /// Non-bypassed effect slots storing an output gain of 0 or less, in the
    /// covering groups' racks and the instrument, main and bus racks.
    zero_output_slots: Vec<String>,
    /// Envelope stages a sounding voice passed through.
    envelope_stages: Vec<String>,
    /// Trace events the bounded buffer dropped.
    trace_dropped: u64,
    /// Mix peak with every zero-output slot set to unity (`--probe`).
    probe_peak: Option<f64>,
    /// The last text a script asked to show (`message()`), which is how a
    /// probe script reports the branch it took.
    last_message: String,
    /// The first link with no evidence behind it.
    broken: String,
    /// The numbers behind `broken`.
    detail: String,
}

impl Record {
    fn failed(path: &Path, error: String) -> Self {
        Self {
            path: path.display().to_string(),
            name: String::new(),
            error: Some(error.clone()),
            key: 0,
            velocity: 0,
            zones_translated: 0,
            zones_prepared: 0,
            zones_covering: 0,
            groups_covering: 0,
            unprepared: BTreeMap::new(),
            voices_started: 0,
            zones_sounding: 0,
            frames_consumed: 0,
            missing_frames: 0,
            voice_gain_max: 0.0,
            voice_peak_max: 0.0,
            peak: 0.0,
            rejections: BTreeMap::new(),
            rejections_release: BTreeMap::new(),
            triggers: BTreeMap::new(),
            script_play_note: 0,
            script_ignore_event: 0,
            keys_min: 0,
            keys_max: 0,
            ignored_notes: 0,
            group_gain_min: 1.0,
            bus_gain_min: 1.0,
            filter_hz_min: None,
            sample_peak_min: None,
            region_frames_min: 0,
            wavetable_zones: 0,
            zero_output_slots: Vec::new(),
            envelope_stages: Vec::new(),
            trace_dropped: 0,
            probe_peak: None,
            last_message: String::new(),
            broken: "load_error".into(),
            detail: String::new(),
        }
    }
}

#[derive(Debug)]
struct Args {
    manifest: PathBuf,
    json: Option<PathBuf>,
    keys: Vec<u8>,
    all_keys: bool,
    velocity: u8,
    hold_ms: u32,
    tail_ms: u32,
    sample_frames: u64,
    /// Controllers sent before the note, as `(number, value)`.
    ccs: Vec<(u8, u8)>,
    probe: bool,
    quiet: bool,
}

const USAGE: &str = "usage: kontra-silence --manifest FILE [--json OUT.jsonl] \
[--key auto|N|N,N,...] [--all-keys] [--velocity 1..=127] [--hold-ms N] \
[--tail-ms N] [--sample-frames N] [--cc NUM=VALUE] [--probe] [--quiet]";

fn main() -> Result<()> {
    let args = parse(std::env::args().skip(1))?;
    let paths = manifest(&args.manifest)?;
    if paths.is_empty() {
        bail!("{} lists no instruments", args.manifest.display());
    }
    let mut out = std::io::stdout().lock();
    let mut json = match &args.json {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?,
        )),
        None => None,
    };
    if !args.quiet {
        writeln!(out, "{}", header())?;
    }
    let mut rows = Vec::with_capacity(paths.len());
    for (n, path) in paths.iter().enumerate() {
        let records = match probe(path, &args) {
            Ok(records) => records,
            Err(e) => vec![Record::failed(path, format!("{e:#}"))],
        };
        for record in records {
            if !args.quiet {
                writeln!(out, "{}", line(&record))?;
                out.flush()?;
            }
            if let Some(json) = &mut json {
                serde_json::to_writer(&mut *json, &record)?;
                json.write_all(b"\n")?;
            }
            rows.push(record);
        }
        if !args.quiet && (n + 1) % 50 == 0 {
            eprintln!("{}/{} instruments", n + 1, paths.len());
        }
    }
    if let Some(json) = &mut json {
        json.flush()?;
    }
    if !args.quiet {
        let sound = rows.iter().filter(|r| r.broken == "ok").count();
        eprintln!(
            "{} rows, {sound} sound, {} silent or failed",
            rows.len(),
            rows.len() - sound
        );
    }
    Ok(())
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Args> {
    let mut parsed = Args {
        manifest: PathBuf::new(),
        json: None,
        keys: Vec::new(),
        all_keys: false,
        velocity: 100,
        hold_ms: 500,
        tail_ms: 500,
        sample_frames: 0,
        ccs: Vec::new(),
        probe: false,
        quiet: false,
    };
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--manifest" => parsed.manifest = PathBuf::from(value()?),
            "--json" => parsed.json = Some(PathBuf::from(value()?)),
            "--all-keys" => parsed.all_keys = true,
            "--probe" => parsed.probe = true,
            "--quiet" => parsed.quiet = true,
            "--velocity" => parsed.velocity = number(&value()?)?,
            "--hold-ms" => parsed.hold_ms = number(&value()?)?,
            "--tail-ms" => parsed.tail_ms = number(&value()?)?,
            "--sample-frames" => parsed.sample_frames = number(&value()?)?,
            "--cc" => {
                let pair = value()?;
                let (cc, value) = pair
                    .split_once('=')
                    .with_context(|| format!("--cc takes NUM=VALUE\n{USAGE}"))?;
                let (cc, value): (u8, u8) = (number(cc)?, number(value)?);
                if cc > 127 || value > 127 {
                    bail!("--cc {cc}={value} is outside 0..=127");
                }
                parsed.ccs.push((cc, value));
            }
            "--key" => {
                let value = value()?;
                if value != "auto" {
                    for part in value.split(',') {
                        parsed.keys.push(number(part)?);
                    }
                }
            }
            other => bail!("unknown argument {other}\n{USAGE}"),
        }
    }
    if parsed.manifest.as_os_str().is_empty() {
        bail!("--manifest is required\n{USAGE}");
    }
    if parsed.velocity == 0 || parsed.velocity > 127 {
        bail!("velocity {} is outside 1..=127", parsed.velocity);
    }
    Ok(parsed)
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    text.parse::<T>()
        .map_err(|e| anyhow::anyhow!("{text}: {e}"))
}

fn manifest(path: &Path) -> Result<Vec<PathBuf>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = if line.starts_with('{') {
            let value: serde_json::Value = serde_json::from_str(line)
                .with_context(|| format!("manifest line is not JSON: {line}"))?;
            value
                .get("path")
                .and_then(serde_json::Value::as_str)
                .with_context(|| format!("manifest line has no path: {line}"))?
                .to_owned()
        } else {
            line.to_owned()
        };
        out.push(PathBuf::from(path));
    }
    Ok(out)
}

fn header() -> String {
    [
        "broken", "key", "tr/prep", "cover", "voices", "frames", "gain", "vpeak", "peak", "probe",
        "rejects", "name",
    ]
    .join("\t")
}

fn line(r: &Record) -> String {
    let rejects: Vec<String> = r
        .rejections
        .iter()
        .map(|(k, v)| format!("{k}:{v}"))
        .collect();
    format!(
        "{}\t{}\t{}/{}\t{}\t{}\t{}\t{:.4}\t{:.4}\t{:.4}\t{}\t{}\t{}",
        r.broken,
        r.key,
        r.zones_translated,
        r.zones_prepared,
        r.zones_covering,
        r.voices_started,
        r.frames_consumed,
        r.voice_gain_max,
        r.voice_peak_max,
        r.peak,
        r.probe_peak
            .map_or_else(|| "-".to_owned(), |p| format!("{p:.4}")),
        rejects.join(","),
        r.name,
    )
}

/// Loads, plays and measures one instrument. One record per key asked for.
fn probe(path: &Path, args: &Args) -> Result<Vec<Record>> {
    let mut ir = kontra_host::instrument::read(path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    // KONTRA_SCRIPT_SOURCE=<dir>: script n's source is replaced by <dir>/n.ksp
    // when that file exists, as `kontra-render` does, so a probe can add a
    // `message()` and read it back from the same harness that measures the row.
    if let Some(dir) = std::env::var_os("KONTRA_SCRIPT_SOURCE") {
        for (n, script) in ir.scripts.iter_mut().enumerate() {
            if let Ok(source) = std::fs::read_to_string(Path::new(&dir).join(format!("{n}.ksp"))) {
                script.source = source;
            }
        }
    }
    let name = ir.name.clone();
    let zones_translated = ir.groups.iter().map(|g| g.zones.len()).sum();
    let sample_count = ir.samples.len();
    let resident: BTreeSet<usize> = ir
        .groups
        .iter()
        .filter(|g| !g.purged)
        .flat_map(|g| g.zones.iter().map(|z| z.sample))
        .collect();
    // Every zone the reader produced, numbered the way the prepare pass numbers
    // them, so the ones it drops can be named after the fact.
    let input: Vec<(usize, usize, usize)> = ir
        .groups
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .zones
                .iter()
                .enumerate()
                .map(move |(m, zone)| (g, m, zone.sample))
        })
        .collect();
    let mut store = Store::new(StoreConfig::DEFAULT).context("creating sample store")?;
    let loaded = kontra_host::instrument::build(ir, RATE, &mut store);
    let plan = Arc::new(loaded.plan);
    let unprepared = unprepared(&plan, &input, sample_count, &resident);

    let candidates = if args.keys.is_empty() {
        KEYS.to_vec()
    } else {
        args.keys.clone()
    };
    let mut keys: Vec<u8> = Vec::new();
    if args.all_keys {
        keys.extend(candidates.iter().copied());
    } else {
        keys.push(best_key(&plan, &candidates, args.velocity));
    }

    let mut records = Vec::with_capacity(keys.len());
    for key in keys {
        records.push(one(
            &plan,
            &name,
            path,
            key,
            args,
            zones_translated,
            unprepared.clone(),
            &mut store,
        )?);
    }
    Ok(records)
}

/// The candidate key the most prepared zones cover; ties go to the key nearest 60.
fn best_key(plan: &Plan, keys: &[u8], velocity: u8) -> u8 {
    let mut best: u8 = 60;
    let mut best_cover = 0usize;
    for &key in keys {
        let cover = plan
            .zones
            .iter()
            .filter(|z| z.keys.contains(&key) && z.velocities.contains(&velocity))
            .count();
        if cover > best_cover || (cover == best_cover && key.abs_diff(60) < best.abs_diff(60)) {
            best = key;
            best_cover = cover;
        }
    }
    best
}

/// Zones the reader produced that the prepare pass did not keep, by reason.
///
/// The plan holds only prepared zones, so a zone with no `(group, member)` in
/// the plan was dropped. The reason names why its sample was not loaded, as
/// `kontra-inspect load` names it.
fn unprepared(
    plan: &Plan,
    input: &[(usize, usize, usize)],
    sample_count: usize,
    resident: &BTreeSet<usize>,
) -> BTreeMap<String, usize> {
    let prepared: BTreeSet<(usize, usize)> = plan
        .zones
        .iter()
        .map(|z| (z.group as usize, usize::from(z.member)))
        .collect();
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for &(group, member, sample) in input {
        if prepared.contains(&(group, member)) {
            continue;
        }
        let reason = if sample >= sample_count {
            "invalid_sample_index"
        } else if !resident.contains(&sample) {
            "sample_only_referenced_by_purged_groups"
        } else {
            "resident_sample_unavailable"
        };
        *out.entry(reason.to_owned()).or_default() += 1;
    }
    out
}

/// What one render measured.
#[derive(Debug, Default)]
struct Measured {
    peak: f64,
    missing_frames: u64,
    starts: usize,
    sounding: BTreeSet<usize>,
    gain_max: f64,
    voice_peak_max: f64,
    stages: BTreeSet<String>,
    extent: BTreeMap<usize, (f64, f64)>,
    rejections: BTreeMap<String, usize>,
    rejections_release: BTreeMap<String, usize>,
    ignored_notes: usize,
    script_play_note: usize,
    script_ignore_event: usize,
    trace_dropped: u64,
    last_message: String,
}

#[allow(clippy::too_many_arguments)]
fn one(
    plan: &Arc<Plan>,
    name: &str,
    path: &Path,
    key: u8,
    args: &Args,
    zones_translated: usize,
    unprepared: BTreeMap<String, usize>,
    store: &mut Store,
) -> Result<Record> {
    let covering: Vec<usize> = plan
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| z.keys.contains(&key) && z.velocities.contains(&args.velocity))
        .map(|(i, _)| i)
        .collect();
    let groups: BTreeSet<u32> = covering
        .iter()
        .filter_map(|&i| plan.zones.get(i))
        .map(|z| z.group)
        .collect();
    let measured = render(plan, key, args, store)?;
    let mut record = Record {
        path: path.display().to_string(),
        name: name.to_owned(),
        error: None,
        key,
        velocity: args.velocity,
        zones_translated,
        zones_prepared: plan.zones.len(),
        zones_covering: covering.len(),
        groups_covering: groups.len(),
        unprepared,
        voices_started: measured.starts,
        zones_sounding: measured.sounding.len(),
        frames_consumed: measured
            .extent
            .values()
            .map(|&(lo, hi)| (hi - lo).max(0.0) as u64)
            .sum(),
        missing_frames: measured.missing_frames,
        voice_gain_max: measured.gain_max,
        voice_peak_max: measured.voice_peak_max,
        peak: measured.peak,
        rejections: measured.rejections,
        rejections_release: measured.rejections_release,
        triggers: triggers(plan, &covering),
        script_play_note: measured.script_play_note,
        script_ignore_event: measured.script_ignore_event,
        keys_min: plan
            .zones
            .iter()
            .map(|z| *z.keys.start())
            .min()
            .unwrap_or(0),
        keys_max: plan.zones.iter().map(|z| *z.keys.end()).max().unwrap_or(0),
        ignored_notes: measured.ignored_notes,
        group_gain_min: groups
            .iter()
            .filter_map(|&g| plan.groups.get(g as usize))
            .map(|g| f64::from(g.level.gain.abs()))
            .fold(f64::INFINITY, f64::min),
        bus_gain_min: groups
            .iter()
            .filter_map(|&g| plan.groups.get(g as usize))
            .filter_map(|g| plan.buses.get(g.bus))
            .map(|b| f64::from(b.gain.abs()))
            .fold(f64::INFINITY, f64::min),
        filter_hz_min: None,
        sample_peak_min: None,
        region_frames_min: 0,
        wavetable_zones: 0,
        zero_output_slots: zero_output_slots(plan, &groups),
        envelope_stages: measured.stages.into_iter().collect(),
        trace_dropped: measured.trace_dropped,
        probe_peak: None,
        last_message: measured.last_message.clone(),
        broken: String::new(),
        detail: String::new(),
    };
    record.group_gain_min = finite(record.group_gain_min, 1.0);
    record.bus_gain_min = finite(record.bus_gain_min, 1.0);
    record.filter_hz_min = filter_hz(plan, &groups);
    let (region_frames, tables) = region(plan, &covering);
    record.region_frames_min = region_frames;
    record.wavetable_zones = tables;
    if args.sample_frames > 0 {
        record.sample_peak_min = sample_peak(plan, &covering, args.sample_frames, &store.client());
    }
    let (broken, detail) = verdict(&record);
    record.broken = broken;
    record.detail = detail;
    if args.probe && record.broken == "downstream" && !record.zero_output_slots.is_empty() {
        let mut unmut = (**plan).clone();
        let changed = unity_zero_outputs(&mut unmut, &groups);
        if changed > 0 {
            let new_plan = Arc::new(unmut);
            let probed = render(&new_plan, key, args, store)?;
            record.probe_peak = Some(probed.peak);
            record.detail = format!(
                "{}; with {changed} zero-output slot(s) at unity the mix peaks {:.6}",
                record.detail, probed.peak
            );
        }
    }
    Ok(record)
}

/// Renders the note and samples the engine between blocks.
fn render(plan: &Arc<Plan>, key: u8, args: &Args, store: &mut Store) -> Result<Measured> {
    let mut measured = Measured::default();
    let stream = store.client();
    let mut engine = Engine::new_traced(Arc::clone(plan), stream.clone(), RATE, TRACE_EVENTS);
    engine.set_offline_wait(true);
    // A quarter second after load before the note, as a player gives a program.
    let mut silent = vec![[0.0f32; 2]; BLOCK];
    for _ in 0..(RATE as usize / 4 / BLOCK) {
        engine.process(&mut silent, &[]);
        install(&mut engine, plan);
    }
    engine.clear_rejects();
    engine.collect_rejects(true);

    // Events are timed on the engine's own clock, which the pre-roll has
    // already advanced: a note at `SampleTime(0)` would be 248 ms old when it
    // arrives, and a script that stamps `$ENGINE_UPTIME` on its notes (the
    // Evolution family does) then reads a stale one.
    let start = engine.now().0;
    let hold = u64::from(RATE) * u64::from(args.hold_ms) / 1000;
    let stop = hold + u64::from(RATE) * u64::from(args.tail_ms) / 1000;
    let on = Event::NoteOn {
        note: HostNote {
            port: 0,
            channel: 0,
            key,
            id: -1,
            clap: false,
        },
        velocity: f64::from(args.velocity) / 127.0,
        tune: 0.0,
    };
    let off = Event::NoteOff(HostPattern {
        port: 0,
        channel: 0,
        key: i32::from(key),
        id: -1,
        clap: false,
    });

    let mut audio = vec![[0.0f32; 2]; BLOCK];
    let mut at = 0u64;
    while at < stop {
        let frames = usize::try_from((stop - at).min(BLOCK as u64))?;
        let until = at + frames as u64;
        let block = audio
            .get_mut(..frames)
            .context("a block does not fit the buffer")?;
        let mut timed: Vec<Timed<Event>> = Vec::new();
        if at == 0 {
            // Controllers first: a group's start criteria may read one, and the
            // note's own start is decided when it arrives.
            timed.extend(args.ccs.iter().map(|&(cc, value)| Timed {
                at: SampleTime(start),
                body: Event::midi1(0xb0, cc, value),
            }));
            timed.push(Timed {
                at: SampleTime(start),
                body: on,
            });
        }
        if hold >= at && hold < until {
            timed.push(Timed {
                at: SampleTime(start + hold),
                body: off,
            });
        }
        stream.settle(Duration::from_secs(5));
        let missing_before = engine.stats().missing_frames;
        engine.process(block, &timed);
        if at == 0 {
            // Only the block holding the note-on: every refusal in it belongs to
            // this note or the note a script played in its callback. A later
            // block's refusals belong to notes the instrument plays for itself.
            engine.collect_rejects(false);
        }
        measured.missing_frames += engine.stats().missing_frames - missing_before;
        install(&mut engine, plan);
        for sample in block.iter() {
            measured.peak = measured
                .peak
                .max(f64::from(sample[0].abs()).max(f64::from(sample[1].abs())));
        }
        for event in engine.trace_events() {
            match event {
                kontra_engine::TraceEvent::Voice {
                    zone,
                    event: kontra_engine::VoiceEvent::Start,
                    ..
                } => {
                    measured.starts += 1;
                    measured.sounding.insert(*zone);
                }
                kontra_engine::TraceEvent::Action { action, .. } if at == 0 => match action {
                    kontra_engine::Action::PlayNote => measured.script_play_note += 1,
                    kontra_engine::Action::IgnoreEvent => measured.script_ignore_event += 1,
                    _ => {}
                },
                _ => {}
            }
        }
        engine.clear_trace();
        for voice in engine.inspect_voices() {
            for cell in voice.gain.iter().flat_map(|row| row.iter()) {
                measured.gain_max = measured.gain_max.max(f64::from(cell.abs()));
            }
            measured.voice_peak_max = measured.voice_peak_max.max(f64::from(voice.last_peak));
            measured.stages.insert(voice.envelope_stage.to_owned());
            let entry = measured
                .extent
                .entry(voice.zone)
                .or_insert((voice.position, voice.position));
            entry.0 = entry.0.min(voice.position);
            entry.1 = entry.1.max(voice.position);
        }
        at = until;
    }
    measured.trace_dropped = engine.trace_dropped();
    measured.last_message = engine.last_message().to_owned();
    for record in engine.rejects() {
        let map = if record.attack {
            &mut measured.rejections
        } else {
            &mut measured.rejections_release
        };
        *map.entry(record.reason.name().to_owned()).or_default() += 1;
        if record.reason == kontra_engine::Reject::Ignored {
            measured.ignored_notes += 1;
        }
    }
    Ok(measured)
}

/// Sets every non-bypassed zero-output slot in the probed racks to unity gain,
/// and returns how many slots changed.
fn unity_zero_outputs(plan: &mut Plan, groups: &BTreeSet<u32>) -> usize {
    let mut changed = 0usize;
    let mut rack = |slots: &mut [Option<kontra_engine::ir::Effect>]| {
        for effect in slots.iter_mut().flatten() {
            if !effect.bypass && effect.output <= 0.0 {
                effect.output = 1.0;
                changed += 1;
            }
        }
    };
    rack(&mut plan.inserts);
    rack(&mut plan.main);
    for bus in &mut plan.buses {
        rack(&mut bus.inserts);
    }
    for &group in groups {
        if let Some(group) = plan.groups.get_mut(group as usize) {
            rack(&mut group.inserts);
        }
    }
    changed
}

/// Non-bypassed slots storing an output gain of 0 or less, in the racks that
/// can silence the covering groups.
fn zero_output_slots(plan: &Plan, groups: &BTreeSet<u32>) -> Vec<String> {
    let mut out = Vec::new();
    let mut rack = |where_: &str, slots: &[Option<kontra_engine::ir::Effect>]| {
        for (slot, effect) in slots.iter().enumerate() {
            let Some(effect) = effect else { continue };
            if !effect.bypass && effect.output <= 0.0 {
                out.push(format!(
                    "{where_}/{slot} {:?} output {} dry {}",
                    short(effect.kind),
                    effect.output,
                    effect.dry
                ));
            }
        }
    };
    rack("instrument", &plan.inserts);
    rack("main", &plan.main);
    for (bus, state) in plan.buses.iter().enumerate() {
        rack(&format!("bus{bus}"), &state.inserts);
    }
    for &group in groups {
        if let Some(group) = plan.groups.get(group as usize) {
            rack(&format!("group[{}]", group.name), &group.inserts);
        }
    }
    out
}

/// The effect kind's name without its parameters.
fn short(kind: kontra_engine::ir::EffectKind) -> String {
    let text = format!("{kind:?}");
    text.split(['(', ' ']).next().unwrap_or(&text).to_owned()
}

/// Lowest per-voice filter cutoff on a covering group, when a group has one.
fn filter_hz(plan: &Plan, groups: &BTreeSet<u32>) -> Option<f64> {
    let mut lowest: Option<f64> = None;
    for &group in groups {
        let Some(group) = plan.groups.get(group as usize) else {
            continue;
        };
        for effect in group.inserts.iter().flatten() {
            if let kontra_engine::ir::EffectKind::Filter(f) = &effect.kind {
                let hz = f64::from(f.hz);
                lowest = Some(lowest.map_or(hz, |low: f64| low.min(hz)));
            }
        }
    }
    lowest
}

fn finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

/// Fewest playable source frames among the covering zones, and how many of
/// them are wavetables (whose region fields are unused).
fn region(plan: &Plan, covering: &[usize]) -> (u64, usize) {
    let mut frames = u64::MAX;
    let mut tables = 0usize;
    for &i in covering {
        let Some(zone) = plan.zones.get(i) else {
            continue;
        };
        if zone.region.table.is_some() {
            tables += 1;
            continue;
        }
        frames = frames.min(zone.region.end.saturating_sub(zone.region.start));
    }
    (if frames == u64::MAX { 0 } else { frames }, tables)
}

/// Peak of up to `frames` decoded frames from each covering zone's region start.
///
/// The store's own read path is used, so a sample it cannot supply reads as
/// silence here exactly as it would in a voice. A page that was not resident on
/// the first read is requested by it and read again after the store settles.
fn sample_peak(plan: &Plan, covering: &[usize], frames: u64, client: &StreamClient) -> Option<f64> {
    let mut lowest = f64::INFINITY;
    for &i in covering {
        let Some(zone) = plan.zones.get(i) else {
            continue;
        };
        let sample = &zone.region.sample;
        let want = usize::try_from(frames.min(sample.frames().saturating_sub(zone.region.start)))
            .unwrap_or(0);
        if want == 0 {
            lowest = 0.0;
            continue;
        }
        let mut audio = vec![[0.0f32; 2]; want];
        if sample.read(client, zone.region.start, &mut audio).missing > 0 {
            client.settle(Duration::from_secs(5));
            sample.read(client, zone.region.start, &mut audio);
        }
        let peak = audio
            .iter()
            .fold(0f32, |p, f| p.max(f[0].abs()).max(f[1].abs()));
        lowest = lowest.min(f64::from(peak));
    }
    (lowest != f64::INFINITY).then_some(lowest)
}

fn triggers(plan: &Plan, covering: &[usize]) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for &i in covering {
        let Some(zone) = plan.zones.get(i) else {
            continue;
        };
        let name = match zone.trigger {
            kontra_engine::Trigger::Attack => "attack",
            kontra_engine::Trigger::First => "first",
            kontra_engine::Trigger::Legato => "legato",
            kontra_engine::Trigger::Release => "release",
        };
        *out.entry(name.to_owned()).or_default() += 1;
    }
    out
}

/// The chain link with no evidence behind it, and the numbers for it.
///
/// The order is the order of the chain: a link is reported broken only when
/// every earlier link has evidence it held.
fn verdict(r: &Record) -> (String, String) {
    if r.zones_prepared == 0 {
        return (
            "no_prepared_zone".into(),
            format!(
                "{} zones translated, 0 prepared {:?}",
                r.zones_translated, r.unprepared
            ),
        );
    }
    if r.voices_started == 0 && r.zones_covering == 0 {
        let elsewhere = if r.ignored_notes > 0 {
            "; a script ignored the host note and played no note of its own"
        } else {
            ""
        };
        return (
            "key_not_covered".into(),
            format!(
                "no prepared zone spans key {} at velocity {} (of {} prepared{elsewhere})",
                r.key, r.velocity, r.zones_prepared
            ),
        );
    }
    if r.voices_started == 0 {
        // `ignored` is a note-level event, not a zone verdict, so it never
        // names the link: it is reported beside the zone refusals instead.
        let dominant = r
            .rejections
            .iter()
            .filter(|(reason, _)| reason.as_str() != "ignored")
            .max_by_key(|(_, n)| **n);
        let ignored = if r.ignored_notes > 0 {
            format!(
                "; {} note(s) ignored by a script (ignore_event)",
                r.ignored_notes
            )
        } else {
            String::new()
        };
        return match dominant {
            Some((reason, count)) => (
                format!("no_voice:{reason}"),
                format!(
                    "{} covering zones, 0 voices; first refusal {reason} on {count} zone(s){ignored}; \
                     triggers {:?}; attack refusals {:?}; release refusals {:?}; \
                     script ignore_event {} / play_note {}",
                    r.zones_covering,
                    r.triggers,
                    r.rejections,
                    r.rejections_release,
                    r.script_ignore_event,
                    r.script_play_note
                ),
            ),
            None if r.ignored_notes > 0 => (
                "note_ignored".into(),
                format!(
                    "{} covering zones, 0 voices; {} note(s) ignored, {} play_note / {} \
                     ignore_event calls, and no replacement reached selection",
                    r.zones_covering, r.ignored_notes, r.script_play_note, r.script_ignore_event
                ),
            ),
            None => (
                "no_voice".into(),
                format!(
                    "{} covering zones, 0 voices and no refusal recorded: the note did not reach \
                     selection (trace dropped {})",
                    r.zones_covering, r.trace_dropped
                ),
            ),
        };
    }
    if r.frames_consumed == 0 && r.wavetable_zones == 0 {
        return (
            "empty_region".into(),
            format!(
                "{} voices started in {} zone(s) but read 0 source frames \
                 (shortest covering region {} frames)",
                r.voices_started, r.zones_sounding, r.region_frames_min
            ),
        );
    }
    if r.missing_frames > 0 && r.voice_peak_max == 0.0 {
        return (
            "sample_unavailable".into(),
            format!(
                "{} voices, {} frames consumed, {} source frames rendered as silence \
                 because the data was not there",
                r.voices_started, r.frames_consumed, r.missing_frames
            ),
        );
    }
    if r.voice_peak_max > SILENT {
        if r.peak > SILENT {
            return (
                "ok".into(),
                format!(
                    "{} voices, {} frames, voice peak {:.4}, output peak {:.4}",
                    r.voices_started, r.frames_consumed, r.voice_peak_max, r.peak
                ),
            );
        }
        let zeroed = if r.zero_output_slots.is_empty() {
            "no zero-output slot in the racks".to_owned()
        } else {
            format!("zero-output slots {:?}", r.zero_output_slots)
        };
        return (
            "downstream".into(),
            format!(
                "voices reach {:.4} before group effects but the mix peaks {:.6}: group {:.4}, \
                 bus {:.4}; {zeroed}",
                r.voice_peak_max, r.peak, r.group_gain_min, r.bus_gain_min
            ),
        );
    }
    if r.voice_gain_max == 0.0 {
        return (
            "voice_gain_zero".into(),
            format!(
                "{} voices read {} frames but every gain cell is 0 \
                 (group gain {:.4}, bus gain {:.4}, envelope {:?})",
                r.voices_started,
                r.frames_consumed,
                r.group_gain_min,
                r.bus_gain_min,
                r.envelope_stages
            ),
        );
    }
    let sample = r
        .sample_peak_min
        .map_or_else(|| "not probed".to_owned(), |p| format!("{p:.4}"));
    let filter = r
        .filter_hz_min
        .map_or_else(|| "none".to_owned(), |hz| format!("{hz:.1} Hz"));
    (
        "voice_silent".into(),
        format!(
            "{} voices read {} frames, gain peak {:.4} (envelope {:?}), \
             but the voice audio peaks 0; sample head {sample}, per-voice filter {filter}",
            r.voices_started, r.frames_consumed, r.voice_gain_max, r.envelope_stages
        ),
    )
}

/// Installs the effects and samples a script asked for while loading.
fn install(engine: &mut Engine, plan: &Plan) {
    let mut asked = Vec::new();
    engine.drain_requests(|r| asked.push(r));
    for request in asked {
        engine.install(Prepared::new(request, Some(plan), RATE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        let mut r = Record::failed(Path::new("x.nki"), String::new());
        r.error = None;
        r.key = 60;
        r.velocity = 100;
        r.zones_translated = 3;
        r.zones_prepared = 3;
        r.zones_covering = 2;
        r.groups_covering = 1;
        r.voices_started = 1;
        r.zones_sounding = 1;
        r.frames_consumed = 1000;
        r.voice_gain_max = 0.5;
        r.voice_peak_max = 0.25;
        r.region_frames_min = 4800;
        r.envelope_stages = vec!["sustain".into()];
        r
    }

    #[test]
    fn a_voice_that_sounds_is_not_blamed_downstream() {
        let mut r = record();
        r.peak = 0.3;
        assert_eq!(verdict(&r).0, "ok");
    }

    #[test]
    fn a_loud_voice_and_a_silent_mix_is_downstream() {
        assert_eq!(verdict(&record()).0, "downstream");
    }

    #[test]
    fn no_covering_zone_is_reported_before_voices() {
        let mut r = record();
        r.zones_covering = 0;
        r.voices_started = 0;
        assert_eq!(verdict(&r).0, "key_not_covered");
    }

    #[test]
    fn a_masked_note_names_the_mask() {
        let mut r = record();
        r.voices_started = 0;
        r.rejections.insert("group_mask".into(), 15);
        assert_eq!(verdict(&r).0, "no_voice:group_mask");
    }

    #[test]
    fn the_release_pass_does_not_name_the_break() {
        let mut r = record();
        r.voices_started = 0;
        r.rejections.insert("group_mask".into(), 15);
        r.rejections_release.insert("trigger".into(), 900);
        assert_eq!(verdict(&r).0, "no_voice:group_mask");
    }

    #[test]
    fn an_empty_region_is_not_blamed_on_the_gain() {
        let mut r = record();
        r.frames_consumed = 0;
        assert_eq!(verdict(&r).0, "empty_region");
    }

    #[test]
    fn a_voice_with_no_gain_is_not_blamed_on_the_output() {
        let mut r = record();
        r.voice_gain_max = 0.0;
        r.voice_peak_max = 0.0;
        assert_eq!(verdict(&r).0, "voice_gain_zero");
    }
}
