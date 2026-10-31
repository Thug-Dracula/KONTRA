# Electric Mint: which instrument insert slots Kontakt actually runs, and at what gain

Measured 2026-10-31 on Kontakt 8.14.1 (portable, under wine-staging 11.16) against the oracle's own
Xvfb; KONTRA at `b3cd8bb` (`feat/integration`), built `CARGO_TARGET_DIR=/var/tmp/tgt-live`.
Instrument: the stock `Electric Mint.nki` (1.1.0). One note throughout: key 60, velocity 100,
held 1.5 s, first 2 s from the onset, round-robin index 0.

This answers the question the stage-attribution record left open
(`docs/re/electric-mint-stage-attribution-2026-10-30.md` §Answer: "Distinguishing them needs one
Kontakt capture per slot bypassed in the instrument UI, which this measurement did not do").

## Answer

**Both slots run in Kontakt. Neither is inert.** Kontakt's Choral (slot 6) costs **−1.33 dB**;
Kontakt's Replika Diffusion (slot 4) is **level-neutral (+0.04 dB)** but audibly live.

| stage | Kontakt: level gain when the slot is bypassed | Kontakt's own contribution | KONTRA's (same ablation) | difference |
|---|---:|---:|---:|---:|
| Choral, slot 6 | **+1.331 dB** (spread 0.064) | −1.331 dB | −1.255 dB | **+0.08 dB** |
| Diffusion, slot 4 | **−0.039 dB** (spread 0.084) | +0.039 dB | +1.535 dB | **+1.50 dB** |
| both, slots 4+6 | +1.255 dB (spread 0.042) | −1.255 dB | +0.284 dB | +1.54 dB |
| Supercharger, slot 2 (positive control) | +2.228 dB (spread 0.144) | −2.228 dB | 0.000 dB | +2.23 dB |

So: **Choral's gain law is right to 0.08 dB** (KONTRA −1.255 vs Kontakt −1.331), and **Diffusion's
gain is wrong by 1.50 dB** — Kontakt's stored Diffusion, 0.299 wet over a unity dry, comes out
level-neutral, where KONTRA's adds +1.54 dB.

**The delivered chain's 0.27 dB is two errors cancelling, not a correct chain.** With both stages
ablated on both sides, KONTRA is **1.248 dB below** Kontakt (both-ablated: KONTRA −24.045 dBFS,
Kontakt −22.796 dBFS). The pair is 1.54 dB too hot in KONTRA, and that is what the base error hides
behind. Fixing Diffusion's gain alone therefore moves the delivered chain to about **−1.29 dB** —
worse than it is now — unless the −1.25 dB base error is fixed with it. See §5.

## 1. The bypass route: Kontakt's own store, written by Kontakt, read back from its file

`oracle_click` on the Amps & FX page is not usable (the page is KSP-drawn; a previous lane measured
`changed=83` and nothing happened). Two other routes exist; the one used here is the KSP engine-parameter
route, applied inside a Lua-driven session that also saves the instrument, so the audio capture can then
load a file whose *only* difference is the stored bypass flag.

How the bypass was applied, per case, in one disposable Kontakt session (`probe_variants.lua`):

```ksp
on init
  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 0, -1, 2, $NI_INSERT_BUS)   -- slot 2 (control value)
  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 1, -1, 4, $NI_INSERT_BUS)   -- slot 4
  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 0, -1, 6, $NI_INSERT_BUS)   -- slot 6
end on
```

written into **script slot 3** (slot 1 is Electric Mint's own protected script; 3 is empty), then
`save_instrument` to a copy inside the library. Five files: `control` (all three values 0, the same
write path with no state change), `sup2`, `chor6`, `diff4`, `both46`.

**Addressing was measured, not assumed** (`probe_addr.lua`, KSP slot index 0..7 one file each, then
decode): KSP slot *k* with generic `$NI_INSERT_BUS` flips the stored bypass of instrument-insert
slot *k*, and nothing else — `{0:[], 1:[1], 2:[2], 3:[3], 4:[4], 5:[] (empty slot), 6:[6], 7:[7]}`.
Generic `0` behaves identically here, so the two conventions in the repo's own rigs (`fxrig.py` uses
0, `eqrender.py` `$NI_INSERT_BUS`) do not disagree on this path.

The instrument's insert rack as both readers see it (slot index = the number KONTRA's diagnostics
print):

