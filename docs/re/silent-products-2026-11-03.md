# The 180 silent instruments — where each chain breaks, 2026-11-03

The census of 2026-11-01 recorded 180 instruments that have prepared zones and
make no sound at any of nine keys (36…84). This pass asks, for each one, **which
link in the chain from "the file has zones" to "the output has samples" is the
first one with no evidence behind it**, and answers with a measurement rather
than a hypothesis. Every number below was produced by
`tools/silence` on this branch (`feat/silence-diagnosis`, from
`feat/integration` at `c480dce50`) and is in the JSONL beside this file.

The headline is that the 180 are **not one bug**. Three measured mechanisms
account for 45 of the 97 instruments that still exist on disk; 23 more share a
single *shape* whose deciding branch this pass did not reach; 83 are not about
KONTRA at all (the library is deleted from the machine); 15 no longer reproduce
on today's build.

The base rate matters when reading what follows: 26 % of the 704 keyswept
instruments were silent, so a property shared by a quarter of the corpus
explains nothing. Each cause below is stated with the count of instruments it
accounts for **and** the number that would be expected from the base rate if it
explained nothing.

---

## 1. What was measured, and how

`tools/silence` loads the instrument the way `kontra-render` does, picks the
candidate key with the most prepared-zone coverage (so a silent row is never the
harness picking an unused key), plays one note at velocity 100 held 500 ms with
500 ms of tail, and samples the engine every 128 frames. Per instrument it
reports:

- zones translated by the reader and kept by the prepare pass, with the reason
  for each dropped zone;
- prepared zones whose key and velocity ranges contain the note;
- the engine's own refusal record (`Engine::rejects`, a read-only diagnostic
  compiled into the `trace` build): the first condition that stopped each zone,
  in the order `start_zones` tests them — trigger, group mask, purge, key range,
  velocity, zone slot, bus, start rule — plus notes a script ignored;
- voices started (`voice_start` trace events), zones that sounded;
- source frames consumed, and frames a voice had to render as silence because
  data was unavailable;
- the peak of each voice's gain matrix and the peak of the voice's own audio
  before its group's effects;
- the final stereo peak, the covering groups' gain/bus/filter, and every
  non-bypassed effect slot in the path that stores output gain 0.

```
CARGO_TARGET_DIR=/var/tmp/tgt-silent cargo build -p kontra-silence
/var/tmp/tgt-silent/debug/kontra-silence --manifest silent.txt --sample-frames 4800 \
    --json records.jsonl > records.tsv
```

`--probe` re-renders a `downstream` row with every zero-output slot at unity,
which is how the first cause below was confirmed rather than inferred.

Trace capacity aside, the tool cannot see inside a script (see §7) and does not
send controllers, key switches or a mod wheel; both limitations are in its doc
comment and are the reason §7 is unexplained rather than proven.

---

## 2. The 180, grouped by product

Ranked by instrument count. "Measured cause" is the first broken link the tool
recorded; "expected by base rate" is what a product of this size would show if
silence were unrelated to it (26 % of the keyswept population, applied as a
sanity check only — the honest comparison is between products).

