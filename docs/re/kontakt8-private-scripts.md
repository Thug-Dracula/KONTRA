# Kontakt 8: scripts in the program's private data

Written 2026-10-30. Read from `Cherry Electric Bass.nki` and `EEB Rick - Middle
Only.nki` (both program private version `0x80`), the same records in 34 of the
6707 installed instruments. Implemented by `crates/format/kontakt/src/private_script.rs`
and `translate.rs`.

Older programs save each `BParScript` as a `0x06` child chunk of the program.
Kontakt 8's `0x80` private section writes the same objects **inline**: five
records, one per script slot, after the private scalars. A loader that only
looks for `0x06` chunks finds no scripts at all, and a library whose zone
selection lives in the handlers then plays whatever its own start rules allow
(Cherry Electric Bass fell back to its "Buzzing" layer).

## Where the records are

| | |
|---|---|
| Program chunk | top-level `0x28`, a structured object, version `0x0080` |
| Private section | the structured object's private bytes (`u32` length at body +3) |
| Records start | `private[0x41]` in both instruments read; the section is found by framing, not by this offset |
| Records end | 16 bytes before the end of the private |

The private scalars before the records are **not decoded**. Like
[`links`](../../crates/format/kontakt/src/links.rs), the section is located by
its own framing: the first offset whose records run out inside the 16-byte tail.

## One record

```
u8   0x00            structured-object flag (an inline object, "not structured")
u16  0x0060          BParScript version (the `0x50`/`0x60` of the 0x06 chunk)
i32  source length   negative = no source; else UTF-8 (CP1252 fallback)
…    source bytes
u8   source_editor_open
u8   touched_but_not_applied
u8   bypass
i32  password length, then that many bytes     (non-empty = protected)
i32  description length (-1 = none), then UTF-8
i32  text-file name length (-1 = none), then UTF-8
u32  persistent count
      u32 length, then UTF-8, per saved persistent value: "<name> <values…>"
```

The field order is exactly `BParScript::params` (`vendor/ni-file/src/kontakt/objects/bpar_script.rs`,
native reader `0x140d03920` on 8.13.1), which is where the `0x06` path reads
the same fields today. The record adds only the two-byte version framing in
front of them — the same `{flag 0, u16 version}` framing `links.rs` documents
for the private `BAutomationObject` records (`{0, 0x70|0x71}`).

A slot with no script is still written: 26 bytes (no source, no names, no
values). After slot 5 comes a fixed 16-byte tail, `0, 0, 0, -1` as four `u32`;
it is not a record and nothing reads it. **What those four words are is not
established.**

## Slots: five, in order

Kontakt has five script slots per instrument (`get_instrument_script_source`
takes `0..4`). Both instruments store exactly five records, in slot order:

| slot | Cherry Electric Bass | EEB Rick - Middle Only |
|---|---|---|
| 0 | `Interface 1.09` | `Interface 1.11` (`1 - Interface Handler`) |
| 1 | `Legato 1.03` | `Voice 1.11` (`2 - Voice Handler`) |
| 2 | `Sample 1.01` | `Sample 1.11` (`3 - Sample Handler`) |
| 3 | `Zone 1.01` | `<empty>` (a 65-byte constants script) |
| 4 | empty | empty |

Evidence for the order, not just the set:

- The names EEB stores carry their slot number (`1 -`, `2 -`, `3 -`) and they
  sit in that order in the file.
