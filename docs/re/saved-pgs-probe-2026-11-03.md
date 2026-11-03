# Is a saved PGS table the reason 23 instruments eat the note? — probe, 2026-11-03

**No.** The hypothesis in §7 of
[`silent-products-2026-11-03.md`](silent-products-2026-11-03.md) — "their script
branches on program state, our PGS store starts empty, so we take the branch that
ignores" — is dead on the instrument it was written for. At the top of `on note`
on `Stradivari Violin.nki` the engine already answers **`ARTICULATION[0] = 24`**
(not 0) and `SEQUENCER[0] = 0`, and the note is still ignored. The file stores no
PGS table to read: across 37 instruments of this family, not one PGS key name
occurs anywhere in the preset outside the script source that uses it.

No reader was built. What follows is the measurement, the storage layout that
replaces the guess, and where the first broken link actually is.

---

## 1. The probe, as specified

Instrument: `Stradivari Violin 2.0.0 [Native Instruments]/Instruments/Stradivari Violin.nki`
(the six-instrument Stradivari family shares this script set).

The two calls §7 names were printed as the first statement of `on note` in slot 1
(the 74,131-byte handler script, `1.ksp` in a `KONTRA_SCRIPT_DUMP`), through
`KONTRA_SCRIPT_SOURCE`:

```
message("PROBE seq=" & pgs_get_key_val(SEQUENCER,0) & " art0=" & pgs_get_key_val(ARTICULATION,0)
        & " pgs_art=" & $pgs_art & " rng=" & %rqwb[0] & "-" & %rqwb[1] & " vel=" & $EVENT_VELOCITY)
```

```
KONTRA_SCRIPT_SOURCE=/tmp/psg-probe kontra-render --instrument "…/Stradivari Violin.nki" \
    --out /tmp/strad-probe.wav --key 60 --velocity 100 --hold 0.5 --tail 0.5

last message: PROBE seq=0 art0=24 pgs_art=24 rng=55-101 vel=100
peak 0.000
```

- `ARTICULATION[0] = 24`: the store is **not** empty at the moment the script
  decides. It was filled during load by slot 2's `on persistence_changed`
  (`pgs_set_key_val(ARTICULATION, $i, %art[$i])`, `2.ksp:6179`), and slot 1's
  `on pgs_changed` copied it into `$pgs_art` (`1.ksp:2446`), which is the value
  `on note` branches on.
- `SEQUENCER[0] = 0` is the instrument's own default: slot 2's `$umex` is saved
  as 0 (`$umex 0` at `0x5db8e`), and `on persistence_changed` writes exactly that
  (`pgs_set_key_val(SEQUENCER, 0, $umex)`).
- `%rqwb = 55..101` — the note range the script checks `in_range($xdto, …)`
  against — is also already correct, loaded from the saved `%violin_range 55 101`.

So every value named in the hypothesis reads the same in KONTRA as it would in
Kontakt, and the note is still ignored. Per the rule in §7 of the diagnosis, the
hypothesis is dead at the probe.

## 2. What the file actually stores

There is no PGS chunk. The whole decompressed chunk stream of the preset
(`kontra_format_kontakt::chunks`, the same bytes ni-file frames) is:

| span | id | what |
|---|---|---|
| `0x0` | `0x28` | program, structured version `0xb2`, 9,439,719 bytes |
| `0xd` | — | program private data, 325 bytes (UI/output scalars, not decoded) |
| `0x156` | — | program public data, 247 bytes |
| `0x251` | children | 9,439,132 bytes: `0x3a`×19, `0x45`×16, `0x06`×5, `0x33`, `0x32`, `0x4e` |

The five `0x06` script chunks sit at `0xa96` (50 B), `0xace` (10,962 B),
`0x35a6` (87,078 B), `0x189d2` (285,958 B), `0x5e6de` (820,582 B). Each is a
`BParScript` structured object:

```
u8   0                     structured flag (0 = bare object)
u16  0x0060                version
i32  length, bytes         source text (KSP)
u8 ×3                      editor-open, touched, bypass
i32  length, bytes         password hash
i32  length, bytes         description (-1 = none)
i32  length, bytes         linked text-file name (-1 = none)
u32  count                 saved persistent values
     u32 length, bytes ×count   "<name> <values…>"  (UTF-8)
```

That trailing table is the **only** saved script state in the format, and it is
where every value the instrument's branches depend on comes from. Examples, with
their offsets in the decompressed stream:

