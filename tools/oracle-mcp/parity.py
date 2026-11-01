#!/usr/bin/env python3
"""One parity datum with a verdict: the same note through real Kontakt and through KONTRA.

    tools/oracle-mcp/parity.py [--nki PATH] [--key 60] [--velocity 100] [--hold 1.5]
                               [--window 2.0] [--out DIR]
                               [--max-level-db X] [--max-band-db X] [--max-band-rms-db X]
                               [--max-lag-ms X]

Kontakt side : oracle-mcp's `oracle_render` (aplaymidi -> Kontakt -> PipeWire null sink ->
               pw-record), with the MIDI written here (note on at 0, off at --hold).
KONTRA side  : the `kontra-render` binary (`cargo build --release -p kontra-host
               --bin kontra-render`), given the same MIDI file, the same rate.

The metrics are `tools/oracle/compare.py`'s own: it aligns the two by envelope
cross-correlation, then reports level (Kontakt - KONTRA RMS), the 1/3-octave band
difference with its median removed, and the envelope lag. This script adds limits:
it exits 1 when one is exceeded, so a comparison can fail a test instead of only
printing numbers.

Exit status: 0 pass, 1 a limit exceeded (or a side was silent), 2 a setup error
(no instrument, no binary, the oracle failed).

## The thresholds, and the noise they sit outside of

Measured on this machine 2026-10-30 with this script's own comparison path, Kontakt
against itself (Guitars/Electric Mint, key 60, velocity 100 held 1.5 s, 2 s window):

| comparison                                     | level dB | band RMS dB | worst band dB | lag ms |
| ---------------------------------------------- | -------- | ----------- | ------------- | ------ |
| same session, 6 pairs of 3 renders             | 0.25 rms / 0.37 max | 0.82 rms / 0.98 max | 3.54 max | 0 |
| across two oracle sessions, same render index  | 0.08 rms / 0.12 max | 0.31 rms / 0.39 max | 1.57 max | 0 |

Why the two rows differ: Kontakt's round robin is a **per-session counter**, so render
*n* of one session is the same sample as render *n* of the next. The cross-session row
is therefore the capture noise floor, and the same-session row is the spread between
*different round-robin samples* — an instrument property, not an engine difference.
Parity compares the **first** render after load on both sides, which is index 0 on both,
so the cross-session row is the floor that matters.

Limits, each a multiple of that floor (and `--max-band-db` also above the 1.83 dB
worst-band figure earlier work measured, and the 3.5 dB same-session band noise):

    --max-level-db     0.5 dB   = 4.2x the 0.12 dB floor
    --max-band-rms-db  1.0 dB   = 2.6x the 0.39 dB floor
    --max-band-db      3.0 dB   = 1.9x the 1.57 dB floor, 0.85x the 3.54 dB RR spread
    --max-lag-ms      10.0 ms   = one envelope frame (the measurement's own resolution;
                                 the observed lag is exactly 0)
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(REPO / "tools" / "oracle"))

import numpy as np  # noqa: E402  (compare.py needs it anyway)

# The library root, under `$HOME`; `KONTRA_TEST_LIBRARY` overrides it, as the Rust
# tests and `tools/verify.sh` already read it. Resolved at use, never baked in, so a
# tool copied to another machine points at that machine's library.
LIBRARY = "Armory/VSTs/Kontakt1"


def library_root() -> Path:
    """The sample-library root: `KONTRA_TEST_LIBRARY` when set, else `$HOME/Armory/VSTs/Kontakt1`."""
    set_to = os.environ.get("KONTRA_TEST_LIBRARY")
    if set_to:
        return Path(set_to).expanduser()
    return Path.home() / LIBRARY


def library_path(relative: str) -> Path:
    """A file under the library root: what a tool's default or JSON record spells."""
    return library_root() / relative


def expand(path: str | Path) -> Path:
    """`$HOME`, `${HOME}` and `~` expanded -- how the tool JSON records spell a path."""
    return Path(os.path.expandvars(str(path))).expanduser()