- Cherry's Interface script prints `"Cherry Electric Bass (RELEASE, BUILD " &
  (VERSION[0]+…+VERSION[4]) & ")"` from the five scripts' version keys, and a
  render reports **BUILD 414** = 109+103+101+101+0: slots 0–3 hold the four
  scripts and slot 4 is empty, which is what the record order says. The same
  argument rules out a reversed order for the set, and the handler chain only
  runs in the file order (each of `Legato`, `Sample`, `Zone` acts on a note
  whose `$EVENT_PAR_SOURCE` is the *previous* slot, and only `Interface` accepts
  the host note's `-1`).

A gap in the middle is kept: 8DIO's Misfit instruments store slots 0, 2, 3, 4
with slot 1 empty, and the translated instrument keeps a placeholder there so
that `$CURRENT_SCRIPT_SLOT` and the saved-value slot numbers still line up.

## Protected scripts

A protected script has a non-empty password hash. The source is stored anyway:
Electric Mint's `ElMint` (`0x06` chunk, hash 16 bytes) carries 3.4 MB of KSP,
and the 8DIO/Cinematique/Embertone private scripts above are protected and
readable. Protection only hides the source in Kontakt's own API
(`get_instrument_script_source failed. script is protected`), so a protected
slot loads like any other. A protected slot whose source is missing is
reported (`protected script`) instead of being run empty.

## Persistent values

Each slot's saved persistent values are inside its own record, after the
text-file name — the same `"<name> <values>"` strings a `0x06` chunk's
`BParScript` stores, and exactly what `ir::Script::saved` already takes. A
snapshot's values still override them, by slot, as before.

## Why it is not enough that the source loads

Cherry's handlers run as a chain, and the Legato handler needs
`get_event_ids(%array)` to find the event the previous slot played. The engine
answered every other call but that one (`Action::GetEventIds` had no arm), so
the Legato slot swallowed the note and played nothing: 4 scripts loaded, 0
voices. It now fills the array with the live event ids in creation order, then
a 0 — the semantics of the retired v2 instruction (`ReadEventIds`,
`crates/sampler-core/src/behavior.rs` before #1205).

## Corpus

`KONTRA_SCAN_ROOT=<library>` over the installed VSTs: **34 of 6707** `.nki`
files keep their scripts this way, across Orange Tree Samples (Cherry, Angelic
Zither), Evolution (EEB Rick), Embertone (Chapman Trumpet, Glockenpillar),
Cinematique (Concert Zither) and 8DIO Misfit (Bicycle, Stompbox, Harmonica);
protected and open scripts both occur. Every one of them loaded its slots.

## Parity: Cherry Electric Bass, key 40, velocity 100

Against the stored Kontakt capture
`/var/tmp/parity-sweep/kontakt/cherry-bass-0.wav`, 1/3-octave bands, band
difference in dB (KONTRA − Kontakt):

| band (Hz) | before | after | | band (Hz) | before | after |
|---|---:|---:|---|---|---:|---:|
| 40 | 0.0 | 0.0 | | 806 | 4.0 | −10.1 |
| 50 | 150.4 | −1.0 | | 1016 | 2.3 | −7.9 |
| 63 | 143.3 | −1.1 | | 1280 | 2.9 | −10.5 |
| 80 | 154.7 | −1.2 | | 1613 | 9.9 | −9.1 |
| 101 | 147.4 | −1.3 | | 2032 | 2.7 | −15.2 |
| 127 | 140.7 | −2.3 | | 2560 | 30.0 | +2.4 |
| 160 | 135.3 | −4.2 | | 3225 | 29.1 | −0.1 |
| 202 | 130.4 | −3.2 | | 4064 | 39.5 | +12.1 |
| 254 | 124.9 | −2.5 | | 5120 | 40.5 | +10.1 |
| 320 | 111.5 | −5.6 | | 6451 | 45.8 | +16.8 |
| 403 | 96.5 | −5.6 | | 8127 | 29.6 | +2.5 |
| 508 | 97.6 | −5.6 | | 10240 | 34.8 | −0.4 |
| 640 | 60.4 | −8.3 | | 12902 | 38.3 | −0.4 |

| | before | after |
|---|---:|---:|
| level difference | +49.10 dB | **−2.90 dB** |
| RMS difference | −72.4 dBFS | −20.4 dBFS |
| timbre (band residual RMS) | 61.40 dB | **6.91 dB** |
| waveform correlation | 0.001 | **0.840** |
| envelope lag | −230 ms | −40 ms |

The low/mid bands now sit within 1–5 dB of Kontakt instead of 100–155 dB under
it: the fingered layer plays, not the buzzing one. What remains is the
instrument's own modelling, not the loader — a mid-band deficit growing to
−15 dB at 2 kHz and an excess above 4 kHz. Neither this task nor the sweep
attributes it yet.

`Electric Mint` (whose five scripts are `0x06` chunks and whose private data has
no such section) renders byte-identically before and after
(`sha256 9b6c798f…4dbfb9b`). `EEB Rick - Middle Only`, the same layout with
different slots, renders silence at key 60 without the scripts and peak 0.448
with them.
