//! `dump_persistent <instrument.nki>`: print each script slot's saved persistent values.
//!
//! One line per saved value, prefixed with its slot, so a patch's own stored state
//! (arrays and scalars the script `make_persistent`s) can be read without loading it.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: dump_persistent <instrument.nki>");
        return Ok(());
    };
    let instrument = kontra_format_kontakt::load(std::path::Path::new(&path))?;
    for script in &instrument.scripts {
        for value in &script.saved {
            println!("{}:{}", script.slot, value);
        }
    }
    Ok(())
}
