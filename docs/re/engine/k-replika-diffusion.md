# Kontakt 8 Replika Delay, saved TYPE 4 (Diffusion): the diffuser, and how the type reaches it

Status: **answered**. Replika contains a real diffusion network: `NI::DSP::Diffuser`, embedded
twice inside `FXFDelayLight`, switched into the signal path by the saved type. "Diffusion" is
therefore not a parameter-only variant of the three-head line - it inserts an eight-stage
diffusion network into both filter stages.

This page supersedes the "no diffusion network exists" reading of the previous pass; §9 says
exactly what was wrong there.

Binary: `Kontakt 8.exe` 8.13.1, image base `0x140000000`. Method: Capstone disassembly of the
PE image, RTTI walk, jump-table reads. No oracle, no debugger, nothing measured against Kontakt.

## 1. The class, its vtable, and its xrefs

| Item | Address | Note |
|---|---|---|
| RTTI name | `0x14a4ebcc0` | `.?AVDiffuser@DSP@NI@@` |
| Type descriptor | `0x14a4ebcd0` | vftable ptr `0x1497c9858` |
| Complete object locator | `0x149943740` | signature 1, pTD `0xa4ebcd0` |
| **vtable** | **`0x1479aa618`** | 18 slots, listed below |
| ctor | `0x141a7eda0` | stores the vtable at `[this]` |
| process | `0x141aa73d0` | vtable slot 7 (offset `+0x38`) |
| param setter | `0x141ac54b0` | vtable slot 12 (offset `+0x60`), switch 0..3 |
| float setter | `0x141aa1150` | vtable slot 3 (offset `+0x18`) |
| `dense`/reset helpers | `0x141aa03e0` (slot 6), `0x141aa0dc0` (slot 4), `0x141aa2c50` (slot 5) | |

Xref counts, measured by scanning the whole file:

- **8-byte pointers to the locator `0x149943740`: 1** - at `0x1479aa610`, i.e. `vtable-8`.
- **8-byte pointers to the vtable `0x1479aa618` itself: 0** - no static object holds it.
- **RIP-relative `lea` to the vtable: 1** - at `0x141a7eddd`, inside the ctor.
- **direct calls to the ctor `0x141a7eda0`: 1** - at `0x141a7fbe2`.

So the class has exactly one vtable, one constructor and one construction site. Everything else
reaches it through the vtable, which is why a call-xref search finds no direct callers.

## 2. Where it is constructed, and how many instances

`0x141a7fbe2` is inside the filter-pair constructor `0x141a7fb90`:

```
lea rcx, [rsi + 0x100]      ; the embedded Diffuser
movzx edx, dil              ; dense flag from the filter's own argument
call 0x141a7eda0            ; Diffuser::Diffuser
```

`FXFDelayLight`'s ctor calls that filter ctor twice, with `edx = 0`:

| Core ctor site | Filter object | Embedded Diffuser |
|---|---|---|
| `0x141a7f846` | `+0x11500` | `+0x11600` |
| `0x141a7f855` | `+0x12980` | `+0x12a80` |

Two instances, one per channel pair. They are created with the effect, not with the type: the
network always exists and is simply not run unless Diffusion is selected (§5).

Object size: fields run to `+0x12d8`, so roughly `0x12e0` bytes, allocated as part of the
filter-pair object inside the 0x14980-byte core (wrapper ctor `0x1407c7d40`).

## 3. The network

From the ctor `0x141a7eda0`:

| Offset | Content |
|---|---|
| `+0x00` | vtable |
| `+0x40` | 8 stage records of `0x180` bytes (bank A) |
| `+0xc40` | 8 stage records of `0xc0` bytes (bank B) |
| `+0x1250` | two `0x30`-byte blocks |
| `+0x12a0` | float, 1.0 - slot-3 setter target |
| `+0x12a4` | byte, toggled by slots 4 and 5 |
| `+0x12a8` / `+0x12ac` | counter period / counter |
| `+0x12b0` | LCG state, seeded from an atomic counter: `seed = counter * 0x9e353f7c` (`0x141a7eedc`) |
| `+0x12b8` | 0.5 (double, `0x3fe0000000000000`) |
| `+0x12c0`, `+0x12c4` | size pair (`v`, `1 - v`) |
| `+0x12c8`, `+0x12cc` | coefficient pair |
| `+0x12d0` | dense flag |
| `+0x12d4`, `+0x12d8` | crossfaded coefficient and its complement |