| slot | kind | output | dry | note |
|---:|---|---:|---:|---|
| 0 | — | | | empty |
| 1 | Eq | 1.000 | 0.000 | |
| 2 | Other | 0.704 | 0.000 | Supercharger (no verified law in KONTRA) |
| 3 | Amp | 1.000 | 0.000 | Hot Solo |
| 4 | OtherWet | 0.299 | 1.000 | Replika, Diffusion type |
| 5 | — | | | empty |
| 6 | Other | 1.000 | 0.000 | Choral, 100 % wet |
| 7 | Convolution | 0.375 | 1.000 | |

Three pieces of evidence that this route measures Kontakt's engine and not our file writer:

1. **Only the flag differs.** Decoding each saved file with `kontra-inspect load` and comparing every
   insert slot's kind/output/dry against the stock instrument: `sup2` differs at slot 2 only,
   `chor6` at slot 6 only, `diff4` at slot 4 only, `both46` at 4 and 6 — every other field on every
   slot identical.
2. **The write path is neutral.** `control` writes bypass 0 to slots 2, 4 and 6 (the same three
   `set_engine_par` calls, same script, same save) and renders at −24.051 dBFS against the stock
   instrument's −24.045 dBFS: **0.006 dB**, and 0.32 dB band RMS — the session floor.
3. **A live stage responds.** Bypassing slot 2, which nobody disputes is audible, moves the render by
   **+2.23 dB**. A file-parsing or load-order artefact would not do that.

So "the slot changed nothing" would have been a real measurement, not a failed click. It did not
happen: both slots moved something.

## 2. The capture protocol

One **fresh Kontakt session per capture** (`oracle_up` → `oracle_load` → `oracle_render` →
`oracle_down`), first render only. The round robin is a per-session counter, so the first render of
every session is the same sample — the reason the previous record's floor was computed this way. That
it held here is visible in the spreads: within-configuration level spread is **0.042–0.144 dB**,
where the same record measured **0.19–0.21 dB** between different round-robin samples of one session.

44 renders (6 configurations × up to 24 attempts): **36 clean, 8 rejected** for the capture path's
HF burst (worst 0.2 s window's 12.9 kHz-over-1 kHz excess above 20 dB over the note's median; the
rejects read +24.3…+33.5 dB, the clean ones +6.3…+13.9 dB). Every attempt is in the ledger, kept or
rejected.

## 3. The table

Level and peak are absolute, 2.0 s from the note onset; band RMS is the RMS of the median-removed
1/3-octave difference against `control`; L/R is the correlation of the two channels over the same
window; "sweep" is the mean over bands of the standard deviation of the 1/3-octave level across
100 ms windows (a whole-note band average cannot see a chorus or a diffuser; this can).

| configuration | level dBFS median [min, max] | spread | peak dBFS | band RMS vs control | L/R corr [min, max] | sweep dB [min, max] |
|---|---|---:|---:|---:|---|---|
| `orig` (stock file) | −24.045 [−24.071, −24.002] | 0.069 | −11.12 | 0.32 | 0.899 [0.890, 0.903] | 7.89 [7.69, 7.95] |
| `control` (saved, nothing bypassed) | −24.051 [−24.075, −23.979] | 0.096 | −11.23 | 0.00 | 0.897 [0.891, 0.900] | 7.82 [7.70, 7.97] |
| `sup2` (slot 2 bypassed) | −21.823 [−21.888, −21.743] | 0.144 | −9.39 | 0.60 | 0.898 [0.894, 0.902] | 7.70 [7.60, 7.98] |
| `chor6` (Choral bypassed) | −22.720 [−22.765, −22.700] | 0.064 | −9.43 | 0.54 | 0.928 [0.926, 0.931] | 7.90 [7.76, 8.02] |
| `diff4` (Diffusion bypassed) | −24.090 [−24.121, −24.037] | 0.084 | −11.27 | 0.30 | 0.903 [0.902, 0.904] | 8.57 [8.51, 8.69] |
| `both46` (both bypassed) | −22.796 [−22.818, −22.776] | 0.042 | −9.47 | 0.56 | 0.936 [0.936, 0.937] | 8.51 [8.48, 8.59] |

