# Velocity selection: the ranges are read right, and Kontakt does not fall back

Measured 2026-11-03 on Kontakt 8.14.1 (the `oracle-mcp` rig) and on this tree.
The question: `Evolution Steel Strings` renders 0.192361 at key 60, velocity 100
in Kontakt, while a reading of the stored zones said **no** zone admits
velocity 100 — a mis-read of the four stored bounds, or a selection law we are
missing (nearest layer instead of membership)?

Neither. The bounds are read correctly, and Kontakt selects by membership. The
"no zone admits 100" reading is an artefact of asking about the *host's*
velocity on an instrument whose script replaces the note.

## What the file stores

`ZoneParams`' four bounds are `i16` little-endian at these offsets in the zone's
public record (`vendor/ni-file/src/kontakt/objects/zone_data.rs`):

| field | offset |
|---|---|
| `low_velocity` | 12..14 |
| `high_velocity` | 14..16 |
| `low_key` | 16..18 |
| `high_key` | 18..20 |
| `fade_low_velocity` | 20..22 |
| `fade_high_velocity` | 22..24 |

For one zone of `Evolution Steel Strings.nki` (group 2, 0x98):

```
00000000 00000000 de160000 0100 0100 0000 2900 0000 0000 0000 0000 2800 …
         start    end      mod   1    1    0    41   0    0    0    0    40
```

so `low_velocity`/`high_velocity` = 1/1, `low_key`/`high_key` = 0/41, root 40.

The distribution over the whole file (10,706 zones) is **not** a partition of
1..127: it is a ladder of single-velocity bands — `1..1` 405, `2..2` 401,
`3..3` 400, … `72..72` 50 — plus 45 zones at `1..126` and a scatter of wider
pairs (`70..127`, `84..127`, …). Nothing arrives at a partition; the widest
single band tops out at 126, so no zone in the file stores 127.

Velocity 100 is admitted by **45** zones somewhere in the file, and by exactly
**one** zone at key 60:

| zone | group | velocity | keys | root | frames | sample |
|---|---|---|---|---|---|---|
| 1580 | 27 | 1..126 | 60..60 | 60 | 271,860 | `Sympathetic Resonance/sym-res-21.ncw` |

## The reader is not wrong: Kontakt reports the same numbers

`tools/kontakt-oracle/probes/velocity_ranges.lua` asks Kontakt itself, zone by
zone, with `get_zone_geometry` / `get_num_zones`:

* `num_zones` **10,706** — the same count this tree reads (not 11,820).
* the pair histogram is the same, pair for pair: `1..1` 405, `2..2` 401, …,
  `1..126` 45.
* the zones whose key range holds 60: 1,313; the ones admitting velocity 100
  among them: **1**, and it is zone 1580 with `sym-res-21.ncw`, 271,860 frames.

The same check on `Electric Mint` (47,909 zones) reproduces Kontakt's histogram
exactly too, including `10..99` 32 (whose fades Kontakt reports as 57/33), which
is why the offsets above are trusted rather than assumed.

So a "no zone admits 100" report is not a fact about the file. It is what a
scan of the *host* note sees, and this instrument does not let the host note
reach selection (below).

## The selection law, measured with ranges set through Kontakt's own API

`tools/kontakt-oracle/probes/zone_selection_law.lua` takes a real instrument
(`05 Breaths.nki`, one zone per key) and re-ranges it through
`set_zone_low_key` / `set_zone_high_velocity` / …, which is the same record the
file stores. Two bands land on one key, each with a sample of a different pitch:

| zone | keys | velocities | root | sample |
|---|---|---|---|---|
| key 50's | 60..60 | 1..60 | 50 | `Pk_Ob_Breaths_53.ncw` |
| key 70's | 60..60 | 100..127 | 70 | `Pk_Ob_Breaths_13.ncw` |
| all others | 0..0 | unchanged | — | out of reach |

Rendered through `oracle_render` at key 60 (window peaks, 48 kHz):

| velocity | peak | dBFS | what it is |
|---|---|---|---|
| 30 | 0.000084 | −81.5 | inside 1..60 |
| 60 | 0.000143 | −76.9 | inside 1..60, top edge |
| 63 | — | — | (not rendered) |
| 64 | 0.000005 | −106.7 | one step past the low band |
| 80 | 0.000000 | −180.0 | between the bands |
| 99 | 0.000000 | −180.0 | one step below the high band |
| 100 | 0.009589 | −40.4 | inside 100..127, bottom edge |
| 127 | 0.013483 | −37.4 | inside 100..127, top edge |

At a velocity no zone admits Kontakt plays **nothing**: not the nearest band,
not the lowest, not the highest. Between the bands and one step outside each
edge is silence, and the same instrument rendered at a velocity inside a band
sounds. There is no fallback to express, so no law is invented here.

**The key axis behaves the same way.** With the zones above on key 60 only:

| note | peak |
|---|---|
| key 59, velocity 127 | 0.000000 |
| key 60, velocity 127 | 0.013483 |
| key 61, velocity 127 | 0.000000 |

A key one semitone outside every zone plays nothing, so key selection is
membership too — no nearest-key rule.

## Why this instrument still looks like "no zone admits 100"

