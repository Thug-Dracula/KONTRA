//! `dump_scripts <instrument.nki> [out-dir]`: write each script slot's KSP source.
//!
//! Prints one line per slot to stderr and the sources to `out-dir/slot-<n>.ksp`
//! (default: stdout, separated by `// ---- slot <n> ----`).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: dump_scripts <instrument.nki> [out-dir]");
        return Ok(());
    };
    let out = args.next().map(PathBuf::from);
    let instrument = kontra_format_kontakt::load(std::path::Path::new(&path))?;
    for (slot, script) in instrument.scripts.iter().enumerate() {
        eprintln!(
            "slot {slot}: {:?} ({} bytes)",
            script.name,
            script.source.len()
        );
        match &out {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                std::fs::write(dir.join(format!("slot-{slot}.ksp")), &script.source)?;
            }
            None => println!("// ---- slot {slot} ----\n{}", script.source),
        }
    }
    Ok(())
}