| instruments | product | measured cause | evidence (numbers from this pass) |
|---:|---|---|---|
| 83 | Arabian Ethnic Orchestra [Strezov] | **not KONTRA**: the library is not on disk (deleted 2026-11-01) | `os.path.exists()` false for all 83 `.nki`; the tool reports `load_error` before any render |
| 17 | Kinetic Metal | **cause found, fixed**: a legacy Phaser slot storing output 0 / dry 1 was read as dry 0, muting the instrument rack | 15 voices, 721,858 frames, voice peak 0.0098, mix peak **0.000000**; probe with the slot at unity: **0.040616** |
| 13 | Accordions 2 Library | **cause found**: the script's group gate closes every group; the tag it reads (`get_engine_par`) answers 0 | 357/357 prepared, 15 covering, **0 voices**, `group_mask` on **357** zones, 1 note ignored, 1 `play_note`; with scripts silenced the same key peaks **0.040628** |
| 10 | Session Horns Pro v1.5.0 | 8 sound today (stale keysweep), 2 **unexplained**: script ignores the note | 51 covering zones, **0 voices**, 1 ignore, 0 `play_note` |
| 6 | Stradivari Violin 2.0.0 [NI] | **unexplained**: script ignores the note, plays nothing | 7–21 covering zones, 0 voices, 1–2 ignores, **0** `play_note` |
| 6 | Kinetic Toys | **cause found, fixed** (same Phaser slot as Kinetic Metal) | 10–17 voices, voice peak up to 5.65, mix peak **0.000000**; probe **0.49** |
| 5 | Brass Band Soloists [Fracture Sounds] | **cause found**: script's group gate; `get_engine_par(SEQ_LF_GAIN)` tag | 4,035 zones, 108 covering, 0 voices, **4,035** `group_mask` refusals, 2 `play_note`/2 ignores; scripts silenced → **0.560137** |
| 4 | Ilya Efimov - Bass Bundle | sound today (stale keysweep) | peaks 2.8–9.7 |
| 3 | 8DIO Misfit | **cause found**: zones are keyed 1..5 only, and the script ignores the note anyway | 1,400 zones, **280 per key for keys 1…5**; at key 60 → 0 covering; at keys 1…5 → 40 covering, still 0 voices (1 ignore); sample head 1.2e-07 |
| 3 | Heavyocity Gravity 2 | **cause found**: script's group gate; `get_event_par_arr($EVENT_PAR_CUSTOM,1)` is the group tag and reads 0 | 1,071 zones, 44 covering, 0 voices, **1,071** `group_mask`; scripts silenced → **0.698173** |
| 3 | Session Ukulele Library | **unexplained**: script ignores the note | 2 covering, 0 voices, 1 ignore, 0 `play_note` |
| 3 | Scarbee Clavinet Pianet Library | **cause found (link)**: the note a script plays is not in any zone's key range | 3,495 zones, 8 covering, 0 voices, 1 `play_note`, **4,551** `key_range` refusals in the note-on block |
| 2 | Ilya Efimov - Total Guitar Bundle | sound today | peaks 1.7–1.8 |
| 2 | Session Bassist Icon Bass | **unexplained**: script ignores the note | 10 covering, 0 voices, 1 ignore, 0 `play_note` |
| 2 | Guarneri Violin Library | **unexplained**: script ignores the note | 7–21 covering, 0 voices, 2 ignores, 1 `play_note` |
| 2 | Session Bassist Upright Bass | **unexplained**: script ignores the note | 2 covering, 0 voices, 1–2 ignores, 0 `play_note` |
| 1 | Lores | **cause found (link)**: every voice's gain is exactly 0 | 56 voices, 1.9 M frames read, `voice_gain_max` **0.0**, group gain 0.2479, sample head 2.4e-05 |
| 1 | Session Guitarist - Picked Acoustic | **cause found (link)**: no zone matches key 60 at velocity 100, and the script ignores the note | 3,482 prepared, 33 admit velocity 100, 0 cover key 60; 1 ignore, 0 `play_note` |
| 1 | Session Guitarist - Electric Ruby Deluxe [NI] | **unexplained**: script ignores the note | 4 covering, 0 voices, 1 ignore, 0 `play_note` |
| 1 | Evolution Steel Strings | **unexplained**: script ignores the note, and 0 zones admit velocity 100 | 1 covering at velocity 100, 0 voices, 2 ignores, 1 `play_note` |
| 1 | Session Bassist Prime Bass | **unexplained**: script ignores the note | 2 covering, 0 voices, 1 ignore, 0 `play_note` |
| 1 | Session Guitarist Electric Sunburst Deluxe | **unexplained**: script ignores the note | 3 covering, 0 voices, 1 ignore, 0 `play_note` |
| 1 | Evolution Bluegrass Banjo | **unexplained**: script ignores the note; 0 zones admit velocity 100 | 0 covering, 1 ignore, 1 `play_note` |
| 1 | Session Guitarist Picked Nylon | **unexplained**: script ignores the note | 2 covering, 0 voices, 1 ignore, 0 `play_note` |
| 1 | Evolution Django Jazz | **unexplained**: script ignores the note; 0 zones admit velocity 100 | 0 covering, 2 ignores, 1 `play_note` |
| 1 | Evolution Flatwound Bass | same as Django Jazz | 0 covering, 2 ignores, 1 `play_note` |
| 1 | Scene - Lotus [NI] | **cause found (shape)**: script's group gate | 16 covering, **519** `group_mask`, 0 `play_note` |
| 1 | Evolution Jazz Archtop | **unexplained**: script ignores the note; 0 of 11,820 zones admit velocity 100 | 11,820 prepared, **0** with a velocity range containing 100; at velocity 5, 26 cover and the note is still ignored |
| 1 | Evolution Dracus | same family | 0 covering at velocity 100, 2 ignores, 1 `play_note` |
| 1 | Session Guitarist - Electric Vintage | **unexplained**: script ignores the note | 11 covering, 0 voices, 1 ignore, 0 `play_note` |
| 1 | Soundiron - Voices Of Rapture | **cause found (link)**: voices run, gain exactly 0 | 2 voices, 47,872 frames, `voice_gain_max` **0.0**, group gain 1.0, sample head 0.0205 |
| 1 | e-instruments Session Keys Electric S | sounds today | peak 0.170 |

