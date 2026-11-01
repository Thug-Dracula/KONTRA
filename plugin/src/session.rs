//! The session log: one JSON line per event, written and flushed as it
//! happens to `~/.cache/kontra/logs/session-<pid>-<stamp>-1.jsonl`. A host
//! shows what the editor draws and nothing else, so a failure the editor
//! cannot draw (a library that did not open, a page that got no pictures)
//! is written here instead of vanishing.
//!
//! `KONTRA_LOG_DIR` puts the files elsewhere. Quiet is the default: with
//! `KONTRA_LOG` unset (or empty) only the first line — the path, so the file
//! is findable from the plugin alone — and errors are written; set it to
//! anything other than `0` or `off` to add the state-change lines [`info`]
//! writes. `KONTRA_LOG=0` (or `off`) turns the log off entirely. A file stops
//! taking writes at [`MAX_BYTES`] with a final `log_full` line, and the newest
//! [`KEEP`] sessions are kept, so a log can never fill the disk.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use kontra_host::ui::{FailureSink, Missing};

/// Bytes one session file may reach before it stops taking writes.
const MAX_BYTES: u64 = 4 << 20;
/// Session files kept; the oldest beyond this are removed at start.
const KEEP: usize = 16;
/// Buckets [`info`] rate-limits events in.
const SLOTS: usize = 4;

struct Log {
    file: Option<File>,
    path: PathBuf,
    bytes: u64,
    full: bool,
    /// `KONTRA_LOG` is unset or empty: the startup line and errors only.
    quiet: bool,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

/// What `KONTRA_LOG` asks for.
///
/// One parse for the whole module: the variable used to be read twice, once to
/// decide whether the file opens and once to decide whether it takes state
/// changes, and two readings of one variable are two chances to disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    /// `0` or `off`: no file at all.
    Off,
    /// Unset or empty — the default. The file, its first line and errors.
    Quiet,
    /// Anything else: the state-change lines [`info`] writes as well.
    Verbose,
}

impl Level {
    /// The level a `KONTRA_LOG` value names.
    fn of(value: Option<&OsStr>) -> Self {
        match value {
            Some(v) if v == "0" || v == "off" => Self::Off,
            Some(v) if !v.is_empty() => Self::Verbose,
            _ => Self::Quiet,
        }
    }
}

/// Where the session log's directory is.
fn directory() -> PathBuf {
    std::env::var_os("KONTRA_LOG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        })
        .unwrap_or_else(std::env::temp_dir)
        .join("kontra")
        .join("logs")
}

/// The session log's file, opened on first use.
fn log() -> &'static Mutex<Log> {
    LOG.get_or_init(|| {
        let level = Level::of(std::env::var_os("KONTRA_LOG").as_deref());
        let dir = directory();
        let _ = std::fs::create_dir_all(&dir);
        prune(&dir);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = dir.join(format!("session-{}-{stamp}-1.jsonl", std::process::id()));
        let file = (level != Level::Off)
            .then(|| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .ok()
            })
            .flatten();
        Mutex::new(Log {
            file,
            path,
            bytes: 0,
            full: false,
            quiet: level != Level::Verbose,
        })
    })
}

/// Keeps the newest [`KEEP`] session files and removes the rest.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = (entries)
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("session-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (_, path) in files.into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

/// The session log's path, for a report line; empty when it is not open.
#[must_use]
pub fn path() -> PathBuf {
    log().lock().map(|l| l.path.clone()).unwrap_or_default()
}

/// Writes the session's first line, with the path: the log exists from here
/// on, whether or not anything goes wrong. Called on every tick; the first
/// call wins.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let path = path();
        // Not through [`write_line`]: a quiet session still has to say where
        // its file is, which is the one line it cannot lose.
        let Ok(mut log) = log().lock() else {
            return;
        };
        write(
            &mut log,
            "info",
            "session_started",
            &[
                ("log", &path.display().to_string()),
                ("build", crate::BUILD),
                ("pid", &std::process::id().to_string()),
            ],
        );
    });
}

