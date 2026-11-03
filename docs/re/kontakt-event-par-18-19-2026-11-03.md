# Event parameters 18 and 19 — the slots a script family keeps a note's string in

Measured 2026-11-03 on Kontakt 8.14.1 (portable, wine-11.16) through
`tools/oracle-mcp`, against KONTRA at `feat/integration`. Three probes, each a
synthetic instrument whose script answers one question per key — audible for
yes, silent for no — saved by the probe session and rendered in a main session:
`probes/event_par_extended.lua`, `event_par_identity.lua`, `event_par_clamp.lua`
and, for the last section, `mono_vs_stereo.lua`.

## The bug this closes

`eeb-rick-bridge` (Evolution Electric Bass, `EEB Rick - Bridge Only.nki`) sat
**23.06 dB** band RMS from its committed Kontakt takes, with the spectrum two
octaves below the reference: KONTRA's fundamental at 73.2 Hz against 146.5 Hz,
and no content above ~500 Hz where Kontakt has it up to 8 kHz.

The chain is the instrument's own, in three script slots:

| slot | what it does | source |
|---|---|---|
| 1 `Voice` | picks the string the note is played on (1..4) and stamps it on the event it plays: `set_event_par($osexa, $tyjme, $0gv2i)` with `$tyjme = 18` | `slot-1.ksp:32`, `:332` |
| 2 `Sample` | reads it back: `$2lfmo := get_event_par($EVENT_ID, $tyjme)` | `slot-2.ksp:35`, `:249` |
| 2 `Sample` | picks the sample from it: `play_note($j3lhv + ($2lfmo*24 - 48), …)` | `slot-2.ksp:404` |

`$2lfmo` is a string index, so `$2lfmo*24 - 48` selects one of four 24-semitone
blocks of the zone map. KONTRA answered **0** for parameter 18 — it knew only
`$EVENT_PAR_0..3` (0..3) and its own zone slot (15) — so every note landed 48
semitones below the block the script asked for, on a different string's sample:
the sample at key 14 (`SUS_D2_1E_01_B`) instead of the one at key 110
(`SUS_D3_2A_*`), played at step 0.891 instead of 0.999. That is the 23 dB.

## What Kontakt 8.14.1 answers

Two slots of the probe instrument, one instrument, one session. Every question
below is a key of the render:

| question | measured |
|---|---|
| `set_event_par($id, 18, 7)` → `get_event_par($id, 18)` | **7** — 18 is a legal per-note parameter |
| `set_event_par($id, 19, 5)` → `get_event_par($id, 19)` | **5** — and so is 19 |
| a fresh event's parameter 18 | **0** |
| `set_event_par($id, 20, 7)` → `get_event_par($id, 20)` | **7** — the range continues past 19 |
| slot 0 plays a note with 18 = 9, slot 1 reads it | **9** — a `play_note` child carries it into the next slot |
| negative control: set 18 = 7, ask whether it reads 8 | silent — the answers are not automatic |
| parameter 18 written 7, 100, 127, 128, 500 and 250000 | **each reads back unchanged** — no clamp, no scaling |
| `set_event_par($id, 12, 13)` → `get_event_par($id, 12)` | **not 13**: 12 does not round-trip a plain value, so 18 is not `12 + 6` of some uniform block |

The value is a plain integer: 250000 survives, and so does 250000 written by
`set_event_par_arr($id, $EVENT_PAR_MOD_VALUE_ID, 250000, 6)` read back through
`get_event_par($id, 18)`. The reverse does **not** hold — a parameter write does
not move the modulation value — and `$EVENT_PAR_MOD_VALUE_ID` index 7 is not
visible through 19. So 18 answers the modulation target *and* a write there is
local to the event; KONTRA keeps the two stores apart, which is what the
instrument needs (the family only ever writes and reads the plain value).

So KONTRA answers **18 and 19** from `Note::extra`
(`crates/engine/ledger/src/ledger.rs`), read and written in
`crates/engine/engine/src/script.rs` (`extra_slot`), and claims nothing else in
the number space: 0..=3 are `$EVENT_PAR_0..3`, 4..=11 the named parameters and 15
KONTRA's own zone slot. Parameter 20 answered a plain 7 too; the parameters below 18 are
**not established** — a probe over 12..17 gave answers that did not repeat between
runs (the audible-for-yes mechanism latches instrument volume to silence and a
single miss hides the rest), so they are left answering what they answered
before: 0. No instrument in the corpus writes any of them — 18 and 19 are the two
the family uses, and those are the two implemented.

Reach: 95 instruments in the gap corpus (`/var/tmp/gaps/scripts`) write parameter
18 and read it back — the whole Evolution bass family, not one preset.

## The mono-to-stereo question, ruled out

The 23 dB was spectral; what is left after the fix is a level difference, so the
obvious candidate — that Kontakt folds a mono sample into two channels at
`1/√2` each — was measured rather than assumed: one instrument, one 0.5-amplitude
220 Hz source, a mono copy on key 60 and a stereo copy on key 62 of **the same
instrument**, so the instrument's own gains cancel.

| key | left peak | right peak |
|---|---|---|
| 60, mono sample | 0.19192 | 0.19192 |
| 62, stereo sample | 0.19192 | 0.19192 |

Equal to five decimals: Kontakt copies a mono sample to both sides at full
amplitude, exactly as KONTRA does. The hypothesis is dead.

## What the fix moved, and what it did not

`tools/parity-corpus/run.py --case eeb-rick-bridge`: band RMS **23.06 → 1.23 dB**,
level **−3.91 → −3.22 dB**, alignment 40 → 38 ms. The render's fundamental is now
146.5 Hz, and every spectral peak matches the reference to 0.5 dB.

Two things remain, neither spectral, and neither addressed here:

- **a pure gain of about −3 dB**: the reference is 0.70× KONTRA's own signal, with
  the waveforms matching to −27.6 dB residual after that one scale (correlation
  0.994). It is not the velocity→volume table (`%htsm1` — KONTRA's
  `change_vol` answers −2114 mdB, index 392, exactly the table's value) and it is
  constant across velocities (−3.0 dB at velocity 50 and 100, −3.4 at 127), so it
  is a stage downstream of the voice. Not attributed.
- **the note never ends**: at 2.2 s the reference is at −85 dBFS and KONTRA at
  −28.67, and the voice reports `note_released: false`. KONTRA's note-off does not
  reach the script-played child on this instrument. That is the
  `feat/ignored-note` family, not this one.
