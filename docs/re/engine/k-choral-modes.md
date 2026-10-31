# Kontakt 8 Choral (0x5d): the four voice-bank mode tables

Recovered 2026-10-30 from the Kontakt 8.13.1 binary (`FileVersion 8.13.1`,
`SizeOfImage 0xb254000`) at the initialiser the docs call `MODES`, VA `0x14193cd70`.

`k-modfx-classic.md` described these as "four mode tables, seven voices each". They are not stored
data: `0x14193cd70` is inside `.text` and is a switch on `0x740(%rcx)` (the mode) that writes float
immediates into the bank at `0x6c8(%rcx)`. The switch resolves as:

| Mode | Block |
| --- | --- |
| 0 | `0x14193d854` |
| 1 | `0x14193d4b8` |
| 2 | `0x14193d130` |
| 3 | `0x14193cd9e` |
| 4 | `0x14193cd9e` (same as 3) |

Each record is seven delays (seconds), then seven gains, then pans and the loop filters.

## Mode 0 — `0x14193d854`

```
delays  0.003  0.003  0.007  0.007  0.011  0.011  0.013
gains   1.0    1.0    1.42857 1.43  1.7242 1.72765 0.7423
pan     +0x3c 0.5   +0x44 0.5   +0x4c 0.5
filter  +0x54..0x60 6.1   +0x64 0.1   +0x68 0.1
other   +0x6c 1.0   +0x80 0.01  +0x84 0.01   +0x8c 5000   +0x90 5000
```

## Mode 1 — `0x14193d4b8`

```
delays  0.007  0.007  0.007  0.007  0.013  0.013  0.029
gains   1.0    1.0    1.0    1.0    1.0    1.0    0.7423
pan     +0x3c 0.5   +0x40 0.3333 +0x44 0.8333 +0x48 0.6667 +0x4c 0.1667
filter  +0x54 4.1   +0x58 4.1   +0x5c 4.0   +0x60 4.0   +0x64 4.12  +0x68 4.1241
other   +0x6c 1.0   +0x70 1.0   +0x74 1.0
```

## Mode 2 — `0x14193d130`

```
delays  0.011  0.011  0.011  0.011  0.011  0.011  0.053
gains   1.0    1.0    1.0    1.0    1.0    1.0    0.7423
pan     +0x3c 0.5   +0x40 0.5   +0x48 0.25  +0x4c 0.75
filter  +0x54..0x68 6.1
other   +0x6c 1.0   +0x8c 12000  +0x90 12000 +0x94 150  +0x98 150  +0x9c 20000 +0xa0 20000
        +0xa4 50    +0xb0 1.0    +0xb4 1.0    +0xc0 1.0   +0xc4 0.1   +0xc8 0.1
```

## Mode 3 — `0x14193cd9e`

```
delays  0.007  0.007  0.011  0.011  0.013  0.013  0.029
gains   1.0    1.0    1.42857 1.42857 1.7242 1.7242 0.7423
pan     +0x44 0.25  +0x4c 0.25
filter  +0x54..0x68 6.1
other   +0x6c 1.0   +0x78 0.3   +0x7c 0.3   +0x80 0.6   +0x84 0.6
        +0x8c 20000 +0x90 20000 +0x94 20000 +0x98 20000 +0x9c 20000 +0xa0 20000
        +0xa4 50    +0xc0 1.0   +0xc8 0.2
        +0xe0..0xf8 0.99 (seven one-pole coefficients)
```

## Cross-check against measured impulse responses

`tools/re/choral_modes.py` sends an impulse through the emulated kernel with the modulation held
still. The taps land at:

| Mode | First non-zero frame | Time |
| --- | --- | --- |
| 0 | 373 | 7.8 ms |
| 1 | 875 | 18.2 ms |
| 2 | 1372 | 28.6 ms |
| 3 | 872 | 18.2 ms |

Ordering by largest delay agrees with the switch: mode 0 has the shortest bank (13 ms), modes 1 and 3
the middle (29 ms), mode 2 the longest (53 ms).

## Regenerating

```
objdump -d --start-address=0x14193cd70 --stop-address=0x14193dc10 "Kontakt 8.exe"
```

then read the `movl $0x…,(%rax)` / `movl $0x…,0xNN(%rax)` immediates inside each block as f32.

## Effective bank, read back from the emulator

The initialiser leaves some fields at the constructor's values, so the blocks above are not the whole
story. Reading the bank pointer (`[core+0x6c8]`) after the constructor and the mode setter gives what
the kernel actually sees (pans default to 0.0, and mode 4 is mode 3):

