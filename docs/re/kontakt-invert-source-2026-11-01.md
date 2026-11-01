# `$MOD_TARGET_INVERT_SOURCE` — what Kontakt's Invert switch does to a source

Measured 2026-11-01 on Kontakt 8.14.1 (portable, wine-11.16) through
`tools/oracle-mcp`, against KONTRA at `feat/integration`. One instrument, one
session, one load; the patch's own switch flips the flag between two sweeps.

## The question

KONTRA read the flag as a mirror about the middle of the source's range — `1 − x`
for a unipolar source, `−x` for a bipolar one (`voice_gain.rs`, `inverted`) — with
no measurement behind either branch: `docs/re/ksp/SYMBOLS.tsv` called the symbol
"opaque symbolic identity; no native numeric value or behavior implied". Three
laws were on the table: `1 − x` (mirror), `−x` clamped at zero (one-sided), or no
effect on this routing at all.

## The instrument

`FX - Funky MW - Harpsichord.nki`, Soundiron Harpsichord, `#Non NICNT/Custom FX`.
It is the right probe because every part of it is stored the same way on both
sides:

| what | value | from |
|---|---|---|
| `%filter_mods[0]` | `2` → `!filter_mods[2] = "MW_CUTOFF"` | `dump_persistent` |
| the route that name finds | `MW_CUTOFF @39 Cutoff { source: ControllerFrom { cc: 1, start: 0 }, insert: 2, depth: 1.0 }` on 12 groups, `0.0` on the other 132 | `inspect` |
| `%filter_toggles[0]` | `1` — the filter insert is not bypassed | `dump_persistent` |
| `%filter_type_layers[0]` | `0` → `$FILTER_TYPE_LP6POLE` | `dump_persistent` + `slot-0.ksp:1293` |
| `%filter_freqs[0]` | `305233` | `dump_persistent` |
| `%filter_mod_invs[0]` | `0` — the flag is **off** when the file loads | `dump_persistent` |
| `$filter_mod_inv` | `0`, the FILTER panel's own INVERT switch | `dump_persistent` |

The script (slot 0, "Performance") sets both the flag and the depth on that route
whenever a layer's filter selection changes:

```
set_engine_par($MOD_TARGET_INVERT_SOURCE,%filter_mod_invs[0],$gui_counter,find_mod($gui_counter,!filter_mods[%filter_mods[0]]),-1)
set_engine_par($ENGINE_PAR_MOD_TARGET_INTENSITY,1000000,$gui_counter,find_mod($gui_counter,!filter_mods[%filter_mods[0]]),-1)
```

— `slot-0.ksp:14491`, the same pair per layer at `:14933`, `:15375`, `:15817`, and
again in the layer-link paths at `:65803`–`:80425`.

## What `%filter_mod_invs` holds, and the toggle

All 21 `.nki` in this folder that name the symbol store the flag **off**:

```
for f in Harpsichord.nki "Custom FX"/*.nki; do
  dump_persistent "$f" | grep -E 'filter_mod_invs|%filter_mods'
done
```

gives `$filter_mod_inv 0` and `%filter_mod_invs 0` for every one of the 21 (and
`%filter_mods 6 1` for 19 of them — `GRAPH_CUTOFF` — against `2 1` for
`FX - Funky MW` and `1 0` for `Harpsichord.nki`). So no instrument in the library
loads with the flag set, and a file-only probe cannot see it.

A UI toggle exists and is the runtime lever. `$filter_mod_inv` is declared at
`slot-0.ksp:5361` as a `ui_switch` with `popup_radio_button`, 23×23 at
`(411, 286)` in the popup panel, help text *"Invert the active Filter
modulator"* (`:7515`), automation name *"Invert"* (`:7627`), an automation id
(`:8060`), and entry 62 of the patch's control table (`:8201`, `:8398`). Its
`ui_control` handler writes `%filter_mod_invs[$active_layer]` and re-sends
`$MOD_TARGET_INVERT_SOURCE` to every group of the layer (`:88899`–`:88985`).

The patch saves the popup open (`$show_popup 1`), so the switch is on screen as
soon as the instrument page paints.

