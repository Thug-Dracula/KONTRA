# Electric Mint: the session's state, and how to pick it up

Written 2026-10-31 on `feat/integration` at `aba8c0c`. Everything below was
measured on this machine, headless, with the oracle on a private Xvfb. Where a
number came from a subagent rather than from me directly, that is said.

`STATUS.md` is the page of truth for *what is true now*. This file is the
adjacent thing: what the session did, which decisions should not be relitigated,
and which traps this machine sets.

---

## 1. What works

- Electric Mint (both `.nki` files) loads, renders and plays; every control on
  its page responds; the preset list opens, scrolls and selects; the Melody
  instrument's eight `ui_xy` rows select.
- Parity against real Kontakt, one note, key 60 vel 100: **−0.14 dB level /
  0.93 dB band RMS**, inside the measured floors. Reaching that took four
  separate reader-law fixes; see §2.
- Other instruments improved by the same work: **Haunted Strings 2** −9.14 →
  **−0.53 dB**, **Cherry Electric Bass** +49 → **−2.9 dB** (waveform correlation
  0.001 → 0.84), **Soundiron Harpsichord** silence → audible.
- The delivery gate refuses broken builds, and one of its rows now compares the
  UI against **Kontakt's own render** rather than against an earlier render of
  ours.

Fingerprint `7fd5a1ae59790ffc`; page golden 0 of 316,500 px; Kontakt panel row
18.865% against a 21.0% ceiling; `ui-drive` 39 controls on the main page and 44
on Melody.

---

## 2. What landed, and the evidence that justified it

**Reader laws — four of them were wrong, all in `crates/format/kontakt/`.** This
is the pattern to expect: the DSP was right every time; the file format is where
the bugs were.

| law | was | found by |
|---|---|---|
| send-slot `dry` | summed on top of the dry path (+5.9 dB) | ablating a slot and watching the level move |
| EQ/filter slot output | forced to unity, discarding the layer mixer (−2.8 dB) | same |
| Replika Diffusion field order | SIZE and AMOUNT swapped (angle 1.183 vs 0.5708; direct path `sin⁸` 0.54 vs 0.0073) | the live module's captured tap table |
| Supercharger SATURATION | read at CHARACTER's offset (payload 12 instead of 4) | `fx_other.tsv` + `reader_schemas.tsv` + the live 45-byte record |
| Hot Solo stage gain | calibrated against the *whole chain*, absorbing two other errors | bypassing slot 3 inside Kontakt (+7.00 dB vs our +3.08) |

**Modelled from the binary, not guessed**

- **Choral** (`0x5d`, v`0x51`, 34 bytes): the full kernel — two LFOs (the second
  `sin(πy/2)` shaped), per-voice smoother, two-pole loop filter, seven-tap
  circulant mix, per-pair width rotation, global tone filter. Validated at six
  frames against native vectors, worst **1.3e-7** (2–4 ulp).
- **Replika Diffusion** (type 4): `NI::DSP::Diffuser`, vtable `0x1479aa618`,
  captured live through gdb. Rows exact to **6 ulp**, interpolation to the
  capture's precision. The impulse tail does **not** match (correlation 0.012)
  because the tap-to-drift mapping is unread; the port holds taps at the window
  floor and logs `Replika diffusion tap drift`.
- **Hot Solo+** amp: a measured transfer function at Electric Mint's operating
  point, fitted as biquads — *not* a reconstruction of `0x140ad5440`.
- **Supercharger**: static make-up `1.0716536` (`0x1479cb194`) × stored output.

**Kontakt 8 private-data scripts** — five `BParScript` records in the program's
private data, in slot order with persistent values. 34 of 6,707 installed `.nki`
use that layout. Loading them needed `get_event_ids` implemented as well; without
it Cherry loaded four scripts and played nothing.

**Tooling built**: `tools/verify.sh` (the gate), `tools/pixel-diff.py`,
`tools/ui-drive` (headless press-every-control), `tools/oracle-mcp` (real Kontakt
over MCP), `tools/kontra-render` (a first-class renderer), `parity.py` with
measured thresholds, `tools/vendor/…/gui-probe.sh`, `panel-reference.py`.

---

## 3. What is open, with the experiment that closes each

| gap | how it closes |
|---|---|
| **Replika Diffusion tail** (correlation 0.012) | read the tap-to-drift mapping: break at `0x141aa73d0`, dump which rotating pair drives which tap |
| **Hot Solo tone knobs / overdrive / mono** | unmodelled and *reported* per record; the `.nki` knob sweep works (`preset_patch.rs`) but needs an audio-capable Kontakt session |
| **Supercharger CompXT** | gain computer, detector and field→core map unread; today it is a gain (0.22 dB residual on this note) and will not duck transients |
| **Harpsichord ~26 dB loud** | `$EVENT_PAR_ALLOW_GROUP` does not gate the entering note (`KSP_AUDIT.md:85`); 144 groups start |
| **Showroom Violin +10 dB** | different cause: it starts 75 voices at key 67 — check mic selection against Kontakt |
| **Bus slot 7 phantom filter** | instrument stores `bypass: false` but Kontakt behaves as if inert; needs the bypass toggled at runtime by KSP in an audio-capable session (editing the `.nki` is refused — Kontakt validates an MD5 in `BNISoundHeader`) |
| **`ui_xy` cursors not drawn** | state changes, appearance does not; needs a renderer pass for `$CONTROL_PAR_CURSOR_PICTURE` and the active cursor |
| **`inverted()` law** | wired from an unmeasured law; needs one probe for `$MOD_TARGET_INVERT_SOURCE` |
| **`mf_insert_file` refused from a callback** | a KSP-contract question: Kontakt answers an id synchronously, so we gate rather than invent an async form |

