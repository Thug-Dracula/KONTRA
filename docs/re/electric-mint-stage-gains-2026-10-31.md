# Electric Mint: the two stage gains, fixed (2026-10-31)

Follows `docs/re/electric-mint-slot-liveness-2026-10-31.md`, which measured, inside Kontakt itself,
that slot 4 (Replika in its Diffusion type) is level-neutral and slot 2 (Supercharger GT) costs
−2.23 dB. KONTRA added +1.54 dB for the first and nothing for the second. Both are now fixed, and
the chain reads what the two errors were hiding.

## The two laws

### Slot 4, Replika Diffusion: the record's knob order was wrong, not the mixing

Nothing in the wet/output scaling changed: the slot's `output` 0.29891846 and `dry` 0.9999979 are
applied exactly as before, and so are the rows, the interpolation and the re-length clock. What
was wrong was **which saved float is SIZE and which is AMOUNT**.

`crates/format/kontakt/src/replika.rs` read fields 18..22 as BBDTYPE, **AMOUNT, SIZE**, MODULATION
at bytes 60, 64, 68, 72 (`k-replika-diffusion.md` §11.5's table). Electric Mint's record is

```
  04 00 00 00 00 00 40 40 bf ee dc 3e d4 2c 44 3f
  92 ce 5c 3f cd cc cc 3d 00 00 80 3e 9a 99 99 3e
  00 00 80 3f 00 00 00 00 cd cc cc 3d 01 00 00 00
  00 00 3e 00 00 fa 43 00 00 80 3f 00 00 00 80 3f
  00 00 80 3f b9 4f c6 3e 8a 3b e6 3e 01     (77 bytes, dumped from the .nki)
```

so byte 60 = 1.0, byte 64 = **1.0**, byte 68 = **0.38732699**, byte 72 = 0.44967300, byte 76 = 1
(DENSE). Read the documented way, SIZE came out 0.3873 and AMOUNT 1.0.

The module itself says otherwise. The diffusion capture
(`k-replika-diffusion.md` §11, taken from this very instrument's live type-4 Replika) recorded

- `+0x12c4 = π/2 − SIZE = 0.5707964` → **SIZE 1.0**,
- `g = 0.95·AMOUNT + 0.05 = 0.417960644` → **AMOUNT 0.38732699**,
- `spread = g·MODULATION·0.02 = 0.00375891` → **MODULATION 0.44967300**,

and its tap table (`vectors/replika-diffusion-taps.tsv`) is stage lengths × 0.4142 — stage 0 reads
241/356/245/355 of 577/859/587/857. The only assignment of this record's four bytes that produces
all three live values is **BBDTYPE, SIZE, AMOUNT, MODULATION** at 60, 64, 68, 72.

That is a gain error because `sin⁸ angle` is the network's direct path, and the direct path is the
only part of the wet that is coherent with the dry:

| | angle | `sin⁸ angle` (direct) | tap scale |
|---|---:|---:|---:|
| read as AMOUNT@64, SIZE@68 | 1.1834694 | **0.5407** | 0.9910 |
| the record's own order | 0.5707964 | **0.00726** | 0.4142 |

`0.29891846 × 0.5407 = 0.1617` of the dry, added in phase, is the +1.5 dB. The fixed read puts the
direct path 37 dB down, and the slot measures **+0.019 dB** against Kontakt's **+0.039 dB**.

The fix: `size: tail(64)`, `amount: tail(68)`. Pinned by
`replika::tests::the_v11_record_reads_electric_mints_replika`, which now uses the instrument's full
77-byte record and asserts the angle 0.5707964 and stage 0's tap 239.0 (0.4142 × 577) against the
capture's 241.16.

> **Superseded later the same day.** The tap drift was read — two rotors, `2π(1.5 − MODULATION)/rate`
> and ×10/7, one drift per ring pair, reversed on the tick — and the port implements it, so the taps
> no longer sit at the window floor. See `docs/re/engine/k-replika-diffusion.md` §11.4. The paragraph
> below describes the port as it was when these numbers were taken, and is kept for that reason.

**Was open when these numbers were taken:** the tap drift (§11.4) was unread, so the port held the taps at the window
floor and the diffused tail was under-smoothed — the slot now has the right *level* but still less
short-time movement than Kontakt's (band RMS 0.045 dB and a 0.08 dB sweep change, against Kontakt's
0.30 dB and 0.55 dB). That is the `Replika diffusion tap drift` gap, not a gain law.

### Slot 2, Supercharger GT: the module's static make-up, a constant in the binary

New `crates/format/kontakt/src/supercharger.rs`. The CompXT core's gain computer, detector and
ballistics are not recovered (`docs/re/engine/dyn-compressors.md`), so the *compression* is not
modelled and is reported (`Supercharger gain only (CompXT compression not modelled)`).

What is modelled is the module's static transfer, which is a constant. Kontakt's own measured
transfer over a 200 Hz ramp, every input level 0.2..1.0, is 1.0716/1.0717 for TRIM 0/0.5/1,
SATURATION 0/0.5 and COMPRESS 0/0.5/1.0 (`docs/re/engine/vectors/supercharger-static.tsv`), and that
value is a literal in the image:

| constant | where |
|---|---|
| `1.0716536` (f32) | `0x1479cb194`, `0x1479d1150` |
| `1.0716535899579933` (f64) | `0x1479b6048`, `0x1479bd140` |

(`docs/re/kontakt-rdata/constants.tsv`.) Electric Mint stores SATURATION 0, COMPRESS 1.0 and
CHARACTER 1.0 — three knobs the vector reads as that constant.

The slot's stored Output is applied on top, as for every other module:
`0.7035764 × 1.0716536 = 0.75407` = **−2.453 dB**, against Kontakt's measured **−2.228 dB**.
The **0.22 dB** difference is the CompXT's own level dependence and its MIX blend (the record stores
MIX 0.2062, and the vector's MIX rows read 1.0 at both ends, which `dyn-compressors.md` already
records as unexplained); it is reported, not fitted. SATURATION above 0 is level-dependent
(1.0045 at 0.6, 1.0315 at 1.0) and is reported too.

