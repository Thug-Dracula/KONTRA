#!/usr/bin/env python3
"""Every metric for the slot-liveness captures: absolute per configuration, deltas per pair.

    analyze.py CAPDIR [--out analysis.json]

Each capture is the first render of its own Kontakt session (round robin 0). Both sides of every
comparison are Kontakt captures, so alignment is done here (onset threshold) instead of through
compare.py's kontakt-has-0.5-s-pre / kontra-has-none pairing: 2.0 s from the note onset on both
sides.

Metrics, all defined by tools/oracle/compare.py:
  level  : compare.rms_db over the window (absolute, dBFS)
  peak   : compare.db of the window's absolute maximum
  bands  : compare.band_levels, 1/3-octave 40 Hz..16 kHz
  band RMS (delta): RMS of (band difference - its median), compare.py's `timbre_db`
  wave r : compare.waveform_r over the window
  sweep  : mean over bands of the std of the band level across 100 ms windows (a chorus sweeps a
           comb, which a whole-note band average cannot see)
"""
import argparse
import itertools
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tools/oracle"))
import numpy as np  # noqa: E402
import compare  # noqa: E402

WINDOW_S = 2.0
ONSET_DB = -60.0


def onset(x: np.ndarray) -> int:
    env = np.abs(x).max(axis=0)
    thr = 10 ** (ONSET_DB / 20)
    hits = np.nonzero(env > thr)[0]
    return int(hits[0]) if len(hits) else 0


def measure(path: Path) -> dict:
    x = compare.load(path)
    o = onset(x)
    n = int(WINDOW_S * compare.RATE)
    w = x[:, o:o + n]
    bands = compare.band_levels(w)
    # 100 ms sweep variance per band
    seg = int(0.1 * compare.RATE)
    cells = np.stack([compare.band_levels(w[:, i:i + seg])
                      for i in range(0, w.shape[1] - seg, seg)])
    sweep = float(np.mean(cells.std(axis=0)))
    mono = w.mean(axis=0)
    return {
        "wav": str(path),
        "onset_sample": o,
        "level_db": float(compare.rms_db(w)),
        "peak_db": float(compare.db(np.abs(w).max())),
        "band_db": [float(v) for v in bands],
        "sweep_db": sweep,
        "mono": mono,
        "mono_norm": float(np.sqrt(np.dot(mono, mono))),
    }


def best_r(a: dict, b: dict, span=240) -> tuple:
    """Best normalised correlation of the onset-aligned mono sums within +-span samples (FFT).

    Same quantity as compare.waveform_r (normalised correlation at the best lag, + = a later),
    but the whole lag range comes from one cross-correlation instead of 241 dot products per
    pair - the pair matrix is 540 comparisons.
    """
    n = max(len(a["mono"]), len(b["mono"]))
    x = np.zeros(n)
    y = np.zeros(n)
    x[:len(a["mono"])] = a["mono"]
    y[:len(b["mono"])] = b["mono"]
    nfft = 1 << int(np.ceil(np.log2(2 * n)))
    fx, fy = np.fft.rfft(x, nfft), np.fft.rfft(y, nfft)
    corr = np.fft.irfft(fx * np.conj(fy), nfft)
    lags = np.concatenate([np.arange(0, span + 1), np.arange(-span, 0)])
    vals = np.concatenate([corr[:span + 1], corr[-span:]])
    denom = a["mono_norm"] * b["mono_norm"]
    best_lag, best = 0, -2.0
    for lag, v in zip(lags, vals):
        r = float(v / denom) if denom > 0 else 0.0
        if r > best:
            best, best_lag = r, int(lag)
    return best_lag, best


def pair(a: dict, b: dict) -> dict:
    kb, rb = np.asarray(a["band_db"]), np.asarray(b["band_db"])
    resid = (kb - rb) - np.median(kb - rb)
    lag, r = best_r(a, b)
    return {"level_db": a["level_db"] - b["level_db"],
            "peak_db": a["peak_db"] - b["peak_db"],
            "band_rms_db": float(np.sqrt(np.mean(resid ** 2))),
            "band_max_db": float(resid[np.argmax(np.abs(resid))]),
            "band_at_db": float(compare.BANDS[int(np.argmax(np.abs(resid)))]),
            "wave_r": float(r),
            "lag_ms": float(lag * 1000.0 / compare.RATE)}


def summarise(vals):
    v = sorted(vals)
    return {"median": float(np.median(v)), "min": v[0], "max": v[-1], "n": len(v)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("capdir")
    ap.add_argument("--out", default=None)
    ap.add_argument("--order", default="orig,control,sup2,chor6,diff4,both46")
    args = ap.parse_args()
    cap = Path(args.capdir)
    order = args.order.split(",")

    measured = {}
    for name in order:
        wavs = sorted((cap / name).glob("clean-*.wav"))
        if wavs:
            measured[name] = [measure(w) for w in wavs]
    names = [n for n in order if n in measured]

    out = {"window_s": WINDOW_S, "configs": {}}
    for name in names:
        ms = measured[name]
        out["configs"][name] = {
            "n": len(ms),
            "level_db": summarise([m["level_db"] for m in ms]),
            "peak_db": summarise([m["peak_db"] for m in ms]),
            "sweep_db": summarise([m["sweep_db"] for m in ms]),
            "captures": [m["wav"] for m in ms],
        }
        # within-configuration floor: every pair of its own captures
        wl = [pair(a, b)["level_db"] for a, b in itertools.combinations(ms, 2)]
        wb = [pair(a, b)["band_rms_db"] for a, b in itertools.combinations(ms, 2)]
        wr = [pair(a, b)["wave_r"] for a, b in itertools.combinations(ms, 2)]
        out["configs"][name]["within"] = {
            "level_db": summarise(wl), "band_rms_db": summarise(wb), "wave_r": summarise(wr)}

    # cross-configuration: every capture of A against every capture of B
    out["pairs"] = {}
    for a, b in itertools.combinations(names, 2):
        rows = [pair(x, y) for x in measured[a] for y in measured[b]]
        out["pairs"][f"{a}-{b}"] = {
            k: summarise([r[k] for r in rows])
            for k in ("level_db", "peak_db", "band_rms_db", "band_max_db", "band_at_db",
                      "wave_r", "lag_ms")}
        out["pairs"][f"{a}-{b}"]["median_band_at"] = float(np.median([r["band_at_db"] for r in rows]))

    text = json.dumps(out, indent=1)
    if args.out:
        Path(args.out).write_text(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