Six captures per configuration, each from its own session; peak is the median of the six, and the
six peaks within a configuration span 0.12–0.49 dB (peak is dominated by the attack transient and is
the noisiest of the three columns — level is the metric to trust).

Per band, Kontakt, bypassed minus active (dB, positive = bypassing is louder):

| Hz | Choral (slot 6) | Diffusion (slot 4) |
|---:|---:|---:|
| 50 | +2.61 | −0.05 |
| 80 | +1.27 | +0.00 |
| 127 | +1.36 | −0.11 |
| 202 | +1.34 | −0.03 |
| 320 | +0.97 | +0.21 |
| 508 | +1.27 | −0.18 |
| 806 | +0.47 | −0.16 |
| 1280 | +1.42 | −0.21 |
| 2032 | +1.10 | −0.06 |
| 3225 | +1.30 | −0.17 |
| 5120 | +1.20 | +0.09 |
| 8127 | +1.00 | −0.19 |
| 12902 | +1.47 | +0.40 |

Choral is a **flat broadband gain**: 500 Hz–16 kHz every band inside +0.47…+1.47 dB, no tilt.
Diffusion is a **flat zero**: every band inside ±0.40 dB, symmetric about zero. Neither slot changes
the shape; both are gains (or the absence of one).

## 4. Verdict per stage

**Choral (slot 6) — LIVE, and our gain is right.** Bypassing it in Kontakt makes the render
**+1.331 dB** louder (floor 0.064 dB), flat across the spectrum, and drops the stereo correlation
from 0.928 to 0.897 — a 100 %-wet chorus widens a mono-ish guitar and costs a little level, which is
exactly what Kontakt's slot does. KONTRA's Choral takes 1.255 dB out: **0.08 dB from Kontakt's**.
Do not revert it. If anything it is 0.08 dB light, which is inside this measurement's floor debate
and is not worth a code change on its own.

**Diffusion (slot 4) — LIVE, but level-neutral in Kontakt; KONTRA's +1.54 dB is the error.** The
level number alone would not settle liveness (bypass changes it by −0.039 dB, i.e. nothing), so it
was measured two further ways:

- **short-time spectral movement**: with Diffusion active the 100 ms band-level deviation is
  **7.82 dB** [7.70, 7.97]; bypassed it is **8.57 dB** [8.51, 8.69] — a 0.55 dB separation with no
  overlap, and `both46` (8.51 [8.48, 8.59]) tracks `diff4`, so the whole effect is attributable to
  slot 4. That is a diffuser doing what a diffuser does: smearing transients.
- **stereo**: L/R correlation 0.897 [0.891, 0.900] active vs 0.903 [0.902, 0.904] bypassed — a
  0.006 shift that just clears the session spread.

So the slot runs, and its **net level contribution is +0.04 dB (zero)** — not +1.54 dB. The target
for the gain/width law is: **a 0.299-wet-over-unity-dry diffusion must come out level-neutral.**

**Supercharger (slot 2) — LIVE, and KONTRA does not model it.** Bypassing it moves Kontakt by
+2.23 dB. KONTRA renders slot 2 as an identity **in both states** (bypass flag ignored; `sup2` and
`control` render to the same sample, −23.760 dBFS both). This is consistent with
`docs/re/engine/dyn-compressors.md` ("no verified law") — it is named here because it is the largest
single known mismatch in the chain (§5), not because it was the question.

## 5. The accounting correction: the delivered 0.27 dB is two errors cancelling

The previous record compared each KONTRA ablation against **Kontakt's full chain**, and concluded
"the uneffected chain is the one that matches Kontakt". Kontakt's own slots are not neutral — its
pair costs 1.25 dB — so that comparison is against a chain that is already 1.25 dB down. Matched
configuration against matched configuration, KONTRA at `b3cd8bb`:

