# Kontakt 8 Replika Delay, saved TYPE 4 (Diffusion): what the binary says

Status: **not modelled, and the model is not derivable from what this pass read**. The saved
type is a five-value enum, the effect runs one fixed per-sample network for every type, and no
diffusion/allpass network was found inside it. This page records every address read, what was
recovered, and the links that are still missing, so the next pass starts from here instead of
re-deriving it.

Binary: `Kontakt 8.exe` 8.13.1 (image base `0x140000000`). Method: Capstone disassembly of the
PE image and direct data reads. No oracle, no Ghidra, no completed Kuna export. Nothing here is
measured against Kontakt.

## 1. The saved type is a five-value enum, mapped by identity

The record reader (`0x140d02250`; the v0x11 branch is `0x140d0241a`, v0x12 is `0x140d0226c`)
reads field 0 through `0x140a847f0`:

```
call 0x142a78270            ; read one enum
test eax,eax / je  -> [out]=0
sub eax,1 / je     -> [out]=1
sub eax,1 / je     -> [out]=2
sub eax,1 / je     -> [out]=3
cmp eax,1 / jne    -> default [out]=0
                   -> [out]=4
```

Stored 0..4 lands at parameter-object `+0xa8` unchanged. The five names are consecutive at
`0x144746b28`..`0x144746d38` ("Type: Modern" … "Type: Diffusion"), in the same order.

## 2. Two four/five-value "mode" controls exist, and neither is tied to the type yet

| Control | Address | Law | Values |
|---|---|---|---|
| core index 9 → `+0x93c` | `0x141ac5d26`..`0x141ac5d7c` | `min(trunc(v*4.0), 3)`, `4.0` at `0x1447149e4` | 4 |
| `+0x830`/`+0x604` writer | `0x141aca670` → `0x141aca715` | `min(trunc(w*5.0), 4)`, `5.0` at `0x1447149f0` | 5 |

Index 9 has four values, which matches the UI string "BBD Type: Selects one of four BBD delay
models (Grunge, Dark, Warm, Clean)" (English description strings at `0x144...`/`.rsrc`), not
the five delay types. The earlier pages' note that "TYPE 4 would clamp to 3 at `+0x93c`"
therefore describes the wrong control; `+0x93c` is **not** the delay type.

The five-value control is equally unproven as the type path: it reads `[core+0x8b0]`, and the
only writer of `+0x8b0` found in the core (`0x141aa7aac`, inside `0x141aa7480`) stores a
**fractional part** of a four-lane phase computation, not a saved parameter. So `+0x8b0` looks
like lane state, and the path from saved TYPE to either control is **not established**.

## 3. The per-sample network is fixed for every type

`NI::DSP::FXFDelayLight` is one class for all five types. The wrapper ctor `0x1407c7d40`
allocates 0x14980 bytes, constructs the core at `+0x1c0` (`0x141a7f4d0`), and zeroes 53
parameter slots (`+0x145f0` pointers, `+0x14580` enable bytes, `+0x14798` dwords). The core
ctor builds exactly these sub-objects:

| Offset | Object | Ctor |
|---|---|---|
| `+0xac0`, `+0x6380`, `+0xbc40` | three "head" tap objects | `0x141a7ea50` |
| `+0x11500`, `+0x12980` | two filter/section pairs (3 sections each) | `0x141a7fb90` |
| `+0x13e80`, `+0x13f80` | two modulator objects | `0x141adc050` |
| `+0x14080`, `+0x1410c` | two saturators | `0x141a82070` |
| `+0x14198`, `+0x141cc` | two two-state sections (reverb-family code) | `0x140d93d50` |

There is no allpass chain, no delay network class, and no fourth head. The per-sample loop
`0x141aa9480` confirms the order: input pair, ducking envelope, head 1 write+read, head 3 read,
modulator (`+0x13f80`, `vtable+0x38`, only when `path & ~4 == 0`), sum/scale, filter pair 1,
head 2 write+read, filter pair 2, head 3 write, saturation (`0x141aa44a0`), 2×2 output matrix,
feedback. Head write is `0x141acd110`, head read `0x141ab7a90` / `0x141ab8c60`, filter pair
`0x141aa3ec0`, modulator `0x141ae3ba0` (LFO phase + table lookups, **no
delay-line reads** - it is a modulation stage, not a diffuser).

