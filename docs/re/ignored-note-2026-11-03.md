# The note `ignore_event` ate: the quick-start pane never shut

2026-11-03. `kontra-silence` reported 20-odd instruments across 13 products as
`note_ignored`: a script ignored the host note and no note of its own reached
selection. Nearly all of them are NI's Session framework (Session Guitarist,
Session Bassist, Session Ukulele, Session Horns), which is one script shape, so
this pass asked for **the condition that chose to ignore** rather than for a
fix per instrument.

The answer is one condition, and it is a value we answered wrongly:

> **`%welcome_state[$IS_MELODY]` is 0 because `load_array(%welcome_state, 1)` —
> the library's own data folder — answered "no file", so `on persistence_changed`
> re-opens the instrument's quick-start pane and the pane's `on note` branch
> ignores the note and plays nothing.**

Kontakt answers that load from `<Kontakt user data>/<library>/Data/welcome_state.nka`,
which holds `%welcome_state` = `0 1`; the second cell is what keeps the pane shut.

---

## 1. The probe: which branch ran

`Electric Ruby Deluxe (Melody)` at key 36 (the key `--key auto` picks) ignores
the note. Message at the top of `on note`, read back through
`KONTRA_SCRIPT_SOURCE` (now honoured by `kontra-silence`, which also reports the
last `message()` as `last_message`):

```
last message: HP=1 WS=0 CP=0 INIT=0
```

`HP` is `$help_pane_open_button`, `WS` is `%welcome_state[$IS_MELODY]`, `CP` is
`$current_page`. `on note` opens with

```
on note
ignore_event($EVENT_ID)
if ($help_pane_open_button=0)
    ... the whole body, including every play_note ...
end if
```

so with `HP=1` the callback is `ignore_event` and nothing else — exactly what
`kontra-render` counts: `calls by block 0: [(IgnoreEvent, 1), (EventId, 1)]`, one
`EventId` and no `EventNote`, i.e. the script never even read the key.

Two forced values settled the causality without touching the engine:

| probe (`KONTRA_SCRIPT_SOURCE`) | peak |
|---|---|
| nothing (as loaded) | 0.000000 |
| `$help_pane_open_button := 0` at the top of `on note` | **0.311** |
| `%welcome_state[$IS_MELODY] := 1` after `load_array(%welcome_state,1)` | 0.000000 |

The second probe plays. The third does **not**, because the value is written
after `on init` and the saved persistent values are restored at the end of the
callback — the script's own comment in the KSP manual:

> "When loading an array within the `on init` callback, please note that the
> array will implicitly be made persistent …, which results in loaded data being
> overwritten at the end of the callback."
> — [KSP manual, load/save commands](https://docs.native-instruments.com/ni-tech-manuals/ksp-manual/en/load-save-commands)

A message inside `on persistence_changed` read `PC WS0=0 WS1=0 MEL=1 FI=1`
while a message right after the assignment in `on init` read `INIT WS0=0 WS1=1`:
the restore put the 0 back.

## 2. What Kontakt does with the same instrument

One oracle session, `Upright Bass (Melody)`, key 48, velocity 100, one note:

| | peak |
|---|---|
| Kontakt 8.14.1 (`oracle_render`) | **0.310712** |
| KONTRA before | 0.000000 |
| KONTRA after this change | **0.311228** |

0.17 % apart, and the note the pane swallows is the same note.

The pane is not merely inferred: `oracle_shot` after the load is the full
instrument page (bass photograph, MELODY/PATTERN key list, keyboard strip). A
`%welcome_state[$IS_MELODY]` of 0 paints the quick-start pane instead, which is
what `show_help_pane` does on the first init.

Where the file lives, and why the folder name is not the product name:

```
~/Armory/VSTs/Kontakt 8 Portable/UserData/Session Bassist - Upright Bass/Data/welcome_state.nka
%welcome_state
0
1
```

Kontakt's `Settings.cfg` in the same folder carries one section per registered
library, with the `ContentDir` it was added under. `Session Guitarist - Electric
Sunburst Deluxe` reads its data from a folder named `Session Guitarist - Electric
Storm Deluxe` — the registration from before NI renamed the product — which is
why the resolution matches on the content folder and not on the `.nicnt`'s
`<Name>`.

**The control case that proves the rule, including its failure mode.**
`Electric Sunburst Deluxe (Melody)` has `load_array(%welcome_state, 1)` too, and
its library's data folder is *not* where its registration points. Kontakt is
**silent** on it (peak 0.000000, rendered through the oracle) — and so are we,
before and after. The two implementations agree on a payload they both fail to
find; neither is a KONTRA defect.

## 3. The split: it is one framework, twice over

`grep` over the extracted scripts (`/var/tmp/gaps/scripts`, manifest
`/var/tmp/gaps/manifest.jsonl`) for the deciding construct:

| construct | instruments |
|---|---|
| `on note` = `ignore_event` then `if ($help_pane_open_button=0)` (NI Session framework) | 17, all of Session Guitarist / Bassist / Ukulele / Horns |
| `on note` = `ignore_event`, `$kav3p` / `$zhxtx` obfuscated copy of the same framework | same family, older builds (2022-2024) |
| `on note` = `ignore_event` then `pgs_set_key_val(…)`, `set_event_par(…)` (Orange Tree "Evolution") | 6, all of Evolution |
| `on note` = `ignore_event` then `set_event_par_arr($EVENT_PAR_CUSTOM, …)` (8DIO) | 3 |

The NI Session family is one framework (the same `on note`, the same
`on persistence_changed`, the same `$welcome_state`) and `load_array(…, 1)`
appears once per product: Session Guitarist ×5, Session Bassist ×4, Session
Ukulele ×3, Session Horns ×2. Evolution and 8DIO ignore the note for their own
reasons (below) and are **not** this bug.

## 4. The change

`load_array(<array>, 1)` reads the library's data folder. The folder is resolved
once per instrument (`crates/app/host/src/user_data.rs`):

1. `KONTRA_USER_DATA` names it, when set.
2. Otherwise the folder is beside an installed Kontakt — `<install>/UserData`
   for a portable install (`kontakt::candidates` covers the standard ones, and a
   bounded search of `$HOME` to depth 4 covers an extracted portable).
3. `<user data>/<registered name>/Data`, where the registered name is the
   `Settings.cfg` section whose `ContentDir`'s last component is this library's
   folder; the `.nicnt` product name is the fallback.

The bytes fill the array's cells through the same readers mode 2's resources use
(`data_array`, `text_data_array`) before the script runs, so an `on init` read
sees them — mode 1 is documented synchronous in `on init`.

**And written again after the saved values.** That second half is not decoration:
with only the load, `%welcome_state` reads `[0, 0]` (the saved entry is
`%welcome_state 0`, one value, and the restore repeats a short payload's last
value into the cells it does not name). Kontakt ends with `[0, 1]`, i.e. the
loaded value survives. So a mode-1 array's cells are written last
(`ScriptPlan::after_restore`).

**The general restore law was left alone on purpose.** The tempting fix is to
stop repeating the last value for every short payload — and it silences a
product:

| Flugelhorn, key 55, velocity 100 | peak |
|---|---|
| Kontakt 8.14.1 | 0.190226 |
| broadcast (unchanged law) | 0.173757, 2 voices |
| "fill only the cells named" | 0.048656, 1 voice |
| "skip an entry whose count differs" | **silent** (`group_mask` on 15448 zones) |

Flugelhorn's own arrays are mode-2 resources, whose tails *are* the saved zeros.
Rewriting the law to suit `%welcome_state` moved that product 0.17 → 0.05, so the
narrow change (mode 1 written after the restore) is the one that fits the
evidence; the general law stays as it was and is still unproven against Kontakt
for mode 2 arrays.

## 5. Per product, before and after

`KONTRA_USER_DATA` pointed at an empty folder reproduces the old behaviour
exactly (no mode-1 file is found), so the two columns are one binary on one
machine. Auto key, velocity 100, 500 ms hold:

| product | instrument | before | after |
|---|---|---|---|
| Session Guitarist – Picked Acoustic | Picked Acoustic (Melody) | `key_not_covered`, 0.000000 | **ok, 0.136834** (key 60) |
| Session Guitarist – Picked Nylon | Picked Nylon (Melody) | `note_ignored`, 0.000000 | **ok, 0.205340** (key 60) |
| Session Guitarist – Electric Vintage | Electric Vintage (Melody) | `note_ignored`, 0.000000 | **ok, 0.161369** (key 60) |
| Session Bassist – Prime Bass | Prime Bass (Melody) | `note_ignored`, 0.000000 | **ok, 0.184202** (key 48) |
| Session Bassist – Upright Bass | Upright Bass (Melody) | `note_ignored`, 0.000000 | **ok, 0.311228** (key 48) |
| Session Ukulele | Soprano / Tenor / Charango (Melody) | `note_ignored` at every key | **ok** at 48–72: 0.084966 / 0.142469 / 0.226365 |

**6 products, 8 instruments.** The Ukulele rows only move off `--key auto`'s key
36 because 36 is a *melody-articulation keyswitch* in the Melody framework
(`$KEYBOARD_MELODY_ARTIC1`), which no instrument sounds on by design; at 48–72
they play.

**Nothing else moved.** All 32 rows of the run are field-for-field identical
before and after except these five (`peak`, `broken`, `voices`, rejection counts,
frames, gain). Session Horns keeps its 2 voices and 0.1738, Kinetic-era rows keep
theirs, the Evolution and 8DIO rows keep theirs.

## 6. What is still silent, and the probe that would settle it

| product | first broken link | probe |
|---|---|---|
| Session Bassist – Upright Bass (the **pattern** patch, `Upright Bass.nki`) | `note_ignored`; Kontakt plays **0.302181** at key 48, we 0.000000 | `on listener` **is** called (`SIG=3`, `$NI_SIGNAL_TIMER_MS`, `$ENGINE_UPTIME` 2000 ms — the script sets `set_listener($NI_SIGNAL_TIMER_MS, 2000000)`), but the tick's `$isWaitingForStartPlaybackAfterChordDetection` branch produced no `play_note`, even with the key held 3 s. Next: message inside `start_playback_after_chord_detection` and compare with Kontakt's `$ENGINE_UPTIME` at its first tick |
| Session Guitarist – Electric Sunburst Deluxe | **matches Kontakt**: both silent (peak 0 through the oracle); its data folder is not where its registration points | none — Kontakt does not find it either |
| Evolution ×6 (Steel Strings, Bluegrass Banjo, Django Jazz, Flatwound Bass, Jazz Archtop, Dracus) | velocity gating: Jazz Archtop has 11,820 prepared zones and **0** admit velocity 100 (single-value ranges 1..60 per zone). Flatwound Bass sounds at key 72 | the previous pass's probe, unchanged: compare one Evolution zone's raw low/high velocity with Cherry Electric Bass, whose zones band (1..31, 33..63, …) and which sounds at velocity 100 |
| 8DIO Misfit ×3 | zones keyed **1..5** only (1,400 zones, 280 per key); at keys 1..5, 40 cover and the script still ignores | the script's own `ignore_event`/`play_note` on keys 1..5 |
| Soundiron – Voices Of Rapture | `voice_gain_zero` (voices run, 47,872 frames, gain 0.0) | unchanged from `silent-products-2026-11-03.md` §6 |
| Scene – Lotus | known gap (needs an uninstalled `.nka`); not chased | — |

## 7. Tests

- `a_library_data_array_survives_the_saved_values_that_name_it`
  (`crates/app/host/tests/script.rs`): a mode-1 script over a two-cell array, one
  of them saved. Reads `0 0` before the change and `1 1` after (both runs
  recorded in the commit message).
- `upright_bass_plays_through_its_quick_start_pane_state`
  (`crates/app/host/tests/instrument.rs`): the real instrument, key 48, asserts
  it sounds; prints `2 cells from the library's data folder, peak 0.325` at
  velocity 0.8, and skips on a machine with no Kontakt user data folder.
- `user_data::tests` — the registration parser, the renamed-library case
  (`… Electric Sunburst Deluxe` reading from `… Electric Storm Deluxe`), the
  `.nicnt` fallback, the folder-beside-an-install rule.

## 8. Reproducing

```sh
git -C /var/tmp/wt-int worktree add /var/tmp/wt-note -b feat/ignored-note feat/integration
cd /var/tmp/wt-note
CARGO_TARGET_DIR=/var/tmp/tgt-note cargo build -p kontra-silence -p kontra-host --bin kontra-render

# before: the same binary with the mode-1 folder unreachable
mkdir -p /tmp/empty-userdata
KONTRA_USER_DATA=/tmp/empty-userdata /var/tmp/tgt-note/debug/kontra-silence \
    --manifest silent.txt --json before.jsonl
/var/tmp/tgt-note/debug/kontra-silence --manifest silent.txt --json after.jsonl

# the probe that named the branch (a copy of the script with one message added)
KONTRA_SCRIPT_SOURCE=/tmp/probe /var/tmp/tgt-note/debug/kontra-render \
    --instrument "…/Upright Bass (Melody).nki" --key 48 --out /tmp/ub.wav   # `last message:`

# Kontakt's own answer
python3 tools/oracle-mcp/client.py '{"tool":"oracle_up","arguments":{}}'
python3 tools/oracle-mcp/client.py '{"tool":"oracle_load","arguments":{"nki":"…/Upright Bass (Melody).nki"}}'
python3 tools/oracle-mcp/client.py '{"tool":"oracle_render","arguments":{"notes":[{"start_s":0.1,"length_s":0.8,"key":48,"velocity":100}],"seconds":2}}'
python3 tools/oracle-mcp/client.py '{"tool":"oracle_down","arguments":{}}'
```

Gate: `tools/parity-corpus/run.py` **20/20**, every case `+0.00/+0.00`;
`tools/verify.sh` **VERIFY PASSED**, fingerprint `f18acaab5b897588`.
