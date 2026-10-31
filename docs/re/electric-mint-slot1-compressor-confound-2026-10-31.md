# Electric Mint: slot 1's Eq is 0.17 dB, not 0.82 — the Supercharger sits between it and the output

Follows `docs/re/electric-mint-base-gain-2026-10-31.md` §6, whose last open item was a pair of
opposing numbers: "slot 1 (Eq) −0.82" and "everything else (`s137`: voice, level, pan, output
volume) +0.68". Both were derived from **one-slot ablations of the delivered chain**, and every
one of those ablations puts the signal through **insert slot 2, the Supercharger**, which is a
compressor. A compressor's gain depends on its input level, so an ablation that changes the
level *into* it does not measure the stage it removed; it measures that stage through slot 2.

This record re-measures slot 1 with slot 2 bypassed **on both sides**, and measures slot 2's own
contribution at two input levels, entirely inside Kontakt.

## Answer

**Slot 1's Eq is right to 0.17 dB.** With the Supercharger out of the chain, Kontakt's Eq
contributes **+3.435 dB** and KONTRA's **+3.605 dB**. The 0.82 dB in the delivered chain is
**0.17 dB of Eq + 0.65 dB of the compressor eating the boost**.

**The 0.68 dB is the same compressor, not the voice path.** Kontakt's CompXT contributes
**−2.218 dB** at the `control` input level and **−1.571 dB** with the Eq bypassed — a 0.647 dB
change of its own gain over that operating point, measured Kontakt-against-Kontakt. KONTRA
applies a constant **−2.453 dB**, which is 0.24 dB too much in the first state and 0.88 dB too
much in the second. The voice path itself is **+0.21 dB loud**, not 0.68 dB quiet.

So the two numbers the previous record found were not two errors cancelling; they were **one
unmodelled stage read twice, with opposite signs**, because slot 2 sits between slot 1 and the
output.

| configuration | Kontakt (median of 6) | KONTRA | Kontakt − KONTRA |
|---|---:|---:|---:|
| `control` (Eq in, Supercharger in) | −24.029 | −23.883 | **−0.146** |
| `s1` (Eq out, Supercharger in) | −26.817 | −27.488 | **+0.671** |
| `s2` (Eq in, Supercharger **out**) | −21.811 | −21.430 | **−0.381** |
| `s12` (Eq out, Supercharger **out**) | −25.246 | −25.035 | **−0.211** |
| `s137` (1, 3, 7 out; Supercharger in) | −34.825 | −35.503 | **+0.678** |

Contribution of each stage, from those rows (Kontakt-only columns are measured entirely inside
Kontakt, so no KONTRA error enters them):

| stage | Kontakt, compressor in | Kontakt, compressor out | KONTRA | KONTRA − Kontakt |
|---|---:|---:|---:|---:|
| slot 1 (Eq), `s2` − `s12` | +2.788 | **+3.435** | +3.605 | **+0.17** |
| slot 2 (Supercharger), `control` − `s2` | **−2.218** | — | −2.453 | −0.24 |
| slot 2 (Supercharger), `s1` − `s12` | — | **−1.571** | −2.453 | **−0.88** |

The two Supercharger rows are Kontakt against itself, one slot apart. Their 0.647 dB difference
is the compressor's level dependence, and it is the whole of the "0.82/0.68" pair:
`0.817 = 0.170 (Eq) + 0.647 (the boost the compressor removes)`.

## 1. Why the old ablation could not see this

Slot order is 1 Eq → 2 Supercharger → 3 Hot Solo → 4 Diffusion → 5 — → 6 Choral → 7 Convolution.
A one-slot ablation reads a stage at the top and subtracts it with the stage off. With a
level-dependent stage *after* the ablated slot, the second capture enters that stage at a
different level, so the difference carries the compressor's gain change. The previous record's
per-band table showed it plainly — KONTRA's Eq looked 1.1 dB hot at 127 Hz and 1.2 dB hot at
8–10 kHz, a *frequency-dependent* excess that no Eq-law error of a single gain would produce —
but it was read as an Eq shape error.