| offset | entry | chunk |
|---|---|---|
| `0x15c21` | `%violin_range 55 101` | `0x35a6` (slot 1 handler) |
| `0x5dd76` | `$warm_bright_slider 1` | `0x189d2` (slot 2 UI) |
| `0x5e2f9` | `%art 24 -1 1 -1 9 -1 15 -1` | `0x189d2` |

These are restored after `on init` and before `on persistence_changed`, which is
the documented lifecycle; KONTRA already does this and the probe above proves it
(`ARTICULATION` reaching 24 is nothing but `%art[0]` fanning out through
`make_persistent` → `on persistence_changed` → `pgs_set_key_val`).

**PGS is not persisted by Kontakt.** The manual calls it "a dynamic memory" that
`pgs_create_key` creates in `on init`; unlike `make_persistent`, there is no
saved form for it. Instruments that want a PGS value to survive a save persist the
*underlying variable* (as Stradivari does with `%art`, `$warm_bright_slider`,
`$umex`) and republish it from `on persistence_changed`.

### Corpus check

`cargo run -p kontra-format-kontakt --example pgs_names -- <files…>` counts every
`pgs_create_key(NAME, …)` name in the decompressed stream and in the script
sources alone, matching whole identifiers only. Over the 37 surviving instruments
of the products in §7 (Stradivari ×6, Guarneri ×2, Session Horns ×10, Session
Ukulele ×3, Icon Bass ×2, Upright Bass ×2, Prime Bass, Sunburst Deluxe, Picked
Nylon, Electric Ruby Deluxe, Electric Vintage, Evolution ×6, plus the Brass Band
Soloists patch whose name matched the same filter):

- **every file: `outside 0`** — no name occurs anywhere except the script that
  uses it, so there is no stored table to decode;
- **22 of the 37 declare no PGS key at all** (`keys 0`): for those, PGS cannot be
  the deciding state even in principle.

## 3. Where the note is actually lost

`kontra-silence` on the same patch: `note_ignored key 60, 31134 translated /
23037 prepared zones, 7 covering, 0 voices, ignored:2, peak 0.000000`.

The samples are fine. Emptying slot 1's source (`KONTRA_SCRIPT_SOURCE` with a
zero-byte `1.ksp`) makes the note pass through untouched and the instrument
renders **7 voices, peak 0.165** at key 60 (`sha256 b2597696…`, the same file as
a run where the patched slot merely failed to compile). So the loss is entirely
inside the script's `on note`.

Walking it with `message` markers (each insertion placed at a statement
boundary; the scripts wrap long expressions with Kontakt's `...` continuation
marker, and inserting inside one breaks the compile):

| marker | reached? | values at that point |
|---|---|---|
| top of `on note` | yes | `seq=0 art0=24 pgs_art=24 rng=55-101` |
| after the range check (`1.ksp:1993`) | yes | — |
| inside `if(%hymp[$true] # -1)` (`:1994`) | **no** | `%hymp[24] = -1` |
| `play_velo_layers` top (`:1072`) | yes | `_art=24 ctoa=4 swxq=40 nvl=4 sgix=20 eetb=1` |
| the velocity-layer `play_note` loop (`:1122`) | **no** | — |

`%hymp` is built in `on init` with entries at indices 1, 4, 5, 9, 10, 11, 12,
13, 14, 15, 28, 29 only (`1.ksp:283–295`); articulation 24 is deliberately not one of them, so
the script takes its `else` and calls `start_legato_line` → `play_velo_layers` →
`det_string`. `det_string` then reaches

```
if($lwqb # $ickl and $warm_bright_pos # 1)      // 1.ksp:997, exit at 1014
    …
    call set_and_send_position
    exit
end if
```

with the measured `$lwqb = 0`, `$ickl = 2`, and `$warm_bright_pos = 0`
(`$warm_bright_pos := pgs_get_key_val(CONTROL, 3)`, `1.ksp:923`), and the callback
**exits there**, before any `play_note`. That is the first broken link for
Stradivari, and it is a PGS *consumer*: `CONTROL[3]`.

`CONTROL[3]` is published by slot 2 from `$warm_bright_slider`. The file stores
that variable as **1** (`0x5dd76`), and KONTRA does restore it: a probe at the
first line of `on persistence_changed` reads `$warm_bright_slider = 1`. By the
time `pgs_set_key_val(CONTROL, 3, $warm_bright_slider)` runs later in the same
callback, it is 0, so `CONTROL[3]` is 0 at note time. Warm/bright position 0 is a
valid position, not a saved-state loss — something in the load sequence rewrites
the variable — and that rewrite is the next thing to chase, not PGS storage.

Measured at the moment each ran (`DBG` key written from slot 2, read at `on note`):

