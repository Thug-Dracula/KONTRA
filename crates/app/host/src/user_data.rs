//! The per-library folder a script's `load_array(<array>, 1)` reads.
//!
//! KSP gives a file array two sources by mode: mode 2 reads the library's own
//! resources (`Resources/data/x.nka`, which the format reader collects), and
//! mode 1 reads the *Data* folder — beside the resource container for a user
//! instrument, and `<Kontakt user data>/<library name>/Data` for a library
//! Kontakt Player can load
//! ([KSP manual, load/save commands](https://docs.native-instruments.com/ni-tech-manuals/ksp-manual/en/load-save-commands)).
//!
//! NI's own instruments keep first-run state there. The Session Guitarist,
//! Session Bassist, Session Ukulele and Session Horns scripts write
//! `welcome_state.nka` when the player dismisses their quick-start pane and
//! read it back in `on init`; answering "no such file" leaves
//! `%welcome_state[1]` at 0, so `on persistence_changed` re-opens the pane and
//! the pane's `on note` branch swallows every note (measured: a note is
//! ignored and nothing is played, where Kontakt plays 0.310712 on the same
//! instrument — `docs/re/ignored-note-2026-11-03.md`).
//!
//! The folder is found from a Kontakt install on this machine, because that is
//! where Kontakt keeps it: a portable install writes its user data beside the
//! executable (`<install>/UserData`), and its `Settings.cfg` names each
//! registered library with the `ContentDir` it was added under. A library
//! Kontakt renamed after the fact keeps the name it was registered with —
//! `Session Guitarist - Electric Sunburst Deluxe` still reads its data from
//! `Session Guitarist - Electric Storm Deluxe` — which is why the registration
//! is matched by content folder and not by product name. `KONTRA_USER_DATA`
//! names the folder directly, for an install whose layout this does not
//! cover.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Kontakt's user data folder, as `KONTRA_USER_DATA` names it or as an
/// installed Kontakt keeps it (`<install>/UserData`, holding `Settings.cfg`).
///
/// `installs` are executable paths, as [`kontra_library::kontakt::candidates`]
/// lists them.
#[must_use]
pub fn root(installs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("KONTRA_USER_DATA") {
        let dir = PathBuf::from(named);
        return dir.is_dir().then_some(dir);
    }
    // Discovery walks the disk; the layout does not change while a process
    // runs, so the answer is found once.
    static FOUND: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            beside_an_install(installs)
                .or_else(|| dirs::home_dir().and_then(|home| portable_install(&home, MAX_DEPTH)))
        })
        .clone()
}

/// The folder a Kontakt install keeps its user data in: the nearest ancestor
/// of the executable holding a `UserData` folder with a `Settings.cfg`.
fn beside_an_install(installs: &[PathBuf]) -> Option<PathBuf> {
    installs.iter().find_map(|exe| {
        exe.ancestors().skip(1).find_map(|dir| {
            let user = dir.join("UserData");
            user.join(SETTINGS).is_file().then_some(user)
        })
    })
}

/// A portable install keeps its user data beside the executable, and the
/// executable can be anywhere: Kontakt's own library discovery
/// ([`kontra_library::kontakt::candidates`]) looks in the Windows program
/// folders of every wine prefix it knows, which misses one extracted into a
/// plain folder (`~/Armory/VSTs/Kontakt 8 Portable`). This walks `home` that
/// far down instead, and stops at the first `UserData` with a `Settings.cfg`.
fn portable_install(home: &Path, depth: usize) -> Option<PathBuf> {
    let mut queue = std::collections::VecDeque::from([(home.to_path_buf(), 0usize)]);
    let mut visited = 0usize;
    while let Some((dir, level)) = queue.pop_front() {
        visited += 1;
        if visited > MAX_DIRS {
            return None;
        }
        let user = dir.join("UserData");
        if user.join(SETTINGS).is_file() {
            return Some(user);
        }
        if level >= depth {
            continue;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if queue.len() > MAX_DIRS {
                return None;
            }
            // A file's name never names a user data folder, and a hidden one
            // is a wine prefix, which `kontakt::candidates` already covers.
            if entry.file_type().is_ok_and(|t| t.is_dir())
                && !entry.file_name().to_string_lossy().starts_with('.')
            {
                queue.push_back((entry.path(), level + 1));
            }
        }
    }
    None
}

/// The library's mode-1 data folder: `<root>/<registered name>/Data`, when
/// that folder exists.
///
/// `library` is the library root an instrument was read from
/// ([`kontra_engine::ir::Instrument::library_path`]).
#[must_use]
pub fn library_folder(root: &Path, library: Option<&Path>) -> Option<PathBuf> {
    let library = library?;
    let name = registered(root, library).or_else(|| product_name(library))?;
    let folder = root.join(name).join("Data");
    folder.is_dir().then_some(folder)
}

/// The name Kontakt registered the library under, matched to `library` by the
/// last component of its `ContentDir`.
fn registered(root: &Path, library: &Path) -> Option<String> {
    let folder = library.file_name()?.to_str()?;
    let settings = read_bounded(&root.join(SETTINGS))?;
    let settings = String::from_utf8_lossy(&settings);
    registrations(&settings)
        .into_iter()
        .find(|(_, content)| content.eq_ignore_ascii_case(folder))
        .map(|(name, _)| name)
}

