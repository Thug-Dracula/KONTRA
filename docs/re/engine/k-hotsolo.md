# Kontakt 8 Hot Solo (k-amps-a): measured model

Scope: `BParFXHotSolo` (module id 0x54, stored version 0x10), effect path `Effects/HotSolo`.
Bass Invader shares module id 0x54 at version 0x52 and is a separate record and a separate DSP
class; the reader below throws on any version other than 0x10, so the two never mix.

Status: **modelled at one measured operating point.** The module's linear tone stage is
`ir::EffectKind::Amp` (`crates/format/kontakt/src/hot_solo.rs`), built from a measured transfer
function, not from a reconstruction of Kontakt's coefficients. Electric Mint is the operating
point: its saved record is what the parity render compares, and it is the record the previous
"located, not modelled" note in this file was written about.

## What the code contributed

Read from `$HOME/Armory/VSTs/kontakt-8.13.1-re/Kontakt 8.exe` (FileVersion 8.13.1,
SizeOfImage 0xb254000) with Kuna; addresses are VAs.

| Address | Size | What it does |
| --- | --- | --- |
| `0x140ad5440` | 14548 B | The coefficient recalculation. Two dirty flags gate it: `+0x715c` (cleared, then called) once the ramp at `+0x7200` reaches zero. It reads the knob floats, and writes ~600 products of them with the constant tables at `0x144fc3…` (e.g. `dat_144fc4708`, `dat_144fc3630`) into the coefficient block at `+0x7650` and onward. **Not recovered**: the products are Ghidra-ordered SIMD arithmetic over 300+ constants and it was not turned into a law; nothing in this model comes from it. |
| `0x140ad44f0` | 3167 B | The second recalculation. Its body shifts whole 8×u64 blocks (four 16-byte lanes) from one state region to another and zeroes the old one: this is the recursive network's state shuffle and reset, **not** a coefficient law. Called from the process when `+0x715d` is set and the ramp at `+0x71c8` is zero. |
| `0x140aa2ae0` | 272 B | Slot 23, the block process: chunks of `+0x1bc` = 32 frames; before each chunk it runs the pending recalculations (`+0x715c` → `0x140ad5440`, `+0x715d` → `0x140ad44f0`), steps the per-chunk state (`0x140acb910`, `this+0x7160`), then calls the kernel and the output tail. |
| `0x140aab1e0` | 5362 B | The kernel. Float work on four lanes with double-precision recursion at `+0x7650…`. The piecewise shaper's thresholds (`+0x73e0…`, `+0x73f0…`), slopes (`+0x7400…`) and gain terms (`+0x73a0…`) are **not** resolved. |
| `0x140ac21f0` | 639 B | The output tail. One gain per bus, ramped **per sample** toward its target by `(target − now)·dat_144f38638` with `dat_144f38638` = 1/180 (0x3bb60b61), so a moved gain settles in ~180 samples (3.75 ms at 48 kHz); then `out = gain · in`. This is the only gain staging whose coefficient is read. |
| `0x140d01b50` | — | The record reader (v0x10 only): eight floats at +0x08…+0x24, then the two bytes at +0x28 (overdrive channel) and +0x29 (mono). |

The chain of evidence that this module is the whole of the Electric Mint parity gap is the
orchestrator's stage-by-stage null test: the band difference survives every instrument and bus
insert being removed, so no other slot explains it.

## The measurement

Same note (MIDI 60, velocity 100, 1.5 s) through real Kontakt 8.14.1 and through KONTRA, 1/3-octave
band levels from `tools/oracle/compare.py`.

**Re-measured 2026-10-31 the only way a stage can be measured: by ablating it inside Kontakt.**
`tools/oracle-mcp/slot-liveness/probe_base_gain.lua` sets slot 3's stored bypass flag through
Kontakt's own engine, `capture_variants.py` renders six fresh sessions per configuration (first
render only, round robin 0), and the stage's curve is `control` minus `s3` — Kontakt against
Kontakt, so no other stage's error can be inside it. Full record:
`docs/re/electric-mint-stage-gains-2026-10-31.md` and
`docs/re/electric-mint-base-gain-2026-10-31.md`.

| Hz | 50 | 63 | 80 | 101 | 127 | 160 | 202 | 254 | 320 | 403 | 508 | 640 | 806 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dB | 3.0 | 5.0 | 4.3 | 5.2 | 6.1 | 6.5 | 6.8 | 7.3 | 8.1 | 8.5 | 9.1 | 11.0 | 12.7 |

| Hz | 1016 | 1280 | 1613 | 2032 | 2560 | 3225 | 4064 | 5120 | 6451 | 8127 | 10240 | 12902 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dB | 14.4 | 15.8 | 18.8 | 19.7 | 20.4 | 21.8 | 22.0 | 22.2 | 21.2 | 19.8 | 18.0 | 14.8 |

Shape: a broad lift from 200 Hz, a peak near 5 kHz at +22 dB, a shelf back down by 13 kHz. The
40 Hz band is not listed: neither render has energy there, so it measures 0.00 by silence — the
old list included it and it dragged the low shelf to −6.67 dB, which made the stage 3.8 dB dark at
50–63 Hz.

