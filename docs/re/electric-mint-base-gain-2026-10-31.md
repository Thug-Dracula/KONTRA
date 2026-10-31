# Electric Mint: the third error is Hot Solo's measured gain (2026-10-31)

Follows `docs/re/electric-mint-stage-gains-2026-10-31.md`, whose §"The cancellation is gone" left
one item: with Choral, Diffusion and Supercharger matched, the chain still read **−3.72 dB** and the
per-band residual was flat, so the rest of the chain — slots 1, 3, 7 and the master/output path —
was adding about 3.4 dB that KONTRA did not. This record prices each of those by bypassing it
**inside Kontakt**, finds the 3.9 dB in slot 3, fixes it, and re-runs parity.

## Answer

**Slot 3 (Hot Solo) is the third error, and it is 3.92 dB.** Kontakt's Hot Solo adds **+7.00 dB**
of level at Electric Mint's saved record; KONTRA's added **+3.08**. Slots 0, 1 and 7 and the
instrument output path were already right: bypassing an empty slot moves nothing (floor), the Eq is
0.8 dB off in the other direction, the Convolution matches to 0.00 dB and the instrument's own
output volume is applied to 0.01 dB. With Hot Solo corrected the chain reads **−0.15 dB** and
**0.88 dB band RMS**, and Hot Solo's own contribution is within **0.04 dB** of Kontakt's.

| slot / path | Kontakt's contribution (dB) | KONTRA's, before | KONTRA's, now |
|---|---:|---:|---:|
| 0 — empty | **−0.005** (floor) | 0.000 | 0.000 |
| 1 — Eq | **+2.787** | +3.605 | +3.605 |
| 2 — Supercharger | −2.228 | −2.453 | −2.453 *(already fixed)* |
| 3 — Hot Solo | **+7.002** | +3.079 | **+6.963** |
| 4 — Diffusion | +0.039 | +0.019 | +0.019 *(already fixed)* |
| 5 — empty | **−0.007** (floor) | 0.000 | 0.000 |
| 6 — Choral | −1.331 | −1.255 | −1.255 *(already fixed)* |
| 7 — Convolution | **+0.160** | +0.159 | +0.159 |
| 1+3+7 together | **+10.795** | +7.736 | **+11.620** |
| instrument output volume, +3.03 → 0 dB | **−3.081** | −3.043 | −3.043 |

Positive = the stage raises the level, i.e. that much comes out when it is switched off.

## 1. The protocol

Same as `electric-mint-slot-liveness-2026-10-31.md`: `probe_base_gain.lua` writes
`$ENGINE_PAR_EFFECT_BYPASS` into one instrument-insert slot at a time through `on init` in KSP
script slot 3, saves the instrument, and Kontakt's own engine honours the stored flag on load.
`capture_variants.py` then renders **six fresh sessions per configuration, first render only**
(round robin 0), and screens every attempt for the capture path's HF burst. Addresses 0–7 were
validated by `probe_addr.lua` in the previous lane; slots 0 and 5 are empty, so they are the
negative control — their saved files come back **byte-identical to `control`**, and their captures
must read 0.

`kontra-inspect load` on every saved copy confirms the files differ only in the named slot's
`bypass` flag (`s1`, `s3`, `s7`: one flag each; `s137`: three; `s0`, `s5`, `control`: none).

| configuration | Kontakt level dBFS (median of 6) | ours, before | ours, now |
|---|---:|---:|---:|
| `control` (nothing bypassed) | −24.029 | −27.767 | −23.883 |
| `s0` (empty slot 0) | −24.024 | −27.767 | −23.883 |
| `s1` (Eq bypassed) | −26.817 | −31.636 | −27.488 |
| `s3` (Hot Solo bypassed) | −31.031 | −30.846 | −30.846 |
| `s5` (empty slot 5) | −24.022 | −27.767 | −23.883 |
| `s7` (Convolution bypassed) | −24.189 | −27.883 | −24.042 |
| `s137` (1, 3 and 7 bypassed) | −34.825 | −35.503 | −35.503 |