Outcome totals over the 180: **83 absent** (library gone), **38 sound** on today's
build (15 stale keysweep rows + 23 fixed in §3), **59 still silent**:
22 group-gate, 23 script-eats-the-note, 9 key/velocity gating, 3 scripted note
out of range, 2 gain-zero.

---

## 3. Cause A — a legacy Phaser slot's stored dry level (23 instruments, fixed)

**Products:** Kinetic Metal (17), Kinetic Toys (6). This is the only cause in
this report with a landed fix.

**The chain.** The tool reported `downstream`: voices start, read hundreds of
thousands of source frames, and the voice audio leaves them at up to 5.65 peak,
but the final mix is exactly 0.000000. The racks name the culprit themselves:
`zero_output_slots` is
`instrument/3 "Phaser" output 0 dry 0` in every one of the 23.

The stored slot state (`BParFX`, 22 bytes) for that slot, read from the file
with a temporary dump, is:

```
instrument insert slot 3: chunk 0x14 version 0x50 stored_output 0 stored_dry 1
```

— a legacy Phaser set to **output 0, dry 1**: a slot passing its dry signal and
no wet, which is the normal way to leave a modulation effect in the chain but
off. Our reader classified 0x14 as a *serial* effect and forced `dry = 0`
(`effects.rs`, the `parallel` set was `0x10..=0x12 | 0x15 | 0x16 | 0x59 | 0x5a`),
so the slot became `0 × input + 0 × wet = 0` and multiplied the whole instrument
rack by nothing.

**Causal probes, not guesses.**

1. `--probe` (the same render with that slot's output set to unity) moves the
   mix from 0.000000 to **0.040616** on Underground Ventilation.
2. Across the patch set: 146 loaded Kinetic Metal + Kinetic Toys instruments,
   **23 `downstream` (all with the same slot) and 123 `ok`** before the fix;
   **146 `ok` after**. The 123 that already sounded include 65 with a
   zero-output slot of their own — but of the Delay/Reverb kind, whose stored
   dry is 1.0 and which the reader already honours. That split is what localises
   the fault to the type list, not to "output 0 in general".

**The fix** (`crates/format/kontakt/src/effects.rs`, `feat/silence-diagnosis`,
separate commit): the stored dry of a 0x14 slot is kept, exactly as its siblings
the legacy Chorus (0x11) and Flanger (0x12) already were. The regression test
`a_legacy_phaser_slot_keeps_its_stored_dry_level` fails on the old predicate and
passes on the new one (both runs recorded below).

**What this does not prove.** That Kontakt itself keeps the dry level for 0x14
is inferred from the file's own numbers (output 0 + dry 1 is coherent only as a
passthrough) and from the product being a shipped instrument, not measured
against Kontakt. The probe that would close it is one oracle capture of any of
the 23; the oracle script drives Kontakt's Options dialog and was not run here.

