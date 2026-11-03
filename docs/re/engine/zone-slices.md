# Zone slices: where they are stored and what KSP answers

2026-11-02. The last KSP gap in the compile sweep: `num_slices_zone`,
`zone_slice_start` and `zone_slice_length` — 396 of the 404 failing script slots
named the first two (Spitfire Studio Brass 204, Heavyocity Vocalise 3 190,
Analog Hybrid Drums 1, Damage Drum Kit 1).

## 1. Storage: the zone's `0x35` child

There was no reader for slices in `crates/format/kontakt`; the corpus dump found
them in the zone's **private raw object**, chunk `0x35`, already decoded by
ni-file's `ZoneSlices` (`vendor/ni-file/src/kontakt/objects/zone_slices.rs`,
written from a Beat Machine capture). The chunk body is a structured object of
version `0x10`; its public payload is

| offset | type | meaning |
|---:|---|---|
| 0 | `u8` | 0 |
| 1 | `i32` | selected marker, `-1` when none |
| 5 | `u32` | **count** of markers |
| 9 | `u32` | grid length in frames — present only when `count > 0` |
| 13 | `count × 12` | `u32` frame, `f32` same frame, `u32` flag (8 on the first only) |

Measured in two of the four products:

| instrument | zone | sample frames | count | grid length | markers |
|---|---:|---:|---:|---:|---|
| `Spitfire_Studio_Brass/…/a - Horn Solo.nki` | 0 | 509805 | 33 | 475686 | 0, 14865, …, 475686 |
| `Heavyocity Vocalise 3/…/Straight Pedal 01 Mono (Carla).nki` | 0 | 460800 | 65 | 460800 | 0, 7200, …, 460800 |
| `Spitfire_Studio_Brass/…/a - Horn Solo.nki` | 0 of a short key | 270960 | 33 | 475686 | the same grid, past the sample |

The frames ascend and the **last marker is the grid's end**, so `count` markers
hold `count - 1` slices. A grid whose last marker is past the sample is dropped
whole: `AVANT Modern Keys.nki` zone 0 stores 64 markers ending at 373516 over a
370813-frame sample, and Kontakt answers `num_slices_zone` 0 for each of its
first 128 zones (all three queries below), while `Heavyocity - Analog Hybrid
Drums/…/Analog Hybrid Loop Designer.nki`, whose grids fit, answers up to 75
slices. Spitfire's short keys rely on the same rule (their samples are shorter
than the shared 33-marker grid), and `a_spitfire_grid_stops_at_its_own_end_before_the_sample_does`
pins it.

## 2. What KSP answers — microseconds, and one fewer than the markers

The oracle ran a probe script in an unused slot of a *saved copy* of the
instrument (the library .nki is encrypted and only reads back with its `.nicnt`
beside it; the copy was deleted afterwards). The readback is the mechanism
`docs/re/engine/ksp-polyphonic-read.md` established: the KSP script encodes a
number into `$ENGINE_PAR_VOLUME` as `E = 1000·(v + 1)` and the Lua probe reads
`get_instrument_volume`, whose law is `dB = −167.38 + 18·log₂(E / 1000)`.

| query (Vocalise zone 0) | Kontakt | stored |
|---|---:|---|
| `num_slices_zone` | **64** | 65 markers |
| `zone_slice_start(z, 0)` | **0** | 0 |
| `zone_slice_start(z, 1)` | **150000** | 7200 frames |
| `zone_slice_start(z, 2)` | **300000** | 14400 frames |
| `zone_slice_start(z, 63)` | **9450000** | 453600 frames |
| `zone_slice_length(z, 0)` | **125000** | — |
| `zone_slice_length(z, 63)` | **125000** | — |
| `get_sample_length(z)` | **9600000** | 460800 frames |

So both the slice positions and the sample length answer in **microseconds**
(`frames × 10⁶ / rate`), the unit every other KSP time uses; that is also the
only reading under which the corpus's own arithmetic works —
`zone_slice_start(...) / get_sample_length(...)` is a 0..1 position (Spitfire
`slot-1.ksp`, `getLoopData`), and a frames-over-microseconds ratio would not be.
The engine converts at the accessor, as it does for `$NOTE_DURATION`.

## 3. `zone_slice_length` is derived, so it is bound but not answered

The stored grid has positions; Kontakt's length is **not** the distance to the
next marker. On the Vocalise zone the markers are 150000 µs apart while every
slice answers 125000 µs — a 1/16 cell at the session's 120 BPM (125 ms), i.e. a
tempo-grid quantity. Reproducing it needs the beat/tempo model the engine does
not carry, so the symbol is bound (the two corpus slots compile) and the host
reports the action unanswered. Its two users are Folds and Strands 2.0, and one
of those uses is commented out.