| checkpoint | `$warm_bright_slider` | `$automate_position_pref` |
|---|---:|---:|
| first line of `on persistence_changed` | 1 | 0 |
| just before the position-button loop (`2.ksp:6238` area) | 0 | — |
| just before `call automate_position_pref` (`2.ksp:6300`) | 0 | 0 |

Guarding the four handlers that write the variable (`on ui_control
($automate_position_pref)` and `$xbsdition_preference_0/1/2`) with an `exit` did
**not** restore the value, so "restoring a UI value dispatches `on ui_control`"
is not the whole story either. That is as far as this pass measured; it is one
variable in one callback and the probe to continue with is listed below.

### Six of the 23, measured on this branch

`kontra-silence --manifest` over one instrument from six different products.
Before and after are the same run, because no code changed:

| instrument | key | translated/prepared | covering | voices | peak | first break | PGS keys declared |
|---|---:|---:|---:|---:|---:|---|---:|
| Stradivari Violin | 60 | 31134/23037 | 7 | 0 | 0.000000 | note_ignored (`ignored:2`) | 15 |
| Guarneri Violin | 60 | 31732/23572 | 7 | 0 | 0.000000 | note_ignored (`ignored:2`) | 15 |
| Evolution Steel Strings | 60 | 10706/2042 | 1 | 0 | 0.000000 | note_ignored (`ignored:2`) | 72 |
| Session Ukulele – Soprano | 36 | 7580/1341 | 2 | 0 | 0.000000 | note_ignored (`ignored:1`) | **0** |
| Icon Bass | 48 | 10319/9127 | 10 | 0 | 0.000000 | note_ignored (`ignored:1`) | **0** |
| Upright Bass | 48 | 12713/6389 | 2 | 0 | 0.000000 | note_ignored (`ignored:1`) | **0** |

Half of the sample declares no PGS key at all, so for those the shape cannot be
a PGS branch in any implementation — which is the second, independent reason the
family is not one storage bug.

## 4. Upstream

`DerpcatMusic/KONTRA` `v3` carries exactly one PGS change, `e293ba5` "P6.4d:
prepare bounded PGS storage for large instruments (#1292)" — bounded admission,
value arena, `prepare_runtime`, tests, and `docs/re/engine/PGS_STORAGE.md`. It is
**storage only**: no reader, no file layout, nothing to port. Reading a PGS table
from the `.nki` would have duplicated nothing because there is nothing there.

## 5. What is left

- Nothing in this family moves, because nothing was changed: a PGS reader would
  have read nothing and changed no branch. The 23 remain silent.
- The next probe for Stradivari, and probably for the other NI-authored members:
  find the writer that takes `$warm_bright_slider` from 1 to 0 between the start
  of `on persistence_changed` and `2.ksp:6238`. `call change_page` and
  `call ksw_page_switch` are the first two candidates (they run in that window),
  and the same question — "is a restored value overwritten during load by a
  callback that Kontakt would not run" — is the shape to look for in the other
  products, whose deciding state is not PGS either (22 of 37 do not declare a
  single PGS key).
- The Scarbee trio is a different link again (the played note's key range), as
  §7 already says.

## 6. Reproducing

```sh
git -C /var/tmp/wt-int worktree add /var/tmp/wt-pgs -b feat/saved-pgs feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-pgs cargo build -p kontra-format-kontakt -p kontra-host -p kontra-silence

# the stored script tables and their offsets
CARGO_TARGET_DIR=/var/tmp/tgt-pgs cargo run -p kontra-format-kontakt --example dump_persistent -- \
    "…/Stradivari Violin.nki"
# PGS names outside the source, over a list of instruments
CARGO_TARGET_DIR=/var/tmp/tgt-pgs cargo run -p kontra-format-kontakt --example pgs_names -- <files…>
# the branch, with the probe script in place
KONTRA_SCRIPT_SOURCE=/tmp/psg-probe /var/tmp/tgt-pgs/debug/kontra-render \
    --instrument "…/Stradivari Violin.nki" --out /tmp/p.wav --key 60 --velocity 100 --hold 0.5 --tail 0.5
```

## 7. Gates on this branch

Nothing in the engine changed, so nothing was expected to move, and nothing did:

| check | result |
|---|---|
| `tools/verify.sh` | **VERIFY PASSED**, fingerprint `f18acaab5b897588` |
| `tools/parity-corpus/run.py` | **20/20**, every case moved **+0.00/+0.00** |
| `kontra-silence` over six of the 23 | 6 rows, all `note_ignored`, voices 0, peak 0.000000 (same before and after) |

The parity corpus is the check that would have caught a PGS reader that changed
script branching: none moved, because none was added.