**Base-rate check.** 23 instruments across 2 products is 16 % of the 4,800
presets in those two products (146 of them load here), not a corpus-wide
property.

---

## 4. Cause B — a script closes every group in `on note` (22 instruments)

**Products:** Accordions 2 (13), Brass Band Soloists (5), Heavyocity Gravity 2
(3), Scene - Lotus (1).

The shape is the same in all 22: the host note is ignored, a replacement note is
started by `play_note`, and **every zone is refused with `group_mask`** — the
`allow_group`/`disallow_group` mask the script built for that note. The zone and
sample data are fine: with the scripts silenced (`KONTRA_SCRIPT_SOURCE` pointed
at empty slots) the same key renders

| instrument | zones | covering at key | group_mask refusals | scripts silenced |
|---|---:|---:|---:|---:|
| Steirisches Accordion | 357 | 15 | 357 | **0.040628** |
| 01. Solo Cornet | 4,035 | 108 | 4,035 | **0.560137** |
| Gravity 2 Menu | 1,071 | 44 | 1,071 | **0.698173** |

So the loss is the mask, not the samples. What the mask is built from differs
per product — this is one *shape*, three accessors:

- **Accordions 2** (`0.ksp`, `InstrumentInit`): `%yrudk[get_engine_par($ENGINE_PAR_START_CRITERIA_CC_MIN, g, 0, -1)] := g`,
   then in `on note` a `disallow_group($ALL_GROUPS)` followed by 15
  `allow_group(%yrudk[...] * get_control_par(...))` calls. With the
  `START_CRITERIA_*` answer at 0 the array stays all zeros, so every call
  resolves to `allow_group(0)` — and group 0 of that instrument is the empty
  keyswitch group.
- **Brass Band Soloists** (`1.ksp`): the mask is `allow_group($g)` inside
  `if (get_engine_par($ENGINE_PAR_SEQ_LF_GAIN, $g, 0, -1) = $w1xhc and …)`. The
  library uses that engine parameter as a *tag* (which dynamic layer a group
  belongs to). `init: unanswered []` in the render, so the call is answered; what
  it answers is not the stored tag, and no group matches.
- **Heavyocity Gravity 2** (`1.ksp`): `allow_group(get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 1))` —
  the group is named by a custom event parameter of the note. With that
  parameter at 0, group 0 is allowed and nothing else.

**Base-rate check.** These products are 0.6 % of the keyswept corpus, so the
22 instruments are not a coincidence. All 13 of Accordions 2's keyswept
instruments are in this group: 100 % of the product.

**Probe that would settle each sub-mechanism.** Answer one
`get_engine_par($ENGINE_PAR_SEQ_LF_GAIN, g, 0, -1)` from the file's group record
and compare with the script's expected tag; and read
`get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 1)` for a note the script
played. Both are already traced as actions; the values are not.

### 4a. Measured in Kontakt 8.14.1, 2026-11-03

The three reads above were measured through `tools/oracle-mcp` (one disposable
Kontakt per load, `oracle_down` after) and compared with KONTRA's own answer on
the same instrument. Probes:

| probe | what it builds |
|---|---|
| `tools/kontakt-oracle/probes/start_criteria_law.lua` | a synthetic instrument whose four groups carry known start options, plus a group with none |
| `tools/kontakt-oracle/probes/start_criteria_patch.lua` | a copy of `Steirisches Accordion.nki`, the named groups read in place |
| `tools/kontakt-oracle/probes/solid_geq_readback.lua` | a synthetic instrument with the factory INIT Solid G-EQ, four script passes (read, write, read back, shelves) |
| `tools/kontakt-oracle/probes/solid_geq_patch.lua` | `01. Solo Cornet.nki`, the first six groups' Solid G-EQ read in place |
| `tools/kontakt-oracle/probes/event_custom_law.lua` | a synthetic instrument whose script answers one event-slot question per key, saved and rendered in the main session |

