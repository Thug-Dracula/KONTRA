//! Kontakt's own preset content. A stock effect preset names its files
//! relative to the Kontakt install's `Presets` folder (`/presets/Effects/…`),
//! not to the instrument's library, so the sample resolver — which never
//! leaves the library root — reported every response such a preset names as
//! missing. Here those names are resolved against the install's preset root.

use std::path::{Path, PathBuf};

use crate::samples::{Location, Resolver};

/// The folder this machine's Kontakt installs sit in, when
/// `KONTRA_KONTAKT_PRESETS` does not name the preset folder itself.
const INSTALLS: &str = "Armory/VSTs";

/// The factory preset roots, newest install first.
pub(crate) struct Presets {
    roots: Vec<PathBuf>,
    /// Where the roots were looked for, so a miss can say so.
    installs: PathBuf,
}

impl Presets {
    /// The roots: `KONTRA_KONTAKT_PRESETS` alone when it names a folder, else
    /// every portable Kontakt installed under `~/Armory/VSTs`.
    pub(crate) fn from_environment() -> Self {
        let installs = std::env::var_os("HOME")
            .map_or_else(|| PathBuf::from("/"), PathBuf::from)
            .join(INSTALLS);
        let over = std::env::var_os("KONTRA_KONTAKT_PRESETS")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty());
        Self::new(discovered(over, &installs), installs)
    }

    /// `roots` in the order they are tried; `installs` is only named in errors.
    pub(crate) fn new(roots: Vec<PathBuf>, installs: PathBuf) -> Self {
        Self { roots, installs }
    }

    /// The factory preset file `name` names, from the first root that has it.
    /// `Ok(None)` when the name is not a factory path at all; `Err` names
    /// every root tried, because a root this build cannot find is exactly the
    /// difference between a response loading and being reported missing.
    pub(crate) fn locate(
        &self,
        name: &str,
        resolver: &mut Resolver,
    ) -> Result<Option<Location>, String> {
        let Some(relative) = relative(name) else {
            return Ok(None);
        };
        let parts: Vec<&str> = relative
            .split('/')
            .filter(|part| !part.is_empty() && *part != "." && *part != "..")
            .collect();
        for root in &self.roots {
            let Some((path, taken)) = descend(root, &parts) else {
                continue;
            };
            if taken == parts.len() {
                if path.is_file() {
                    return Ok(Some(Location {
                        file: path,
                        offset: 0,
                        size: None,
                        keystream: None,
                    }));
                }
                continue;
            }
            // The parts left over name a member of an archive Kontakt ships
            // its responses in (`K4IR.nkx`).
            let member = parts.get(taken..).unwrap_or_default().join("/");
            if let Some(found) = resolver.archive_at(&path, &member)? {
                return Ok(Some(found));
            }
        }
        Err(self.nowhere())
    }

    fn nowhere(&self) -> String {
        if self.roots.is_empty() {
            return format!(
                "not found; no Kontakt presets root (KONTRA_KONTAKT_PRESETS is unset and {}/*/Kontakt */Presets has none)",
                self.installs.display()
            );
        }
        let tried: Vec<String> = self
            .roots
            .iter()
            .map(|root| root.display().to_string())
            .collect();
        format!(
            "not found in the instrument's library or Kontakt presets ({})",
            tried.join(", ")
        )
    }
}

/// Whether `name` is a path into Kontakt's own preset content: `/presets/…`,
/// the leading slash and the case of the prefix both as Kontakt writes them
/// on either platform.
pub(crate) fn is_preset(name: &str) -> bool {
    relative(name).is_some()
}

/// `name` after `presets/`, when it begins with it, with `\` and `|` read as
/// `/` (Kontakt stores both spellings).
fn relative(name: &str) -> Option<String> {
    let name = name.replace(['\\', '|'], "/");
    let rest = name.trim_start_matches('/');
    let head = rest.get(..8)?;
    head.eq_ignore_ascii_case("presets/")
        .then(|| rest.get(8..).unwrap_or_default().to_owned())
}

/// `root` and `parts` walked one name at a time, compared without case
/// (Kontakt writes its own paths on Windows), stopping at the first file:
/// how many parts the walk used.
fn descend(root: &Path, parts: &[&str]) -> Option<(PathBuf, usize)> {
    let mut path = root.to_path_buf();
    for (taken, part) in parts.iter().enumerate() {
        if !path.is_dir() {
            return Some((path, taken));
        }
        let entry = std::fs::read_dir(&path).ok()?.flatten().find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(part)
        })?;
        path = entry.path();
    }
    Some((path, parts.len()))
}