## The probe

Nine wheel positions, one note each, the same key/velocity/hold. The MIDI was a
type-0 SMF at 480 PPQ / 120 bpm holding CC1 at t = 0, note-on key 60 velocity 100
at 0.05 s, note-off at 1.55 s — the controller is in place before the note is
struck, which is what the route reads at note-on. `oracle_render` grew a `cc`
argument for this (`tools/oracle-mcp/server.py`, `write_midi`), so the whole
probe is one JSONL script:

```bash
python3 tools/oracle-mcp/client.py --script /tmp/opencode/steps-funky.jsonl
# oracle_up → oracle_load <nki> → [nine oracle_render, invert off]
# → oracle_click (784, 386) → [nine oracle_render, invert on] → oracle_down
```

The click landed on the switch: `changed=77` px, and the radio dot goes from
hollow to filled (`shot-001.png` / `shot-002.png` in
`/tmp/opencode/oracle-mcp/run-20261101-152010/`). `kontakt_leftovers: []` on the
way down, one load, 18 renders.

KONTRA rendered the same nine MIDI files:

```bash
kontra-render --instrument "$NKI" --out "wav/k-funky-$v.wav" \
              --midi "/tmp/opencode/wheel/wheel-$v.mid" --quiet
```

Band RMS and centroid are `/tmp/opencode/bands.py` over 0.15–1.40 s from each
render's own onset (first frame over −50 dBFS), so the same part of the note is
compared on both sides. `peak` is the render's own full-file peak in dBFS.

### The nine points

`low` 0–500 Hz, `mid` 0.5–4 kHz, `high` 4–16 kHz, each normalised to the
window's total power (dB); `cen` is the power-weighted centroid in Hz.

| wheel | Kontakt, invert **off** | | | | | Kontakt, invert **on** | | | | |
|---|---|---|---|---|---|---|---|---|---|---|
| | peak | low | mid | high | cen | peak | low | mid | high | cen |
| 0 | −23.96 | −0.39 | −10.64 | −60.86 | 278 | −27.27 | −0.33 | −11.42 | −101.13 | 277 |
| 16 | −25.07 | −0.68 | −8.38 | −67.09 | 389 | −24.34 | −0.00 | −32.10 | −102.83 | 253 |
| 32 | −28.25 | −2.07 | −4.25 | −25.38 | 668 | −27.68 | −0.00 | −59.53 | −101.86 | 124 |
| 48 | −23.64 | −3.58 | −3.36 | −10.02 | 1426 | −46.33 | −0.00 | −83.09 | −106.90 | 42 |
| 64 | −22.20 | −4.68 | −2.78 | −8.80 | 1803 | −66.57 | −0.00 | −84.89 | −108.86 | 28 |
| 80 | −24.34 | −5.11 | −2.89 | −7.49 | 1965 | −60.35 | −0.00 | −89.52 | −113.21 | 26 |
| 96 | −26.86 | −4.06 | −3.54 | −7.85 | 1868 | −65.78 | −0.00 | −93.04 | −118.02 | 25 |
| 112 | −23.79 | −2.11 | −5.46 | −10.01 | 1204 | −62.59 | −0.00 | −89.64 | −116.66 | 26 |
| 127 | −25.53 | −1.82 | −5.45 | −12.45 | 926 | −64.36 | −0.00 | −91.41 | −114.29 | 29 |

KONTRA, same nine MIDI files (the file stores the flag off, so this is the
un-inverted route):

| wheel | peak | low | mid | high | cen |
|---|---|---|---|---|---|
| 0 | −25.86 | −2.29 | −4.17 | −15.80 | 838 |
| 16 | −25.66 | −3.01 | −3.54 | −12.41 | 1121 |
| 32 | −25.71 | −3.28 | −3.40 | −11.41 | 1245 |
| 48 | −25.08 | −3.19 | −3.51 | −11.29 | 1261 |
| 64 | −25.05 | −3.76 | −3.06 | −10.74 | 1382 |
| 80 | −25.05 | −4.11 | −2.89 | −10.06 | 1515 |
| 96 | −25.05 | −4.27 | −2.93 | −9.34 | 1620 |
| 112 | −25.05 | −4.28 | −3.02 | −8.95 | 1657 |
| 127 | −25.05 | −4.25 | −3.00 | −9.11 | 1637 |