**Start criteria — the synthetic law.** `get_engine_par($ENGINE_PAR_START_CRITERIA_*)`
answers **row 0** of the group's record, field for field:

| group | mode | next | key min..max | controller | cc min..max | zone / slice | seq |
|---|---:|---:|---|---:|---|---|---:|
| controller 2, cc 0..=64 (set) | 2 | 0 | 24..24 | 2 | 0..64 | -1 / -1 | 0 |
| key 36..40 + controller 4, cc 10..=90 (set) | 1 | 0 | 36..40 | 1 | 0..64 | -1 / -1 | 0 |
| key 48..48 (set) | 1 | 0 | 48..48 | 1 | 0..64 | -1 / -1 | 0 |
| no rows at all | 0 | 0 | 24..24 | **1** | **0..64** | -1 / -1 | 0 |

A row's unused fields hold the record's defaults, and so does a group with no
stored row: **controller 1 and cc 0..=64, not zero**. KONTRA's reader produced
the same rows for the saved file (all values identical), except that a group
with no row answered all zeros — see the fix below.

**Start criteria — the real patch.** `Steirisches Accordion.nki`, group names
as Kontakt reports them; KONTRA's reader (same file) in the last column:

| group | Kontakt mode | controller | cc min..max | KONTRA reader |
|---|---:|---:|---|---|
| `SUS_VEL1_RR1` | 2 | 2 | 0..64 | same |
| `SUS_VEL2_RR1` | 2 | 2 | 4..64 | same |
| `BUTTON_RR1` | 2 | 4 | 0..64 | same |
| `RELEASE_RR1` | 2 | 3 | 0..64 | same |
| `KS` | **2** | 1 | 11..0 | same |

The handoff's "keyswitch and filler groups mode 1" is **wrong**: the keyswitch
group `KS` stores mode 2 (a controller row whose range 11..=0 lowers to no
rule). Kontakt and KONTRA both answer 2, so no code changes; the prose does.

**Solid G-EQ — the synthetic law.** With the factory INIT preset in group 0's
insert slot 0:

| call | Kontakt answer |
|---|---|
| `SEQ_LF_BELL` / `SEQ_HF_BELL` on the INIT preset | 0 / 0 |
| after `set_engine_par(SEQ_LF_BELL, 2)` and `(SEQ_HF_BELL, 1)` | 1 / 1 (a switch stores 1 for any value ≥ 1) |
| after both set to 0 | 0 / 0 |
| `SEQ_LF_GAIN` after setting 123456 | 123456 |
| `SEQ_LMF_GAIN` after setting 700000 | 700000 |
| `SEQ_LMF_Q` after setting 100000 | 100000 |
| `SEQ_LMF_FREQ` after setting 300000 | 300000 |
| `SEQ_HF_GAIN` after setting 654321 | 654321 |
| `SEQ_HF_FREQ` after setting 250000 | 250000 |

So `_GAIN` reads the band's **gain** field, not the Q (100000) or the Hz
(300000) the gain's law also moves. A save/reload kept 654321 and 123456, and
KONTRA reads those two fields at the same record offsets (33 and 0; v0x11's
record is the v0x10 one plus ten bytes).

**Solid G-EQ — the real patch.** `01. Solo Cornet.nki`, groups 0..5, the five
calls in order `LF_GAIN, LMF_GAIN, HF_GAIN, LF_BELL, HF_BELL`:

| group | Kontakt | KONTRA (same file, same calls) |
|---|---|---|
| 0 | 500000, 500000, 500000, 0, 0 | same |
| 1 (`---CLOSE---`) | 500000, 500000, 500000, 0, 0 | same |
| 2 (`---SHORT1---`) | 9, 500000, 500000, 0, 0 | same |
| 3 (`CLOSEShort1RR1`) | 9, 1, 500000, 0, 1 | same |
| 4 (`CLOSEShort1RR2`) | 9, 2, 500000, 0, 1 | same |
| 5 (`CLOSEShort1RR3`) | 9, 3, 500000, 0, 1 | same |