## Measured, four configurations

Kontakt side: the six clean `orig`/`control`/`sup2`/`chor6`/`diff4`/`both46` captures of
`electric-mint-slot-liveness-2026-10-31.md`. KONTRA side: one `kontra-render` per configuration, same
MIDI (`one-note.mid`, key 60 velocity 100 held 1.5 s), same 2 s window from the onset, same metrics.

| configuration | level vs Kontakt (dB) | band RMS (dB) |
|---|---:|---:|
| **full chain** (`control`), before | **+0.291** | 0.98 |
| full chain, Diffusion fixed only | −1.263 | 0.93 |
| full chain, Supercharger fixed only | −2.161 | 1.18 |
| full chain, both fixed | **−3.716** | 1.23 |

Per stage, KONTRA's own contribution (level when the slot is ablated, negative = the slot takes it out):

| slot | Kontakt | KONTRA before | KONTRA after |
|---|---:|---:|---:|
| 6 Choral | −1.331 | −1.255 | −1.255 (untouched) |
| 4 Diffusion | +0.039 | +1.535 | **+0.019** |
| 2 Supercharger | −2.228 | 0.000 | **−2.453** |

`parity.py` on the full chain, one run per Kontakt `orig` capture (six): before
**PASS** level −0.24…−0.31 dB, band RMS 0.84…0.98; after **FAIL** level −3.70…−3.77 dB, band RMS
1.15…1.23.

New fingerprints:
full chain `kontra-render` sha256 `8da4b950e6cec1a9…` (was `99a24700a1501ac6…`), and the gate's own
`kontra-inspect play` fingerprint **`audio FNV-1a: 7f606e026bb07c2e`** (was `2c139e8d7cda4317`).

## The cancellation is gone, and what it was hiding

The delivered +0.28 dB was **three** errors: +1.50 (Diffusion too loud), −2.23 (Supercharger missing)
and about **−3.44 dB** in everything else. With both stages right the chain reads **−3.72 dB**, and
the per-band residual is flat (every band 3.0…5.5 dB, no tilt), so the rest of the chain is a gain,
not a shape. That is exactly the item `electric-mint-slot-liveness-2026-10-31.md` §5.1 left to the
next measurement: "the rest of the chain — slots 1, 3, 7, and the master/output path — must be adding
back more than 3 dB that KONTRA does not". It is **not** attributable to slots 2, 4 or 6 and is not
touched here.

No other instrument moves: Haunted Strings 2 (`06cf13eee3f8f54e…`) and Cherry Electric Bass
(`0279ba0fd86927bd…`) render byte-identical before and after, and neither stores a Replika or a
Supercharger (Haunted Strings 2: 0x10, 0x11, 0x16, 0x17, 0x18, 0x1e, 0x22, 0x44; Cherry: 0x16, 0x17,
0x18, 0x1a).

## Reproducing

```
CARGO_TARGET_DIR=/var/tmp/tgt-gains2 cargo build --release -p kontra-host --bin kontra-render
LIB="/home/dracula/Armory/VSTs/Kontakt1/Native Instruments Session Guitarist Electric Mint 1.1.0/\
Session Guitarist - Electric Mint Library/Instruments"
for c in orig control sup2 chor6 diff4 both46; do
  nki="$LIB/__slotliveness/$c.nki"; [ "$c" = orig ] && nki="$LIB/Electric Mint.nki"
  /var/tmp/tgt-gains2/release/kontra-render --instrument "$nki" \
      --midi /var/tmp/live/cap/one-note.mid --out /var/tmp/gains2/after/$c.wav --quiet
done
python3 /var/tmp/gains2/compare_configs.py --kontra /var/tmp/gains2/after
python3 tools/oracle-mcp/parity.py --kontakt-wav /var/tmp/live/cap/orig/clean-1.wav \
    --out /var/tmp/gains2/parity/run1
```