/// An error: what happened, and the values that say where. Always written,
/// quiet or not.
pub fn error(event: &str, data: &[(&str, &str)]) {
    write_line("error", event, data);
}

/// A significant state change, at most once a second per event name, so a
/// drag or a load cannot flood the log. The control side writes these,
/// never the audio thread. Quiet unless `KONTRA_LOG` turns them on.
pub fn info(event: &str, data: &[(&str, &str)]) {
    static LAST: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];
    // The slot is a hash of the whole name, not its first letter: an event
    // no longer shares a slot merely because its name shares a prefix (every
    // future `c...` name with `control_applied`). Four slots are not a
    // collision-free table; the test below holds every name the plugin
    // actually emits to its own slot.
    let Some(at) = LAST.get(slot(event)) else {
        return;
    };
    if !admit(at, now_ms()) {
        return;
    }
    write_line("info", event, data);
}

/// [`info`]'s rate limit, as a state machine over one slot: says whether an
/// event may write at `now` and records the write when it may.
///
/// The store is `now.max(1)`, not `now`: [`NEVER`] is what "has not written
/// yet" means, and the first state change of a session happens at t = 0 on
/// purpose — a load starts the session — so a raw 0 would read as no write at
/// all. The price is that the second event in a session is allowed at 1001 ms
/// rather than 1000.
fn admit(last: &AtomicU64, now: u64) -> bool {
    if !due(last.load(Ordering::Relaxed), now) {
        return false;
    }
    last.store(now.max(1), Ordering::Relaxed);
    true
}

/// The `last` a slot holds before it has written anything.
const NEVER: u64 = 0;

/// Whether an event whose slot last wrote at `last` ms may write at `now`:
/// once before it has ever written, then at most once a second. `last == 0`
/// reading as "never" is why [`info`] stores `now.max(1)`.
fn due(last: u64, now: u64) -> bool {
    last == NEVER || now.saturating_sub(last) >= 1000
}

/// FNV-1a over the whole event name, reduced to a rate-limit slot. Stable
/// across runs and builds; [`tests::every_event_the_plugin_logs_gets_its_own_slot`]
/// holds every name the plugin emits to a separate one.
const fn slot(event: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut rest = event.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        rest = tail;
    }
    (hash % SLOTS as u64) as usize
}

/// Milliseconds since the plugin started, for [`info`]'s rate.
fn now_ms() -> u64 {
    static AT: OnceLock<std::time::Instant> = OnceLock::new();
    AT.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// One JSON line, written and flushed now; dropped once the file is full and
/// dropped entirely when it is a state change in a quiet session.
fn write_line(level: &str, event: &str, data: &[(&str, &str)]) {
    let Ok(mut log) = log().lock() else {
        return;
    };
    emit(&mut log, level, event, data);
}

/// [`write_line`]'s gate: `info` goes only to a session with `KONTRA_LOG` set,
/// errors always go.
fn emit(log: &mut Log, level: &str, event: &str, data: &[(&str, &str)]) {
    if log.quiet && level == "info" {
        return;
    }
    write(log, level, event, data);
}

/// [`write_line`], for a log the caller holds.
fn write(log: &mut Log, level: &str, event: &str, data: &[(&str, &str)]) {
    if log.full {
        return;
    }
    let mut text = String::with_capacity(128);
    text.push_str("{\"level\":\"");
    text.push_str(level);
    text.push_str("\",\"event\":\"");
    push_escaped(&mut text, event);
    text.push('"');
    for (key, value) in data {
        text.push_str(",\"");
        push_escaped(&mut text, key);
        text.push_str("\":\"");
        push_escaped(&mut text, value);
        text.push('"');
    }
    text.push_str("}\n");
    let size = text.len() as u64;
    let Some(file) = log.file.as_mut() else {
        return;
    };
    if log.bytes + size > MAX_BYTES {
        log.full = true;
        let _ = file.write_all(b"{\"level\":\"error\",\"event\":\"log_full\"}\n");
        let _ = file.flush();
        return;
    }
    if file.write_all(text.as_bytes()).is_ok() {
        let _ = file.flush();
        log.bytes += size;
    }
}

/// `text` with JSON's escapes; control characters become spaces (a path or a
/// reason never needs them).
fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
}