The tags the gate compares are exactly these: LF gain 9 (the dynamic layer), the
LMF gain an RR index 1..3, and HF is a peak band (`HF_BELL` 1).

**`$EVENT_PAR_CUSTOM` — the law.** A note event is the only place `$EVENT_ID` is
legal, and a disposable probe Kontakt launched with a Lua script hears no MIDI,
so the answers were made audible instead: a saved synthetic instrument sets the
instrument's volume to unity when the condition holds and to silence when it
does not. One render, five keys:

| key | condition | window peak |
|---:|---|---:|
| 60 | `get(symbol, 1) = 4242` after `set(symbol, 4242, 1)` | 0.025094 |
| 62 | `get(28, 1) = 4242` after the same set — `$EVENT_PAR_CUSTOM` is **28** | 0.025095 |
| 64 | `get(symbol, 0) = 0` on a fresh event | 0.025090 |
| 65 | `get(symbol, 0) = 7` after `set(28, 7, 0)` | 0.025096 |
| 66 | **negative control**: `set(symbol, 4242, 1)` then ask `= 1234` | **0.000000** |

The control is silent, so the four audible windows are the conditions holding,
not the sample playing anyway.

**The one divergence, and its fix.** A group with no stored start-criteria row
answered all zeros in KONTRA where Kontakt answers the record's defaults
(mode 0, key 24..=24, controller 1, cc 0..=64, zone and slice -1). The engine
now answers that record (`NO_START_CRITERION` in
`crates/engine/engine/src/params.rs`), and
`a_script_reads_a_groups_start_criteria_eq_bell_and_event_slots` reads a group
with no row as well as one with a row: it fails before the change
(`…,0,0,0,0,…`) and passes after (`…,0,1,0,64,…`). No product is affected —
every group of every Accordions 2 instrument stores a row — so `tools/silence`
reports the same figures:

| product | instruments | voices | peak |
|---|---:|---|---|
| Accordions 2 (`--cc 2=64`) | 13/13 sound | 3..11 | 0.0249..0.3666 |
| Brass Band Soloists | 5/5 sound | 3 | 0.0541..0.1165 |
| Heavyocity Gravity 2 | 3/3 sound | 1..3 | 0.0107..0.2280 |

Same on both sides of the change, so the measurement did not move the sound of
anything that already played; what it moved is what the engine *answers*.



---

## 5. Cause C — key and velocity gating (9 instruments)

**Products:** 8DIO Misfit (3), Evolution Bluegrass Banjo / Django Jazz /
Flatwound Bass / Jazz Archtop / Dracus (5), Picked Acoustic (1).

Two different measurements end up in this bucket because both are "no prepared
zone can be selected by the note that was played":

1. **8DIO Misfit** — the zones are keyed **1…5** in the file: 1,400 zones, 280
   per key, no zone at any keysweep key. At keys 1…5 the tool finds 40 covering
   zones and still gets 0 voices, because the script ignores the note (1 ignore,
   3–4 `play_note` calls). So the key range explains the keysweep's silence at
   36…84 but not the instrument's own behaviour at 1…5.
2. **Evolution (5) and Picked Acoustic** — the zones carry **single-value
   velocity ranges** that cycle 1…60 per zone rather than banding the keyboard
   (Jazz Archtop: 11,820 prepared zones, **0** with a velocity range containing
   100; the ranges are `1..=1, 2..=2, … 60..=60`, 20 zones per key pair). The
   consequence is that no MIDI velocity above 60 can select any zone. This is
   either how the file stores a round-robin/index value in the velocity field or
   a misread of that field for this format version; the tool cannot tell which,
   and **no fix should be attempted until it can**.
   The control that says it is not simply "the instrument is quiet at velocity
   100": at velocity 5, 26 Jazz Archtop zones cover — and the note is still
   ignored by the script, so the instrument stays silent for a second reason.