Within-configuration level spread is **0.05–0.16 dB**; the two empty slots read **−0.005** and
**−0.007 dB**, which is that floor and nothing else.

## 2. The capture that was rejected

48 Kontakt renders were kept, 8 were rejected by the HF-burst screen (ledger in
`/var/tmp/base/cap/captures.json`), and **one kept capture turned out to be bad**: `control`
`clean-1` is a **2.44 dB band-RMS** outlier against five `control` captures all inside 0.34 dB, and
no other capture in any configuration exceeds 0.34 dB. Its level is only 0.12 dB off, so the level
screen and the burst screen both passed it; it is caught by comparing each capture's band curve
with its own configuration's median. It is excluded from every number here and every configuration
is summarised by the **median** of its six captures, which is why the tables above are robust to
it. It is also why `parity.py` reports a nonsense 540 ms lag for that one file: its envelope
correlates 0.53 against the render where the other five sit at 0.98.

## 3. The law: Hot Solo's transfer function was measured against the wrong baseline

`crates/format/kontakt/src/hot_solo.rs` carries Hot Solo as a **measured** transfer function
(`ir::Amp`: three bands and a stage gain), because the module's coefficient law (`0x140ad5440`) and
its shaper (`0x140aab1e0`) are not recovered — `docs/re/engine/k-amps-a-hotsolo.md` is explicit
that nothing in this module is a law. What was wrong is *how it was measured*, in three ways that
compound:

1. **The old curve was not a differential.** `k-hotsolo.md` measured "Kontakt's full chain minus
   KONTRA's full chain with slot 3 passed through". That is the missing stage **plus every other
   stage's error**, and at the time slots 2 and 4 were 3.8 dB apart between the two engines. It
   read **1.73 dB low**. This lane measures the stage the same way it measures every other stage:
   Kontakt with the slot active against Kontakt with the slot bypassed, so the difference is the
   stage and nothing else.
2. **The old fit's target contained a band with no signal.** The `MEASURED` list in
   `tools/dsp-research/hot_solo_fit.py` began with `(40, 0.00)`. Neither render has energy in the
   40 Hz band — the value is 0.00 by silence, not by response — and it dragged the low shelf to
   −6.67 dB, which is why the old model was 3.8 dB dark at 50–63 Hz. The list now starts at 50 Hz.
3. **The stage gain had been calibrated to cancel the chain around it.** `k-hotsolo.md` says so in
   as many words: "the biquad fit gave ×1.672 (+4.46 dB), and the engine's own render then read
   2.30 dB loud against the same capture, so the stage gain is ×1.283 (+2.17 dB). That number is
   calibrated on this comparison, which is why it is stated rather than derived." A gain tuned so
   that the *whole chain* matches one capture absorbs whatever the rest of the chain is missing —
   and what the rest of the chain was missing, this lane measures: the Supercharger (2.45 dB) and
   the Diffusion read (1.52 dB) that were fixed on 2026-10-31. (For the record, 1.283 = 1.672 ×
   0.76766, the ramp rig's bypass scale that `k-amps-a-hotsolo.md` says to *divide* out; whether
   that is how the number was reached or a coincidence, it is 4.0 dB below what the module does.)

The fix is the fit of the same four-parameter model to the clean curve, printed by
`tools/dsp-research/hot_solo_fit.py` (bounded to the module's topology — one low shelf, one
upper-mid peak, one treble shelf — because unbounded the surface has a second basin of equal cost):

| | low shelf | peak | treble shelf | stage gain |
|---|---|---|---|---|
| before | 40 Hz, −6.67 dB | 2903 Hz, Q 0.374, +13.69 dB | 4539 Hz, +8.77 dB | 1.283 (+2.17 dB) |
| now | 43.61 Hz, −6.57 dB | 3654 Hz, Q 0.260, +16.10 dB | 5151 Hz, +2.55 dB | **1.886 (+5.51 dB)** |

