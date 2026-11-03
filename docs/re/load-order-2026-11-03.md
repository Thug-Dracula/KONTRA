# Who writes 0 to `$warm_bright_slider`? — nobody. Two load-order divergences, 2026-11-03

The probe in [`saved-pgs-probe-2026-11-03.md`](saved-pgs-probe-2026-11-03.md) §5 said
the next thing to chase was the writer that takes `$warm_bright_slider` from 1 to 0
between the start of `on persistence_changed` and the `CONTROL[3]` publish, with
`call change_page` and `call ksw_page_switch` as the first suspects.

**There is no such writer.** Instrumented with a probe that accumulates every write,
the variable reads **1 at every checkpoint in the callback**, including immediately
before and immediately after the publish. What is 0 at note time is `CONTROL[3]` — the
PGS key — because the publish had **not run yet**: `on persistence_changed` was
suspended for 500 ms by its own `wait()`, and the note arrives 250 ms after load.

Once that was fixed the note still did not play, for a second and independent reason:
`exit` inside a user function ended the whole callback, so `det_string` (which ends with
`exit` and is called on every path of `on note`) never returned to `play_velo_layers`.

Both are fixed, both are pinned by a test, and Stradivari and Guarneri now play.

---

## 1. The write trace

A probe cannot print to a log — `kontra-render` keeps only the last message — so the
trace accumulates in a PGS **string** key shared between slots: slot 2 appends
`<tag><value>` after each statement it touches and writes the string, slot 1 prints it at
the top of `on note`. Tags:

| tag | point in `2.ksp` |
|---|---|
| `E` | first line of `on persistence_changed` (6156) |
| `C` | after `call change_page` (6161) |
| `K` | after `call ksw_page_switch` (6162) |
| `k` | after `call ksw` (6199) |
| `u` | after `call update_arp_tempo_label` (6206) |
| `h` | after `call update_human_lbl` (6209) |
| `f` | after `call show_hide_fretboard` (6216) |
| `m` | after the two mixer loops, before `pgs_set_key_val(CONTROL, 0, …)` (6222) |
| `P` | before `pgs_set_key_val(CONTROL, 3, $warm_bright_slider)` (6238) |
| `Q` | after it (6244) |
| `a`–`j` | every assignment to `$warm_bright_slider`: `automate_position_pref` (5929) and the three `$xbsdition_preference_*` handlers (5946/5948/5951, 5965/5967/5970, 5984/5986/5989) |

Two runs, same instrument (Stradivari, key 60, velocity 100):

```
printed at the top of on note   (t = 245 ms):  E1C1K1k1u1h1f1
printed at the end of the callback (t = 500 ms):  E1C1K1k1u1h1f1m1P1Q1
```

- **No tag carries a 0.** The value is 1 at entry, 1 at the publish, 1 after it.
- **`a`–`j` never appear**: no `ui_control` handler runs during load, so the four guards
  the previous lane added were answering a call that does not happen.
- The callback reaches `f` before the note and finishes at **500 ms** — a 255 ms gap
  between the note and the rest of the callback. Single-probe checks agree with the
  accumulation: `PUB v=1 t=500` at 6238, `PCS-entry v=1`, and at the note
  `c3=0 wbp=0 lwqb=0 ickl=2`.

So the previous table's "0 just before the position-button loop" was the published
**PGS** value, not the variable: at that point the publish had not happened yet.

## 2. Why the callback was suspended

Between `f` and `m` sits the `$i < num_elements(%mic_vol_id)` loop that calls
`mixer_fader_lbl_`, and that function ends (2.ksp:4400-4402):

```
if($NI_CALLBACK_TYPE # $NI_CB_TYPE_PERSISTENCE_CHANGED)
    wait(500000)
end if
```

500 ms is exactly the gap in the trace. In Kontakt the guard is **false** in this
callback and the wait is skipped; in KONTRA `$NI_CALLBACK_TYPE` read **5**
(`$NI_CB_TYPE_INIT`), because `persistence_changed()` ran the callback as
`Running::new(Kind::Init, None)` and `Kind` has no persistence-changed variant at all.

