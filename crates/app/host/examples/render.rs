//! The old positional renderer, kept so the research notes' commands still work.
//!
//! `cargo run --release -p kontra-host --example render -- <file> <key> <out.wav> [wheel]`
//!
//! This is a shim over [`kontra_host::render`]: the work, and the `KONTRA_*`
//! variables it honours, live in `kontra-host`'s `render` module and are reached
//! through the first-class `kontra-render` binary. Diagnostics go to stdout, as
//! this example always printed them; nothing here is a stable interface — that is
//! `kontra-render`'s JSON.

use std::path::PathBuf;

use kontra_host::render::{Note, Request};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path: PathBuf = args
        .next()
        .ok_or("usage: render <file> <key> <out.wav> [wheel]")?
        .into();
    let key: u8 = args
        .next()
        .and_then(|k| k.to_str()?.parse().ok())
        .unwrap_or(60);
    let out: PathBuf = args.next().unwrap_or_else(|| "render.wav".into()).into();
    // Optional: the mod wheel moves to this value one second in.
    let wheel: Option<u8> = args.next().and_then(|a| a.to_str()?.parse().ok());

    let request = Request {
        instrument: path,
        out,
        note: Note {
            key,
            ..Note::default()
        },
        wheel,
        ..Request::default()
    };
    let mut stdout = std::io::stdout();
    kontra_host::render::run(&request, &mut stdout)?;
    Ok(())
}