| Mode | delays (s) | gains | pans |
| --- | --- | --- | --- |
| 0 | .003 .003 .007 .007 .011 .011 .013 | 1, 1, 1.42857, 1.43, 1.7242, 1.72765, 0.7423 | 0, .5, 0, .5, 0, .5, 0 |
| 1 | .007 .007 .007 .007 .013 .013 .029 | 1 ×6, 0.7423 | 0, .5, .33333, .83333, .66666, .16666, 0 |
| 2 | .011 ×6, .053 | 1 ×6, 0.7423 | 0, .5, .5, 0, .25, .75, 0 |
| 3 | .007 .007 .011 .011 .013 .013 .029 | 1, 1, 1.42857, 1.42857, 1.7242, 1.7242, 0.7423 | 0, 0, 0, .25, 0, .25, 0 |
| 4 | same as 3 | same as 3 | same as 3 |

Loop-filter constants, per mode, the six floats after the pans: mode 0 `6.1 6.1 6.1 6.1 .1 .1`,
mode 1 `4.1 4.1 4.0 4.0 4.12 4.1241`, mode 2 and 3 `6.1 ×6`.

Read back with the emulator (`tools/re/choral_modes.py` uses the same core construction):

```
bank = e.read(core + 0x6c8, 'Q'); f = e.read(bank, 'f', 56)
```

## The bank, field by field

The mode switch writes float per-voice columns into the bank; `0x14193e160` then copies them onto the
voice structs (`V` = `core+0xbb0+0x290·i`, the pointer the `process` uses). Reading the bank at
`[core+0x6c8]` and the voices after `prepare`, the columns are:

| bank | voice field | meaning |
| --- | --- | --- |
| `+0x00` | `V-0x230`, `V-0x228` | base delay, seconds; `base × (delay·5+0.1) × fs` in samples |
| `+0x1c` | `V-0x220` | gain: LFO rate multiplier (`0x14193b140`) and the output gain |
| `+0x38` | `V-0x158` | the LFO's phase offset ("pan") |
| `+0x54` | `V-0x1d8` | loop-filter value; scales the second LFO and divides its tap |
| `+0x70` | `V-0x1e0` | second LFO's tap, and the flag `V-0x1a0` = `tap > 1e-8` |
| `+0x8c` | `V-0x80` | loop-filter corner, Hz |
| `+0xa8` | `V-0x1c0` = `1-s`, `V-0x1b8` = `s` | 0 takes the lowpass tap, 1 the bandpass tap |
| `+0xc4` | `V-0x1f8` | smoother's triangle coefficient `c1` |
| `+0xe0` | `V-0x200` | smoother's one-pole coefficient `c2` |

Where a mode block does not write a column the constructor's defaults stand, which is why mode 1's
corners are 4000/6000/10000 Hz and every mode's voice 6 is 50 Hz with blend 1 and tap 0.

**Correction to the earlier tables**: `V-0x1e0` is not the voice's share of the seven-tap mix. It is
the second LFO's depth; the `cmpb -0x1a0` branch tests `tap > 1e-8`, and the mix weights come from the
*scatter* parameter alone (`0x14193a620`). The width factors are `1 − i/7` by *pair* (`i` and `i+1`
share an angle), not `1 − ⌊i/2⌋/7`.

## The kernel, per frame (`0x14193be50`)

Per frame, in this order:

1. **Seven-tap cyclic mix** (`0x141939270`, coefficients at `core+0x7c0`): a circulant over the
   *previous* frame's reads at `core+0x830`,
   `mix[j] = Σᵢ C[(i − j) mod 7] · read[i]`. The coefficients are `0x14193a620`'s scatter matrix:
   `C[0] = 1 − 0.5540970937771941·s`, `C[1] = C[2] = C[4] = −0.261203874963741·s`,
   `C[3] = C[5] = C[6] = 0.445902906222806·s` (the constants are `0x147697a88`, `0x147697ac0`,
   `0x147697a80`). Rows sum to one; `scatter` 0 is diagonal.
2. **Per voice** (`0x14193c300`), a two-pole loop filter on `x = A·G2ᵢ·mixᵢ + makeup·input`
   (`A` is the invert polarity at `core+0x1b80`, `G2` at `core+0x780`):

   ```
   k  = tan(π·hz/fs)                     # 0x14193e321, tan by x + x³/3 + x⁵/7.5
   k2 = 1 / (1 + √2·k + k²)
   q      = (s2 + k·(x − s1)) · k2
   s2, s1 = 2q − s2, s1 + 2q·k
   low    = (s1_old + s1) / 2
   band   = x − √2·q − low
   y      = (1 − blend)·low + blend·band
   ```

   `y` is written to the voice's ring twice — at `widx` and at `widx + size` (`size` = 32768,
   `mask` = `0x7fff` at `voice+0x48`) — and `widx` advances, so the read pointer always sits one frame
   ahead of the write.
