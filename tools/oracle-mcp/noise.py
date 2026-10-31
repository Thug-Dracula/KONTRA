#!/usr/bin/env python3
"""How much a Kontakt render varies from itself: the noise floor parity thresholds sit above.

    tools/oracle-mcp/noise.py [--nki PATH] [--key 60] [--renders 3] [--out DIR]
                              [--analyse DIR] [--json FILE]

Drives real Kontakt twice (two `oracle_up` sessions) and renders the same MIDI
several times in each, then compares the renders with `parity.py`'s own metrics:

  same session   several renders inside one session (round robin, disk, capture timing)
  across session the same note in the other session (a fresh launch, fresh capture)

Reports, per metric, the RMS and the worst value over every pair; those are the
numbers `parity.py`'s limits are set above. `--analyse` re-runs the comparison over
an existing directory tree, without starting Kontakt.
"""
from __future__ import annotations

import argparse
import importlib.util
import itertools
import json
import shutil
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

_spec = importlib.util.spec_from_file_location("parity", HERE / "parity.py")
parity = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(parity)  # `__name__` is "parity", so its main() does not run

LOG = None


def log(message: str) -> None:
    """One line to the log file (flushed) and stderr, so a live run can be watched."""
    line = f"{time.strftime('%H:%M:%S')} {message}"
    print(line, file=sys.stderr, flush=True)
    if LOG is not None:
        LOG.write(line + "\n")
        LOG.flush()


def render_session(nki: Path, midi: Path, out: Path, renders: int, label: str) -> list[Path]:
    """One `oracle_up` … `oracle_down`, with `renders` renders of the same note in it."""
    out.mkdir(parents=True, exist_ok=True)
    client = parity.client_mod.Client()
    wavs = []
    try:
        client.handshake()
        up = parity.fetch_tool(client, "oracle_up", {})
        log(f"{label}: up on {up.get('display')} in {up.get('launch_s')}s")
        parity.fetch_tool(client, "oracle_load", {"nki": str(nki)})
        for n in range(renders):
            rendered = parity.render_kontakt(client, midi)
            dest = out / f"{n}.wav"
            shutil.copyfile(rendered["wav"], dest)
            log(f"{label} render {n}: {rendered['frames']} frames, "
                f"peak {rendered['peak_dbfs']} dBFS, onset {rendered['onset_s']} s")
            wavs.append(dest)
    finally:
        try:
            parity.fetch_tool(client, "oracle_down", {})
        except Exception as e:  # noqa: BLE001 - a failed teardown must still be reported
            log(f"{label}: oracle_down failed: {e}")
        client.close()
    return wavs


def pair_metrics(a: Path, b: Path, window: float) -> dict:
    """Both sides are captures with a 0.5 s lead-in, so neither side is trimmed."""
    cap_a = parity.load_as_capture(a, a.with_suffix(".capture-a.wav"))
    cap_b = parity.load_as_capture(b, b.with_suffix(".capture-b.wav"))
    return parity.compare_renders(cap_a, cap_b, window, pre_a=parity.compare.PRE,
                                  pre_b=parity.compare.PRE)


def analyse(root: Path, window: float) -> dict:
    """Every same-session and cross-session pair under `root`."""
    sessions = sorted(p for p in root.iterdir() if p.is_dir())
    by_session = {}
    for s in sessions:
        for wav in sorted(s.glob("*.wav")):
            if ".capture-" in wav.name:
                continue
            by_session.setdefault(s.name, []).append(wav)
    rows_same, rows_cross = [], []
    for name, wavs in sorted(by_session.items()):
        for a, b in itertools.combinations(wavs, 2):
            row = pair_metrics(a, b, window)
            rows_same.append({"kind": "same", "session": name, "a": a.name, "b": b.name,
                              **{k: row.get(k) for k in
                                 ("level_db", "band_rms_db", "worst_band_db", "lag_ms",
                                  "wave_r", "silent")}})
            log(f"same {name} {a.name} vs {b.name}: "
                f"level {row.get('level_db', float('nan')):+.2f} dB, "
                f"band rms {row.get('band_rms_db', float('nan')):.2f} dB, "
                f"worst {row.get('worst_band_db', float('nan')):+.2f} dB, "
                f"lag {row.get('lag_ms', float('nan')):+.1f} ms")
    for (s1, w1), (s2, w2) in itertools.combinations(sorted(by_session.items()), 2):
        for a in w1:
            for b in w2:
                if b.name != a.name:
                    continue
                row = pair_metrics(a, b, window)
                rows_cross.append({"kind": "cross", "session": f"{s1}|{s2}", "a": a.name,
                                   "b": b.name,
                                   **{k: row.get(k) for k in
                                      ("level_db", "band_rms_db", "worst_band_db", "lag_ms",
                                       "wave_r", "silent")}})
                log(f"cross {s1} {a.name} vs {s2} {b.name}: "
                    f"level {row.get('level_db', float('nan')):+.2f} dB, "
                    f"band rms {row.get('band_rms_db', float('nan')):.2f} dB, "
                    f"worst {row.get('worst_band_db', float('nan')):+.2f} dB, "
                    f"lag {row.get('lag_ms', float('nan')):.1f} ms")
    summary = {}
    for kind, rows in (("same_session", rows_same), ("across_sessions", rows_cross)):
        ok = [r for r in rows if not r.get("silent")]
        summary[kind] = {"pairs": len(rows), "silent": len(rows) - len(ok)}
        for key in ("level_db", "band_rms_db", "worst_band_db", "lag_ms"):
            values = [abs(r[key]) for r in ok if r.get(key) is not None]
            if not values:
                continue
            summary[kind][key] = {
                "rms": round(float(np.sqrt(np.mean(np.square(values)))), 3),
                "max": round(float(max(values)), 3),
            }
        if ok:
            summary[kind]["wave_r_min"] = round(float(min(r["wave_r"] for r in ok)), 4)
    return {"pairs": rows_same + rows_cross, "summary": summary}


def main() -> int:
    global LOG
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--nki", default=parity.DEFAULT_NKI)
    ap.add_argument("--key", type=int, default=60)
    ap.add_argument("--velocity", type=int, default=100)
    ap.add_argument("--hold", type=float, default=1.5)
    ap.add_argument("--window", type=float, default=2.0)
    ap.add_argument("--renders", type=int, default=3, help="renders per session")
    ap.add_argument("--out", default="/tmp/opencode/parity/noise")
    ap.add_argument("--analyse", help="compare the sessions already under this directory")
    ap.add_argument("--json")
    args = ap.parse_args()

    root = Path(args.analyse or args.out)
    root.mkdir(parents=True, exist_ok=True)
    LOG = open(root / "noise.log", "a", encoding="utf-8")
    log(f"noise: {root} (log {root / 'noise.log'})")
    if not args.analyse:
        nki = Path(args.nki).expanduser().resolve()
        midi = root / "noise.mid"
        server = parity.load_mod("oracle_mcp_server", HERE / "server.py")
        server.write_midi(midi, [{"start_s": 0.0, "length_s": args.hold, "key": args.key,
                                  "velocity": args.velocity}])
        for session in ("session1", "session2"):
            if (root / session).exists():
                shutil.rmtree(root / session)
            render_session(nki, midi, root / session, args.renders, session)
    result = analyse(root, args.window)
    text = json.dumps(result["summary"], indent=2)
    print(text)
    print(f"pairs: {len(result['pairs'])} (details in {root / 'pairs.json'})")
    (root / "pairs.json").write_text(json.dumps(result, indent=1, default=float) + "\n")
    if args.json:
        Path(args.json).write_text(json.dumps(result["summary"], indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