Every other ablation in that record has the same confound and the same size, which is why its
numbers are self-consistent and wrong: `s1` and `s137` both enter the compressor with the Eq out
(hence both read +0.67), `control`, `s3` and `s7` all enter it with the Eq in (hence all read
−0.15 … −0.19, independent of Hot Solo and Convolution, which are *after* the compressor).
That binary split on slot 1's state, at 10 dB of level apart, is itself the fingerprint of a
level-dependent stage: a compressor cannot be the explanation only if the states differ in level,
and here the states that differ by 10 dB agree to 0.01 dB because their *compressor input* is the
same.

## 2. The measurement

Same protocol as `electric-mint-slot-liveness-2026-10-31.md`: one fresh Kontakt session per
capture, first render only (round-robin 0), HF-burst screen, six clean captures per
configuration, median. Two new configurations, written into the library by Kontakt itself
(`probe_s2.lua`, `__s2/`):

| case | stored bypass set to 1 |
|---|---|
| `s2` | slot 2 |
| `s12` | slots 1, 2 |

`dump_fx` on the saved copies shows they differ from `control.nki` in exactly those flags and in
nothing else (slot 1 `bypass=false/true`, slot 2 `bypass=true`, every other slot identical,
slot 1's record bytes unchanged: LF gain 0.720181, LF Hz 0.376744, output 0.9999979).

Captures: 12 kept (6 per configuration), 4 rejected for the HF burst. Levels above are the 2.0 s
window from the note onset; within-configuration spread 0.16 dB (`s2`) and 0.09 dB (`s12`).

A third configuration, `all` (every insert bypassed), was attempted as a pure voice-path
reference and **could not be captured**: all 20 attempts tripped the HF-burst screen at +42…+47 dB
(the clean configurations read +6…+14). With every slot bypassed the note is 12 dB quieter than
`control` and the capture path's burst dominates the 12.9 kHz band, so the screen rejects it. That
is a measurement limit, not a finding; `s12` is the closest usable reference and it still carries
Hot Solo, Diffusion, Choral and Convolution.

## 3. Parity, and what it says

`tools/oracle-mcp/parity.py` over the existing `control` captures (five good — `clean-1` is the
known 2.44 dB band-RMS outlier and is excluded) and over the new ones:

| configuration | level (Kontakt − KONTRA) | band RMS | worst band | verdict |
|---|---:|---:|---|---|
| `control` (5 good) | −0.146, −0.099, −0.137, −0.157, −0.135 → **−0.137** | 0.889–0.988 (median 0.925) | +1.86 … +2.00 @ 50 Hz | **PASS** |
| `s2` (Eq in, Supercharger out) | −0.309 … −0.464 → **−0.355** | 0.711–0.879 (median 0.815) | ±1.6 … ±2.0 | **PASS** |
| `s12` (Eq and Supercharger out) | −0.168 … −0.259 → **−0.197** | 0.642–0.797 (median 0.651) | ±1.3 … ±2.2 | **PASS** |
| `s1` (Eq out, Supercharger in) | +0.645 … +0.707 → **+0.674** | 0.765–0.889 | ±1.5 … ±2.3 | **FAIL level** |

The delivered chain passes at −0.14 dB, and it passes on its merits: with the compressor out,
where no level-dependent stage can hide anything, the chain is −0.35 dB and its band RMS is
0.82 dB — both inside the limits. The failing configuration is `s1`, and it fails **only in
level** (+0.67), which is the compressor's 0.88 dB at that operating point.

## 4. Why no gain was fitted to slot 1

The brief asked for slot 1's Eq gain to be fixed by 0.82 dB. **That change was applied in a
scratch tree and measured** (scale the Solid EQ's applied level by −0.82 dB, rebuild, no commit):

| configuration | before | with slot 1 "fixed" by −0.82 dB |
|---|---:|---:|
| `control` (delivered chain) | −0.137 | **+0.674** — level FAIL (> 0.5) |
| `s2` (Eq alone, Supercharger out) | −0.381 | **+0.440** |

So the prescribed change does not reach parity: it moves the delivered chain to +0.67 dB, and it
leaves the Eq **0.65 dB quieter than Kontakt's** (3.605 − 0.82 = 2.785 against 3.435), visible in
`s2`, the one configuration where the Eq can be seen alone. Adding the second prescribed fix (the
"0.68 dB quiet elsewhere") on top would bring the delivered chain back to about −0.01 dB — a
third cancellation, of the kind the previous two records were written to stop counting as parity,
by one slot's worth of gain on a stage that does not have it.

The number is real; it belongs to slot 2.

What is left, in order of size, once the compressor is removed from the comparison:

| component | KONTRA − Kontakt | basis |
|---|---:|---|
| slot 1 (Eq) | **+0.17** | `s2`/`s12`, level |
| voice path + slots 3/4/6/7 | **+0.21** | `s12`, level (Hot Solo +0.04, Diffusion +0.02, Choral +0.08 of it) |
| slot 2 (Supercharger), at `control` | **−0.24** | static make-up −2.453 against −2.218 |
| slot 2 (Supercharger), at the Eq-out input | **−0.88** | static make-up −2.453 against −1.571 |

The Supercharger is the only term above 0.5 dB in any configuration, and it is the row the repo
already reports rather than models (`docs/re/engine/dyn-compressors.md`: the CompXT's gain
computer is unread). Two operating points are not a law, so nothing was written from them; the
step that would close `s1` is the CompXT's gain computer, not a curve fitted to two numbers.

What the Eq *is* worth keeping an eye on is its shape: with the compressor out, KONTRA is
+0.3…+0.7 dB high across the LF shoulder (125–508 Hz) and +0.7 dB at 10–13 kHz, −0.9 dB at
63 Hz — the 0.17 dB level is the net of that. That is the Solid G-EQ law's known approximation
(`laws::solid_eq`: linear interpolation on `solid-geq-responses.tsv`'s native knots), and it is
0.05× the number this brief was about.

## 5. Reproducing

```
git worktree add -f /tmp/opencode/wt-eq -b fix/slot1-eq feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-eq cargo build --release -p kontra-host -p kontra-inspect \
    --example dump_fx -p kontra-format-kontakt

# 1. Kontakt writes the two bypass flags into its own copies
python3 tools/oracle-mcp/slot-liveness/run_lua.py \
    tools/oracle-mcp/slot-liveness/probe_s2.lua "$LIB/Instruments/__s2/variants.jsonl" \
    --nki "$LIB/Instruments/Electric Mint.nki"

# 2. six fresh sessions per configuration, round-robin 0, burst-screened
python3 tools/oracle-mcp/slot-liveness/capture_variants.py --out /var/tmp/eq/cap \
    --variant-dir "$LIB/Instruments/__s2" --configs s2,s12 --clean 6

# 3. the verdict, on both new configurations and on the delivered one
KONTRA_RENDER=/var/tmp/tgt-eq/release/kontra-render python3 tools/oracle-mcp/parity.py \
    --nki "$LIB/Instruments/__s2/s2.nki" --kontakt-wav /var/tmp/eq/cap/s2/clean-2.wav \
    --out /var/tmp/eq/par/s2-2
```

## 6. Fingerprints

No source under `crates/` changes in this record, so every render is byte-identical to
`feat/integration` at `9019bb2`: `kontra-render` of `control.nki` sha256
**`585f5f7dbb10c28c…`** (unchanged), Haunted Strings 2 `06cf13eee3f8f54e…` and Cherry Electric
Bass `0279ba0fd86927bd…` unchanged (they are the stored captures of the previous lane; this
lane's builds reproduce them because no code differs).