## The model

`ir::Amp` — a pre-gain, four bands in series, an output gain — built from a least-squares fit of
that curve as RBJ biquads (`tools/dsp-research/hot_solo_fit.py`, scipy, 120 restarts, bounded to
this module's topology — one low shelf, one upper-mid peak, one treble shelf — because unbounded
the surface has a second basin of equal cost; the script prints these four values):

| Band | Type | Hz | Q / slope | dB |
| --- | --- | --- | --- | --- |
| 0 | low shelf | 43.6 | 1.0 | −6.57 |
| 1 | peak | 3654 | 0.260 | +16.10 |
| 2 | high shelf | 5151 | 1.0 | +2.55 |
| 3 | — | — | — | flat |

Stage gain: ×1.886 (+5.51 dB) at the measured record. Fit residual against the measured curve:
**0.40 dB RMS, 0.98 dB worst band**. The three shape parameters are not tightly identified
(several sets fit within the same 0.4 dB); the stage gain is, and the gain is what was wrong.
Kontakt prices this stage at **+7.00 dB** of level where KONTRA now prices **+6.96**.

Gains: the record's prenormal and master are applied as their ratio to the measured point
(`MEASURED_GAIN · prenormal/0.333 · master/0.420`, and 0 when either is 0). Native support is
thin and is stated where it is used: both knobs mute at 0, and master 0.5 → 1.0 measured +5 dB
where a proportional law predicts +6 (that render clipped at full scale).

## Before and after

Whole chain, `parity.py`, six `control` captures (one rejected as a 2.4 dB outlier, see the
2026-10-31 record):

| | level (Kontakt − KONTRA) | band RMS |
| --- | --- | --- |
| before (gain ×1.283) | +3.72 dB | 1.19 dB |
| after (gain ×1.886) | **−0.15 dB** | **0.88 dB** |

The previous pass calibrated the gain so the *whole chain* matched one capture — "the engine's own
render then read 2.30 dB loud against the same capture, so the stage gain is ×1.283". That is a
gain fitted to cancel the rest of the chain, and what it was cancelling was two stages that were
also wrong at the time (the Supercharger, 2.45 dB, and Replika's Diffusion read, 1.52 dB). Both are
fixed, and the stage is now priced from its own ablation instead.

## What is still not modelled

1. **The coefficient law (`0x140ad5440`).** The bands above are a transfer function, not the
   module's own filter topology. If the law were read, the bands would become functions of the
   five tone knobs instead of constants.
2. **The five tone knobs' laws (presence, depth, treble, mid, bass).** Only the measured
   combination is modelled; a record that differs by more than 0.001 on any of them is
   reported (`Hot Solo tone knobs not modelled (…)`) with impact Audible, so no instrument
   silently gets Electric Mint's tone. Reading them needs a `.nki` knob sweep: patch the record
   (the reader's offsets are above), render each, band-analyse. The patching and rendering
   method is in this unit's work, but the Kontakt GUI could not be driven reliably enough on this
   machine to complete it (`oracle_load`'s Ctrl+O is intermittent, and a second session held the
   shared Xvfb and null sink for most of the window).
3. **The shaper and the overdrive channel.** `0x140aab1e0`'s piecewise thresholds are unread, and
   the overdrive channel (byte +0x28) is a different voicing with inverted polarity in the native
   render. Electric Mint stores overdrive 0, so its pre-overdrive knob is inactive (measured
   2026-10-21: pre-overdrive 0, 0.5 and 1 give identical output), and the modelled path is the
   non-overdrive one only. A record with overdrive set is reported.
4. **Mono (byte +0x29)** changes which output carries the processed signal in the native render;
   not modelled, and reported.
5. **The kernel's recursion.** Whether the tone path is exactly linear is assumed, not proved:
   the fit is level-independent evidence only in that a 1/3-octave difference taken at one
   velocity is reproduced to 0.8 dB. A two-velocity render through the same stage would settle it.

## Evidence

- Binary: `Armory/VSTs/kontakt-8.13.1-re/Kontakt 8.exe`. Decompile: Kuna
  `decompile-all --addr …` of the eight addresses above (kept at `/var/tmp/hotsolo-re/funcs.c`);
  constant `dat_144f38638` read from `.rdata` at file offset 0x4f36838.
- Measured pairs: `/tmp/opencode/oracle-mcp/parity/kontakt-as-capture.wav` + `kontra.wav`,
  `.../parity-hf-before/…`, band levels by `tools/oracle/compare.py`.
- Module record, DSP class, process entry and output tail: [k-amps-a-hotsolo.md](k-amps-a-hotsolo.md)
  (unchanged; this file adds the measurement and the model).
- Record values in Electric Mint: prenormal 0.333, pre-overdrive 0.470, master 0.420,
  presence 0.962, depth 0.603, treble 0.603, mid 0.752, bass 0.643, overdrive 0, mono 0
  (insert slot 3 of the program's 0x3a rack, read by `kontra-format-kontakt`'s own parser).