/// The preset roots to try: `over` alone when given, else each
/// `<install>/Kontakt <version>/Presets` under `installs`, newest version
/// first.
fn discovered(over: Option<PathBuf>, installs: &Path) -> Vec<PathBuf> {
    if let Some(root) = over {
        return vec![root];
    }
    let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(installs) else {
        return Vec::new();
    };
    for install in entries.flatten().map(|entry| entry.path()) {
        let Some(name) = install
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !name.to_lowercase().starts_with("kontakt") || !install.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&install) else {
            continue;
        };
        for child in children.flatten().map(|entry| entry.path()) {
            let Some(version) = child
                .file_name()
                .and_then(|name| version(name.to_string_lossy().as_ref()))
            else {
                continue;
            };
            let presets = child.join("Presets");
            if presets.is_dir() {
                found.push((version, presets));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    found.into_iter().map(|(_, root)| root).collect()
}

/// `8.14.1` from `Kontakt 8.14.1`; `None` for any other folder name.
fn version(name: &str) -> Option<Vec<u32>> {
    let rest = name
        .get(..8)
        .filter(|head| head.eq_ignore_ascii_case("kontakt "))
        .and_then(|_| name.get(8..))?;
    let version: Vec<u32> = rest
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    (!version.is_empty()).then_some(version)
}

#[cfg(test)]
pub(crate) mod scratch {
    use std::path::{Path, PathBuf};

    /// A folder of one test's own, removed when it is dropped.
    pub(crate) struct Scratch(pub(crate) PathBuf);

    impl Scratch {
        pub(crate) fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("kontra-factory-{}-{test}", std::process::id()));
            std::fs::create_dir_all(&path).expect("a temp folder");
            Self(path)
        }

        pub(crate) fn file(&self, at: &str) -> PathBuf {
            let path = self.0.join(at);
            let parent = path.parent().unwrap_or(Path::new("/"));
            std::fs::create_dir_all(parent).expect("the file's folder");
            std::fs::write(&path, b"response").expect("the file");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::scratch::Scratch;

    #[test]
    fn a_preset_path_is_recognised_with_or_without_its_leading_slash() {
        assert!(is_preset(
            "/presets/Effects/Convolution/15 Special Effects/Mix/IR Samples/Tape Echo.wav"
        ));
        assert!(is_preset(
            "presets/effects/convolution/K4IR.nkx/K4 IR Samples/L300 8.7s.wav"
        ));
        assert!(is_preset("PREsets\\Effects\\Convolution\\x.wav"));
        assert!(!is_preset("/Presets"));
        assert!(!is_preset("Resources/ir_samples/Tape Echo.wav"));
    }

    #[test]
    fn the_newest_install_is_the_first_root_and_the_override_is_the_only_one() {
        let scratch = Scratch::new("roots");
        for install in [
            "Kontakt 7 Portable/Kontakt 7",
            "Kontakt 8 Portable/Kontakt 8",
            "Kontakt 8.14.1 Portable/Kontakt 8",
        ] {
            scratch.file(&format!("{install}/Presets/Effects/Convolution/x.txt"));
        }
        std::fs::create_dir_all(scratch.0.join("Kontakt1/Library")).unwrap();
        let roots = discovered(None, &scratch.0);
        let named: Vec<String> = roots
            .iter()
            .map(|root| root.strip_prefix(&scratch.0).unwrap().display().to_string())
            .collect();
        assert_eq!(
            named,
            [
                "Kontakt 8.14.1 Portable/Kontakt 8/Presets",
                "Kontakt 8 Portable/Kontakt 8/Presets",
                "Kontakt 7 Portable/Kontakt 7/Presets"
            ]
        );
        let over = PathBuf::from("/elsewhere/Presets");
        assert_eq!(discovered(Some(over.clone()), &scratch.0), [over]);
        assert!(discovered(None, &scratch.0.join("nothing")).is_empty());
    }

    #[test]
    fn a_factory_file_resolves_though_its_name_has_another_case() {
        let scratch = Scratch::new("case");
        let response = scratch
            .file("Factory/Effects/Convolution/15 Special Effects/Mix/IR Samples/Tape Echo.wav");
        let instrument = scratch.file("Library/Instruments/test.nki");
        let mut resolver = Resolver::new(&instrument, None).unwrap();
        let presets = Presets::new(vec![scratch.0.join("Factory")], scratch.0.clone());
        let found = presets
            .locate(
                "/presets/effects/convolution/15 special effects/mix/ir samples/tape echo.wav",
                &mut resolver,
            )
            .unwrap()
            .unwrap();
        assert_eq!(found.file, response);
        assert_eq!((found.offset, found.size), (0, None));
    }

    #[test]
    fn an_unknown_factory_path_names_every_root_tried() {
        let scratch = Scratch::new("unknown");
        let roots = vec![scratch.0.join("one"), scratch.0.join("two")];
        let instrument = scratch.file("Library/Instruments/test.nki");
        let mut resolver = Resolver::new(&instrument, None).unwrap();
        let presets = Presets::new(roots.clone(), scratch.0.clone());
        let error = presets
            .locate("/presets/Effects/Convolution/missing.wav", &mut resolver)
            .unwrap_err();
        for root in &roots {
            assert!(
                error.contains(&root.display().to_string()),
                "{error} names no {}",
                root.display()
            );
        }
        assert!(
            presets
                .locate("Resources/ir_samples/x.wav", &mut resolver)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn no_root_at_all_says_where_it_looked() {
        let scratch = Scratch::new("none");
        let instrument = scratch.file("Library/Instruments/test.nki");
        let mut resolver = Resolver::new(&instrument, None).unwrap();
        let presets = Presets::new(Vec::new(), scratch.0.join("VSTs"));
        let error = presets
            .locate("/presets/Effects/Convolution/missing.wav", &mut resolver)
            .unwrap_err();
        assert!(error.contains("KONTRA_KONTAKT_PRESETS"), "{error}");
        assert!(error.contains("VSTs"), "{error}");
    }
}