## 4. The cost, corrected: the gate that failed was `EFFECT_TYPE`

Compiling these names does more than make slots pass: Spitfire's 15k-line
framework (`slot 2`, "Instrument") was rejected by the compiler before, so KONTRA
played those patches without it. It then ran, and **silenced them**:
`b - Horns a4 - Decorative techniques` went from peak 0.170 to 0.000, and so did
`a - Horn Solo` and `b - Horns a4`.

The earlier note here said `$ENGINE_PAR_FILTER_*` was "not in the
engine-parameter model" and every read was `-1`. **That was wrong.** All eleven
filter names are bound (`crates/format/kontakt/src/script_knobs.rs:346-398`) as
`ir::SlotKnob` targets, `$ENGINE_PAR_CUTOFF`/`RESONANCE` as group knobs, and the
getter answers the stored knob through the slot's laws. Measured by running a
probe script in the instrument (appended through `KONTRA_SCRIPT_SOURCE`), group 0
insert 7 of that patch reads back

```
type=0 sub=19 by=1 cut=60 res=41 ta=0 wa=0 sb=0 rb=0 rc=0
```

and every value but `type` is what Kontakt returns. The tags are real: the file
stores group 0's slot 7 cutoff as the f32 `0.000060` (and group 1's as
`0.000126`), so Kontakt's 0..1e6 readback is 60 and 126.

The one wrong answer is `$ENGINE_PAR_EFFECT_TYPE`, which the engine left
unanswered (0) for every slot. Spitfire's framework snapshots a group's tags only
when the slot passes

```ksp
get_engine_par($ENGINE_PAR_EFFECT_TYPE, g, slot, -1) = $EFFECT_TYPE_FILTER
```

so with 0 the whole snapshot was skipped and every group stayed untagged:
100 `disallow_group($ALL_GROUPS)`, 0 `allow_group`, silence.

Kontakt 8.14.1 measured the answers directly (`tools/oracle-mcp`, 2026-11-02): a
Versatile filter set into group 0 insert 7 through `set_engine_par` reads
`EFFECT_TYPE` **7**, `EFFECT_SUBTYPE` **19**, `EFFECT_BYPASS` **1**, and
`CUTOFF`/`RESONANCE`/`TYPEA`/`BYPA`/`SHIFTB`/`RESB`/`RESC` **exactly the values
set** (41 000, 73 000, 100 000, 500 000, 200 000, 300 000, 400 000) — no law is
applied on readback; the stored 0..1 knob × 10⁶ is the answer. The engine now
answers `EFFECT_TYPE` from the slot's `ir::EffectKind` (every filter family —
EQ bands, formant and clocked modules included — is `$EFFECT_TYPE_FILTER`), and
leaves the module ids it cannot name unanswered rather than guessing.

Result, with the baseline **not** consulted: `b - Horns a4 - Decorative
techniques`, key 57, is again within **0.03 dB level / 0.13 dB band RMS** of
Kontakt's reference. The old peak of 0.170 was *not* correct — it was the
framework skipped, every zone playing, **+29.7 dB louder than Kontakt**. The
corpus's recorded baseline (`level_db -29.72`) was that bug's fingerprint, so the
case moves when it is fixed; `tools/parity-corpus/` records the new baseline as
the documented `--record` promotion, not as a threshold change.

The same read path is what Vocalise 3, Strands 2.0 and Folds needed to compile;
none of them gates its groups this way, and all three sound after the change
(key 60 peaks 0.076, 0.259 and 0.960) — their silence had no separate cause.

The gate's three cases in `tools/verify.sh` are unaffected (`VERIFY PASSED`), and
Electric Mint's fingerprint is unchanged (`f18acaab5b897588`).

## 5. Compile sweep

`kspcheck` over the same 8,016 extracted slots as `docs/re/ksp-bindings-2026-11-02.md`:

| | failing slots | instruments |
|---|---:|---:|
| `feat/integration` 604345ef | 404 | 404 |
| this branch | **8** | 2 |

`comm` on the slot sets: 404 → 8 with **no slot newly failing**; the 8 that
remain fail on `set_sample` (3), `%NI_USER_ZONE_IDS` (1) and three instruments'
own undeclared variables (`?main_controls_inst_panels_1_icon_panel_stage`,
`?home_vr_xypad`, one `more values than array elements`).