| configuration | KONTRA − Kontakt level | band RMS | KONTRA absolute | Kontakt absolute |
|---|---:|---:|---:|---:|
| both active (`control`) | **+0.291 dB** | 0.92 | −23.760 | −24.051 |
| Choral out (`chor6`) | +0.215 dB | 0.82 | −22.505 | −22.720 |
| Diffusion out (`diff4`) | −1.205 dB | 0.85 | −25.295 | −24.090 |
| both out (`both46`) | **−1.248 dB** | 0.87 | −24.045 | −22.796 |

Read as two independent errors:

1. **Everything except slots 4 and 6 is 1.248 dB too quiet in KONTRA** (both-out row). Slot 2 alone
   is 2.23 dB of that in the opposite direction (Kontakt attenuates, KONTRA does nothing), so the
   rest of the chain — slots 1, 3, 7, and the master/output path — must be adding back more than
   3 dB that KONTRA does not. **Not attributed here**; it is the next measurement, and it needs the
   same slot-by-slot ablation for slots 1, 3 and 7.
2. **The pair is 1.532 dB too loud in KONTRA** (+0.284 vs −1.255), of which Choral is 0.08 dB and
   Diffusion 1.50 dB.

They sum to the delivered +0.284 dB, which is why the delivered chain looks close to Kontakt while
both of its stages are individually wrong. Concretely: **apply only the Diffusion gain fix and the
delivered chain becomes ≈ −1.29 dB** — a level failure against `parity.py`'s 0.5 dB limit. The two
fixes have to land together, or the Diffusion fix has to wait for the base error.

## 6. What this does and does not establish

- It establishes, by measurement inside Kontakt, that slots 4 and 6 are both applied, that Choral's
  level law matches ours to 0.08 dB, and that Diffusion's does not (0.04 dB in Kontakt, +1.54 dB in
  KONTRA). It also establishes that the delivered parity number is a cancellation.
- **Choral's modulation law is not measured here.** The level and the stereo-width change are
  consistent with a 100 %-wet chorus, but a 2 s band average cannot certify that the sweep is right;
  that needs a short-time comparison against Kontakt's own Choral.
- **Diffusion's internal law is not measured.** Its de-transient and stereo signatures are 0.55 dB
  and 0.006 in these metrics; whether the tap network is right needs the native vectors in
  `docs/re/engine/k-replika-diffusion.md`, not this.
- **The bypass was applied through Kontakt's own save path**, not live via CC/KSP at playback time.
  Kontakt serialises the flag and its engine honours it on load (proved by slot 2), but this measures
  the stored-state path. A live `set_engine_par` during playback was not tried: the slot-2 control
  makes it unnecessary for a liveness verdict.
- Slots 1 (Eq), 3 (Hot Solo) and 7 (Convolution) were **not** ablated.

## Reproducing

```
git worktree add -f /tmp/opencode/wt-live -b measure/slot-liveness feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-live cargo build --release -p kontra-host --bin kontra-render

# 1. variants: Kontakt writes the bypass into five copies of the instrument (one session, 43 s)
python3 tools/oracle-mcp/slot-liveness/run_lua.py \
    tools/oracle-mcp/slot-liveness/probe_variants.lua \
    "$ELECTRIC_MINT_LIBRARY/Instruments/__slotliveness/variants.jsonl" \
    --nki "$ELECTRIC_MINT_LIBRARY/Instruments/Electric Mint.nki"

# 2. read back the stored state of every copy (must differ only in the named slot's flag)
kontra-inspect load "$ELECTRIC_MINT_LIBRARY/Instruments/__slotliveness/chor6.nki" --json chor6.json

# 3. captures: 6 configurations x 6 clean sessions, first render only, burst-screened
python3 tools/oracle-mcp/slot-liveness/capture_variants.py --out /var/tmp/live/cap \
    --configs orig,control,sup2,chor6,diff4,both46 --clean 6

# 4. every metric, and the pair matrix with its floor
python3 tools/oracle-mcp/slot-liveness/analyze.py /var/tmp/live/cap --out analysis.json
```

The five instrument copies live in the library folder
(`…/Session Guitarist - Electric Mint Library/Instruments/__slotliveness/`) because KONTRA resolves
the library's `.nicnt` by walking up from the instrument file; they are regenerated by step 1.
Captures, ledgers and the per-attempt burst scores: `/var/tmp/live/` and
`docs/re/electric-mint-slot-liveness-2026-10-31.json`.