**Probe that settles it.** Compare the raw zone record of one Evolution zone
(`low_velocity`/`high_velocity` as `ni-file` decodes them) against Cherry
Electric Bass, from the same vendor, whose prepared zones *do* band velocity
(1..31, 33..63, …, 45 zones each) and which sounds at velocity 100. If the field
is the same field and holds 1..60 in one file and bands in the other, the file
means something else by it and the reader needs a version branch.

---

## 6. Cause D — voices that run with zero gain (2 instruments)

**Lores** and **Voices Of Rapture**. These are not "no voice": the engine starts
voices, the play cursors advance (1,900,064 frames for Lores) and the gain matrix
is **exactly 0.0** at every sample. Group gain is 0.2479 (Lores) and 1.0 (VoR),
bus 1.0, so the zero comes from the zone's own gain or from a `Volume`
modulator — the tool reports which by exclusion, not by naming the route.

Lores also shows the shape of a purged library: 46,704 zones translated, 2,009
prepared, 44,695 dropped as `sample_only_referenced_by_purged_groups`.

**Probe that settles it.** One `Engine::inspect_routes()` snapshot on a sounding
voice: a `Volume` route whose `routed` value is 0 names the modulator; if every
route is 1, the zone's `gain` is where to look.

---

## 7. Still unexplained — the script eats the note (23 + 3 instruments)

**Products:** Stradivari Violin (6), Session Horns (2), Session Ukulele (3),
Icon Bass (2), Guarneri (2), Upright Bass (2), Electric Ruby Deluxe, Electric
Vintage, Picked Nylon, Sunburst Deluxe, Prime Bass, Evolution Steel Strings (1
each); and the Scarbee Clavinet trio (3) which breaks one step later.

The tool's record is exact about the first break and honest about where its
sight ends: **the host note is consumed by a script (`ignore_event`) and no
replacement note starts a voice**. Two shapes, nothing in between:

| shape | instruments | ignore_event | play_note |
|---|---:|---:|---:|
| note ignored, nothing played | 14 | 1 | 0 |
| note ignored, a replacement played but also dropped | 9 | 2 | 1 |

`notes_refused` is 0 in the session trace of Stradivari Violin, so `play_note`
did not fail to admit; in the 14 `play_note: 0` cases it was never called. The
Scarbee three are the same shape one step further on: `play_note` is called and
the child note's zones all refuse it on `key_range` (4,551 refusals in the
note-on block).

**What is *not* the cause.** Not timing: re-running the whole 23 with a 3 s hold
and 3 s tail changes nothing (23/23 still silent). Not the samples: for the
`play_note: 0` cases the chain never reaches a zone.

**Leading hypothesis, not proven.** These scripts branch on program state —
Stradivari's `on note` tests `$zygz := pgs_get_key_val(SEQUENCER, 0)` and
`$_art := pgs_get_key_val(ARTICULATION, …)` before it decides to ignore — and
this engine's PGS store starts empty (nothing in the format reader reads a PGS
chunk from the file). If the branch Kontakt takes depends on saved program
state, we take the other one. That would explain the shape (a script that
ignores in one branch and plays in the other) and the concentration in
NI-authored instruments.

**The probe that settles it.** Dump `pgs_get_key_val(SEQUENCER, 0)` and
`pgs_get_key_val(ARTICULATION, 0)` at the top of `on note` for one Stradivari
patch (a `message()` line via `KONTRA_SCRIPT_SOURCE` is enough), and compare with
the value the file stores; then set the key before the note and re-render. If the
value is 0 in both, this hypothesis is dead and the next probe is to print which
`on note` branch ran.

---

## 8. Not causes, measured

- **83 instruments are not on disk.** Every one is in
  `Arabian Ethnic Orchestra [Strezov Sampling]`, deleted 2026-11-01. The tool
  reports `load_error` for them before rendering; the census's 180 counted them
  while the library still existed. They are not evidence about KONTRA.
