#!/usr/bin/env python3
"""Hot Solo's measured tone stage: the curve, and the band fit `ir::Amp` carries.

    tools/dsp-research/hot_solo_fit.py

No arguments: the measured curve below is Hot Solo's own response at Electric
Mint's saved record, taken from two Kontakt-versus-KONTRA pairs (see
docs/re/engine/k-hotsolo.md) and reproducible from the captures with
tools/oracle/compare.py's band_levels. Printing the fit is how the four band
constants in crates/format/kontakt/src/hot_solo.rs are audited.

Needs numpy and scipy (research tooling; the engine does not).
"""
import numpy as np
from scipy.optimize import least_squares

RATE = 48_000

# 1/3-octave centres and the measured difference, dB: Kontakt with slot 3 active
# against Kontakt with slot 3's stored bypass flag set, twelve fresh sessions,
# first render of each (`tools/oracle-mcp/slot-liveness/`, 2026-10-31), taken as the
# median of the six per configuration because one `control` capture is a 2.4 dB
# band-RMS outlier against five captures all inside 0.34. The 40 Hz band is not in
# the list: neither side has energy there, so it measures nothing and used to pin
# the low shelf against a point that was 0.00 by silence, not by response.
MEASURED = [
    (50, 3.01), (63, 4.97), (80, 4.34), (101, 5.17), (127, 6.07),
    (160, 6.49), (202, 6.81), (254, 7.25), (320, 8.12), (403, 8.46),
    (508, 9.05), (640, 11.00), (806, 12.74), (1016, 14.38), (1280, 15.79),
    (1613, 18.78), (2032, 19.65), (2560, 20.39), (3225, 21.79), (4064, 21.97),
    (5120, 22.16), (6451, 21.15), (8127, 19.83), (10240, 18.02), (12902, 14.80),
]


def biquad_db(f, kind, f0, q, gain_db):
    """RBJ's cookbook band sampled at `f`; a shelf's `q` is its slope as the
    engine's `q` is (`sin(w)/2·√2` at slope 1)."""
    w0 = 2 * np.pi * f0 / RATE
    a = 10 ** (gain_db / 40)
    alpha = np.sin(w0) / (2 * q)
    z = np.exp(-1j * 2 * np.pi * f / RATE)
    if kind == "peak":
        b = [1 + alpha * a, -2 * np.cos(w0), 1 - alpha * a]
        d = [1 + alpha / a, -2 * np.cos(w0), 1 - alpha / a]
    else:
        s = 2 * np.sqrt(a) * alpha
        if kind == "highshelf":
            b = [a * ((a + 1) + (a - 1) * np.cos(w0) + s), -2 * a * ((a - 1) + (a + 1) * np.cos(w0)),
                 a * ((a + 1) + (a - 1) * np.cos(w0) - s)]
            d = [(a + 1) - (a - 1) * np.cos(w0) + s, 2 * ((a - 1) - (a + 1) * np.cos(w0)),
                 (a + 1) - (a - 1) * np.cos(w0) - s]
        else:
            b = [a * ((a + 1) - (a - 1) * np.cos(w0) + s), 2 * a * ((a - 1) - (a + 1) * np.cos(w0)),
                 a * ((a + 1) - (a - 1) * np.cos(w0) - s)]
            d = [(a + 1) + (a - 1) * np.cos(w0) + s, -2 * ((a - 1) + (a + 1) * np.cos(w0)),
                 (a + 1) + (a - 1) * np.cos(w0) - s]
    return 20 * np.log10(np.abs((b[0] + b[1] * z + b[2] * z**2) / (d[0] + d[1] * z + d[2] * z**2)))


def model(p, f):
    """Low shelf (40 Hz fixed: the fit sits on its bound), a broad peak, a high
    shelf, and a constant gain — the four numbers hot_solo.rs carries."""
    f_ls, g_ls, f_pk, q_pk, g_pk, f_hs, g_hs, g_out = p
    return (biquad_db(f, "lowshelf", f_ls, 0.707, g_ls) + biquad_db(f, "peak", f_pk, q_pk, g_pk)
            + biquad_db(f, "highshelf", f_hs, 0.707, g_hs) + g_out)


def main() -> int:
    centres = np.array([hz for hz, _ in MEASURED], dtype=float)
    target = np.array([db for _, db in MEASURED])
    # Bounds hold the module's own topology: one low shelf, one upper-mid peak,
    # one treble shelf. Left open, the surface has a second basin of the same
    # cost with the "low shelf" up at 337 Hz, which is no longer a low shelf;
    # inside these bounds the shape parameters are still loose (several sets fit
    # within the same 0.4 dB), but the stage gain is stable at +5.5 dB and the
    # constants in hot_solo.rs are what this script prints.
    lo = [20, -12, 1500, 0.2, 8, 2500, 0, 0]
    hi = [150, 8, 5000, 1.2, 20, 9000, 14, 10]
    best = None
    for trial in range(120):
        rng = np.random.default_rng(trial)
        start = [rng.uniform(a, b) for a, b in zip(lo, hi)]
        res = least_squares(lambda p: model(p, centres) - target, start, bounds=(lo, hi),
                            max_nfev=6000)
        best = res if best is None or res.cost < best.cost else best
    p = best.x
    names = ["low shelf hz", "low shelf dB", "peak hz", "peak Q", "peak dB", "high shelf hz",
             "high shelf dB", "gain dB"]
    for name, value in zip(names, p):
        print(f"  {name:14} {value:9.3f}")
    print(f"  {'gain linear':14} {10 ** (p[7] / 20):9.5f}")
    err = model(p, centres) - target
    print(f"residual: RMS {np.sqrt(np.mean(err ** 2)):.2f} dB, worst {np.abs(err).max():.2f} dB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
