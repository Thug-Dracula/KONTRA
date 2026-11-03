# The last four KSP binding gaps — 2026-11-02

`docs/re/ksp-bindings-2026-11-01.md` §3 left four names of the same family open,
all of them first errors in the compile sweep: `_pgs_changed` (42 slots),
`dont_use_machine_mode` (25), `mf_get_note_length` (7), `set_num_user_zones` (4).
Three are closed, one is bound at the command only, and the measurement says
which.

**478 failing script slots in 470 instruments → 404 in 404.** 74 slots and 66
instruments moved from failing to compiling; **none** moved the other way
(`comm` on the before-`ok` and after-`err` slot sets is empty). Every number
below comes from compiling the same extracted corpus, so none of it needs
Kontakt to check.

Nothing audible changed: `tools/verify.sh` prints `VERIFY PASSED`, Electric
Mint's fingerprint is still `f18acaab5b897588`, and `tools/parity-corpus/run.py`
is 20/20 with every case `moved +0.00/+0.00`.

## How this was measured

The harness is the gap lane's, unchanged: `/var/tmp/gaps` holds the 8,016 slots
extracted from the 3,694 loaded instruments, and `kspcheck` links the
workspace's `kontra-ksp` by path and calls `kontra_ksp::compile(src, &[])` on
each. `compile-before-funcs.tsv` is the baseline, built from the branch point
`f2d8c2c7c` (it reproduces the recorded 478/470 exactly); `compile-after-funcs.tsv`
is this branch.

```sh
ls /var/tmp/gaps/scripts/*/slot-*.ksp | tools/census/kspcheck/target/release/kspcheck > compile.tsv
awk -F'\t' '$1=="err"' compile.tsv | wc -l     # 478 before, 404 after
CENSUS_WORK=/var/tmp/gaps-funcs python3 tools/census/instruments.py
```

`instruments.py` reads fixed filenames (`compile-before.tsv`,
`compile-after2.tsv`) despite its docstring, so the two runs were symlinked
under those names into `/var/tmp/gaps-funcs` rather than replacing the pass's
own files.

Before → after, per gap:

| gap | slots before | slots after | what it is |
|---|---:|---:|---|
| `unknown callback _pgs_changed` | 42 | **0** | Kontakt 2's spelling of `on pgs_changed` |
| `unknown command dont_use_machine_mode` | 25 | **0** | a Zone Command, a no-op where nothing has a machine mode |
| `unknown function mf_get_note_length` | 7 | **0** | the note length, in ticks, under the MIDI object's cursor |
| `unknown command set_num_user_zones` | 4 | 4, now `set_sample` (3) and `%NI_USER_ZONE_IDS` (1) | the command is bound; the user-zone model is not |

## 1. `mf_get_note_length` — and the interface it was holding shut

The KSP reference does not list the function. Checked in the 7.10 manual: the
MIDI object chapter documents `$EVENT_PAR_NOTE_LENGTH` through
`mf_get_event_par()`, and the version history's list of the MIDI file commands
(`mf_get_first`, `mf_get_next`, `mf_get_pos`, …) does not include it. What it is
therefore comes from the corpus, which is unusually clear:

| call site | shape |
|---|---|
| Garage Kit - Full / Lite, Session Kit - Full / Lite, Stadium Kit - Full / Lite (NI Studio Drummer 1.4.1) | `$mf_note_length := ticks_to_ms(mf_get_note_length())`, then `play_note(…, $mf_note_length)` |
| Mike Mangini (Mixwave) | `%_midi_len[$_i10] := mf_get_note_length()`, later `mf_set_event_par($id, $EVENT_PAR_NOTE_LENGTH, %_midi_len[…])` |

Six sites pass it to `ticks_to_ms()`, whose manual entry is "converts a
tempo-dependent MIDI ticks value into a value in microseconds"; the seventh
stores it into `$EVENT_PAR_NOTE_LENGTH`, the parameter `mf_get_event_par()`
reads. Both say the same unit: **ticks**. It takes no arguments at any of the
seven.

What it does is read the event the position marker sits on — the same event
`mf_get_command()`, `mf_get_byte_one()` and `mf_get_byte_two()` read — and
answer that event's note length. **Kontakt's answer for an event that is not a
note is neither documented nor measured here.** This answers the 0
`$EVENT_PAR_NOTE_LENGTH` already carries for a non-note in our model, rather than
inventing a second convention. The probe that would settle it is one script in
the oracle: insert a MIDI file with a note, a CC and a note with no Note Off,
walk it with `mf_get_next()` and `message()` the three values.

