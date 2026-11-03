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

## 4. The cost, measured: the Spitfire framework now runs

Compiling these names does more than make slots pass: Spitfire's 15k-line
framework (`slot 2`, "Instrument") was rejected by the compiler before, so KONTRA
played those patches without it. It now runs, and **it silences them**:
`b - Horns a4 - Decorative techniques` went from peak 0.170 to 0.000, and so did
`a - Horn Solo` and `b - Horns a4`.

The cause is measured, not the slices: the framework stores each group's
articulation index in the **filter** engine parameters
(`%3f0ck[group] := get_engine_par($ENGINE_PAR_FILTER_RESB, group, slot, -1) - 1`,
`slot-2.ksp:3057`), then `disallow_group($ALL_GROUPS)` and re-allows only groups
whose stored index matches the incoming note
(`slot-2.ksp:14439-14470`). `$ENGINE_PAR_FILTER_*` is not in the engine-parameter
model (`crates/format/kontakt/src/engine_pars.rs`, `FIXED`), so every index reads
`-1` and no group is allowed: 100 `disallow_group` calls, 0 `allow_group`. That
is an engine-parameter gap, not a slice one, and it belongs to that lane — the
values it needs are Kontakt's raw filter-parameter units, which the model
converts rather than stores.

`tools/parity-corpus/run.py` therefore fails `spitfire-horns-a4` (silent);
its baseline has **not** been moved. The gate's three cases in
`tools/verify.sh` are unaffected (`VERIFY PASSED`), and Electric Mint's
fingerprint is unchanged (`f18acaab5b897588`) — it calls none of these names.

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