---

## 4. How to check anything here

```
tools/verify.sh            # the gate; refuses to pass on a broken build
tools/verify.sh --quick    # everything except the audio render
tools/oracle-mcp/smoke.sh  # real Kontakt up → load → shot → click → render → down
tools/oracle-mcp/parity.py # one note through both, with thresholds
```

The gate is the authority, not a summary. It checks: tests in nine crates, the
plugin exports `clap_entry`, Electric Mint renders completely with no dropped
frames or clipping, every control hit-tests where it is drawn (both pages), the
page matches its golden, and **the preset panel matches Kontakt's own render**
within a measured ceiling. It also fails if a probe copy is left in a sample
library.

---

## 5. Decisions not to relitigate

- **A single click on the preset name opens the list; a double click edits it.**
  The field is a real `ui_text_edit` that the script reads back, but Kontakt
  edits in place and opens the list from the plate, so a single click falls
  through to the menu. One line changes this if wanted.
- **Diffusion taps stay in samples with a recorded `rate`**, not seconds:
  a seconds round-trip through f32 can land one ULP off and change interpolation;
  `rate as f32 / d.rate as f32` is exactly 1.0 at 48 kHz.
- **The amp is one operating point, and says so.** Any record differing from
  Electric Mint's is *reported* as unmodelled rather than drawn with the same
  curve.
- **`STATUS.md` is a page of truth, not a summary.** Rows without a command
  behind them get deleted.
- **One PR, not a stack.** #1275–#1278 were cut for review, no human read them,
  and the work outgrew the cut; everything is consolidated into #1274.

---

## 6. Traps this machine sets

- **Three clones of the repo existed at once** (`kv3`, `kontra-src`, `kontra`).
  Branches made in one are invisible from another — that swallowed a lane's
  commits until they were ferried across. `git worktree list` and note the common
  dir before starting.
- **`/tmp` is a 16 GB tmpfs and it is wiped by a reboot.** `/var/tmp` is real
  disk; a release build does not fit in `/tmp` (`kontra-inspect --json` alone
  writes ~740 MB). Put **worktrees and anything you want to keep** under
  `/var/tmp`, not `/tmp`. Measured 2026-11-01: the machine rebooted at 05:28 and
  every worktree, screenshot, oracle log and the background job board under
  `/tmp/opencode` was gone; `/var/tmp` kept the target dirs and a running sweep's
  shard files. Nothing was actually lost because every commit had been pushed —
  which is the other half of this trap: **push early, and never leave a lane's
  only copy of its work in `/tmp`.**
- **A shared `CARGO_TARGET_DIR` lets worktrees link each other's rlibs**, which
  produces false passes. One target dir per lane.
- **`mv` preserves mtime**, so restoring a file with it makes cargo skip the
  rebuild and report a stale result. `touch` after any `mv`-restore.
- **`pkill -f "<pattern>"` kills your own shell** when the pattern is in your own
  command line. Kill by PID.
- **The server restarts.** Five lanes were killed mid-task in one day; two before
  writing anything. Brief lanes to commit as soon as it builds.
- **Probe copies must not stay in a sample library.** Three lanes saved
  in-library `.nki` copies (Kontakt refuses instruments outside a library) and
  left 113 MB in the user's Electric Mint folder, which their browser listed as
  instruments. The gate now fails on any `__*` directory under a library's
  `Instruments/`.
- **`cargo clippy -- -D warnings` without `--no-deps` can never pass here**; the
  vendored crates carry their own diagnostics.

---

## 7. Next actions in order

1. **Read `STATUS.md` first** — it is shorter than this file and is the state.
2. Land the PR: #1274 is current and gated; it needs a human read, not more code.
3. Close the diffusion tail (the only known DSP that is structurally right and
   audibly wrong).
4. Model the CompXT gain computer, then re-ablate the Supercharger: its 0.22 dB
   residual is level-dependent and will move once dynamics exist.
5. Harpsichord's allow-group gate — a script-semantics fix that unblocks a whole
   instrument family, not one file.
6. Draw `ui_xy` cursors, so a selected table row looks selected.

---

## 8. What is *not* evidence

- A pixel golden that is a render of ourselves is a change detector, not a
  reference. That is why the Kontakt panel row exists, and why it was proved to
  fail before being trusted.
- `wave_r` carries no information at one note: Kontakt correlates 0.604 with
  itself.
- One render is not a level. Round robin is a per-session counter; the
  cross-session floor is 0.31 dB band RMS / 1.57 dB worst band.
- A test that has not been seen to fail is a hope. Every fix in this session
  shipped with the failure it prevents, and the ones that did not were found
  later by review — twice claiming to pass on exactly the state they were written
  to catch.