/// A part's picture failures, for the loader of the face of `instrument`:
/// the first one with its reason and asset, then one line per 64 with the
/// running count. A frame asks for hundreds of pictures, and one line each
/// would bury the reason; a page that stays pictureless still says so, and
/// how many it lost.
pub fn picture_failures(instrument: Option<PathBuf>) -> FailureSink {
    /// Failures between two counted lines.
    const PICTURES_PER_LINE: usize = 64;
    let seen = AtomicUsize::new(0);
    let instrument = instrument.map_or_else(String::new, |p| p.display().to_string());
    Box::new(move |missing: Missing, asset: &str| {
        let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 || n.is_multiple_of(PICTURES_PER_LINE) {
            error(
                "picture_missing",
                &[
                    ("reason", &missing.to_string()),
                    ("asset", asset),
                    ("instrument", &instrument),
                    ("count", &n.to_string()),
                ],
            );
        }
    })
}

/// A picture whose bytes were read but did not decode, for the face of
/// `instrument`: the first with its asset, then one line per 64 with the
/// running count. The event is `picture_missing`'s, so one place lists every
/// picture a page lost; the reason says the library held the file.
pub fn picture_undecodable(instrument: Option<PathBuf>) -> kontra_ui::pictures::Undecoded {
    /// Failures between two counted lines.
    const PICTURES_PER_LINE: usize = 64;
    let seen = AtomicUsize::new(0);
    let instrument = instrument.map_or_else(String::new, |p| p.display().to_string());
    Box::new(move |asset: &str| {
        let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 || n.is_multiple_of(PICTURES_PER_LINE) {
            error(
                "picture_missing",
                &[
                    (
                        "reason",
                        "the bytes read for that path are not a picture this decodes",
                    ),
                    ("asset", asset),
                    ("instrument", &instrument),
                    ("count", &n.to_string()),
                ],
            );
        }
    })
}