### The measurement's own noise

The patch picks among samples per note, so consecutive notes differ. Five
renders at fixed wheel positions in a second session (`run-20261101-152450`):
wheel 0 three times gives peak −31.57 / −32.94 / −32.93 dB and centroid 346 / 313
/ 292 Hz; wheel 80 twice gives −26.31 / −26.42 dB and centroid 2028 / 1244 Hz. So
a level column carries ≈1.5 dB of sample-selection noise and a centroid ≈±25 %
at mid wheel.

That is why the law is read from the *shape* columns, where the endpoints differ
by 20 dB, and not from peak. It also explains the 3 dB between Kontakt's wheel-0
peak with the flag off and on: those are the 1st and 10th notes of the session,
i.e. different round-robin samples, while the centroid (278 vs 277 Hz) and the
band split (−0.39/−10.64 against −0.33/−11.42) do not move at all.

## The law

**The flag negates the source value about zero.** A unipolar source `0..=1` reads
`0..=−1`, not `1..=0`; a bipolar one reads `−x`, which the existing `(x+1)/2`
normalisation turns back into the same mirror.

- **`1 − x` is out.** Wheel 0 is the same render either way (centroid 278 Hz /
  high band −60.9 dBFS with the flag off, 277 Hz / −101 dBFS with it on). A
  mirror puts the inverted read at the *bright* end there — wheel 127 with the
  flag off, centroid 926 Hz and −12.5 dBFS in the same band, which is 19 dB away
  from what is measured.
- **`−x` clamped at zero is out.** Wheel 16, 32 and 48 with the flag on land at
  253, 124 and 42 Hz — darker than wheel 0 and darker than the un-inverted route
  reaches at *any* wheel position. A clamp holds them at wheel 0's 277 Hz.
- **No effect is out.** Wheel 127 with the flag on is 39 dB below the same wheel
  with it off (−64.4 against −25.5 dBFS peak) and 25 dB below it in RMS; the
  route is being read, in the other direction.
- **By wheel 64 the cutoff is under the fundamental**: −66.6 dBFS, i.e. the
  target's own floor. That is the negative branch saturating, not the source
  clamping — wheel 32 was already past anything the positive branch can reach.

Kontakt's own manual agrees and was already quoted in this repo
(`docs/audit-2026-10-10/modulation-control-remap-fix/source-manifest.json:1117`):
*"`$ENGINE_PAR_MOD_TARGET_INTENSITY` … controls the modulation amount in the
positive range only. In order to apply a negative modulation amount, Invert
button needs to be toggled"* and *"`$MOD_TARGET_INVERT_SOURCE` The Invert button
of a modulation assignment."* A positive-only depth with the source negated is a
negative contribution; that is exactly this curve.

`docs/re/ksp/SYMBOLS.tsv` still says "opaque symbolic identity; no native numeric
value or behavior implied" for the symbol. That file is a generated artifact of a
crate path that no longer exists (`crates/sampler-ksp/src/…`, against
`crates/script/ksp`), so it was not regenerated here; the fact lives in
`docs/architecture-v3/gold/modulation.md` §1.7 as a `[doc]` + `[probe]` pair.

## What changed in KONTRA

`crates/engine/engine/src/voice_gain.rs`: `inverted(invert, raw)` is `-raw` when
the flag is set, for every source. The `source` parameter is gone — the law does
not branch on the source's kind any more, which is also what closes the second
finding (a `Source::Script` read is no longer assumed unipolar by the invert).
`Engine::unit` still normalises a bipolar source to `(x + 1)/2` *after* the
invert, so a bipolar route keeps reading `1 − u` — the same negation, and the one
branch of this that is still not measured directly (no patch in the library sets
the flag on an LFO route; it is the identity of the negation on a bipolar
domain).

Tests:

- `voice_gain::tests::an_inverted_route_negates_its_source` — the measured
  points: 0 stays 0, 0.25 reads −0.25, a bipolar value mirrors.