`$NI_CB_TYPE_PERSISTENCE_CHANGED` is 14 in `crates/script/ksp/data/constants.tsv`,
measured in Kontakt 8.13.1 by `tools/kontakt-oracle/probes/callback_constants.lua`.
A script that compares `$NI_CALLBACK_TYPE` against that constant is asking for the type
number *of the callback it is in*; the instrument's own guard is written to be false
exactly there. Reading 5 makes it true, so the slot waits half a second, and everything
after the fader loop — including the `CONTROL[3]` publish — lands **after** the first
note has been decided.

Note what this is not: `CONTROL[3] = 0` is not a lost value and not a wrong value. It is
the store's default, read before the publisher got there.

## 3. The second divergence, found once the first was fixed

With the callback type correct the note passes the branch at `1.ksp:997`
(`c3=1 wbp=1 lwqb=0 ickl=2`, so `$warm_bright_pos # 1` is false — no exit at 997), and it still
did not play: `play_velo_layers` is entered (`xbfd=0`), and its velocity-layer loop never
runs a body. A probe placed immediately after `call det_string` never fired, so
**`det_string` was not returning**.

`det_string` ends with `exit` (1.ksp:1069, the last statement before `end function`). `exit` compiled to `Op::Exit` unconditionally,
and the VM's `Op::Exit` returns `Step::Done` — it ends the coroutine and ignores the
call stack. So a function that ends in `exit`, called from `on note`, terminated the
note callback.

Kontakt's `exit` leaves the function and returns to its caller. Measured, not assumed:

- The instrument plays in Kontakt (below), and every one of its note paths calls
  `det_string`, which ends with `exit`; an `exit` that ended the callback would make the
  instrument permanently silent in Kontakt too.
- The fix is pinned by a test that fails on the old compiler: `exit` inside a function
  `Ret`s, and the statement after the call runs (`"-9 3"` → `"0 2"`).

## 4. The fix

`crates/script/ksp/src/compile/expr.rs` — `exit` emits `Op::Ret` when compiling inside a
user function (`Ctx::current` is `Some`) and `Op::Exit` at callback top level.

`crates/engine/ledger/src/running.rs` — `Running::persistence_changed()`: the kind stays
`Kind::Init` (so `on persistence_changed` keeps the init-path call behaviour KONTRA
already had, e.g. `load_array(…, 2, 2)`), and only the reported type number changes, to
`PERSISTENCE_CHANGED_TYPE` 14. `crates/engine/engine/src/recall.rs` uses it in the one
place the callback is run — the load loop in `engine/lib.rs` and `recall()` both go
through `persistence_changed(slot)`.

Nothing special-cases `$warm_bright_slider`, nothing touches `det_string`, and
`pgs_get_key_val` still returns what the store holds.

## 5. Before / after

`kontra-silence --manifest` (key 60, velocity 100):

| instrument | before | after |
|---|---|---|
| Stradivari Violin | 0 voices, peak 0.000000, `note_ignored` | **6 voices, peak 0.1617** |
| Guarneri Violin | 0 voices, peak 0.000000, `note_ignored` | **6 voices, peak 0.0608** |

`kontra-render` (key 60, velocity 100, hold 0.5, tail 0.5): Stradivari peak 0.000
(`6af8ae5c…`) → **0.165** (`ec723443…`).

Probe at the note, Stradivari: `CONTROL[3] 0 → 1`, `$warm_bright_pos 0 → 1`.

**The emptied-slot control still sounds.** Emptying slot 1 (zero-byte `1.ksp`) with the
old binary gives the previous lane's figure exactly — 7 voices, peak 0.165, sha256
`b2597696…`. With this fix the same control gives 7 voices, peak 0.291 (`65d0a7e4…`):
slot 2's publishes now land *before* the note instead of 250 ms into it. Forcing the
500 ms `wait()` back into slot 2's `mixer_fader_lbl_` (`if(1 = 1)`) under the fixed
binary reproduces `b2597696…` again, bit for bit, which is what identifies the moved
peak as the suspend timing and nothing else. So the fix is not "the script stopped
running": the instrument with its script plays one voice *fewer* than the control, at a
different level, as a scripted instrument should.

## 6. Kontakt's side