/// A picture too big for the renderer's atlas, said once: past that size
/// vello draws nothing and the page loses it without a word.
pub fn picture_too_large(side: u32, limit: u32) {
    static SAID: AtomicUsize = AtomicUsize::new(0);
    if SAID.swap(1, Ordering::Relaxed) == 0 {
        error(
            "picture_too_large",
            &[("side", &side.to_string()), ("limit", &limit.to_string())],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh temporary directory for one test's log file, so tests that set
    /// their own verbosity cannot collide.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kontra-session-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp directory");
        dir
    }

    /// A log over `file`, as a session with `quiet` would open it.
    fn open(file: &Path, quiet: bool) -> Log {
        let _ = std::fs::remove_file(file);
        Log {
            file: OpenOptions::new().create(true).append(true).open(file).ok(),
            path: file.to_path_buf(),
            bytes: 0,
            full: false,
            quiet,
        }
    }

    #[test]
    fn a_line_is_one_json_object_with_its_escapes() {
        let mut text = String::new();
        push_escaped(&mut text, "a\"b\\c\nd");
        assert_eq!(text, "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn a_line_reaches_the_file_as_it_is_written() {
        let file = scratch("flush").join("session-flush-1-1-1.jsonl");
        let mut log = open(&file, false);
        write(
            &mut log,
            "error",
            "picture_missing",
            &[("reason", "no library"), ("asset", "pictures/x.png")],
        );
        // Read while the writer is still alive: the line is on disk already.
        let text = std::fs::read_to_string(&file).expect("the line is flushed");
        assert_eq!(
            text,
            "{\"level\":\"error\",\"event\":\"picture_missing\",\"reason\":\"no library\",\"asset\":\"pictures/x.png\"}\n"
        );
    }

    #[test]
    fn quiet_is_the_default_and_only_a_setting_turns_state_changes_on() {
        assert_eq!(Level::of(None), Level::Quiet, "unset is quiet");
        assert_eq!(
            Level::of(Some(OsStr::new(""))),
            Level::Quiet,
            "empty is quiet"
        );
        assert_eq!(
            Level::of(Some(OsStr::new("0"))),
            Level::Off,
            "0 turns the log off"
        );
        assert_eq!(
            Level::of(Some(OsStr::new("off"))),
            Level::Off,
            "off turns the log off"
        );
        assert_eq!(Level::of(Some(OsStr::new("1"))), Level::Verbose);
        assert_eq!(Level::of(Some(OsStr::new("debug"))), Level::Verbose);
    }

    /// The first state change of a session happens at t = 0, when the load
    /// that started it begins; it must be written, not swallowed by the
    /// once-a-second limit, and the second one must still wait. Driven through
    /// the same [`admit`] `info` calls, so a store that lost the "written"
    /// record fails here too.
    #[test]
    fn the_first_state_change_of_a_session_is_written() {
        let slot = AtomicU64::new(NEVER);
        assert!(admit(&slot, 0), "an event at t = 0 must write once");
        assert!(!admit(&slot, 0), "two events in the first millisecond");
        assert!(!admit(&slot, 1_000), "the write at 0 counts as 1 ms");
        assert!(admit(&slot, 1_001));

        // And a slot nothing has written to yet is never held back.
        assert!(admit(&AtomicU64::new(NEVER), 500));
        // The limit is one second, measured from the last write.
        let slot = AtomicU64::new(NEVER);
        assert!(admit(&slot, 500));
        assert!(!admit(&slot, 1_499));
        assert!(admit(&slot, 1_500));
    }

    #[test]
    fn a_quiet_session_writes_an_error_but_no_state_change() {
        let file = scratch("quiet").join("session-quiet-1-1-1.jsonl");
        let mut log = open(&file, true);
        emit(&mut log, "info", "control_applied", &[("commands", "1")]);
        emit(
            &mut log,
            "error",
            "picture_missing",
            &[("reason", "no library")],
        );
        let text = std::fs::read_to_string(&file).expect("the error is flushed");
        assert!(
            !text.contains("control_applied"),
            "a quiet session wrote a state change:\n{text}"
        );
        assert!(
            text.contains("\"event\":\"picture_missing\""),
            "a quiet session dropped its error:\n{text}"
        );
    }

    /// The event names `lib.rs` passes to [`info`], read from its source: a
    /// third one added later is covered without editing this list.
    fn emitted_info_events() -> Vec<String> {
        const CALL: &str = "session::info(";
        let mut rest = include_str!("lib.rs");
        let mut names = Vec::new();
        while let Some(at) = rest.find(CALL) {
            rest = rest[at + CALL.len()..].trim_start();
            if let Some(name) = rest.strip_prefix('"')
                && let Some(end) = name.find('"')
            {
                names.push(name[..end].to_owned());
            }
        }
        names
    }

    #[test]
    fn every_event_the_plugin_logs_gets_its_own_slot() {
        let names = emitted_info_events();
        assert!(
            names.len() >= 2,
            "lib.rs no longer logs its state changes, so this test pins nothing: {names:?}"
        );
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(
                    slot(a),
                    slot(b),
                    "{a} and {b} share a rate-limit slot and would silence each other"
                );
            }
        }
        // And the hash covers the whole name, not its first byte: a
        // first-letter bucket puts these two `c...` events in one slot.
        assert_ne!(
            slot("control_applied"),
            slot("cache_missed"),
            "the slot is a first-letter bucket again"
        );
    }
}
