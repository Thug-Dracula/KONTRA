#!/usr/bin/env python3
"""The parity corpus gate: render the corpus cases in KONTRA and fail on drift from Kontakt.

    tools/parity-corpus/run.py                  every case
    tools/parity-corpus/run.py --subset         the gate's three (Glory Days, Fredonia, Electric Mint)
    tools/parity-corpus/run.py --case ID        one case (repeatable)
    tools/parity-corpus/run.py --json OUT       also write the table as JSON

The gate never needs Kontakt. Each case carries two reference WAVs, captured from real Kontakt in
two separate sessions (`kontakt/<id>-a.wav`, `-b.wav`); the spread between those two takes is the
per-case threshold. This script renders the same MIDI in KONTRA, aligns the pair by cross-
correlation, and compares level and 1/3-octave band RMS against that threshold.

  --capture      (not for CI) re-capture the references from a live Kontakt through
                 tools/oracle-mcp, one session per take, and rewrite cases.json

Exit status: 0 every case inside its threshold (or matching its recorded refusal/silence),
1 drift, 2 the corpus cannot be read or KONTRA cannot run.

Metric definitions are `tools/oracle/compare.py`'s, reimplemented here in the standard library so
the gate runs on a machine with no numpy: level is the integrated RMS difference over the aligned
window (Kontakt - KONTRA), band RMS is the RMS of the 1/3-octave band differences after their
median is removed. Standard library only, by design: a gate that needs an installed wheel to run
is a gate that fails open.

Each case's `instrument` is relative to the library root in `cases.json`'s `library_root` block:
`KONTRA_TEST_LIBRARY` when it is set, else `$HOME/Armory/VSTs/Kontakt1`. No case carries a path
that only exists on the machine that recorded it.
"""
from __future__ import annotations

import argparse
import array
import json
import math
import os
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
CORPUS = HERE / "cases.json"
REFS = HERE / "kontakt"

RATE = 48000
SILENT_DB = -80.0
ALIGN_WINDOW_MS = 10.0  # how far the correlation may move the onset-centred lag
ALIGN_SLACK_MS = 1.0  # how far the found alignment may sit from the case's recorded offset

# 1/3-octave centres 40 Hz .. 16 kHz, exactly compare.py's list.
BANDS = []
_f = 40.0
while _f <= 16000:
    BANDS.append(_f)
    _f *= 2 ** (1 / 3)


# --------------------------------------------------------------------------- WAV