Residual **0.40 dB RMS, 0.98 dB worst band**. Measured against the capture, Hot Solo now prices
**+6.963 dB** where Kontakt prices **+7.002** — **0.04 dB**, the level floor.

The three shape parameters are the part that is *not* tightly identified: several
parametrisations of this model fit the curve within the same 0.4 dB. The **stage gain is stable at
+5.5 dB** across them, and it is the gain that was wrong.

## 4. Slots 0, 1 and 7, and why nothing changed there

- **0 and 5** are empty and Kontakt will not write a bypass into them: the saved copies decode
  byte-identical to `control` and their captures read −0.005 / −0.007 dB. The addressing is right
  and the pipeline is inert when it should be.
- **1 (Eq)** is 0.82 dB **too loud** in KONTRA (+3.605 against Kontakt's +2.787), and its per-band
  shape differs by up to 1.2 dB (`6.26` vs Kontakt's `5.10` at 127 Hz, `2.09` vs `3.38` at 50 Hz).
  That is a shape difference, not a single gain, and it is **not fixed here** — see §6.
- **7 (Convolution)** matches to **0.00 dB** and per band to ±0.4 dB. Nothing to do.

## 5. The main/output path: the instrument volume is read correctly

Electric Mint has no main-rack slots (`main: [None × 8]`) and no send, so the only gain between its
insert rack and its output is the instrument's own volume. `probe_main.lua` sets
`Kontakt.set_instrument_volume` and saves a copy that differs only in that number; six fresh
sessions per copy, first render only, same protocol as the slots:

| case | stored volume | Kontakt level | KONTRA level | Kontakt's contribution | KONTRA's |
|---|---:|---:|---:|---:|---:|
| `volwrite` (written back unchanged) | +3.03278327 dB | −24.009 | −23.883 | — | — |
| `vol0` | 0 | −27.089 | −26.926 | **−3.081** | **−3.043** |
| `volm6` | −6 | −33.074 | −32.946 | **−9.065** | **−9.063** |

Kontakt reports the stock instrument's output volume as **+3.03278327 dB** (its own 6 dB per
doubling: stored ratio **1.4195**). KONTRA reads the same stored float from the program header and
applies **+3.0432 dB** (20·log10 of the same 1.4195) — 0.010 dB apart, which is the 6 dB-per-doubling
law against 20·log10 at that one value, not an error. Setting the knob away from its stored value
prices it — 3.081 against 3.043 dB, and 9.065 against 9.063 over a 9 dB span — and in all three
cases the chain's residual stays put at −0.13 dB, so the output path contributes nothing to the
gap. Nothing is changed there.


## 6. What is left, and it is a cancellation again

The chain now **passes**: level **−0.10 … −0.16 dB**, band RMS **0.89 … 0.99**, worst band
1.9 … 2.0 dB at 50 Hz, over the five good `control` captures (`parity.py`, limits 0.5 / 1.0 / 3.0 /
10 ms). That is not the same as "every stage is right":

| component | Kontakt − KONTRA (dB) |
|---|---:|
| slot 1 (Eq) | **−0.82** |
| slot 3 (Hot Solo) | +0.04 |
| slot 7 (Convolution) | +0.00 |
| slots 2, 4, 6 | +0.16 |
| everything else (`s137`: voice, level, pan, output volume) | **+0.68** |

The 0.68 dB is what is left of the chain with all three live slots bypassed on both sides — the
voice/loudness path, not an insert. It is **0.68 dB quiet in KONTRA**, the EQ is **0.82 dB loud**,
and the two cancel to −0.15 dB. Same shape as the two earlier "passes" this instrument has already
had, and it is recorded here so it is not mistaken for parity: fixing either one alone moves the
chain to ≈ ±0.7 dB, which still passes the 0.5 dB limit only by luck, and fixing both is the next
measurement. Per band the residual's largest terms are +1.9–2.4 dB at 50–63 Hz and +1.3–1.8 dB at
5–8 kHz, which is the shape of that remaining pair and of the `s137` base, not of Hot Solo.

## 7. Measured, before and after

Per 1/3-octave band, Kontakt − KONTRA on the full chain (`control`), positive = Kontakt louder:

| Hz | 40 | 50 | 63 | 80 | 101 | 127 | 160 | 202 | 254 | 320 | 403 | 508 | 640 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| before | 0.00 | +5.09 | +5.55 | +3.92 | +3.62 | +3.48 | +2.98 | +4.53 | +4.02 | +3.75 | +3.28 | +4.36 | +3.85 |
| after | 0.00 | +2.06 | +2.39 | +0.62 | +0.21 | −0.03 | −0.64 | +0.80 | +0.07 | −0.42 | −1.08 | −0.32 | −1.00 |

| Hz | 806 | 1016 | 1280 | 1613 | 2032 | 2560 | 3225 | 4064 | 5120 | 6451 | 8127 | 10240 | 12902 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| before | +5.33 | +5.09 | +5.47 | +5.53 | +5.03 | +5.02 | +4.38 | +4.79 | +5.05 | +5.00 | +4.63 | +2.89 | +2.91 |
| after | +0.34 | +0.04 | +0.50 | +0.76 | +0.48 | +0.65 | +0.14 | +0.63 | +1.25 | +1.76 | +1.84 | +0.54 | +1.52 |

| | level (Kontakt − KONTRA) | band RMS | verdict |
|---|---:|---:|---|
| before (`fix/stage-gains`, sha `8da4b950…`) | +3.74 dB | 1.20 dB | **FAIL** |
| after | **−0.15 dB** | **0.88 dB** | **PASS** |

Both rows are the same six Kontakt `control` captures against the two KONTRA builds, so the
difference is the change and nothing else.

Fingerprints, full chain at the shipped tree: `kontra-render` sha256
**`585f5f7dbb10c28c…`** (was `8da4b950e6cec1a9…`), and `kontra-inspect play`'s
**`audio FNV-1a: 7fd5a1ae59790ffc`** (was `7f606e026bb07c2e`). These move because this instrument
stores Hot Solo; Haunted Strings 2 (`06cf13eee3f8f54e…`) and Cherry Electric Bass
(`0279ba0fd86927bd…`) are byte-identical before and after, `cargo test` passes for `kontra-format-kontakt`, `kontra-effects` and `kontra-host` (165 tests).

## 8. Reproducing

```
git worktree add -f /tmp/opencode/wt-base -b feat/mint-base-gain feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-base cargo build --release -p kontra-host -p kontra-inspect

# 1. Kontakt writes the bypass into one copy per slot (one session, 49 s)
python3 tools/oracle-mcp/slot-liveness/run_lua.py \
    tools/oracle-mcp/slot-liveness/probe_base_gain.lua \
    "$ELECTRIC_MINT_LIBRARY/Instruments/__basegain/variants.jsonl" \
    --nki "$ELECTRIC_MINT_LIBRARY/Instruments/Electric Mint.nki"

# 2. the main/output path: the instrument volume, which no slot can reach
python3 tools/oracle-mcp/slot-liveness/run_lua.py \
    tools/oracle-mcp/slot-liveness/probe_main.lua \
    "$ELECTRIC_MINT_LIBRARY/Instruments/__basegain/main.jsonl" \
    --nki "$ELECTRIC_MINT_LIBRARY/Instruments/Electric Mint.nki"

# 3. captures: 6 fresh sessions per configuration, first render only, burst-screened
python3 tools/oracle-mcp/slot-liveness/capture_variants.py --out /var/tmp/base/cap \
    --variant-dir "$ELECTRIC_MINT_LIBRARY/Instruments/__basegain" \
    --configs orig,control,s0,s1,s3,s5,s7,s137 --clean 6

# 4. the stage curve, and the four constants the module carries
python3 tools/dsp-research/hot_solo_fit.py

# 5. the verdict
KONTRA_RENDER=/var/tmp/tgt-base/release/kontra-render python3 tools/oracle-mcp/parity.py \
    --kontakt-wav /var/tmp/base/cap/control/clean-3.wav --out /var/tmp/base/parity-final/run3
```
