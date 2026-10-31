# Kontakt 8 reference harness

Measure Kontakt 8 (standalone under Wine) against KONTRA on the same MIDI scenario.
Outputs go to `~/.cache/kontra-reference/` (never the repo).

    tools/kontakt-reference/kontakt.sh start "/path/Instrument.nki"   # ~1 min, GUI-scripted
    tools/kontakt-reference/run.sh "/path/Instrument.nki" tools/kontakt-reference/scenarios/note.txt
    tools/kontakt-reference/kontakt.sh stop

- `scenario.py`: scenario text (`T on|off|note|cc|pedal|bend|pc ...`) to .mid.
- `record.sh`: `aplaymidi` into Kontakt, `pw-record` from the `kontra_ref` null sink (48 kHz float).
- `sampler-native render-kontakt-midi IN.nki OUT.wav SCEN.mid`: KONTRA side.
- `compare.py`: onset-aligned RMS/peak/centroid/pitch per hop, tail time, summary.

Caveats: Kontakt applies its default -6 dB instrument volume (expect ~5-6 dB lower than KONTRA
unless the instrument volume is set to 0 dB); capture is realtime, so onsets are aligned by
detection (~ms jitter); stop Kontakt with `kontakt.sh stop` and `pactl unload-module` the sink.

## The panel picture the gate compares against

`electric-mint-panel-kontakt.png` is not one of our renders. It is the Sound Preset panel
**cut out of Kontakt 8.14.1's own page**, and it backs the `pixel diff vs Kontakt panel` row in
`tools/verify.sh`. It exists because the whole-page row (`electric-mint-page.png`, the golden)
compares us to an earlier render of *ours*, so it can only report that the page changed --
never that it was wrong when it was captured. A preset field drew a stock frame over its plate
through three review rounds for exactly that reason: each round agreed with the last wrong
picture. This reference has no memory of us in it.

Provenance: `oracle_shot` in `tools/oracle-mcp`, Kontakt 8.14.1 (portable) under wine 11.16 on
its private Xvfb `:77` with DXVK over lavapipe, Electric Mint loaded, window-relative capture
1010x698, sha256 of that capture `d77080db…` recorded in `oracle-mcp`'s step log. It was cut on
2026-10-31, alongside the three review rounds that motivated it.

### Regenerating it

    tools/oracle-mcp/client.py --script tools/kontakt-reference/panel-steps.jsonl
    python3 tools/kontakt-reference/panel-reference.py <shot from the oracle_shot step>

The oracle prints the capture's path (`.../run-<timestamp>/shot-001.png`); pass that to
`panel-reference.py`. It finds Kontakt's page inside the capture by template-matching our own
page render (`electric-mint-page.png`) against it -- an 8x8-block coarse pass, then full
resolution -- and prints the origin it found with the residual, so a capture that does not line
up says so instead of quietly writing a shifted reference. Measured 2026-10-31: origin
`(361,89)`, residual 3.55/channel. It then cuts a fixed window in *page* coordinates, the same
coordinates our render uses, so both sides of the row are compared in the same pixels:

| | |
| --- | --- |
| panel window | `444,12,177,69` in page coordinates |
| how it was chosen | the preset plate's rect, `(446,32,171,21)`, grown to take in the "Sound Preset" header row and the "Link to Song" row, 2 px clear either side |
| scale | none: Kontakt's page is 633x500 and ours is 633x500, so the crop is 1:1 |
| output | 177x69, 6 colours of plate, 12,213 px |

Re-running it against the committed file reports how many pixels the new capture differs from
the old one, so a regeneration can be checked before it is committed. If the capture was taken
on the same Kontakt build, that number is 0 -- verified: the file in this directory is the
byte-for-byte output of `panel-reference.py` on `run-20261031-152811/shot-001.png`.

The row's ceiling (21.0%, in `tools/verify.sh` next to the measurement it came from) allows for
the residue that is still ours to fix -- a plate frame one pixel off, text rasterisation, the
header pictograms, the mint swatch -- and fails on the defect it was built for: the field's
stock frame over the plate again measures 29.12%.