- **15 instruments sound on today's build.** The keysweep ran on 2026-11-01
  06:53; `feat/integration` has landed script bindings and the zone-slice reader
  since. Confirmed independently of the tool with `kontra-render`: Modern (Ilya
  Efimov Bass) peaks **7.093782** at key 55, where the keysweep recorded 0.
  The 15: Ilya Efimov Bass (4), Session Horns (8), Ilya Efimov Total Guitar (2),
  Session Keys Electric S (1).
- **`LoadArray` and `GetEnginePar` are still not causes**, as §5 of the gap
  inventory says. Accordions 2 *does* call `GetEnginePar`, but the number that
  explains its silence is 15 `allow_group` calls resolving to group 0, not the
  presence of the call.
- **The 23 fixed instruments are not a base-rate artefact**: 23 of 146 presets in
  two products, and 0 of the 123 that lacked the slot.

---

## 9. Reproducing

```sh
git -C /var/tmp/wt-int worktree add /var/tmp/wt-silent -b feat/silence-diagnosis feat/integration
cd /var/tmp/wt-silent
CARGO_TARGET_DIR=/var/tmp/tgt-silent cargo build -p kontra-silence

# the 97 silent-with-zones instruments still on disk
python3 - <<'PY'
import json, os
ks = [json.loads(l) for l in open('/var/tmp/gaps/keysweep-results.jsonl')]
sil = [k for k in ks if not k['sounds_at'] and k['zones_ready'] > 0 and os.path.exists(k['path'])]
open('/tmp/silent97.txt','w').write('\n'.join(k['path'] for k in sil) + '\n')
PY
/var/tmp/tgt-silent/debug/kontra-silence --manifest /tmp/silent97.txt \
    --sample-frames 4800 --json silent-products-2026-11-03.jsonl
```

The Kinetic Metal/Kinetic Toys before/after run is
`silent-kinetic-2026-11-03.jsonl` (146 rows, all `ok`; the pre-fix version of
the same run is in the commit message, 23 `downstream`).

---

## 10. Verdict

**Silence is not one bug; it is a short queue of four different failures with a
fifth still unmeasured, and one of them is now fixed.** The 97 instruments that
still exist split into four measured mechanisms — a reader law that muted a
whole rack through one effect slot's stored dry level (23, fixed, 0.000000 →
0.040616 proven by probe), a script idiom that closes every group in `on note`
through three different engine accessors that answer 0 where the library stored
a tag (22), key/velocity gating that leaves no zone selectable for the note
played (9), and a gain path that multiplies to exactly zero (2) — plus one shape
that 23 instruments share and this pass could only measure to the script's door:
the note is ignored and no replacement sounds. Eight of those 23 are the same
NI-authored instrument family with a PGS-driven branch, which is why the next
step is one probe of saved program state and not 23 separate fixes. The next
lane should therefore **fix one thing (the Phaser dry law, already on this
branch), then answer one question (PGS state), and only then build a queue** —
the queue is real, but it is five items long, not a hundred and eighty.

---

## Appendix — runs quoted in this file

| run | command | result |
|---|---|---|
| 97 silent-with-zones, on disk | `kontra-silence --manifest silent97.txt --sample-frames 4800` | 38 ok, 23 note_ignored, 22 group_mask, 9 key_not_covered, 3 key_range, 2 gain_zero |
| 146 Kinetic Metal + Toys, before fix | same tool, `kinetic.txt` | 123 ok, **23 downstream** |
| 146 Kinetic Metal + Toys, after fix | same tool, `kinetic.txt` | **146 ok**, peaks 0.0098–1.65 |
| Accordion / Brass / Gravity, scripts silenced | `KONTRA_SCRIPT_SOURCE=<empty> kontra-render` | 0.040628 / 0.560137 / 0.698173 |
| Modern (Ilya Efimov Bass) at key 55 | `kontra-render` | 7.093782 |
| 23 ignored at 3 s hold / 3 s tail | `kontra-silence --hold-ms 3000 --tail-ms 3000` | 23/23 still silent |
| phaser test before the fix | `cargo test -p kontra-format-kontakt a_legacy_phaser` | FAILED |
| phaser test after the fix | same | ok |