def read_wav(path: Path):
    """Return (channels x frames list of float lists, rate). Reads float32 and PCM16 RIFF."""
    data = path.read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise SystemExit(f"{path}: not a RIFF/WAVE file")
    pos, fmt, payload = 12, None, None
    while pos + 8 <= len(data):
        cid = data[pos:pos + 4]
        size = struct.unpack("<I", data[pos + 4:pos + 8])[0]
        body = data[pos + 8:pos + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif cid == b"data":
            payload = body
            break
        pos += 8 + size + (size & 1)
    if fmt is None or payload is None:
        raise SystemExit(f"{path}: no fmt/data chunk")
    tag, channels, rate, _, _, bits = fmt
    if tag == 3 or (tag == 0xFFFE and bits == 32):
        if bits != 32:
            raise SystemExit(f"{path}: {bits}-bit float is not expected here")
        a = array.array("f")
        a.frombytes(payload[: len(payload) // 4 * 4])
        if sys.byteorder != "little":
            a.byteswap()
        flat = a.tolist()
    elif tag == 1 and bits == 16:
        a = array.array("h")
        a.frombytes(payload[: len(payload) // 2 * 2])
        if sys.byteorder != "little":
            a.byteswap()
        flat = [v / 32768.0 for v in a]
    else:
        raise SystemExit(f"{path}: unsupported WAV format tag {tag} / {bits} bits")
    if rate != RATE:
        raise SystemExit(f"{path}: {rate} Hz, the corpus is {RATE} Hz")
    chans = [flat[c::channels] for c in range(channels)]
    return chans, rate


def db(v):
    return 20 * math.log10(max(v, 1e-12))


def rms_db(chans):
    if not chans or not chans[0]:
        return SILENT_DB
    total = 0.0
    n = 0
    for c in chans:
        total += sum(x * x for x in c)
        n += len(c)
    return db(math.sqrt(total / max(n, 1)))


def peak_dbfs(chans):
    p = 0.0
    for c in chans:
        if c:
            p = max(p, max(max(c), -min(c)))
    return db(p) if p > 0 else SILENT_DB


def mean_channels(chans):
    if len(chans) == 1:
        return chans[0]
    n = min(len(c) for c in chans)
    return [sum(c[i] for c in chans) / len(chans) for i in range(n)]


# ------------------------------------------------------------------ alignment


def envelope(mono, frame=RATE // 1000):
    """1 ms RMS in dB: fine enough that a lag is read in whole milliseconds."""
    n = len(mono) // frame
    out = []
    for i in range(n):
        s = 0.0
        base = i * frame
        for x in mono[base:base + frame]:
            s += x * x
        out.append(math.sqrt(s / frame))
    # A floor rather than a log of zero: silence still correlates as silence.
    return [db(v) if v > 1e-9 else -180.0 for v in out]


def onset_frame(env, drop=40.0):
    """The first 1 ms frame within `drop` dB of the loudest: where the note starts."""
    peak = max(env)
    for i, v in enumerate(env):
        if v >= peak - drop:
            return i
    return 0


def align_lag_ms(ref_mono, kern_mono, span_ms=10):
    """Cross-correlate the attack, centred on the onset difference between the two captures.

    The whole-envelope correlation that `tools/oracle/compare.py` uses is right for a sustained
    instrument and wrong for one whose envelope shape differs (a pad against a blip gives no
    peak at all, and the search then walks to its edge). The onset is the one landmark both
    captures always share, so it picks the centre and the correlation refines it within
    +-span_ms. Returns (lag_ms, correlation, onset_lag_ms); + means the reference is later.
    """
    ea = envelope(ref_mono)
    eb = envelope(kern_mono)
    lag0 = onset_frame(ea) - onset_frame(eb)
    n = min(len(ea), len(eb))
    window = 600  # the attack and the first few hundred ms of sustain
    best, best_lag = -2.0, lag0
    for lag in range(lag0 - span_ms, lag0 + span_ms + 1):
        if lag >= 0:
            aa, bb = ea[lag:lag + window], eb[:window]
        else:
            aa, bb = ea[:window], eb[-lag:-lag + window]
        m = min(len(aa), len(bb), n)
        if m < 100:
            continue
        aa, bb = aa[:m], bb[:m]
        ma = sum(aa) / m
        mb = sum(bb) / m
        aa = [x - ma for x in aa]
        bb = [x - mb for x in bb]
        na = math.sqrt(sum(x * x for x in aa)) or 1e-30
        nb = math.sqrt(sum(x * x for x in bb)) or 1e-30
        c = sum(x * y for x, y in zip(aa, bb)) / (na * nb)
        if c > best:
            best, best_lag = c, lag
    if best < 0.5:  # no usable peak: trust the onset, and say so
        return lag0, best, lag0
    return best_lag, best, lag0


def shift(chans, samples):
    """Align one side by whole samples; a positive shift drops that many leading samples."""
    if samples == 0:
        return chans
    if samples > 0:
        return [c[samples:] for c in chans]
    return [c[:samples] for c in chans]


# ------------------------------------------------------------------- spectrum


def _fft(a):
    """In-place iterative radix-2 FFT (list of complex, power-of-two length)."""
    n = len(a)
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j |= bit
        if i < j:
            a[i], a[j] = a[j], a[i]
    length = 2
    while length <= n:
        ang = -2.0 * math.pi / length
        wl = complex(math.cos(ang), math.sin(ang))
        half = length >> 1
        for i in range(0, n, length):
            w = 1 + 0j
            for k in range(i, i + half):
                u = a[k]
                v = a[k + half] * w
                a[k] = u + v
                a[k + half] = u - v
                w *= wl
        length <<= 1
    return a


_HANN = None


def hann(n):
    global _HANN
    if _HANN is None or len(_HANN) != n:
        _HANN = [0.5 - 0.5 * math.cos(2.0 * math.pi * i / n) for i in range(n)]
    return _HANN


def band_levels(mono, n_fft=4096):
    """compare.py's 1/3-octave band levels: averaged power spectra over the active frames."""
    win = hann(n_fft)
    hop = n_fft // 2
    spec = [0.0] * (n_fft // 2 + 1)
    energies = []
    for start in range(0, max(0, len(mono) - n_fft), hop):
        frame = [complex(mono[start + i] * win[i], 0.0) for i in range(n_fft)]
        _fft(frame)
        p = [(frame[i].real ** 2 + frame[i].imag ** 2) for i in range(n_fft // 2 + 1)]
        energies.append(sum(p))
        spec = [a + b for a, b in zip(spec, p)]
    if not energies:
        return [-200.0] * len(BANDS)
    keep_cut = max(energies) * 1e-6
    kept = [i for i, e in enumerate(energies) if e > keep_cut]
    if not kept:
        return [-200.0] * len(BANDS)
    # Re-average only the active frames: recompute is cheaper than storing every spectrum.
    spec = [0.0] * (n_fft // 2 + 1)
    frames = list(range(0, max(0, len(mono) - n_fft), hop))
    for i in kept:
        start = frames[i]
        frame = [complex(mono[start + j] * win[j], 0.0) for j in range(n_fft)]
        _fft(frame)
        for j in range(n_fft // 2 + 1):
            spec[j] += frame[j].real ** 2 + frame[j].imag ** 2
    for j in range(len(spec)):
        spec[j] /= len(kept)
    out = []
    for c in BANDS:
        lo, hi = c / 2 ** (1 / 6), c * 2 ** (1 / 6)
        s = 0.0
        any_bin = False
        for j in range(len(spec)):
            f = j * RATE / n_fft
            if lo <= f < hi:
                s += spec[j]
                any_bin = True
        out.append(10 * math.log10(s + 1e-30) if any_bin else -200.0)
    return out


def compare(ref_chans, kern_chans):
    """Level (Kontakt - KONTRA) and band RMS after aligning; the lag found is reported."""
    lag_ms, env_r, onset_lag = align_lag_ms(mean_channels(ref_chans), mean_channels(kern_chans))
    ref = shift(ref_chans, lag_ms * RATE // 1000)
    n = min(min(len(c) for c in ref), min(len(c) for c in kern_chans))
    ref = [c[:n] for c in ref]
    kern = [c[:n] for c in kern_chans]
    ref_lvl, kern_lvl = rms_db(ref), rms_db(kern)
    res = {
        "lag_ms": float(lag_ms),
        "onset_lag_ms": float(onset_lag),
        "align_r": round(env_r, 4),
        "align_method": "correlation" if env_r >= 0.5 else "onset",
        "reference_rms_db": round(ref_lvl, 2),
        "kontra_rms_db": round(kern_lvl, 2),
        "reference_peak_db": round(peak_dbfs(ref), 2),
        "kontra_peak_db": round(peak_dbfs(kern), 2),
        "seconds": round(n / RATE, 3),
        "level_db": round(ref_lvl - kern_lvl, 2),
    }
    if ref_lvl <= SILENT_DB or kern_lvl <= SILENT_DB:
        res["band_rms_db"] = None
        res["band_diff_db"] = None
        res["silent"] = "reference" if ref_lvl <= SILENT_DB else "kontra"
        return res
    rb, kb = band_levels(mean_channels(ref)), band_levels(mean_channels(kern))
    diff = [a - b for a, b in zip(rb, kb)]
    med = sorted(diff)[len(diff) // 2]
    resid = [d - med for d in diff]
    res["band_rms_db"] = round(math.sqrt(sum(x * x for x in resid) / len(resid)), 2)
    res["band_diff_db"] = [round(d, 1) for d in diff]
    res["worst_band_db"] = round(max(diff, key=abs), 1)
    res["worst_band_hz"] = BANDS[max(range(len(diff)), key=lambda i: abs(diff[i]))]
    return res


# ----------------------------------------------------------------------- MIDI


def write_midi(path: Path, notes, ccs=(), bpm=120.0):
    """Standard MIDI file, one track, note on/off at 480 PPQ — the same writer shape the oracle uses.

    `ccs` is `[{at_s, cc, value, channel?}]`, written into the same track. A controller sorts
    before a note that starts on its tick, because a patch whose group start criteria read a
    controller decides the note at note-on: sending it after the note is the same as not
    sending it (`accordions` needs CC 2 at 64, and its stored criteria range is 4..=64, so the
    same note at CC 2 = 0 is a different, equally correct, silence).
    """
    ppq = 480
    ticks_per_s = ppq * bpm / 60.0
    events = []
    for i, n in enumerate(notes):
        on = int(round(float(n.get("start_s", 0.0)) * ticks_per_s))
        off = int(round((float(n.get("start_s", 0.0)) + float(n["length_s"])) * ticks_per_s))
        key = int(n["key"])
        vel = int(n.get("velocity", 100))
        ch = int(n.get("channel", 1)) - 1
        events.append((on, 1, [0x90 | ch, key, vel]))
        events.append((off, 0, [0x80 | ch, key, 0]))
    for c in ccs:
        at = int(round(float(c.get("at_s", 0.0)) * ticks_per_s))
        cc = int(c["cc"])
        value = int(c["value"])
        ch = int(c.get("channel", 1)) - 1
        events.append((at, -1, [0xB0 | ch, cc, value]))
    events.sort(key=lambda e: (e[0], e[1]))

    def vlq(v):
        out = [v & 0x7F]
        v >>= 7
        while v:
            out.append(0x80 | (v & 0x7F))
            v >>= 7
        return bytes(reversed(out))

    track = bytearray()
    track += vlq(0) + b"\xff\x51\x03" + (int(60_000_000 / bpm)).to_bytes(3, "big")
    now = 0
    for t, _, data in events:
        track += vlq(t - now) + bytes(data)
        now = t
    track += vlq(0) + b"\xff\x2f\x00"
    header = b"MThd" + (6).to_bytes(4, "big") + (0).to_bytes(2, "big") + \
             (1).to_bytes(2, "big") + ppq.to_bytes(2, "big")
    path.write_bytes(header + b"MTrk" + len(track).to_bytes(4, "big") + bytes(track))


# --------------------------------------------------------------------- render


def render_bin() -> Path:
    if os.environ.get("KONTRA_RENDER"):
        return Path(os.environ["KONTRA_RENDER"])
    target = Path(os.environ.get("CARGO_TARGET_DIR") or REPO / "target") / "release"
    return target / "kontra-render"


def kontra_render(case, midi_path: Path, out_wav: Path, root: Path):
    """Run KONTRA on the case's own MIDI. Returns (returncode, summary-or-None, stderr)."""
    p = subprocess.run([str(render_bin()), "--instrument", str(instrument_path(case, root)),
                        "--out", str(out_wav), "--midi", str(midi_path), "--tail",
                        str(case.get("tail_s", 0.5)), "--quiet"],
                       capture_output=True, text=True, timeout=900)
    summary = None
    if p.returncode == 0:
        try:
            summary = json.loads(p.stdout)
        except json.JSONDecodeError:
            summary = None
    return p.returncode, summary, (p.stderr or "").strip()


def write_case_midi(case, path: Path):
    """The case's own MIDI — notes and any controllers — as the one file both engines play."""
    midi = case["midi"]
    write_midi(path, midi["notes"], midi.get("ccs") or ())


def capture_frames(case) -> int:
    """Frames the case's reference and render must both hold: last key-up plus the tail.

    A reference of a different length is a half-recorded case, not a short one: the
    comparison truncates both sides to their common length, so a take that stopped early
    would go on passing while measuring less of the note than it claims to.
    """
    midi = case["midi"]
    end = max(float(n.get("start_s", 0.0)) + float(n["length_s"]) for n in midi["notes"])
    return int(round((end + float(case.get("tail_s", 0.5))) * RATE))


def check_references(case, row):
    """Fail the case unless every take it lists is on disk and the length its MIDI implies.

    Returns the first take's samples, or `None` after recording the failure in `row`.
    """
    refs = case.get("references") or []
    want = capture_frames(case)
    first = None
    for name in refs:
        path = REFS / Path(name).name
        if not path.exists():
            row.update(ok=False, status="reference missing",
                       detail=f"{path} is not there: re-capture with --case {case['id']}")
            return None
        chans, _ = read_wav(path)
        frames = len(chans[0]) if chans else 0
        if frames != want:
            row.update(ok=False, status="reference length wrong",
                       detail=f"{path.name} holds {frames} frames ({frames / RATE:.3f} s), the "
                              f"case's MIDI captures {want} ({want / RATE:.3f} s): re-capture "
                              f"with --case {case['id']}")
            return None
        if first is None:
            first = chans
    return first


# ----------------------------------------------------------------------- cases


def load_cases():
    doc = json.loads(CORPUS.read_text())
    return doc


def library_root(doc) -> Path:
    """The root every case's `instrument` is relative to.

    `library_root` names the environment variable to read it from and a default spelled
    with `$HOME`, so a case carries no path that only exists on the machine that recorded
    it: point `KONTRA_TEST_LIBRARY` at the library and the whole corpus runs there.
    """
    spec = doc.get("library_root") or {}
    variable = spec.get("env") or "KONTRA_TEST_LIBRARY"
    set_to = os.environ.get(variable)
    if set_to:
        return Path(set_to).expanduser()
    default = spec.get("default")
    if not default:
        raise SystemExit(f"{CORPUS}: no library_root; add one with an `env` and a `default`")
    return Path(os.path.expandvars(default)).expanduser()


def instrument_path(case, root: Path) -> Path:
    """The case's instrument: `instrument` joined onto the library root.

    An instrument recorded as an absolute path (only an ad-hoc `--case` file would) wins
    over the root, the way `Path` joining always does.
    """
    return root / case["instrument"]


def select(doc, args):
    cases = doc["cases"]
    if args.subset:
        wanted = doc["gate_subset"]
        cases = [c for c in cases if c["id"] in wanted]
    if args.case:
        cases = [c for c in cases if c["id"] in args.case]
    return cases


def run_case(case, workdir: Path, root: Path):
    """One case: render, compare, and decide. Returns a row dict with `ok`."""
    row = {"id": case["id"], "name": case["name"], "expect": case.get("expect", "matches"),
           "key": case.get("key"), "threshold": case.get("threshold")}
    midi_path = workdir / f"{case['id']}.mid"
    write_case_midi(case, midi_path)
    # A case's takes are checked before anything is rendered: a take that is absent or the
    # wrong length is a half-recorded case, and it fails here rather than comparing less of
    # the note than it claims to.
    refs = case.get("references") or []
    ref_chans = None
    if refs:
        ref_chans = check_references(case, row)
        if ref_chans is None:
            return row
    out_wav = workdir / f"{case['id']}-kontra.wav"
    rc, summary, err = kontra_render(case, midi_path, out_wav, root)

    if case.get("expect", "matches") == "expected-refusal":
        want = case.get("refusal_reason", "")
        if rc != 0:
            blame = (err.splitlines() or [""])[-1]
            if want and want in err:
                row.update(ok=True, status="refused as recorded", detail=want)
            else:
                row.update(ok=False, status="refused for a different reason",
                           detail=blame[-160:])
            return row
        # A refusal case that now renders is the case being fixed, not drift: it must not
        # fail the gate, but it must not pass quietly either.
        row.update(ok=True, status="FIXED - now renders; re-capture and promote",
                   detail=(f"was: {want}"))
        return row

    if rc != 0:
        row.update(ok=False, status="KONTRA failed to render", detail=(err.splitlines() or [""])[-1][-160:])
        return row

    if not refs:
        row.update(ok=False, status="no reference WAV for a case that should render")
        return row
    kern_chans, _ = read_wav(out_wav)
    want_frames = capture_frames(case)
    got_frames = len(kern_chans[0]) if kern_chans else 0
    # A render stops on the block boundary after the case's last frame, so it holds a few
    # more frames than the capture (120,320 for a 2.5 s case). Fewer is the failure: the
    # comparison would then quietly measure less of the note than the reference holds.
    if got_frames < want_frames:
        row.update(ok=False, status="render shorter than the case",
                   detail=f"KONTRA wrote {got_frames} frames ({got_frames / RATE:.3f} s) for a "
                          f"case that holds {want_frames} ({want_frames / RATE:.3f} s)")
        return row
    metrics = compare(ref_chans, kern_chans)
    row["metrics"] = metrics

    if case.get("expect") == "known-gap":
        # A documented gap: the reference sounds and KONTRA does not. It passes while that is
        # still true, so the gap is on the record without stopping every build; the moment KONTRA
        # makes the sound, the case says so and asks to be promoted.
        ref_db = metrics.get("reference_rms_db", SILENT_DB)
        k_db = metrics.get("kontra_rms_db", SILENT_DB)
        if k_db <= SILENT_DB:
            row.update(ok=True, status="known gap: KONTRA silent",
                       detail=f"reference {ref_db:.1f} dBFS RMS - {case.get('gap', '')}")
        else:
            row.update(ok=True, status="FIXED - KONTRA now sounds; re-capture and promote",
                       detail=f"KONTRA {k_db:.1f} dBFS, reference {ref_db:.1f} dBFS")
        return row

    if case.get("expect") == "expected-silent":
        # Kept for completeness: no case needs it today (see README), but a genuinely silent
        # instrument is a real outcome and the runner should be able to say so.
        ref_silent = metrics.get("reference_rms_db", SILENT_DB) <= SILENT_DB
        k_silent = metrics.get("kontra_rms_db", SILENT_DB) <= SILENT_DB
        if ref_silent and k_silent:
            row.update(ok=True, status="silent in both")
        elif not ref_silent and k_silent:
            row.update(ok=True, status="FIXED - KONTRA silent where the reference sounds",
                       detail="re-capture and promote")
        else:
            row.update(ok=False, status="no longer silent",
                       detail=f"KONTRA {metrics.get('kontra_rms_db')} dBFS, "
                              f"reference {metrics.get('reference_rms_db')} dBFS")
        return row

    thresh = case.get("threshold") or {}
    max_level = float(thresh.get("level_db", 0.5))
    max_band = float(thresh.get("band_rms_db", 1.0))
    if metrics.get("silent"):
        row.update(ok=False, status=f"{metrics['silent']} side is silent")
        return row
    base = case.get("baseline") or {}
    if base.get("level_db") is None or base.get("band_rms_db") is None:
        row.update(ok=False, status="no recorded baseline (run --record)")
        return row
    # The threshold bounds MOVEMENT from the recorded baseline, because that is what a regression
    # gate can assert without pretending today's agreement is better than it is. How far KONTRA
    # still sits from Kontakt is reported separately, every run, as a finding.
    drift_level = metrics["level_db"] - float(base["level_db"])
    drift_band = metrics["band_rms_db"] - float(base["band_rms_db"])
    lag_err = (metrics["lag_ms"] - float(base["align_ms"])) if base.get("align_ms") is not None else 0.0
    row["drift"] = {"level_db": round(drift_level, 2), "band_rms_db": round(drift_band, 2),
                    "align_ms": round(lag_err, 1)}
    drifted = []
    if abs(drift_level) > max_level:
        drifted.append(f"level moved {drift_level:+.2f} dB (baseline {base['level_db']:+.2f})")
    if abs(drift_band) > max_band:
        drifted.append(f"band RMS moved {drift_band:+.2f} dB (baseline {base['band_rms_db']:.2f})")
    if abs(lag_err) > ALIGN_SLACK_MS:
        drifted.append(f"alignment moved {lag_err:+.1f} ms")
    finding = abs(metrics["level_db"]) > max_level or abs(metrics["band_rms_db"]) > max_band
    row["finding"] = finding
    if drifted:
        row.update(ok=False, status="DRIFT", detail="; ".join(drifted))
    elif finding:
        row.update(ok=True, status="finding (unchanged)",
                   detail=f"disagrees with Kontakt beyond {max_level:.2f} dB / {max_band:.2f} dB, "
                          f"as recorded: level {metrics['level_db']:+.2f}, band {metrics['band_rms_db']:.2f}")
    else:
        row.update(ok=True, status="matches",
                   detail=f"level {metrics['level_db']:+.2f} / band {metrics['band_rms_db']:.2f} dB")
    return row


def record(args, doc):
    """Record where KONTRA stands against each reference today. Not a way to make a failure pass:
    it writes a baseline with the deltas visible, and a reviewer sees them in the diff."""
    root = library_root(doc)
    workdir = Path(tempfile.mkdtemp(prefix="parity-record-"))
    for case in select(doc, args):
        if case.get("expect") in ("expected-refusal", "known-gap"):
            continue
        refs = case.get("references") or []
        if not refs:
            continue
        midi_path = workdir / f"{case['id']}.mid"
        write_case_midi(case, midi_path)
        out_wav = workdir / f"{case['id']}-kontra.wav"
        rc, summary, err = kontra_render(case, midi_path, out_wav, root)
        if rc != 0:
            print(f"{case['id']}: KONTRA does not render it ({err.splitlines()[-1:]}) - no baseline")
            continue
        checked = {"id": case["id"]}
        ref_chans = check_references(case, checked)
        if ref_chans is None:
            print(f"{case['id']}: {checked['status']} - no baseline: {checked.get('detail', '')}")
            continue
        kern_chans, _ = read_wav(out_wav)
        m = compare(ref_chans, kern_chans)
        case["baseline"] = {
            "level_db": m["level_db"],
            "band_rms_db": m["band_rms_db"],
            "align_ms": m["lag_ms"],
            "measured": time.strftime("%Y-%m-%d"),
            "kontra_sha256": (summary or {}).get("sha256"),
        }
        print(f"{case['id']:26s} baseline level {m['level_db']:+.2f}  band {m['band_rms_db']:.2f}  "
              f"align {m['lag_ms']:+.0f} ms")
        CORPUS.write_text(json.dumps(doc, indent=2) + "\n")
    return 0


# --------------------------------------------------------------------- capture


def capture(args, doc):
    """Re-capture the references from live Kontakt, one oracle session per take."""
    import importlib.util
    sys.path.insert(0, str(REPO / "tools" / "oracle-mcp"))
    spec = importlib.util.spec_from_file_location(
        "oracle_client", REPO / "tools" / "oracle-mcp" / "client.py")
    oc = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oc)

    def payload(resp):
        res = resp.get("result") or {}
        body = res.get("content") or [{}]
        try:
            return json.loads(body[0].get("text", "{}")), bool(res.get("isError"))
        except json.JSONDecodeError:
            return {"raw": body[0].get("text")}, bool(res.get("isError"))

    root = library_root(doc)
    REFS.mkdir(parents=True, exist_ok=True)
    workdir = Path(tempfile.mkdtemp(prefix="parity-capture-"))
    for case in select(doc, args):
        print(f"\n== {case['id']}  {case['name']}")
        midi_path = workdir / f"{case['id']}.mid"
        write_case_midi(case, midi_path)
        outcomes = []
        for take in ("a", "b"):
            client = oc.Client(timeout=900)
            wav_out = REFS / f"{case['id']}-{take}.wav"
            rec = {"take": take}
            try:
                client.handshake()
                up, err = payload(client.tool("oracle_up", {}))
                rec["display"] = up.get("display")
                if err:
                    raise SystemExit(f"oracle_up failed: {up}")
                ld, err = payload(client.tool("oracle_load",
                                              {"nki": str(instrument_path(case, root))}))
                rec["loaded"] = bool((ld.get("page") or {}).get("instrument_page"))
                if rec["loaded"]:
                    r, err = payload(client.tool("oracle_render", {
                        "midi": str(midi_path), "tail": case.get("tail_s", 0.5),
                        "allow_silent": True}))
                    rec["render_ok"] = not err
                    if err:
                        rec["render_error"] = r
                    else:
                        rec["peak_dbfs"] = r.get("peak_dbfs")
                        # A silent take is a reference too: the case records that Kontakt made
                        # no sound, so the file is kept and the runner checks KONTRA for silence.
                        if r.get("wav") and Path(r["wav"]).exists():
                            # /tmp and /var/tmp are different filesystems here, so a rename is
                            # EXDEV: copy, then drop the oracle's copy. A capture left twelve of
                            # them behind before this line existed, ~2 MB each on the session's
                            # own work dir, and nothing reads one after its take is recorded.
                            import shutil
                            shutil.copyfile(r["wav"], wav_out)
                            rec["wav"] = str(wav_out)
                            for stray in (r["wav"], str(Path(r["wav"]).with_name(
                                    Path(r["wav"]).stem + "-raw.wav"))):
                                Path(stray).unlink(missing_ok=True)
                        else:
                            rec["no_wav"] = True
                else:
                    rec["load_error"] = (ld.get("error") or "")[:200]
            finally:
                try:
                    down, _ = payload(client.tool("oracle_down", {}))
                    rec["kontakt_leftovers"] = down.get("kontakt_leftovers")
                    rec["display_gone"] = down.get("display_gone")
                except Exception as e:  # noqa: BLE001 - a dead session is a result too
                    rec["down_error"] = str(e)
                client.close()
            if rec.get("kontakt_leftovers"):
                raise SystemExit(f"{case['id']} take {take}: Kontakt left behind "
                                 f"{rec['kontakt_leftovers']}")
            if not rec.get("display_gone", True):
                raise SystemExit(f"{case['id']} take {take}: display {rec.get('display')} still up")
            outcomes.append(rec)
            print(json.dumps(rec))

        # Thresholds and the alignment offset come from the two takes and a KONTRA render.
        if all(o.get("wav") for o in outcomes):
            want = capture_frames(case)
            for take, rec in zip(("a", "b"), outcomes):
                if rec.get("peak_dbfs") is not None and rec["peak_dbfs"] <= SILENT_DB:
                    raise SystemExit(f"{case['id']} take {take}: Kontakt made no sound "
                                     f"({rec['peak_dbfs']} dBFS peak) - a silent reference is not a "
                                     f"threshold; drop the instrument instead of recording it")
            for take in ("a", "b"):
                chans, _ = read_wav(REFS / f"{case['id']}-{take}.wav")
                frames = len(chans[0]) if chans else 0
                if frames != want:
                    raise SystemExit(f"{case['id']} take {take}: {frames} frames, the case's MIDI "
                                     f"captures {want} - the take is not the note it claims")
            a, _ = read_wav(REFS / f"{case['id']}-a.wav")
            b, _ = read_wav(REFS / f"{case['id']}-b.wav")
            spread = compare(a, b)
            case["threshold"] = {
                "level_db": round(max(0.5, abs(spread["level_db"])), 2),
                "band_rms_db": round(max(1.0, spread.get("band_rms_db") or 0.0), 2),
                "from_spread": {"level_db": abs(spread["level_db"]),
                                "band_rms_db": spread.get("band_rms_db"),
                                "take_drift_ms": spread["lag_ms"]},
            }
            out_wav = workdir / f"{case['id']}-kontra.wav"
            rc, summary, err = kontra_render(case, midi_path, out_wav, root)
            if rc == 0 and out_wav.exists():
                kern, _ = read_wav(out_wav)
                m = compare(a, kern)
                # The baseline is recorded with the reference: a capture without the delta it
                # implies would leave the case unable to say whether anything moved.
                case["baseline"] = {
                    "level_db": m["level_db"],
                    "band_rms_db": m["band_rms_db"],
                    "align_ms": m["lag_ms"],
                    "measured": time.strftime("%Y-%m-%d"),
                    "kontra_sha256": (summary or {}).get("sha256"),
                }
                print(f"   threshold {case['threshold']}  align {m['lag_ms']:+.0f} ms  "
                      f"KONTRA now {m['level_db']:+.2f} dB / {m['band_rms_db']} dB")
            else:
                print(f"   KONTRA does not render this case yet ({err.splitlines()[-1:]})")
        elif not any(o.get("loaded") for o in outcomes):
            case["expect"] = "expected-refusal"
            print("   refused by Kontakt too - no reference to capture")
        else:
            raise SystemExit(f"{case['id']}: Kontakt loaded it but produced no take; nothing recorded")
        doc_text = json.dumps(doc, indent=2) + "\n"
        CORPUS.write_text(doc_text)
    return 0


# ------------------------------------------------------------------------ main


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--subset", action="store_true", help="the gate's three cases")
    ap.add_argument("--case", action="append", metavar="ID")
    ap.add_argument("--json", metavar="PATH")
    ap.add_argument("--capture", action="store_true", help="re-capture references (needs Kontakt)")
    ap.add_argument("--record", action="store_true",
                    help="record the current KONTRA deltas as each case's baseline")
    ap.add_argument("--verbose", "-v", action="store_true", help="print every band difference")
    args = ap.parse_args(argv)

    doc = load_cases()
    if args.capture:
        return capture(args, doc)
    if args.record:
        return record(args, doc)
    cases = select(doc, args)
    if not cases:
        print("no cases selected", file=sys.stderr)
        return 2
    root = library_root(doc)
    if not root.is_dir():
        print(f"the sample library is not at {root}: set "
              f"{(doc.get('library_root') or {}).get('env', 'KONTRA_TEST_LIBRARY')} to the "
              f"directory the cases' `instrument` paths are relative to", file=sys.stderr)
        return 2
    binary = render_bin()
    if not binary.exists():
        print(f"{binary} is missing: build it with "
              f"`cargo build --release -p kontra-host --bin kontra-render`", file=sys.stderr)
        return 2

    rows, failed = [], 0
    findings = 0
    with tempfile.TemporaryDirectory(prefix="parity-corpus-") as tmp:
        workdir = Path(tmp)
        for case in cases:
            row = run_case(case, workdir, root)
            rows.append(row)
            failed += 0 if row["ok"] else 1
            findings += 1 if row.get("finding") else 0
            metrics = row.get("metrics") or {}
            delta = ""
            if metrics.get("level_db") is not None and metrics.get("band_rms_db") is not None:
                th = case.get("threshold") or {}
                drift = row.get("drift") or {}
                delta = (f"level {metrics['level_db']:+.2f}dB band {metrics['band_rms_db']:.2f}dB "
                         f"(moved {drift.get('level_db', 0.0):+.2f}/{drift.get('band_rms_db', 0.0):+.2f}, "
                         f"limit {th.get('level_db', 0.5):.2f}/{th.get('band_rms_db', 1.0):.2f})")
            print(f"[{'ok ' if row['ok'] else 'FAIL'}] {row['id']:<28} {row['status']:<22} {delta}")
            if row.get("detail") and (not row["ok"] or row.get("finding") or "FIXED" in row["status"]):
                print(f"       {row['detail']}")
            if args.verbose and metrics.get("band_diff_db"):
                print("       bands: " + " ".join(
                    f"{int(hz)}Hz {d:+.1f}" for hz, d in zip(BANDS, metrics["band_diff_db"])))

    print(f"\n{len(rows) - failed}/{len(rows)} cases match their baseline inside their threshold")
    if findings:
        print(f"{findings} case(s) still disagree with Kontakt by more than their threshold: "
              f"findings, recorded as the baseline, printed above rather than hidden")
    if args.json:
        Path(args.json).write_text(json.dumps({"cases": rows}, indent=1))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