- `voice_gain::tests::an_inverted_wheel_walks_the_cutoff_down_from_its_unmodulated_value`
  — the shape: fixed at 0, monotone down, no folding back.
- `crates/app/host/tests/script.rs::an_inverted_route_reads_its_source_negated`
  — the KSP path actually reaches it: a script sets
  `$MOD_TARGET_INVERT_SOURCE` and `inspect_routes` reports the route reading
  −32/127. It asserted `1 − 32/127` before, and failed on the change.
- `tools/test_oracle_mcp.py` — `oracle_render`'s `cc` argument puts the
  controller ahead of the note-on.

Nothing rendered moves: `tools/parity-corpus/run.py` 20/20 with every case at
`moved +0.00/+0.00`, and the Electric Mint fingerprint `f18acaab5b897588` is
unchanged. That is what the library predicts: of the 4324 instruments this
reader accepts, **0 have a route with the flag set** (`scan_source` over the
whole library: `instruments with any inverted route: 0`), so no case can render
through the changed line.

## Finding 2: a modulation whose source is a script-written value

Search: every `.nki` under `~/Armory/VSTs/Kontakt1` (6667 files) loaded with
`crates/format/kontakt/examples/scan_source.rs`, which reports every route whose
source is `Source::Script` — over all `ModKind` variants, not a hand-picked few
— and whether the instrument's scripts name
`$EVENT_PAR_MOD_VALUE_ID`, the KSP call that writes one. Result: **4324 readable
instruments, 8 with a script-source route, 5 where a script actually sends the
value.**

| instrument | script-source routes | script sends |
|---|---|---|
| `Scene - Lotus [Native Instruments]/Instruments/Scene - Lotus.nki` | 16 | yes |
| `Session Guitarist Picked Nylon/…/Picked Nylon.nki` | 948 | yes |
| `Session Guitarist Picked Nylon/…/Picked Nylon (Melody).nki` | — | yes |
| `Lores 1.0.1 [Native Instruments]/…/Instruments/Lores.nki` (both copies) | 2296 | yes |
| `Folds [Void & Vista]/Instruments/FOLDS.nki` | ~470 (`KSP_PITCH`, `Pitch { source: Script(1), semitones: 12.0 }`) | no |
| `Strands 2.0 [Void & Vista]/Instruments/Strands*.nki` | a few | no |

**None of those routes has the flag set** — every one reports `invert false`
(`scan_source` on the three folders above: 948, 2296 and 16 routes, all false),
which is the same answer the 21 Harpsichord patches give and the same answer the
whole library gives: `instruments with any inverted route: 0` across the 4324
that load. So the script *value* side is not measured either, and by the law
above it needs no separate law: the flag negates whatever value the route reads,
and `Source::Script` is documented in the IR as −1..=1, where negation is the
same mirror a pitch bend gets.

What would settle it end-to-end: one of these five, with the flag set on its
script route. `Scene - Lotus.nki` is the smallest — 16 routes, one note, and it
is already a parity-corpus case (currently a recorded "KONTRA silent" gap) — so
it is the instrument to use, not a hypothetical one. That probe still needs the
flag set, and neither the file nor its script sets it: it would be the patch's
own INVERT switch if the route had one, or a one-line
`set_engine_par($MOD_TARGET_INVERT_SOURCE, 1, 0, <slot>, -1)` added in Kontakt's
script editor. The band `Lores`/`Picked Nylon` routes go through
`Volume`/`Pan`-style targets rather than a cutoff, so their curve would be
clamped at the target's floor rather than walk octaves.

## Open: the un-inverted route already disagrees

Not part of this change and not caused by it, recorded because the table shows
it: at wheel 0 KONTRA's render is 838 Hz of centroid and −15.8 dBFS above 4 kHz
against Kontakt's 278 Hz and −60.9 dBFS, and the two curves saturate differently
(Kontakt peaks at wheel 80 and falls back by 127, KONTRA flat from 64). That is
the `MW_CUTOFF` route's depth/law on a 6-pole lowpass, not the invert flag —
KONTRA is monotone where Kontakt is a hump, which no sign change explains.