/// The product name a library's `.nicnt` carries, when it has exactly one.
fn product_name(library: &Path) -> Option<String> {
    let found: Vec<PathBuf> = std::fs::read_dir(library)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nicnt"))
        })
        .collect();
    if found.len() != 1 {
        return None;
    }
    kontra_library::product(found.first()?).map(|(name, _)| name)
}

/// The library sections of a `Settings.cfg`, as (section name, the last
/// component of its `ContentDir`).
///
/// Kontakt writes one section per registered library, with the values
/// `sz:`-prefixed and Windows paths. A section without a `ContentDir` is a
/// setting, not a library.
fn registrations(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.to_owned(), None));
            continue;
        }
        // `ContentDir=sz:X:\VSTs\Kontakt1\Session Ukulele Library`, and a
        // quoted form for a path with a trailing space.
        if let Some((key, value)) = line.split_once('=')
            && key == "ContentDir"
            && let Some(slot) = out.last_mut()
        {
            let value = value.strip_prefix("sz:").unwrap_or(value).trim();
            let value = value.trim_matches('"');
            let last = value.rsplit(['\\', '/']).next().unwrap_or_default();
            slot.1 = Some(last.to_owned());
        }
    }
    out.into_iter()
        .filter_map(|(name, content)| Some((name, content?)))
        .collect()
}

/// A file's bytes, up to the limit any resource a script loads has to stay
/// under; nothing for a larger file or a device.
fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    const LIMIT: u64 = 1 << 22;
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= LIMIT).then_some(bytes)
}

/// The settings file Kontakt writes beside each library's data.
const SETTINGS: &str = "Settings.cfg";
/// How far below `home` a portable install's user data folder can be
/// (`~/Armory/VSTs/Kontakt 8 Portable/UserData` is four).
const MAX_DEPTH: usize = 4;
/// A ceiling on folders examined by one search, so a home folder of many
/// small entries cannot make a load slow.
const MAX_DIRS: usize = 20_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kontra-user-data-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_registration_names_the_folder_the_library_reads_from() {
        let text = "fftw=sz:noise\r\n\
                    [Session Guitarist - Electric Storm Deluxe]\r\n\
                    Name=sz:Session Guitarist - Electric Storm Deluxe\r\n\
                    ContentDir=sz:X:\\VSTs\\Kontakt1\\Session Guitarist - Electric Storm Deluxe [Native Instruments]\r\n\
                    [Unrelated]\r\n\
                    Name=sz:Unrelated\r\n";
        assert_eq!(
            registrations(text),
            vec![(
                "Session Guitarist - Electric Storm Deluxe".to_owned(),
                "Session Guitarist - Electric Storm Deluxe [Native Instruments]".to_owned()
            )]
        );
    }

    #[test]
    fn a_library_reads_its_data_from_the_folder_it_was_registered_under() {
        let root = temp("registered");
        let library =
            root.join("Kontakt1/Session Guitarist - Electric Storm Deluxe [Native Instruments]");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(
            root.join(SETTINGS),
            "[Session Guitarist - Electric Storm Deluxe]\r\n\
             Name=sz:Session Guitarist - Electric Storm Deluxe\r\n\
             ContentDir=sz:X:\\vst-dir\\Session Guitarist - Electric Storm Deluxe [Native Instruments]\r\n",
        )
        .unwrap();
        // Kontakt renamed the product after registering it: the content
        // folder still decides, not the name on the `.nicnt`.
        std::fs::write(
            library.join("Session Guitarist - Electric Sunburst Deluxe.nicnt"),
            "x",
        )
        .unwrap();
        let data = root.join("Session Guitarist - Electric Storm Deluxe/Data");
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(library_folder(&root, Some(&library)), Some(data));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_library_kontakt_never_registered_falls_back_to_its_product_name() {
        let root = temp("product");
        let library = root.join("Session Bassist - Upright Bass");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(root.join(SETTINGS), "").unwrap();
        std::fs::write(
            library.join("Session Bassist - Upright Bass.nicnt"),
            "<?xml version=\"1.0\"?><ProductHints><Product><Name>Session Bassist - Upright Bass</Name><Company>Native Instruments</Company></Product></ProductHints>",
        )
        .unwrap();
        let data = root.join("Session Bassist - Upright Bass/Data");
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(library_folder(&root, Some(&library)), Some(data));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_instrument_that_is_not_in_a_library_has_no_data_folder() {
        let root = temp("none");
        assert!(library_folder(&root, None).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_user_data_folder_is_beside_the_installed_executable() {
        let root = temp("install");
        let exe = root.join("Kontakt 8/x64/Kontakt 8.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        std::fs::create_dir_all(root.join("UserData")).unwrap();
        std::fs::write(root.join("UserData").join(SETTINGS), "").unwrap();
        assert_eq!(beside_an_install(&[exe]), Some(root.join("UserData")));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