`tools/oracle-mcp` (Kontakt 8.14.1 on the private Xvfb), `oracle_load` + `oracle_render`,
key 60 velocity 100, no controllers:

| take | peak |
|---|---:|
| session A, first render | 0.3277 |
| session A, second render | 0.1538 |
| session B | 0.3300 |
| session C, first render | **0.0000** |

Kontakt plays this note, and its own take-to-take spread is about 2× in peak (the
instrument randomises its positions), so a single take is not a reference. One take
returned silence and one whole session failed on a window-geometry race
(`KeyError: 'X'` in `oracle_load`) — recorded here so the next lane does not read that as
a measurement.

`$NI_CALLBACK_TYPE` **inside** `on persistence_changed` could not be read from Kontakt on
this machine: the library slots are protected (`set_instrument_script_source` →
`"script is protected"`), a Lua `load_instrument` does not run that callback at all
(`docs/re/ksp/PERSISTENCE_CENSUS.md`), and saving the probe instrument needs a modal
dialog (`save_instrument` blocks; the probe timed out). The value 14 therefore rests on
the measured constant plus the instrument's own guard — which is exactly the shape the
constant exists for.

Kontra's level at this note is not yet a parity datum: 0.165 at CC1 0 against Kontakt's
0.15–0.33 spread, and 0.055 against Kontakt's 0.138 at CC1 64 (the dynamics direction
differs; unmeasured, another lane's question).

## 7. Gates

| check | result |
|---|---|
| `tools/parity-corpus/run.py` | **20/20**, every case **moved +0.00/+0.00** |
| `tools/verify.sh` | **VERIFY PASSED**, fingerprint `f18acaab5b897588`, init unanswered 51 (ceiling 51), actions_unhandled 55 (ceiling 55), 0 missing frames, 0 clipped |
| `cargo test -p kontra-host --test script` | 108 passed |
| the new test | `a_function_that_exits_returns_and_persistence_changed_names_itself`: `-9 3` before the fix, `0 2` after |

No corpus case ran with a corrupted variable, so a load-order fix moves nothing — and it
moved nothing.

## 8. What remains

- **The rest of the family is unchanged.** After the fix, `kontra-silence` still reports
  `note_ignored`, 0 voices, peak 0.000000 for Evolution Steel Strings, Session Ukulele –
  Soprano, Icon Bass and Upright Bass. Their first broken link is not these two.
- **Stradivari's level is not calibrated against Kontakt** (see §6): the note plays, the
  branch is the intended one, but no reference take exists for this instrument in the
  corpus, and Kontakt's own spread is 2×.
- **`on persistence_changed` still runs with `Kind::Init`.** Kontakt gives it its own
  callback type; whether it also allows `on init`-only calls (the reason the kind was
  kept) is not measured either way.

## 9. Reproducing

```sh
git -C /var/tmp/wt-int worktree add /var/tmp/wt-order -b feat/load-order feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-order cargo build -p kontra-silence -p kontra-host --bin kontra-render

# the write trace: in 2.ksp, after each statement in §1's table,
#   @wbtr := @wbtr & "<tag>" & $warm_bright_slider
#   pgs_set_str_key_val(WBTRACE, @wbtr)
# with `declare @wbtr` and `pgs_create_str_key(WBTRACE)` at the top of `on init`;
# in 1.ksp, at the top of `on note`,
#   message("TR " & pgs_get_str_key_val(WBTRACE))
KONTRA_SCRIPT_SOURCE=<dir with the edited 1.ksp and 2.ksp> /var/tmp/tgt-order/debug/kontra-render \
    --instrument "…/Stradivari Violin.nki" --out /tmp/p.wav --key 60 --velocity 100 --hold 0.5 --tail 0.5

# the emptied-slot control, and the same control with the suspend forced back
KONTRA_SCRIPT_SOURCE=<dir with a zero-byte 1.ksp> …       # 7 voices, 0.291
KONTRA_SCRIPT_SOURCE=<dir with 2.ksp's wait forced on> …  # 7 voices, 0.165, sha b2597696…
#   (2.ksp:4400 rewritten to `if(1 = 1)`)
```