Head write/read carry a four-way *line character* at `+0xc0`, set by `0x141aca4a0`:
0 = plain two-ring write, 1 = nonlinear/table stage (`0x141aa3c60`), 2 = wow-flutter
(`0x14199c870` driven by `[head+0x5880]`), 3 = smoothed stage (`0x141aa31f0`). Those four
match the four BBD models, again pointing at index 9 rather than the delay type.

## 4. If `0x141aca715` is the type writer (unproven), mode 4 does this

Recorded because it is cheap to re-check, not as an assumption:

```
[core+0x830] = [core+0x604] = arg
switch (arg): 0->1->2->3 keep the value, 4 -> head type 0 and flag=1, other -> head type 0
```

- all three heads get line type 0 (clean);
- the rotator (`+0x990`, `+0x980`, `+0x988`) is fed 0, so it stops;
- `[core+0x11504]` and `[core+0x12984]` (first byte of each filter pair) are 1 only for this
  argument value;
- `setParam(arg)` on both modulators; saturators refreshed (`0x141aba680`); section states
  `+0x14198`/`+0x1419c`/`+0x141cc`/`+0x141d0` zeroed.

The only per-sample branch keyed on `path == 4` is `0x141aaa15f`: with `path == 4` and the
output-level follower `+0x14328` non-zero, its log is thresholded and the active flag `+0x41`
is held. That is a **tail-hold test**, not a separate signal path.

So even under the unproven reading, "Diffusion" would be the same network with clean heads and
the LFO stopped - not a diffusion network.

## 5. The class that could provide one was not located

`NI::DSP::Diffuser` exists in the image: RTTI name `.?AVDiffuser@DSP@NI@@` at `0x14a4ebcc0`,
type descriptor `0x14a4ebcd0`, complete object locator at `0x149941944`. No 8-byte reference to
that locator was found, and no constructor call into a Diffuser-shaped object appears in the
Replika core ctor. Whether Replika can use it at all is **unproven**; finding its vtable is the
first thing a continuation should do.

## 6. Still unread

| Piece | Consequence |
|---|---|
| The writer that puts the saved TYPE anywhere the DSP reads | No dispatch can be claimed; `+0x8b0`, `+0x93c` both fail as the type. |
| AMOUNT (record field 19), SIZE (20), MODULATION (21) destinations | These are the knobs the UI describes as "amount of diffusion … resulting in a reverb effect"; without them no diffusion law exists. `k-replika.md` has 20..24 as "raw store only". |
| Head time/gain laws | `0x141a9d4a0`, `0x141a9d660`, `0x141a9d7c0` (head index/time; entry read, constants not decoded), `0x141abba60` (three section gains), `0x141abc0d0`, `0x141acac40`, `0x141acb240`, `0x141acb2b0`, `0x141ac44a0`. |
| Filter pair flag (`+0x11504`) | Only set for the value in §4; its reader was not found, so its meaning is open. |

## 7. Consequence for the port

`replika.rs` keeps returning `None` for saved types 1..4 and keeps naming them in the gap log,
now pinned by `the_other_four_types_are_named_not_rendered` so a later change cannot quietly
fold Diffusion onto Modern's line. Modelling it needs §6 closed first: a five-knob delay whose
network was guessed would be invented DSP.

Two earlier statements are corrected here:

- `k-replika.md` / `k-replika-core.md`: `+0x93c` is a four-value control consistent with BBD
  Type, not the saved delay type.
- `k-replika-core.md`: the TIME clamp is 4000 ms (`0x14756bdf8` = 4000.0), confirmed;
  `0x144f39ff8` = 0.00025 normalises it to 0..1.
