# Kontakt 8 Replika Delay, saved TYPE 4 (Diffusion): the diffuser, and how the type reaches it

Status: **answered, one residual in the tail**. Replika contains a real diffusion network: `NI::DSP::Diffuser`, embedded
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
eight series stages with the lengths above, the 4x4 mixing rows, the interpolation, the LCG
re-lengthing every `+0x12a8` frames, and the two-bank Dense switch. Section 10 records how far
that got, and exactly which of those still resists.

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

## 10. Implementation pass: structure read, the mixing kernel does not ship

Second pass, 8.13.1. What was read end to end this time, and what stopped the port.

### 10.1 The setters, and the laws that are now exact

`0x141ac54b0`, switch on the index argument (cases 0..3):

| Case | Site | Law |
|---|---|---|
| 0 | `0x141ac562a` | `+0x12c0 = v`, `+0x12c4 = 1.5707964 - v` (`0x144714960` = pi/2); then every stage of the active bank is re-lengthed with that scale: `0x141abc760` (bank A, `0x180` records) or `0x141abc560` (bank B, `0xc0` records) |
| 1 | `0x141ac55db` | `g = 0.95*v + 0.05` (`0x144737744`, `0x1447148c0`) into `+0x12c8`; `spread = g * [+0x12cc] * 0.02` (`0x1447148b8`) into `+0x12d4`; `+0x12d8 = g - spread` |
| 2 | `0x141ac5594` | the same with `[+0x12cc] = v`: `spread = [+0x12c8] * v * 0.02` |
| 3 | `0x141ac54e5` | `+0x12d0 = (v >= 0.5)` (`0x14470bad0`); rebuilds bank A (`+0x40`, 8 x `0x180`, `0x141abc760` + `0x141ab9e80`) or bank B (`+0xc40`, 8 x `0xc0`, `0x141abc560` + `0x141ab9de0`) |

The three coefficients the process consumes are therefore `+0x12c4` (length scale),
`+0x12d8` (mixing gain) and `+0x12d4` (tap spread). Which saved knob feeds case 0 / 1 / 2 is
still open (Size is the shape of case 0, Amount the shape of case 1/2).

### 10.2 The process, read to the bottom

`0x141aa73d0`:

- Every `+0x12a8` frames (`+0x12ac` counts down) it draws one LCG value per stage,
  `state = state * 0x19fb4787 + 0x16d1b86b` (`0x141aa74c0`), and compares it against
  `+0x12b8 = 0.5` to choose between the two lengths a stage can take.
- It then computes tap read positions: for every stage, four integers plus four fractional
  remainders, written into per-stage arrays (`[+0x170]`, `[+0x2f0]`, `[+0x470]`, ...) from the
  stage's integer lengths (`[+0x158]`, `[+0x2d8]`, `[+0x458]`, ...) scaled by
  `g + spread`, `g - spread` and their L/R mirrors.
- Dense (`+0x12d0 != 0`) runs the `0x180` bank: a loop of **8 stages, stride `0x180`**
  (`0x141aa7c5c`..`0x141aa7cac`), each stage processed by `0x141aa36c0`, chained in series (the
  stage's output pair is stored back into the input pair for the next iteration,
  `0x141aa7c93`). Non-dense runs the `0xc0` bank from `0x141aa7cb3`, two taps per stage instead
  of four.

### 10.3 The stage kernel `0x141aa36c0`, and why it blocks the port

This is not an allpass. It is a 4-tap feedback node:

- four tap samples in `+0x90`, `+0x94`, `+0x98`, `+0x9c`, each broadcast and multiplied by a
  coefficient row at `+0x00`, `+0x10`, `+0x20`, `+0x30`, summed into `+0x80`;
- input injection through the rows at `+0x40` and `+0x50`, added to that sum;
- the output at `+0xe0` / `+0xe4` from the rows at `+0x60`..`+0x78`, then written into **four
  ring buffers** per stage (readers at `+0x100`: rings at `+0`, `+0x30`, `+0x60`, `+0x90`, each
  with its own write index, mask and length), after which the ring heads advance
  (`0x141aa3864`..`0x141aa3908`);