`Evolution Steel Strings` does not select on the host's velocity. Its script
takes the note and re-plays it (`docs/re/ignored-note-2026-11-03.md` is the
sibling finding for the same pattern):

* `$pj1ah := $EVENT_VELOCITY` (line 17389 of the dumped script), then a velocity
  curve through the instrument's own tables (`%rnrgd`, `%4z2ox`), then
  `play_note($j3lhv, $pj1ah, …)` — the host's velocity is only an input to the
  instrument's curve.
* In KONTRA's run of that script the curve lands on **65** at host velocity 100
  (read from the zones that answered: both store `65..65`), so the zones that
  answer are the `65..65` ones — not the `sym-res` zone alone. The same script
  runs in Kontakt, but which value its curve produces there is not readable
  through the Lua API, so this is KONTRA's number, not Kontakt's.

`cargo run --release -p kontra-host --features trace --example sounding_zones --
"…/Evolution Steel Strings.nki" 60 100` names them:

```
zones prepared: 2042 of 10706 translated
peak 0.823260
zone 1729: group 28 member 52  keys 0..=127 velocities 65..=65  sample …/Strumming/f-strum-loud-1-05.ncw
zone 1652: group 27 member 20  keys 60..=60  velocities 1..=126  sample …/Sympathetic Resonance/sym-res-21.ncw
zone  704: group  2 member 704 keys 0..=60  velocities 65..=65  sample …/Loud Fingered Sustain/f-sus-loud-B-D1-01.ncw
```

So the engine already reaches the resonance zone the range question asked
about, plus the two 65..65 layers the script asks for. Measured the same way,
the host velocity no longer decides anything on our side either: our render at
velocity 127 also sounds (`0 covering zones`, 3 voices), as Kontakt's does.

## What KONTRA does, and what changed

`crates/engine/engine/src/start.rs` refuses a zone whose ranges do not contain
the note (`Reject::Velocity`, `Reject::KeyRange`) — membership, in the order the
engine tests it. That is Kontakt's law as measured here, so **nothing was
changed**. Two tests now pin it:

* `crates/engine/engine/tests/engine.rs::a_note_between_two_velocity_bands_starts_no_voice`
  — bands 1..5 and 100..127 on one key; velocities 6, 64 and 99 start no voice.
  Stubbing a fallback into `refusal` fails it at velocity 6 and leaves the
  pre-existing `a_zone_outside_the_velocity_range_is_silent` failing too.
* `crates/format/kontakt/tests/kontakt.rs::velocity_100_on_key_60_reaches_one_zone_and_it_is_the_sympathetic_resonance`
  — the file's 10,706 zones, the 45 that admit 100 anywhere, the single one at
  key 60, named by its sample, and no zone at all at 127.

The second is library-gated (skips where `~/Armory/VSTs/Kontakt1` is absent, as
the file's other tests do).

## Open, measured, not fixed here

1. **Level against Kontakt on the same note.** Key 60 velocity 100, the first
   0.45 s: Kontakt 0.0952 (−20.4 dBFS), KONTRA 0.806 (−1.9 dBFS) — **+18.6 dB**,
   and +26.3 dB at host velocity 5, +22.1 dB at 64. The gap shrinks as velocity
   rises, and even inside one band Kontakt's own level moves 40 dB from velocity
   30 to 100 — the shape of a **velocity → amplitude** law on top of selection.
   KONTRA's voice gain here is a flat 1.568 (the render's own voice dump), so it
   is a gain law, not a range law. The corpus case says the same thing the other
   way round over a whole note: `evolution-steel-strings` (key 62, 2 s hold)
   reads **−12.01 dB / 4.25 dB** because our note decays faster than Kontakt's.
   Louder at the attack, quieter over the note: whoever takes it needs the
   envelope and the curve together, measured, not a constant.
2. **A strumming zone on a single note.** Voice 1729 above is
   `Strumming/f-strum-loud-1-05` started by a one-note event. Whether Kontakt's
   script reaches that branch too is not decidable from the Lua API (it exposes
   zones, not voices); it needs either a spectral comparison against Kontakt's
   render or an instrument-level visible state.

## Reproducing

```sh
# Kontakt's own geometry: zone count, velocity pairs, the key-60 candidates
python3 tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
  "nki":"/home/dracula/Armory/VSTs/Kontakt1/Evolution Steel Strings 1.2.5 [Orange Tree Samples]/Instruments/Evolution Steel Strings.nki",
  "probe":"tools/kontakt-oracle/probes/velocity_ranges.lua"}}'

# the selection-law instrument (writes selection_law.nki next to the probe output)
python3 tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
  "nki":"/home/dracula/Armory/VSTs/Kontakt1/#Non NICNT/Parker Oboe [Musical Sampling]/Instruments/05 Breaths.nki",
  "probe":"tools/kontakt-oracle/probes/zone_selection_law.lua"}}'

# the stored pairs straight from the file, and which zones admit (60, 100)
cargo run --example dump_velocity -p kontra-format-kontakt -- "<instrument.nki>"

# which zones our engine starts, named by sample
cargo run --release -p kontra-host --features trace --example sounding_zones -- \
  "<instrument.nki>" 60 100
```

Sessions are one load each and were closed with `oracle_down`
(`kontakt_leftovers: []`); the synthetic copies live under `/tmp/opencode` and
are not part of the tree.