def resolve(path: str | Path) -> Path:
    """A recorded path, relocated to this machine.

    `$HOME`/`~` expanded, then rebased onto `KONTRA_TEST_LIBRARY` when it sits under the
    default library root -- so a plan or a step file written as
    `$HOME/Armory/VSTs/Kontakt1/...` follows the one variable to wherever the library is.
    """
    resolved = expand(path)
    set_to = os.environ.get("KONTRA_TEST_LIBRARY")
    default = Path.home() / LIBRARY
    if set_to and resolved.is_relative_to(default):
        return Path(set_to).expanduser() / resolved.relative_to(default)
    return resolved


# The single-instrument note `tools/oracle/compare.py` compares on, and the one
# `crates/app/host/src/render.rs` defaults to: velocity 100 held 1.5 s. Relative to
# the library root, so it resolves on whichever machine is running.
DEFAULT_NKI = ("Native Instruments Session Guitarist Electric Mint 1.1.0/"
               "Session Guitarist - Electric Mint Library/Instruments/Electric Mint.nki")

# Limits. See the module docstring for the Kontakt-against-itself noise each sits above.
LEVEL_LIMIT_DB = 0.5
BAND_LIMIT_DB = 3.0
BAND_RMS_LIMIT_DB = 1.0
LAG_LIMIT_MS = 10.0
# Below this a side is silent, not comparable (`compare.py`'s own floor).
SILENT_DB = -80.0


