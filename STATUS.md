# STATUS

The single place to look for where KONTRA stands. If this file and an agent
disagree about the state, this file wins — and if it is out of date, that is a
bug worth fixing before anything else.

Last updated: 2026-10-31.

## What is verified, and how

Everything below was produced by a command, not by a claim. The command is
given so it can be re-run rather than believed.

| Claim | Evidence |
|---|---|
| The workspace builds and every test suite passes | `tools/verify.sh` — engine, host, ui, mui, native-host, plugin, ui-pictures and dsp-delay suites, 0 failures (the counts in the gate's own table are the authority; they grow) |
| The plugin a host loads is the one that was tested | gate builds `kontra-plugin`, asserts `clap_entry` is exported, and installs to **both** `~/.clap` and `~/.vst3` |
| Electric Mint plays end to end | gate render: completes, 0 missing frames, 0 clipped, `actions_unhandled` 55, `init unanswered` 51 |
| The rendered audio is reproducible | fingerprint `8e664c1f9088be9f` (stable run to run; moved from `170201c…` when Hot Solo+ went in and `8e664c1…` when Choral did) |
| The page hit-tests where it is drawn | `tools/ui-drive` — 39 controls checked, every press lands on the rect it was drawn in, a press 2 points outside selects nothing |
| The page looks like the reviewed page | `tools/pixel-diff.py` against `tools/kontakt-reference/electric-mint-page.png` — 0 of 316500 px differ |
| The page fits the renderer's image atlas at the live device scale | test asserts the page's pictures fit at 1 **and** 2 device pixels (the 2× case is what shipped blank once) |
| Real Kontakt can be driven headless | `tools/oracle-mcp` — Kontakt 8.14.1 on Xvfb with DXVK + lavapipe paints its instrument page and takes pointer input (`paints=yes click_px=44485`) |
| The Hot Solo+ amp really closes the gap | an independent re-measurement on fresh sessions: level +0.02 → **−0.06 dB**, band RMS 0.94 → **0.90 dB**, worst band +2.2 → **+2.12 dB @ 1613 Hz**, per-band max difference 1.07 dB / RMS 0.32 dB — which is exactly the cross-session capture noise (0.31 dB) |
| Parity can be judged, not admired | `tools/oracle-mcp/parity.py` exits non-zero on a level, band-RMS, worst-band or lag breach; the limits (0.5 dB / 1.0 dB / 3.0 dB / 10 ms) are 4.2×, 2.6×, 1.9× and 1× the measured Kontakt-against-itself floor |
| KONTRA's Rust meets its own lint bar | 48 opted-in crates, `clippy --all-targets --no-deps -- -D warnings` → 0 diagnostics, 0 `#[allow]`/`#[expect]` added |

## What is implemented

- **Formats**: Kontakt `.nki` (script, zones, effects, UI), UVI/Falcon, SFZ,
  native. Picture containers (`.nkr`/`.nkx`) with sidecar metadata.
- **KSP**: the script VM, the UI/control model, `mf_*` MIDI object, string-array
  control parameters, `$KSP_TIMER` on the CPU clock.
- **DSP**: filters, dynamics, drive, delay (Modern + Choral banks), reverb,
  spatial, modulators; Hot Solo+ amp measured at Electric Mint's operating point.
- **Host/plugin**: CLAP + VST3, a rack with parts, a browser, an editor, and a
  one-slot ring so a control edit applies even when the host renders no audio.

## Parity across the library

`docs/re/parity-sweep-2026-10-30.md`. Fifteen instruments attempted, ten measured,
five recorded as failures with reasons rather than skipped. Only one library is
registered in the oracle's Kontakt (Electric Mint), so most Session Guitarist
libraries and the factory library could not be reached.

| instrument | level | band RMS | what it is |
|---|---|---|---|
| Electric Mint | −0.01 dB | 0.85 dB | passes |
| VSTBuzz Cyber | | 0.01 dB | passes |
| VSTBuzz Legendary | | 0.25 dB | passes |
| Electric Mint (Melody) | | 5.38 dB | **inside the instrument's own round-robin spread** (Kontakt differs from itself by 9.60 dB) — not a KONTRA defect |
| Vertigo Violin | −2.35 dB | 5.83 dB | missing impulse response, insert slot 3 |
| Showroom Violin | −8.96 dB | 7.20 dB | unexplained; low-mid deficit at 508 Hz |
| Haunted Strings 2 | −9.10 dB | 2.07 dB | missing impulse response, send slot 0 |
| Cherry Electric Bass | **+44.03 dB** | 57.94 dB | voice filter closes: 1 group, cutoff 93.6 Hz, renders −68 dBFS |

Recorded failures, not skipped: 8Dio Fire Sax (**KONTRA refuses the file** — `Unexpected internal modulator category 3`), Soundiron Harpsichord (**renders exact silence** — 6232 zones, 432 groups purged, `voice filters: 576 groups []`, and a script compile error `%NOTE_DURATION is not an array`), Chris Hein Harmonica / Taiko Creator / 8Dio organ (no sound from any key tried — likely keyswitch articulations).

## What is open, and what would close it

| Gap | Why it is open | What closes it |
|---|---|---|
| **Choral moves Electric Mint and has not been re-measured against Kontakt** | Choral is now decoded (`0x5d`, reader `0x140d00630`, v0x51, 34 bytes), carried as `ir::Chorus` and routed, and Electric Mint stores it at insert slot 6 — its render moved (peak −11.5%) and the fingerprint to `8e664c1f9088be9f`. The parity measurements for Electric Mint predate this | re-run `parity.py` on Electric Mint with Choral active; the oracle is held by the diffuser lane |
| **Replika Diffusion mode** | Electric Mint's record is type 4. The network is now **located and structurally read**: `NI::DSP::Diffuser` (vtable `0x1479aa618`, ctor `0x141a7eda0`) embedded twice at `+0x11600`/`+0x12a80`, 8 stages in series each with four ring buffers, dense/non-dense banks from tables `0x141a7ef1f`/`0x141a7f038`, and all four setter laws (Size, Amount, case 2, Dense) read with their constants. It does **not** ship because the stage's 4×4 coefficient rows and the ring interpolation are only partly decoded — writing them from a partial read would be invented DSP | one debugger capture on a live type-4 Replika (Electric Mint is one): break at `0x141aa73d0` with `[filter+4] != 0` and dump one stage's rows `+0x00..+0x78`, its four ring descriptors, `+0x90..+0x9c` for a frame, `+0x12c4/+0x12d4/+0x12d8/+0x12d0`, and one impulse response per channel. The capture list is in `docs/re/engine/k-replika-diffusion.md` §10.4 |
| **Hot Solo+ is one operating point** | the module's coefficient recalculation (`0x140ad5440`, 14.5 KB of SIMD) is unread, so the stage is a *measured transfer function* fitted as biquads | read the recalculation; until then any record that differs from Electric Mint's is **reported** as unmodelled, never silently drawn with the same curve |
| **Hot Solo+ tone knobs, overdrive, mono** | same | the `.nki` knob sweep is possible (`preset_patch.rs` rewrites a copy) but needs an audio-capable Kontakt session |
| **NKS2 navigation, MIDI export areas** | the KSP manual has no entry for these commands and there is nothing in KONTRA for the values to land in | the NKS2 SDK, or a decision to model a page/slot tree |
| **Voice filters close to silence** | Cherry Electric Bass renders 44 dB quiet because its voice filter group list is empty and a 93.6 Hz cutoff is applied; Harpsichord purges 432 groups and renders exact silence. One parser gap with two faces | locate the filter field that is not being read in `crates/format/kontakt/` |
| **Impulse responses are not found** | Haunted Strings 2 (send 0) and Vertigo Violin (insert 3) are 9 dB and 2.35 dB down with `missing impulse response` | a lookup bug or a genuinely absent asset — the difference matters and is being established |
| **8Dio Fire Sax is refused** | the reader rejects `Unexpected internal modulator category 3` | read that modulator category |
| **Soundiron Harpsichord script will not compile** | `%NOTE_DURATION is not an array` | the script engine's array declaration path |
| **Bus slot 7 phantom filter** | the instrument stores it `bypass: false`, but Kontakt behaves as if it is not running; removing it closes 11.8 dB at 12.9 kHz | wrong to decide by assumption. The decisive test needs the bypass toggled at runtime via KSP in an audio-capable session — editing the `.nki` is refused because Kontakt validates an MD5 in `BNISoundHeader` |
| **A control edit can cost one silent block** | the price of having no lock on the audio path: if the host resumes while the control tick holds the audio side, that block renders silence | accepted trade-off; counted and logged as `block_missed` |

## What is *not* verified — do not cite these as facts

- **No live Kontakt pixel comparison is in the gate.** The pixel check is a
  *regression golden* of our own render. `tools/oracle-mcp` can screenshot real
  Kontakt, and that is the on-demand check, but nothing fails a build on it.
- **Parity is measured on one note, on ten instruments, from one library.** There
  is no articulation matrix and no regression history, and five instruments could
  not be measured at all (see the sweep).
- **`wave_r` is not a parity statistic** for a single note: Kontakt correlates
  0.604 against itself within a session.
- **Click coordinates in the oracle are fixed**, not resolution-independent.
- **`dump.py` does not run as shipped** — its default EXE path does not exist on
  this machine and its `WINEDLLOVERRIDES=…=b` override is the wined3d assumption
  the oracle disproved.

## How to check the tree

```
tools/verify.sh          # the delivery gate; refuses to pass on any of the above
tools/verify.sh --quick  # everything except the audio render
```

`VERIFY PASSED` means: tests pass, the plugin builds and exports a CLAP entry,
Electric Mint renders completely with no dropped frames or clipping, the UI
hit-tests where it draws, and the page matches its golden. It does **not** mean
the instrument matches Kontakt — that is the oracle's job, and it is on demand.

## Where the work is

The change set is a stack of four PRs against `DerpcatMusic/KONTRA`:
#1275 (UI), #1276 (audio), #1277 (engine), #1278 (tooling). Each is based on the
previous; the tip is tree-identical to the branch they were cut from.