The acceptance case, `Mixwave - Mike Mangini/Instruments/Mike Mangini.nki`
(18,052 lines, 1,249 UI declarations):

| `kontra-inspect load` | before | after |
|---|---|---|
| UI models | 0 | **1** |
| compiled callback / dispatch entries | 0 | **972** |
| referenced UI assets | 0 resolved / 0 not resolved | **211 resolved / 9 not resolved** |
| diagnostics | 2 — `MIXWAVE DRUMFRAME: unknown function mf_get_note_length at %_midi_len[$_i10] := mf_get_note_length()`, impact Broken; a cosmetic bypassed Tape Saturator | 1 — the Tape Saturator |

`ui-shots` before writes no file at all (the instrument has no pages to draw);
after it draws the whole kit page: 1,248 widgets, 143 shown, 220 pictures,
`Mike_Mangini-0.png` — the kit art, the KIT VIEW / MIXER 1 / MIXER 2 / SETTINGS /
ABOUT tabs, and the clickable control dots over the drums.

## 2. `on _pgs_changed` — a spelling, not a second callback

Kontakt 2 spelled builtins with a leading underscore, and `Builtin::from_name`
has always accepted that for names (`_read_persistent_var` is the one the
tests name). The 36 corpus instruments that write `on _pgs_changed` also write
`_create_key`, `_get_key_val`, `_set_key_val` and `_read_persistent_var` — the
same dialect, in the same scripts. The manual's entry for the callback is the
one both spellings mean: *"executed whenever any `pgs_set_key_val()` command is
executed in any script slot."*

So `callback_kind()` maps `"_pgs_changed"` to `CallbackKind::PgsChanged` and
nothing else changes: the callback is queued a sample after the write and
coalesced per sample by the queue that already existed
(`an_underscore_pgs_changed_callback_runs_once_per_write_batch` writes the key
twice in one callback and asserts one run). The alias is that one name, not a
rule: `on _note` is still `unknown callback _note`.

## 3. `dont_use_machine_mode(<id-number>)` — a no-op that is asked for

The manual's only mention (Zone Commands, in the command list; it has no
detailed page): *"Play the specified event in Sampler mode (only makes sense
when the groups allowed for playback are in one of Machine modes)."* Nothing in
a KONTRA plan carries a machine mode — every group plays in sampler mode — so
the request describes the state that already holds. The call compiles, reaches
the host, and the host answers it with nothing (`Action::DontUseMachineMode`),
on the same footing as `hide_inst_icon`: answered, not counted as unanswered.
All 25 call sites pass an event id (`$EVENT_ID`, `$new_note`, `$temp_id`);
none is a bare call.

## 4. `set_num_user_zones(<value>)` — bound, and honestly not modelled

The manual: *"Creates empty user zones. … `%NI_USER_ZONE_IDS` is the array of
size `<value>` with all the user zone IDs."* The array is the point of the
command, and a script-sized system array is not something this front end can
declare: `%NI_USER_ZONE_IDS` has no compile-time size, and answering it with
the manual's 1024-zone maximum would make `num_elements()` return 1024 where
Kontakt returns the count — a wrong number that a script acts on.

So the binding stops at the call. `set_num_user_zones(3)` compiles, reaches the
host, and the engine reports it unanswered (there is no user-zone model to
create anything in). **The 4 slots it was counted for do not compile, before or
after**: three go on to fail on `set_sample` and one on `%NI_USER_ZONE_IDS is
not an array`. The inventory's "4 slots fail for this reason alone" was the
first-error count, not an isolation test — measured, it is wrong in the
direction that matters. What closes it is the user-zone model: the array, zone
creation, `set_sample`/`get_sample` and the parameters of a zone with no
sample — the same `AttachZone`/zone-slice family the inventory ranks separately.

## 5. What is left

| slots | first error | what it is |
|---:|---|---|
| 362 | `unknown function num_slices_zone` | the zone-slice model (Spitfire Studio Brass, Vocalise 3) |
| 34 | `unknown function zone_slice_start` | the same model |
| 3 | `unknown function set_sample` | the user-zone model, above |
| 1 | `%NI_USER_ZONE_IDS is not an array` | the same |
| 3 | `?…_stage`, `?home_vr_xypad is not declared` | undeclared text variables |
| 1 | `more values than array elements` | KSP lets the surplus be ignored |

Everything in the front-end binding family is now closed. The remaining work in
this file is two models, not names.