- the taps are read back at integer offsets with a 2-sample bias and at least two taps per ring
  (`0x141aa398b` onward).

So the running object is 8 series stages x 4 taps x 4 rings: 32 delay lines per channel pair.
The rows above are filled by `0x141abc760`, which calls `0x1443c95d0` once (a sin/cos pair in
xmm0 lanes 0 and 1), builds `c*c`, `s*c`, `s*s` with a single sign-mask XOR
(`0x2c58331` = `0x80000000`) at `0x141abc859`..`0x141abc886`, and then replicates the 4x4 block
with vectorised copies.

**What resists:** which matrix entry receives which product, the sign pattern beyond the three
stores read, and the interpolation the ring reads use. Those are the coefficients of the
network; writing them from intuition would be invented DSP, so nothing was shipped.

### 10.4 What a watchpoint would need to capture

A live Replika (the oracle lane) with a debugger on the diffuser object would settle this in one
session. Break at the process entry (`0x141aa73d0`) with `[filter+4] != 0` and dump:

1. one bank-A stage (`0x180` bytes): rows `+0x00`..`+0x78`, the four ring descriptors at
   `+0x100` (base, length, mask), and `+0x90`..`+0x9c` for one frame;
2. `+0x12c4`, `+0x12d4`, `+0x12d8`, `+0x12d0` and the 8 integer lengths, for two or three
   known Size/Amount/Dense settings - that turns the "which entry gets which product" question
   into a fit against `sin`/`cos`;
3. one impulse response per channel with Dense 0 and 1, to pin the tap assignment and the
   interpolation.

With those, the shared DSP can be written from data rather than from a reading of the vectoriser.

## 11. The oracle pass: rows, taps and the impulse response (8.14.1), and the port

Third pass. Everything below was read from a **live** Kontakt 8.14.1 (Electric Mint loaded,
Replika in its saved TYPE 4) through `tools/oracle-mcp` and gdb, or disassembled in the 8.14.1
image; the addresses are 8.14.1's, and §1-10's 8.13.1 offsets move by about +0x16_0000 in
`.text`. §11.2-§11.5 say what the numbers are; §11.6 says what is still unread.

The 8.14.1 anchors, found from the RTTI name `.?AVDiffuser@DSP@NI@@` at `0x14a6a21b0`:

| Item | 8.14.1 |
|---|---|
| vtable | `0x147b18e98` (slot 7 = process) |
| ctor | `0x141bc1cd0` |
| process | `0x141bea300` |
| stage kernel (dense bank) | `0x141be65f0` |
| row builder | `0x141bff690` |
| stage ctors | `0x141bc0080` (bank A), `0x141bbff20` (bank B) |
| param setter (slot 12) | `0x141c083e0` |
| float setter (slot 3) | `0x141be4080` |

### 11.1 How to attach, for the next person

`oracle_up` leaves one wine process whose PE is mapped at its preferred base, so the module base
is `0x140000000` and every VA above is directly breakable — no relocation, no symbol file.
Attaching a debugger needs nothing special:

```
gdb -p <kontakt pid> -batch -nx -ex "break *0x141bea300" -ex continue ...
```

`ptrace_scope` is 0 on this machine, so the same-user attach works. Three practical points,
all learned the hard way:

- **Break on audio, not on load.** `process` is only called while the engine is running; start
  a render (`oracle_render`) and attach during it, or the breakpoint never hits.
- **Two calls per sample.** Both filter pairs run a diffuser per frame, so the breakpoint hits
  twice per sample with different `rcx` values; filter on the object you want.
- **Inferior calls are unreliable here.** `call` into the process works for small functions
  (the row builder) but aborts whenever another wine thread takes `SIGUSR1`, which wine uses
  for thread control: `process` itself, and the param setter, fault at their first instruction
  mid-call. The captures below therefore use *no* inferior calls: read the object, or drive the
  network by breakpoint-stepping (write the input at the entry trap, read the output at the
  return trap).
