# Kontakt vs KONTRA: sweep of the installed library

One note per instrument (the key is in the table), velocity 100, held 1.5 s, first 2 s compared with `tools/oracle/compare.py`'s alignment and 1/3-octave bands, through `tools/oracle-mcp/parity.py`'s verdict. Kontakt 8.14.1 standalone under Wine on a private Xvfb (PipeWire null sink `kontra_ref`), KONTRA `kontra-render` on the same MIDI file. Measured 2026-10-30.

**Numbers are numbers; causes marked `guess:` are guesses, and `evidence:` lines are quoted from KONTRA's own diagnostics.** The *Kontakt RR spread* column is Kontakt rendering the same note twice: a row's band error is only a KONTRA problem if it is larger than that, because Kontakt's round robin picks a different sample each note-on.

Verdicts use the limits `parity.py` set from the measured noise floor: level 0.5 dB, band RMS 1.0 dB, worst band 3.0 dB, lag 10 ms.

| # | instrument | family | key | level Δ dB | band RMS dB | worst band | Kontakt RR spread dB | waveform r | verdict | cause |
|---|---|---|---:|---:|---:|---|---:|---:|---|---|
| 1 | Cherry Electric Bass | electric bass (dry DI) | 40 | +44.03 | 57.94 | +108.9 dB @ 80 Hz | 1.82 | 0.00 | **FAIL** | evidence: KONTRA's diag reports `voice filters: 1 groups []` and the voice's cutoff is 93.6 Hz, so KONTRA closes the filter and renders -68 dBFS; not a sample-mapping problem |
| 2 | Showroom Violin | solo violin (dry) | 67 | -8.96 | 7.20 | -19.9 dB @ 508 Hz | 0.65 | 0.62 | **FAIL** | guess: low-mid body missing (KONTRA 9 dB quiet, -20 dB at 508 Hz); could be a filter/envelope stage rather than the samples (waveform r 0.62) |
| 3 | Haunted Strings 2 - Main | strings (twisted, heavy FX) | 62 | -9.10 | 2.07 | +9.3 dB @ 40 Hz | 1.50 | 0.83 | **FAIL** | evidence: KONTRA reports `missing impulse response` for instrument send slot 0; guess: the convolution send is the 9 dB |
| 4 | Vertigo Violin | solo violin (processed) | 67 | -2.35 | 5.83 | +12.2 dB @ 127 Hz | 0.19 | 0.95 | **FAIL** | evidence: KONTRA reports `missing impulse response` (insert slot 3); guess: that insert is most of the 5.8 dB band shape |
| 5 | Electric Mint (Melody) | session guitar (melody patch) | 60 | -0.31 | 5.38 | +19.2 dB @ 12902 Hz | 9.60 | 0.91 | **FAIL** | guess: indistinguishable from round robin -- Kontakt's own two renders differ by 9.6 dB band RMS, more than this row's 5.4 dB |
| 6 | Ovation Acoustic Bass | acoustic bass (reverb printed) | 40 | +0.90 | 3.69 | +7.3 dB @ 2560 Hz | 5.47 | 0.87 | **FAIL** | guess: within round-robin/noise (Kontakt's own spread is 5.5 dB); the +7.3 dB at 2.5 kHz may be the printed reverb |
| 7 | Parker Oboe - Workhorse | solo wind (legato) | 62 | -0.49 | 1.23 | -3.1 dB @ 202 Hz | 0.30 | 0.98 | **FAIL** | evidence: KONTRA reports an impulse-response size mismatch (send slot 0); guess: the small 1.2 dB band error is that send |
| 8 | Electric Mint | session guitar (amp + Hot Solo FX) | 60 | -0.01 | 0.85 | +1.7 dB @ 1613 Hz | 0.76 | 0.73 | pass | passes; the known unmodelled Supercharger/Replika/Choral cost <=1.7 dB per band |
| 9 | VSTBuzz Legendary | synth (tonal) | 60 | +0.00 | 0.25 | -1.2 dB @ 12902 Hz | 0.47 | 1.00 | pass | passes |
| 10 | VSTBuzz Cyber - Synth Rhythm2 | synth / rhythm loop (heavy FX) | 48 | +0.00 | 0.01 | +0.0 dB @ 12902 Hz | 0.00 | 0.55 | pass | passes; a sample loop with no live processing (Kontakt's own spread is 0.0 dB) |

## Worst five

1. **Cherry Electric Bass** -- level +44.03 dB, band RMS 57.94 dB, worst +108.9 dB at 80 Hz. evidence: KONTRA's diag reports `voice filters: 1 groups []` and the voice's cutoff is 93.6 Hz, so KONTRA closes the filter and renders -68 dBFS; not a sample-mapping problem
2. **Showroom Violin** -- level -8.96 dB, band RMS 7.20 dB, worst -19.9 dB at 508 Hz. guess: low-mid body missing (KONTRA 9 dB quiet, -20 dB at 508 Hz); could be a filter/envelope stage rather than the samples (waveform r 0.62)
3. **Haunted Strings 2 - Main** -- level -9.10 dB, band RMS 2.07 dB, worst +9.3 dB at 40 Hz. evidence: KONTRA reports `missing impulse response` for instrument send slot 0; guess: the convolution send is the 9 dB
4. **Vertigo Violin** -- level -2.35 dB, band RMS 5.83 dB, worst +12.2 dB at 127 Hz. evidence: KONTRA reports `missing impulse response` (insert slot 3); guess: that insert is most of the 5.8 dB band shape
5. **Electric Mint (Melody)** -- level -0.31 dB, band RMS 5.38 dB, worst +19.2 dB at 12902 Hz. guess: indistinguishable from round robin -- Kontakt's own two renders differ by 9.6 dB band RMS, more than this row's 5.4 dB

## Not measured (5 of 15)

| instrument | what happened | guess |
|---|---|---|
| Chris Hein Harmonica Full | Kontakt could not load it | guess: the page verdict fails and every key tried (65, 60) renders silence -- the patch likely needs a keyswitch for its articulation |
| 8Dio Fire Sax - Sustain Dynamic | KONTRA could not render what Kontakt rendered (key 58) | evidence: KONTRA refuses the file -- `group: 0: Generic error: Unexpected internal modulator category 3` (format reader gap) |
| In Session Audio Taiko Creator | Kontakt could not load it | guess: keyswitch/mapping outside the keys tried (36, 48, 60), or the custom UI defeats the page verdict and the load |
| 8Dio Studio Vintage Organ - Soul | Kontakt could not load it | guess: the preset is a preset of a multi (its page differs) and the keys tried (60, 55) do not reach its mapping |
| Soundiron Harpsichord | one side silent (b) (key 60) | evidence: KONTRA renders exact silence (peak 0.000) with 6232 zones loaded, 432 groups purged, `voice filters: 576 groups []`, and a broken script compile (`%NOTE_DURATION is not an array`) |

## How the sample was chosen, and what it could not include

The oracle's Kontakt has exactly one NICNT library registered (Session Guitarist - Electric Mint). Every other `.nki` that belongs to a library is refused with *"This instrument belongs to a library that is not installed currently!"* -- including copies outside the library folder, and after adding the library's `Settings.cfg` entry and content-database row. Non-NICNT instruments (`#Non NICNT/…`) load normally, so the sweep is Electric Mint plus two renders of it and thirteen non-NICNT instruments.

That means two requested families could not be represented at all: **Kontakt Factory Library is not installed on this machine** (no such library folder exists under `Armory/VSTs`), and the other Session Guitarist libraries (Sunburst, Ruby, Picked Acoustic/Nylon, Strummed Acoustic 2) are NICNT libraries the oracle cannot load. The spread below is still deliberate: two session-guitar patches, two basses (one dry DI, one with printed reverb), two solo winds, a sax, an organ, a harpsichord, a synthetic loop, a processed string ensemble, a textural violin, a dry violin, and a synth.

Each instrument got a fresh `oracle_up` … `oracle_down`. In one shared session, from the third load on, instruments began rendering silence in Kontakt (measured; the same instrument in a fresh session rendered normally), so the batch mode was abandoned.

`sweep.json` beside this file holds every row, including the per-band differences and the KONTRA diagnostics.