def load_mod(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


compare = load_mod("oracle_compare", REPO / "tools/oracle/compare.py")
wavutil = load_mod("oracle_wavutil", REPO / "tools/kontakt-oracle" / "wavutil.py")
client_mod = load_mod("oracle_mcp_client", HERE / "client.py")


def render_bin() -> Path:
    """The `kontra-render` binary: KONTRA_RENDER, else the cargo target dir, else ./target."""
    if os.environ.get("KONTRA_RENDER"):
        return Path(os.environ["KONTRA_RENDER"])
    target = Path(os.environ.get("CARGO_TARGET_DIR") or REPO / "target") / "release"
    return target / "kontra-render"


def write_float_wav(path: Path, audio: np.ndarray, rate: int = 48000) -> None:
    """Interleaved float32 stereo, the layout `kontra-render` and pw-record both write."""
    a = np.asarray(audio, dtype="<f4")
    payload = a.tobytes()
    hdr = (b"RIFF" + (36 + len(payload)).to_bytes(4, "little") + b"WAVEfmt "
           + (16).to_bytes(4, "little") + (3).to_bytes(2, "little")
           + (2).to_bytes(2, "little") + rate.to_bytes(4, "little")
           + (rate * 8).to_bytes(4, "little") + (8).to_bytes(2, "little")
           + (32).to_bytes(2, "little") + b"data" + len(payload).to_bytes(4, "little"))
    path.write_bytes(hdr + payload)


def read_float_wav(path: Path) -> np.ndarray:
    _, _, chans = wavutil.read(str(path))
    return np.stack([np.asarray(c, dtype=float) for c in chans])


def parse_summary(stdout: str) -> dict:
    """`kontra-render`'s stdout: one JSON object and nothing else."""
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    raise ValueError(f"kontra-render printed no JSON summary: {stdout[-400:]!r}")


def render_kontra(nki: Path, midi: Path, out: Path, rate: int = 48000,
                  timeout: int = 900) -> dict:
    """Renders `midi` through KONTRA and returns `kontra-render`'s JSON summary."""
    binary = render_bin()
    if not binary.exists():
        raise FileNotFoundError(
            f"missing {binary}; build it with:\n"
            f"  CARGO_TARGET_DIR={os.environ.get('CARGO_TARGET_DIR', '$PWD/target')} "
            f"cargo build --release -p kontra-host --bin kontra-render")
    out.parent.mkdir(parents=True, exist_ok=True)
    env = {"PATH": "/usr/bin:/bin", "HOME": str(Path.home())}
    p = subprocess.run([str(binary), "--instrument", str(nki), "--midi", str(midi),
                        "--out", str(out), "--rate", str(rate), "--quiet"],
                       env=env, capture_output=True, text=True, timeout=timeout)
    if p.returncode != 0 or not out.exists():
        raise RuntimeError(f"kontra-render failed rc={p.returncode}: {p.stderr[-800:]}")
    return parse_summary(p.stdout)


def load_as_capture(kontakt_wav: Path, out: Path) -> Path:
    """Rebuilds a capture.py-style slot file: 0.5 s before the onset, then the render.

    `compare.py` drops `PRE` seconds from the start of a capture, so both sides line
    up at the note's onset. `oracle_render`'s WAV starts 50 ms before the onset; this
    adds the rest of the 0.5 s as silence instead of guessing at a latency.
    """
    k = read_float_wav(kontakt_wav)
    loud = np.abs(k).max(axis=0) > 10 ** (-60 / 20)
    first = int(np.argmax(loud)) if loud.any() else 0
    pre = int(compare.PRE * compare.RATE)
    start = first - pre
    if start >= 0:
        k = k[:, start:]
    else:
        k = np.concatenate([np.zeros((2, -start)), k], axis=1)
    write_float_wav(out, k.T)
    return out


def compare_pair(kontakt_capture: Path, kontra_wav: Path, window: float) -> dict:
    """`compare.py`'s alignment and metrics for one Kontakt capture and one render."""
    return compare_renders(kontakt_capture, kontra_wav, window,
                           pre_a=compare.PRE, pre_b=0.0)


def compare_renders(a_path: Path, b_path: Path, window: float, pre_a: float = 0.0,
                    pre_b: float = 0.0) -> dict:
    """The same metrics for any two renders; `pre_*` trims a capture's 0.5 s lead-in."""
    n = int(window * compare.RATE)
    k_all = compare.load(a_path)[:, int(pre_a * compare.RATE):]
    r_all = compare.load(b_path)[:, int(pre_b * compare.RATE):]
    out = {
        "a_rms_db": compare.rms_db(k_all[:, :n]),
        "b_rms_db": compare.rms_db(r_all[:, :n]),
    }
    if out["a_rms_db"] < SILENT_DB or out["b_rms_db"] < SILENT_DB:
        out["silent"] = "a" if out["a_rms_db"] < SILENT_DB else "b"
        return out
    # First pass: the whole ±2.5 s, to find the pair's own offset. Second pass: ±300 ms
    # around it, so a local envelope match cannot win over the real alignment.
    coarse, _ = compare.envelope_lag(compare.envelope(k_all),
                                     compare.envelope(r_all[:, : n + 2 * compare.RATE]))
    lag_frames, env_r = compare.envelope_lag(compare.envelope(k_all),
                                             compare.envelope(r_all[:, : n + 2 * compare.RATE]),
                                             center=coarse, span=30)
    lag = lag_frames * compare.RATE // 100  # samples; + means Kontakt is later
    if lag >= 0:
        k, r = k_all[:, lag:], r_all
    else:
        k, r = k_all, r_all[:, -lag:]
    m = min(k.shape[1], r.shape[1], n)
    k, r = k[:, :m], r[:, :m]
    out["seconds"] = m / compare.RATE
    out["a_rms_db"] = compare.rms_db(k)
    out["b_rms_db"] = compare.rms_db(r)
    out["a_peak_db"] = compare.db(np.abs(k).max())
    out["b_peak_db"] = compare.db(np.abs(r).max())
    out["level_db"] = out["a_rms_db"] - out["b_rms_db"]
    out["peak_db"] = out["a_peak_db"] - out["b_peak_db"]
    out["lag_ms"] = lag_frames * 10.0
    out["env_r"] = env_r
    ke, re = compare.envelope(k), compare.envelope(r)
    out["onsets_a_ms"] = [round(i * 10.0, 1) for i in compare.onsets(ke)]
    out["onsets_b_ms"] = [round(i * 10.0, 1) for i in compare.onsets(re)]
    kb, rb = compare.band_levels(k), compare.band_levels(r)
    diff = kb - rb
    # The broadband offset is the level metric; the bands are compared after removing
    # their median, so a gain-only mismatch does not read as a timbre mismatch.
    resid = diff - np.median(diff)
    out["band_rms_db"] = float(np.sqrt(np.mean(resid ** 2)))
    worst = int(np.argmax(np.abs(resid)))
    out["worst_band_hz"] = float(compare.BANDS[worst])
    out["worst_band_db"] = float(resid[worst])
    out["band_diff_db"] = [round(float(v), 2) for v in diff]
    out["band_resid_db"] = [round(float(v), 2) for v in resid]
    fine_lag, fine_r = compare.waveform_r(k[:, : 2 * compare.RATE], r[:, : 2 * compare.RATE])
    out["wave_r"] = fine_r
    out["wave_lag_ms"] = fine_lag * 1000.0 / compare.RATE
    return out


def verdict(row: dict, level_db: float, band_db: float, band_rms_db: float,
            lag_ms: float) -> dict:
    """Whether one comparison passes, and by how much each limit was missed."""
    if "silent" in row:
        return {"pass": False, "failures": [f"silent: {row['silent']}"], "limits": {
            "level_db": level_db, "band_db": band_db, "band_rms_db": band_rms_db,
            "lag_ms": lag_ms}}
    failures, over = [], {}
    checks = [
        ("level", abs(row["level_db"]), level_db, "dB"),
        ("band", abs(row["worst_band_db"]), band_db, "dB"),
        ("band_rms", row["band_rms_db"], band_rms_db, "dB"),
        ("lag", abs(row["lag_ms"]), lag_ms, "ms"),
    ]
    for name, value, limit, unit in checks:
        over[name] = round(value - limit, 3)
        if value > limit:
            failures.append(f"{name}: {value:.2f} {unit} > {limit:g} {unit}")
    return {"pass": not failures, "failures": failures, "over_by": over,
            "limits": {"level_db": level_db, "band_db": band_db,
                       "band_rms_db": band_rms_db, "lag_ms": lag_ms}}


# A render whose measured onset is this close to the recording's start lost the note's
# attack: pw-record began after aplaymidi's note-on. Measured once in six renders
# (2026-10-30), and a pair that includes one reads a -260 ms envelope lag.
ONSET_FLOOR_S = 0.03


def load_instrument(client, nki: Path, attempts: int = 2) -> dict:
    """`oracle_load` with a fresh session between attempts.

    Measured: the Ctrl+O that opens the Load dialog is intermittent — when it does not
    open, the typed path lands in the browser's search box ("No results found") and the
    page never paints. A new session is the only state that reliably clears it.
    """
    last = None
    for attempt in range(attempts):
        try:
            return fetch_tool(client, "oracle_load", {"nki": str(nki)})
        except RuntimeError as e:
            last = e
            print(f"  oracle_load {nki.name}: attempt {attempt + 1}/{attempts} failed: "
                  f"{str(e).splitlines()[0]}", file=sys.stderr)
            if attempt + 1 == attempts:
                break
            fetch_tool(client, "oracle_down", {})
            fetch_tool(client, "oracle_up", {})
    raise last if last is not None else RuntimeError("oracle_load: no attempt was made")


def render_kontakt(client, midi: Path, attempts: int = 3) -> dict:
    """Renders the MIDI through the loaded instrument, retrying a capture that lost its attack."""
    last = None
    for attempt in range(attempts):
        rendered = fetch_tool(client, "oracle_render", {"midi": str(midi)})
        last = rendered
        if rendered["onset_s"] >= ONSET_FLOOR_S:
            return rendered
        print(f"  oracle_render: onset {rendered['onset_s']} s is at the start of the "
              f"capture (attempt {attempt + 1}/{attempts}); rendering again", file=sys.stderr)
    return last


def fetch_tool(client, name: str, arguments: dict) -> dict:
    """Calls one oracle tool and returns its JSON body."""
    resp = client.tool(name, arguments)
    res = resp.get("result") or {}
    body = client_mod.text_of(resp)
    if res.get("isError"):
        raise RuntimeError(f"{name} failed: {body}")
    try:
        return json.loads(body)
    except json.JSONDecodeError as e:
        raise RuntimeError(f"{name} returned no JSON ({e}): {body[:300]}") from e


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--nki", default=str(library_path(DEFAULT_NKI)))
    ap.add_argument("--key", type=int, default=60)
    ap.add_argument("--velocity", type=int, default=100)
    ap.add_argument("--hold", type=float, default=1.5)
    ap.add_argument("--window", type=float, default=2.0)
    ap.add_argument("--out", default="/tmp/opencode/parity/one")
    ap.add_argument("--kontakt-wav", help="reuse this Kontakt WAV instead of driving the oracle")
    ap.add_argument("--json", help="write the whole result here")
    ap.add_argument("--max-level-db", type=float, default=LEVEL_LIMIT_DB)
    ap.add_argument("--max-band-db", type=float, default=BAND_LIMIT_DB)
    ap.add_argument("--max-band-rms-db", type=float, default=BAND_RMS_LIMIT_DB)
    ap.add_argument("--max-lag-ms", type=float, default=LAG_LIMIT_MS)
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    nki = expand(args.nki).resolve()
    if not nki.exists():
        print(f"nki not found: {nki} "
              f"(set KONTRA_TEST_LIBRARY to the sample-library root)", file=sys.stderr)
        return 2

    midi = out / "parity.mid"
    server = load_mod("oracle_mcp_server", HERE / "server.py")
    meta = server.write_midi(midi, [{"start_s": 0.0, "length_s": args.hold,
                                     "key": args.key, "velocity": args.velocity}])
    print(f"midi: {midi} {meta['notes']} note(s) {meta['duration_s']}s", file=sys.stderr)

    if args.kontakt_wav:
        kontakt_wav = Path(args.kontakt_wav)
        rendered = {"wav": str(kontakt_wav), "sha256": None, "frames": None,
                    "duration_s": None, "peak_dbfs": None, "onset_s": None}
        print(f"kontakt: reusing {kontakt_wav}", file=sys.stderr)
    else:
        # One session, exactly as an agent would drive it: up, load, render, down.
        client = client_mod.Client()
        try:
            client.handshake()
            fetch_tool(client, "oracle_up", {})
            load_instrument(client, nki)
            rendered = render_kontakt(client, midi)
            print("oracle: up, loaded, rendered", file=sys.stderr)
        except Exception as e:  # noqa: BLE001 - report, tear down, and fail
            print(f"oracle failed: {e}", file=sys.stderr)
            return 2
        finally:
            try:
                fetch_tool(client, "oracle_down", {})
            except Exception as e:  # noqa: BLE001
                print(f"oracle_down failed: {e}", file=sys.stderr)
            client.close()
        kontakt_wav = Path(rendered["wav"])

    capture = load_as_capture(kontakt_wav, out / "kontakt-as-capture.wav")

    kontra_wav = out / "kontra.wav"
    try:
        kontra = render_kontra(nki, midi, kontra_wav, timeout=900)
    except (RuntimeError, FileNotFoundError) as e:
        print(f"kontra failed: {e}", file=sys.stderr)
        return 2

    row = compare_pair(capture, kontra_wav, args.window)
    result = verdict(row, args.max_level_db, args.max_band_db, args.max_band_rms_db,
                     args.max_lag_ms)
    summary = {
        "nki": str(nki),
        "note": {"key": args.key, "velocity": args.velocity, "hold_s": args.hold},
        "window_s": args.window,
        "kontakt": {**rendered, "sha256": rendered.get("sha256")
                    or hashlib.sha256(kontakt_wav.read_bytes()).hexdigest()},
        "kontra": kontra,
        "compare": row,
        "verdict": result,
        "thresholds": {"level_db": args.max_level_db, "band_db": args.max_band_db,
                       "band_rms_db": args.max_band_rms_db, "lag_ms": args.max_lag_ms},
    }
    text = json.dumps(summary, indent=2, default=float)
    if args.json:
        Path(args.json).write_text(text + "\n")
    print(text)
    print(
        "PARITY {}  level {:+.2f} dB  band rms {:.2f} dB  worst {:+.2f} dB @ {} Hz  "
        "lag {:+.0f} ms  wave r {:.3f}".format(
            "PASS" if result["pass"] else "FAIL", row.get("level_db", float("nan")),
            row.get("band_rms_db", float("nan")), row.get("worst_band_db", float("nan")),
            row.get("worst_band_hz", float("nan")), row.get("lag_ms", float("nan")),
            row.get("wave_r", float("nan"))),
        file=sys.stderr)
    for failure in result["failures"]:
        print(f"  {failure}", file=sys.stderr)
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