Eight stages, with the delay lengths baked into the ctor from two hard-coded tables. The
non-dense table (`0x141a7f038` onward) opens with `0x241, 0x35b, 0x24b, 0x359`, then
`0x283, 0x3d1, 0x287, 0x3cb`, then `0x36d, 0x515, 0x371, 0x511`, then `0x3fb, ...`; the dense
table (`0x141a7ef1f` onward) opens with `0x223, 0x335, 0x22d, 0x32b`, then
`0x295, 0x3f5, 0x2a1, ...`. The values are samples and come in mirrored pairs (577/587,
859/857; 547/557, 821/811) - the L/R half of each stage is a few samples apart, which is what
makes the network decorrelate the channels.

The process `0x141aa73d0` runs the stages and, every `+0x12a8` frames, draws one LCG value per
stage - `state = state * 0x19fb4787 + 0x16d1b86b` (`0x141aa74c0`..`0x141aa74dc`) - and compares
it against `+0x12b8` (0.5) to pick which of two lengths each stage takes. That is the
randomised reflection pattern behind the Dense control.

## 4. The parameter surface

`0x141ac54b0` (slot 12) switches on its index argument:

| Case | Site | Effect |
|---|---|---|
| 0 | `0x141ac562a` | `+0x12c0 = v`, `+0x12c4 = 1 - v`; re-lengths the active bank through `0x141abc760` (bank A) or `0x141abc560` (bank B) |
| 1 | `0x141ac55db` | `+0x12c8 = v*k1 + k2`; crossfades with `+0x12cc` into `+0x12d4`/`+0x12d8` |
| 2 | `0x141ac5594` | same pair, from the other direction |
| 3 | `0x141ac54e5` | `+0x12d0 = (v >= 0.5)`; rebuilds bank A (`+0x40`, 8 × 0x180, `0x141abc760` + `0x141ab9e80`) or bank B (`+0xc40`, 8 × 0xc0, `0x141abc560` + `0x141ab9de0`) |

Slot 3 (`0x141aa1150`) stores a float at `+0x12a0` and touches the `+0x1240` block.
Slot 15 (offset `+0x78`) is called from `0x141a9dcdb`/`0x141a9dcf5` with `xmm1 = [core+0x930]`
but lands on `0x1404f99a0`, which is a bare `ret`.

The three per-frame feeds found pass case 0 and take their value from `[core+0x140][0]`
(`0x141ab7848`, `0x141ab4b87`, `0x141aa2e4d`), on a 5-frame cycle (`[core+0x2c0]`).

## 5. The gate: one byte, written once

The filter-pair process `0x141aa3ec0` calls the diffuser at `0x141aa41f9`:

```
cmp  byte ptr [rbx+4], bpl      ; bpl = 0
je   skip
lea  rcx, [rbx+0x100]           ; the embedded Diffuser
mov  rax, [rcx]                 ; its vtable
[rsp+0x30] = rsi                ; &L
[rsp+0x38] = rdi                ; &R
rdx = &stack pair
r9d = 1                         ; one frame
call [rax+0x38]                 ; Diffuser::process
```

`[filter+4]` is written in exactly one place: the type writer `0x141aca715`

```
0x141aca83b  mov byte ptr [rdi+0x11504], bpl     ; filter pair 1 + 4
0x141aca842  mov byte ptr [rdi+0x12984], bpl     ; filter pair 2 + 4
```

with `bpl` set at `0x141aca758` (`movzx ebp, cl`) only in the branch taken for the writer's
argument value **4**. For values 0..3 `ebp` stays 0, the diffuser is skipped, and the pair runs
as a plain three-section filter. Both pairs are processed per sample (`0x141aa9d4d` and
`0x141aa9db5` in the loop `0x141aa9480`), so in Diffusion the L/R pair is diffused twice per
sample, once through each pair's own network.

## 6. How the saved TYPE reaches that writer

The missing link from the previous pass is closed by the jump table, not by the setter's shape:

1. **Index 28 is the delay type.** The setter `0x141ac5980` stores the raw value first:
   `movss [rcx + rax*4 + 0x840], xmm6` (`0x141ac5993`), then jumps through the table at
   `0x141ac690c`. Entry 28 is `0x141ac60e5`. That case ORs the rebuild flags
   (`+0x828`..`+0x82c`), sets the pending byte `+0x829`, and calls the selector `0x141aca670`.
2. **The raw store address is `+0x8b0`** (`0x840 + 4*28`), which is exactly what the selector
   reads at `0x141aca67b`.
3. **The selector** `0x141aca670`: `mode = clamp(trunc(5.0 * v), 0, 4)` (`5.0` at `0x1447149f0`),
   compared with `[core+0x830]`; on a change it calls the writer.
4. **The writer** `0x141aca715` sets `[core+0x830] = [core+0x604] = mode`, gives all three heads
   line type 0, stops the rotator (`xmm0 = 0` at `0x141aca7d5`), and sets the two filter bytes
   from §5.

So: saved TYPE 0..4 -> module parameter **28** (normalised 0..1) -> `+0x8b0` -> selector ->
writer -> `[filter+4]` -> `Diffuser::process`. Type 4 (Diffusion) is the only value that sets
that byte.

Two consequences worth recording:

- Core index 9 (`0x141ac5d26`, `min(trunc(4v), 3)` into `+0x93c`, `4.0` at `0x1447149e4`) is a
  **different** control with four values - the BBD-type selector - which is what the previous
  pass mistook for the type.
- With index 28 = engine parameter `0x3d5` (the range-checked "Invalid Replika delay type!" id
  at `0x14078a200`) and index 8 = TIME, the module's index-to-id base is `0x3b9`
  (`0x3d5 - 28`). That is an inference from two anchors, and it is the key to naming the rest:
  index `i` should correspond to `ENGINE_PAR` id `0x3b9 + i`.

## 7. AMOUNT, SIZE, MODULATION (saved fields 19, 20, 21)

Not yet mapped to indices, but the surface they would use is now small and enumerated:

- the diffuser's stage-length setter is slot 12 case 0 (`0x141ac562a`), fed per frame from
  `[core+0x140][0]` - that is the shape a **Size** law would take;
- the coefficient pair is slot 12 cases 1 and 2 (`0x141ac55db`, `0x141ac5594`) - the shape an
  **Amount** law would take;
- **Dense** is slot 12 case 3 (`0x141ac54e5`), which rebuilds one of the two 8-stage banks;
- the float at `+0x12a0` (slot 3) is a plain 0..1 scale initialised to 1.0.

With the index-to-id base from §6, the next pass can read the setter cases for the ids holding
AMOUNT/SIZE/MODULATION and finish the law table. That is a bounded job now, not an open one.

## 8. What this means for the port

Diffusion is a distinct network, so rendering the record as Modern would indeed be wrong - and
so would any "amount" knob on the three-head line. A faithful model needs, per channel pair:
eight stages with the lengths above (or the law that turns Size into lengths,
`0x141abc760`/`0x141abc560`), the allpass coefficient law (cases 1/2), the LCG re-lengthing
every `+0x12a8` frames, and the two-bank Dense switch. All of those now have addresses; none of
them is measured, and none should be approximated from the description strings.

`replika.rs` still returns `None` for type 4. That remains correct until the law table in §7 is
read.

## 9. Corrections to the previous pass on this page

- "No allpass chain, no delay network class ... in the core": **wrong**. The network is
  `NI::DSP::Diffuser`, embedded at `+0x11600`/`+0x12a80` inside the two filter-pair objects; it
  is not a top-level sub-object, which is why the core ctor's direct sub-object list missed it.
- "`NI::DSP::Diffuser` ... no vtable reference, use unproven": the vtable is `0x1479aa618`, with
  one locator xref and one ctor call site; see §1-2.
- "The five-value selector's input `+0x8b0` has no writer": it does - the setter's raw store at
  `0x141ac5993`, `+0x840 + 4*28`; see §6.
- "If `0x141aca715` is the type writer (unproven)": it is; §6.
- The earlier notes on `+0x93c` (four values, BBD type) and the 4000 ms TIME clamp stand.