3. **The LFO** (`0x14193bd90`): a triangle at `offset + phase` (`offset` = the bank's pan), then
   `phase += hz/fs` with a one-pole glide that is inert here (`voice+0x28` stays 0). The second LFO is
   the *shaped* triangle of `0x1419393d0`, `(π/2)y − 0.641592653589793y³ + 0.0707963267948966y⁵` —
   `sin(πy/2)` to 4e-4 — at rate `hz · filter`, and it is only called when `tap > 1e-8`.
   Both then feed the smoother: `s = (1−c2)·[(2 − c1(1−t))·(t+1)/4 + (½·t₂+1)·tap/filter] + c2·s`.
4. **The read**: `d = base + depth·s` samples, `trunc(d)` frames back from the read pointer, a
   Catmull-Rom between `trunc` and `trunc+1` (four taps at `trunc−1 … trunc+2`, the ring running
   backwards), giving this frame's `read`.
5. **The output** (`0x14193c191`): per voice `t = A·(read + G2ᵢ·mixᵢ) + makeup·input`, then the width
   rotation — even voices `[c s; −s c]`, odd `[c −s; s c]` from `core+0x8a0` at
   `θ = width·π/4·(1 − i/7)` per pair, voice 6 identity — scaled by `G1` (`core+0x748`), summed.
6. **The global tone filter** (`core+0x1c20..0x1cd0`): the same two-pole form with
   `k = tan(π·f1/w^¼/fs)`, `f1 = min(0.25/(base₀·(delay·5+0.1)), 50)`, `w = max(1 − G2₀, 0.2)`
   (clamps `0x144f38b08`, `0x144f38948`), and the output
   `(sum + low·(w−1) + q·√2·(√w−1)) · (mix·makeup·(0x1b70·0.707)) + (1−mix)·input`, where
   `0x1b70 = sqrt(0.2·Σ + 0.6)/Σ`, `Σ = 2 + 2(g + h)` and `makeup = (1 − min(G2₀, 0.99)⁴)^¼`.

`core+0x1c60..0x1c74` receives six smoothed `voice+0x50` products — the per-frame delay increment
times the depth, i.e. the LFO's *velocity* — and nothing in the kernel reads them back. They are a
metering feed, not audio; `Chorus` does not compute them.

## Validation against the native vector

`tools/re/kemu_timemod.py choral` renders 3000 frames of `0.5·sin(0.05n) + δ(n)` / `0.5·sin(0.031n)`
in 32-frame blocks; `crates/dsp/delay/tests/chorus.rs` pins both cases.

| frame | case 1 native (L, R) | `Chorus` (L, R) | case 2 native | `Chorus` |
| --- | --- | --- | --- | --- |
| 0 | `3f32c0a0 00000000` | bit-exact | `3e820df2 00000000` | bit-exact |
| 1 | `3c7145ad 3c314b45` | bit-exact | `3bc4ddf0 3b80fe4e` | bit-exact |
| 150 | `3ea49034 bebce4dd` | bit-exact | `3defac3e be071ad4` | bit-exact |
| 777 | `3e88e9e0 be871bd9` | `3e88e9de be871bdc` | `3ded953d bddf899f` | bit-exact |
| 1500 | `be883379 3e1418bb` | `be88337b 3e1418c0` | `bd45a8a0 bdf18584` | bit-exact |
| 2999 | `bdc85a44 bea4ea2d` | `bdc85a32 bea4ea28` | `be25b166 3d3db892` | bit-exact |

Case 1 (mode 1, amount .6, invert) is within 2.5e-6 over all 3000 frames (mean 1.2e-7); case 2 (mode 2,
amount .3, fully wet) within 2.8e-8. The residual is not a missing term: every intermediate checked
against the emulator (`mix`, `s`, `d`, the loop filter's `s1`/`s2` and `y`, the read) agrees to the
last bit or a few ulps, so what remains is the emulator's own `sincos` at `0x1443c9bd0`, which returns
`0.9510565148` for `cos 18°` where libm returns `0.95105651629515`.

## What is still unread

Nothing on the audio path. Still open, and inert for this effect:

- **`core+0x1c60..0x1c74`** — written (six smoothed LFO velocities) and read by nothing in the kernel.
  Whose display consumes it is not traced.
- **`0x14193c7c0`** (the modulation-matrix update) returns at once unless `core+0x3d5` is set, which
  the Choral construct/host path never sets.
- **`core+0x680`** is a *pointer* to the bank, not a second table; its `[0]`, `[4]` … floats are the
  bank's delays, which is how `0x14193ae70` gets `baseᵢ/base₀`.
- **Mode 2's `+0x94/+0x98/+0x9c/+0xa0`** (150/20000) are read into `V-0x188`/`V-0x190`, which the
  kernel never touches — a second, unused filter pair.