- **A session goes deaf after two or three renders** (the sweep's known failure). Fresh
  `oracle_down`/`oracle_up` per capture; a fourth render returns silence while the object is
  still valid.

Object identity: `rcx` at `0x141bea300` is the Diffuser; the filter pair that owns it is
`rcx − 0x100`, and `[filter+4]` is the gate byte (§5), read as 1 while Diffusion plays.

### 11.2 The rows, and the exact law

All eight bank-A stages carry **identical** rows, so the builder's angle is one value for the
whole network (`+0x12c4 = π/2 − SIZE`, measured: SIZE 1 gives `0.5707964`). For that angle the
stage record `+0x00..+0x5c` read, in floats:

```
row0  -0.2918682  -0.4545578  -0.005838143  -0.009092368
row1   0.8413027  -0.5401943   0.0168283    -0.01080533
row2   0.005838143 0.009092368 -0.2918682    -0.4545578
row3  -0.0168283   0.01080533  0.8413027    -0.5401943
row4   0.4545578   0.7079318    0.009092368   0.01416052
row5  -0.009092368 -0.01416052  0.4545578    0.7079318
row6   0.841471    0            0            0
row7   0            0.841471     0            0
+0x148 (direct) 0.5403023
```

They are `M(angle)` rotated 0.02 rad in the (0,2) and (1,3) planes, with `s = sin angle`,
`c = cos angle`:

```
M = [ -s²  -s·c   0    0  ]     input rows  [ s·c  c²   0    0 ]
    [  c   -s    0    0  ]                 [  0    0   s·c  c²  ]
    [  0    0   -s²  -s·c]
    [  0    0    c   -s  ]
```

and the rotation is the 4×4 the stage ctor stores at `+0xa0` with 0.02 as a constant:

```
[ cos 0   sin 0 ]      cos 0.019998666 = 0.999800026
[ 0  cos 0   sin]      sin 0.019998666
[-sin 0  cos 0 ]
[ 0 -sin 0   cos]
```

Rows 6 and 7 and `+0x148` are stored **unrotated**: `+0x60 = c`, `+0xd0... +0x148 = s`. The
rotation is applied by the trailing multiply at `0x141bffa5f`, which reads `+0x00/+0x04/+0x08/
+0x0c`, `+0x10..`, `+0x20..`, `+0x24`, `+0x34`, `+0x38` and the `+0xa0` block — that multiply is
why the fingerprint of the rows is `M·R` and not `M`.

Residual: the port computes `M·R` in `f32` from the same constants and agrees with the capture
to **≤ 6 ulp (5×10⁻⁷ relative)** on every entry; the difference is the order of the multiplies,
not the values. `crates/dsp/delay/src/diffusion.rs`'s `the_rows_are_the_angle_rotated_by_two_
hundredths` holds the numbers.

### 11.3 The stage kernel, the taps and the interpolation

Kernel `0x141be65f0`, per frame, per stage:

```
taps      = four ring reads, interpolated (§ below)
out[ch]   = taps[0]·row6[0] + direct·in[ch]           (ch 0 uses tap 0, ch 1 tap 1)
feedback  = taps·rows0..3 + in[0]·row4 + in[1]·row5
ring[j]  <- feedback[j], write head += 1
```

The ring read at `0x141be6845` is `index = (write − whole + k) & mask` for `k = 2, 1, 0, −1`,
i.e. four consecutive samples ending one before the whole part, and the four are interpolated
with **Catmull–Rom** (constants 1.5 at `0x1477f5740` and 2.5 at `0x147800110`):

```
q = ½·(2p₁ + (p₂−p₀)t + (2p₀−5p₁+4p₂−p₃)t² + (3(p₁−p₂)+p₃−p₀)t³)
     p₀ = x[+2]  p₁ = x[+1]  p₂ = x[0]  p₃ = x[−1]     (relative to write − whole)
```

Verified exactly: the four ring windows of stage 0, their fractions, and the four values
Kontakt returned agree to the digits printed (≤ 1·10⁻⁷ on values of 5·10⁻³), which is why the
order `p₀..p₃` above is not a guess.

Rings: each `Ring` is `{write: u64, data: ptr, end: ptr, capacity: ptr, length: u64, mask: u64}`
at `+0x00..+0x28`, stride `0x30`; the mask is a power of two minus one (1023, 2047 or 4095 in
the capture), and the write wraps at the mask, not at `length`.

Tap positions (`0x141bea589`, per stage, **every frame** — the recompute sits outside the
re-length branch):

```
scale_j = (g − spread) + spread·drift_j          drift_j in −1..1
whole_j = trunc(length_j · scale_j)              frac_j = length_j·scale_j − whole_j
```

with `g = 0.95·AMOUNT + 0.05`, `spread = g·MODULATION·0.02` (`0x1448798ec`, `0x1448569c0`,
`0x1448569b8`). `drift_j` is a component of one of two rotors; §11.4 gives the mapping. The
captured tap table (`docs/re/engine/vectors/replika-diffusion-taps.tsv`) shows one window:
stage 0's scales are 0.4179526/0.4144470/0.4179526/0.4144470 inside
`[g − spread, g + spread] = [0.4142017, 0.4217196]`.

The stage lengths are the ctor's table, not the knobs: 577/859/587/857, 643/977/647/971,
877/1301/881/1297, 1019/1531/1033/1523, 1823/2711/1831/2707, 2293/3389/2309/3371,
3541/2377/3547/2371, 3823/2579/3833/2557 samples at 48 kHz. The ctor's *other* table
(0x223, 0x335, ... in §3) belongs to a construction with the dense flag set, which
`FXFDelayLight` never does (§2) — which is why the lengths are the same in both banks.

### 11.4 The two rotors, and which ring takes which — read (fourth pass)

The question §11.3 left open was narrow: given `g ± spread` and the LCG, **which of the four
rings of a stage gets which drift, and from which rotor**. It is answered now, twice over — once
from the process, once from the live object's own numbers.

**The state.** Two rotors, not one per stage: `+0x1240` and `+0x1270`, `0x30` bytes each, six
doubles — `{x, y, cos δ, sin δ, δ, 1}`. `(x, y)` is the unit vector `(cos θ, sin θ)`; `δ` is the
step; `sin δ` carries the direction and is what the tick flips. The ctor zeroes the blocks and
the rate setter (`0x141be4080`) fills them from `0x141c074b0`:

```
δ₁ = 2π · (1.5 − MODULATION) / rate
δ₂ = 10/7 · δ₁                  // literally: (1 − MOD)·10/7 + 5/7
```

and then rotates `(x, y)` by `(cos δ, sin δ)` **once per frame** (`0x141bea454`), after which the
taps are recomputed. So the drift is a 1.05 Hz and a 1.5 Hz motion at Electric Mint's MODULATION,
not a per-tick choice, and the taps move ~0.002 samples a frame on the longest ring.

**The mapping** (`0x141bea589` onward, eight stage blocks in order, each with its four scale
values in ring order):

| stage | ring 0 | ring 1 | ring 2 | ring 3 |
|---|---|---|---|---|
| 0 | F + s·sin θ₁ | F + s·cos θ₁ | F + s·sin θ₁ | F + s·cos θ₁ |
| 1 | F − s·sin θ₁ | F − s·cos θ₁ | F − s·sin θ₁ | F − s·cos θ₁ |
| 2 | F + s·cos θ₁ | F + s·sin θ₁ | F + s·cos θ₁ | F + s·sin θ₁ |
| 3 | F − s·cos θ₁ | F − s·sin θ₁ | F − s·cos θ₁ | F − s·sin θ₁ |
| 4 | F + s·sin θ₂ | F + s·cos θ₂ | F + s·sin θ₂ | F + s·cos θ₂ |
| 5 | F − s·sin θ₂ | F − s·cos θ₂ | F − s·sin θ₂ | F − s·cos θ₂ |
| 6 | F + s·cos θ₂ | F + s·sin θ₂ | F + s·cos θ₂ | F + s·sin θ₂ |
| 7 | F − s·cos θ₂ | F − s·sin θ₂ | F − s·cos θ₂ | F − s·sin θ₂ |

with `F = g − spread`, `s = spread`. In words: **rings 0 and 2 of a stage read the same drift,
rings 1 and 3 the other**; stages 0–3 and 4–7 use the two different rotors; stages 2–3 and 6–7
swap the pair; odd stages negate it. `crates/dsp/delay/src/diffusion.rs`'s `DRIFT` table is this
table, and `every_ring_takes_the_distance_the_capture_read` holds all 32 distances of the live
tap table to under **0.01 sample**.

The captured object agrees digit for digit: its blocks held `θ₁ = 1.5055020`, `θ₂ = 1.6651271`,
`δ₁ = −1.3748749e-4`, `δ₂ = +1.9641049e-4` (per frame at 48 kHz, directions as the last ticks
left them), and `floor 0.41420174`, `spread 0.003758912`; with those, stage 0's four distances
come out 241.15866 / 356.00998 / 245.3382 / 355.18109. The zero-phase start is `(x, y) = (1, 0)`:
the rate setter rotates the pair by its step and adds 1.0 along x, which is where a freshly
prepared object begins, so a render that starts from phase 0 starts where Kontakt does.

**The tick** (`0x141bea3f0`): every `trunc(rate·0.25)` frames each rotor draws **two** LCG values
(`state = state·0x19fb4787 + 0x16d1b86b`, taken as the two words of a 64-bit uniform) and, when
that draw is below `+0x12b8` (0.5), flips the sign of its step — the rotation reverses. Two draws,
not one per stage: the sign is per rotor, and four of the eight stages inherit it.

**A second correction from the same kernel**: the two output rows are stored **by lane, not by
channel** — lane `k` keeps its `(left, right)` weights at `+0x60 + 8k` / `+0x64 + 8k`. With the
captured values (`+0x60 = +0x74 = 0.841471`) that makes

```
out[0] = taps[0] · 0.841471 + direct · in[0]
out[1] = taps[2] · 0.841471 + direct · in[1]
```

so the right channel reads **lane 2** — the 245-sample ring — and not lane 1. §11.3's "ch 1 uses
tap 1" was the reading of the rows as if they were row-major; the live capture's right-channel
echoes sit at 244/265/364/426, the `tap 2` distances plus their Catmull offset, which is what
settled it.

### 11.5 The port

`crates/dsp/delay/src/diffusion.rs` implements the network as a shared DSP effect (buffers in
`new`, no allocation in `process`, f32 throughout): eight stages × four rings × four taps,
Catmull–Rom reads, the `M·R` rows, `direct = sin angle`, the tap law and both rotors of §11.4
(rotated every frame, reversed on the tick), the re-length counter and its LCG.
`ir::EffectKind::Diffusion` carries it; `crates/format/kontakt/src/replika.rs` maps the saved
record onto it (§ below), and `crates/engine/effects` builds it per slot.

Kontakt's record, fields as `k-replika.md` numbers them:

| Saved field | Byte | Becomes |
|---|---|---|
| SIZE (20) | 64 | `angle = π/2 − SIZE` |
| AMOUNT (19) | 68 | `g = 0.95·AMOUNT + 0.05` |
| MODULATION (21) | 72 | `spread = g·MODULATION·0.02` |
| DENSE (22) | 76 | carried (the port runs bank A; bank B is unread) |
| — | — | `lengths` = the ctor's table, `window = (g − spread, g·MODULATION·0.02)` |
| — | — | `rotor_hz = (1.5 − MODULATION, ×10/7)`, `rotor_phase = (0, 0)` |

The stream order is **BBDTYPE, SIZE, AMOUNT, MODULATION** (bytes 60, 64, 68, 72), not the
field-number order; §11.5 first wrote AMOUNT at 64 and SIZE at 68, which put Electric Mint's angle
at 1.1834694 instead of 0.5707964 and made its Replika +1.5 dB loud
(`docs/re/electric-mint-stage-gains-2026-10-31.md`). The order above is what the live module held
during the capture below: byte 64 is 1.0 and reproduces SIZE 1, byte 68 is 0.38732699 and reproduces
`g = 0.417960644`, byte 72 is 0.449673 and reproduces `spread = 0.00375891`.

A record cut short of those bytes keeps the object's own INIT values
(1.0 / 0.5 / 0.5 / 0.5, `dense` 1) — which is what the v0x11 copy in the tests has, and why the
capture's live values had to be passed in by hand: Electric Mint's saved tail is not in the
repo's copy.

Type 4 is rendered, taps moving; Analogue, Tape and Vintage (1..3) are still named, not
rendered. The gap log no longer carries `Replika diffusion tap drift` — the drift is in.

### 11.6 The impulse response, and the residual in it

`docs/re/engine/vectors/replika-diffusion-ir-l.f32` / `-r.f32` are 4096 frames of the network's
response to a one-sample impulse, captured by zeroing bank A's rings and stepping the engine
one frame at a time (§11.1). `replika-diffusion-taps.tsv` is the tap table in force during it.
The capture harness read the object's block state once, at its first frame, so the rotors'
angles, steps and directions at frame 0 are known (`cap5/diffuser.bin`, `cap5/stages.txt`) — the
test builds the network at exactly that instant.

Measured in `crates/dsp/delay/src/diffusion.rs`, correlation against the stored vectors:

| | left, all | right, all | left, 0..537 | right, 0..537 | left, 0..2048 |
|---|---|---|---|---|---|
| §11.5's static taps (before this pass) | 0.0122 | 0.0196 | 0.9804 | 0.9759 | 0.0658 |
| the tap law + lane 2 (after) | −0.0121 | −0.0082 | **1.0000** | **1.0000** | 0.0159 |

So the half second that carries the direct path and the echo train of the first four stages is
now reproduced **exactly** — both channels — where the old port was 2 % out and the right
channel was reading the wrong ring entirely. The first divergence is at sample 484, and it is a
*sign*: the port has `−2.2e-5` where the capture has `+2.2e-5`, in a component that sensitivity
traces to the lane-2 feedback column (`mix[0][2]`, `input[1][2]`, −0.005838143 and −0.01416052).
From there the two decorrelate.

**What that means, stated plainly:** the tail is still not reproduced, and the tap mapping was
not the last thing in the way. What is left, and what a fifth capture would need:

- **The one sign.** A path arriving at 484–486 differs in sign, in a term that only lane 2 (the
  245-sample ring's feedback into lane 0) touches. Every single-entry sign flip, all 16 whole-lane
  negations, and 12 column permutations of the mixing and input rows were tried against the
  capture; none of them improves the correlation past 0.52, so it is not one transcription error
  in the rows. The next capture should dump, for one frame, the four taps at `+0x90` *and* the
  feedback vector at `+0x80` **and** the four ring values each tap read — one frame is enough to
  see which product the kernel actually formed, instead of inferring it from the echo train.
- **Whether the 4096 recorded frames are consecutive.** The harness drops a frame when the other
  diffuser's entry clears its `pending` flag (`ir_capture.py`, `if rcx != d: pending = False`),
  so a single interleaved call is lost while the network still advances. Dropping up to three
  frames at any position in 300..1600 was replayed against the capture and does not explain the
  residual, so this is not *the* cause — but the capture does not prove the timeline is
  contiguous either, and the next one should count entries as well as record returns.
- **The stage-4..7 echoes.** The capture has no first-order arrival at stage 4's 761, 5's 941,
  6's 1465 or 7's 1584, while it *does* have stage 4's lane-2 arrival at 766. Every eight-stage
  model that reproduces its first 512 frames puts those arrivals in. Until a capture shows the
  chain running all eight stages with the rings zeroed at frame 0, the stored vector cannot be
  called a complete impulse response of this network.

### 11.7 What moved, and the fingerprint

Rendered with `kontra-host --example render` (ELECTRIC MINT, key 60, 1.5 s, 48 kHz), before and
after the port:

| | before (type 4 refused) | after (network rendered) |
|---|---|---|
| peak | 0.274 | 0.324 |
| rms 0.5-1 s | 0.0512 | 0.0635 |
| rms 1.0-1.5 s | 0.0570 | 0.0712 |
| rms after release | 0.0623 | 0.0737 |
| sha256 | `9b6c798f…a4dbfb9b` | `a6f31865…831b6c72` |

The after hash of the tap-law pass is recorded in §12. The after hash here is reproducible run
to run (checked twice, and again after the last edit). The
render moves because a type-4 Replika used to drop out of the chain entirely (its slot had no
`EffectKind`), and now puts the diffusion network in, which is louder here by about 1.9 dB rms
and carries the smeared tail the network is for. Which other library instruments move was not
censused: any whose Replika is saved as TYPE 4 moves the same way, and finding them is a corpus
scan (682 nki) that this pass did not run.

The native path (`sampler-core`'s `dsp/replika`, `PARITY_MATRIX.md`) is untouched: this is the
IR effect path, and its own TYPE 4 handling is a separate question there.

### 11.8 How the mapping was captured (the fourth pass's method)

No new oracle session: the state was already on disk, and the disassembly gave the law that reads
it. In order:

1. **The object's block state** — `cap5/diffuser.bin` is the whole `NI::DSP::Diffuser` (`0x12e0`
   bytes) read at the capture's first entry, so `+0x1240`/`+0x1270` are the two rotors at frame 0
   and `+0x12c0`..`+0x12d8` are SIZE/AMOUNT/MODULATION/DENSE as the module held them.
2. **The mapping, from the code** — `0x141bea454` rotates both pairs every frame and then
   `0x141bea589`.. writes eight stages' worth of `(whole, frac)` arrays, each stage consuming the
   scale values in a fixed order. Reading which `xmm` each stage's four `mulss` uses gives the
   table in §11.4 directly, including the pair swap at stages 2–3/6–7 and the negation at odd
   stages.
3. **The rate law, from the code** — `0x141c074b0`, called from the rate setter, builds `δ` from
   the rate and `MODULATION`, and the `+1.0` into the pair's x is the zero-phase start.
4. **The check** — the two independently (dump vs. table) and the tap table agree: every one of
   the 32 distances comes out of the law at the dumped angles, to under 0.01 sample. That is
   `every_ring_takes_the_distance_the_capture_read`, and it is the test the tail hung on.
5. **The output rows** — the same kernel's output multiply reads `+0x60 + 8k` / `+0x64 + 8k` per
   lane, which the capture's right-channel arrivals at 244/265/364/426 (the lane-2 distances)
   confirm.

The pulse-response vector itself was then re-run against the port: §11.6 has the numbers and
says what is left.

## 12. What the tap law moved

Both tools, 48 kHz, key 60, 1.5 s, `feat/integration` before this pass against it after:

| render | before | after |
|---|---|---|
| `kontra-inspect play` (the gate's fingerprint) | `7fd5a1ae59790ffc` | `318df4e82ab353d7` |
| `kontra-render`, peak / rms | `585f5f7dbb10c28c`, 0.3161 / 0.04910 | `4ee80cd3b66d9bb1`, 0.3134 / 0.04926 |
| Haunted Strings 2 (`kontra-render`) | `06cf13eee3f8f54e` | `06cf13eee3f8f54e` |
| Cherry Electric Bass (`kontra-render`) | `0279ba0fd86927bd` | `0279ba0fd86927bd` |

Electric Mint moves because its Replika's taps now move: at the record's MODULATION (0.449673)
the window is ±0.9 % of each length — 3.3 samples on stage 0's 859 ring — and the two rotors
turn 1.05 Hz and 1.5 Hz, so the tail breathes instead of sitting at the window floor. Peak
−0.085 dB, rms +0.002 dB: the network's energy is the same, its smearing is not.

The two instruments that store no Replika are byte-identical, as they must be: nothing else in
the chain was touched. `tools/verify.sh`: **VERIFY PASSED** (all layers; `kontra-dsp-delay` 31
tests, render complete, 0 missing frames, 0 clipped).
