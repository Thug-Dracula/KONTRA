# Electric Mint: what Choral and Diffusion changed in KONTRA's parity

Measured 2026-10-30 at `39bfd2d`, then **re-measured 2026-10-31 at `fffc618`** after the tree moved
under it (worktree `/tmp/opencode/wt-measure`, branch `measure/electric-mint`, rebased). Kontakt
8.14.1 standalone under Wine on the oracle's private Xvfb (`:77`); KONTRA from
`CARGO_TARGET_DIR=/var/tmp/tgt-measure`.

One note: key 60, velocity 100, held 1.5 s, first 2 s compared with `tools/oracle/compare.py`'s
alignment and 1/3-octave bands, through `tools/oracle-mcp/parity.py`'s verdict and limits (level
0.5 dB, band RMS 1.0 dB, worst band 3.0 dB, lag 10 ms). Same protocol as the sweep in
`parity-sweep-2026-10-30.md`, which measured this instrument at **0.85 dB band RMS, −0.01 dB
level** before either stage was audible.

Two things changed since: Choral (instrument insert slot 6) went from inert to audible, and
Replika's Diffusion type (slot 4) went from dropped to a partially modelled network.

## 0. The tree, and why the gain laws are a no-op here

The first pass ran at `39bfd2d`. Three merges landed underneath it — Replika Diffusion rendering
(`73c9339`, already merged at `39bfd2d`), the dry-gain laws (`2074a43`), and factory impulse paths
with the Kontakt 8 private-data groundwork — so the branch was rebased onto `fffc618` and every
number below re-rendered there.

The gate's own fingerprint is the tree check: `kontra-inspect play` on this build prints
**`audio FNV-1a: 2c139e8d7cda4317`**, which is the current gate fingerprint.

Electric Mint turns out not to be touched by the gain laws. Its chain is six inserts with no send
and no main slots, and its Solid EQ is `insert 1: Eq, output 1.000` — the EQ-output law changes
nothing at unity, and the send-dry law has no send to act on. Measured rather than assumed: the
four configurations re-rendered on `fffc618` differ from their `39bfd2d` renders by **−139.5 dB
rms** (max |difference| 6.9e-7, correlation 1.000000) — float rounding, not behaviour, with level
identical to 0.01 dB. So Haunted Strings' −9.14 → −0.53 dB is not Electric Mint's story: this
instrument's level moved **0.00 dB** with those laws.

## Answer

**Both stages move Electric Mint away from Kontakt, and neither moves it on band shape at all.**
With both bypassed the chain matches Kontakt to 0.02 dB; adding Choral alone puts it
1.27 dB off, adding Diffusion alone 1.52 dB off, and the delivered
pair reads 0.27 dB only because the two errors cancel — Choral takes
1.25 dB off KONTRA's output, Diffusion adds 1.54 dB, both
flat across the spectrum.

| | \|level Δ\| dB | Δ vs both bypassed | band RMS dB |
|---|---:|---:|---:|
| both stages bypassed | 0.02 | — | 0.79 |
| + Choral only | 1.27 | +1.25 | 0.91 |
| + Diffusion only | 1.52 | +1.51 | 0.82 |
| delivered chain (both) | 0.27 | +0.25 | 0.90 |

("+ Choral only" is the `−Diffusion` column — Choral in, Diffusion out. "+ Diffusion only" is
`−Choral`.)

The floor these sit against — Kontakt against itself at the same round-robin index across
sessions — is **0.04 dB level and 0.42 dB band RMS**, so both
individual moves are two orders of magnitude above the level floor and the delivered chain is
still 0.27 dB off it. Band RMS moves by at most 0.12 dB
between any two configurations, well inside the 0.42 dB floor: **this metric sees
no shape contribution from either stage, in either direction.**

### What that does and does not establish

The anchor is a fact about the output: the uneffected chain is the one that matches Kontakt. What
it does **not** establish is that Kontakt's own Choral is level-neutral and KONTRA's is not, because
Kontakt's per-slot gains are not measurable here — the oracle's `dump.lua` records groups, zones and
scripts, no effects, so only the *net* of Kontakt's chain is observable. Two readings fit the data:
Kontakt's two slots are individually neutral and KONTRA's are not, or Kontakt's slots gain and lose
by the same amounts and KONTRA reproduces one but not the other. In both readings 1.25 dB
of level arrives in the delivered chain that Kontakt does not have, and nothing measurable comes
back in shape. Distinguishing them needs one Kontakt capture per slot bypassed in the instrument UI,
which this measurement did not do.

## 1. Current build against Kontakt

`kontra-render` on the unmodified chain, `oracle_render` for Kontakt, `parity.py` for the verdict.
Reference: the first render after load in a fresh session (round-robin index 0), the index parity
compares on, and one without the capture defect of §6.

```
PARITY PASS  level -0.26 dB  band rms 0.92 dB  worst -2.28 dB @ 160 Hz  lag +0 ms  wave r 0.778
```

Averaged over 6 clean round-robin-0 captures from 6 sessions:

| configuration | \|level Δ\| dB | band RMS dB | worst band | Hz | lag ms | waveform r |
|---|---:|---:|---:|---:|---:|---:|
| full chain | 0.27 | 0.90 (0.85–0.96) | −2.28 | 160 | +0 | 0.752 |
| −Choral | 1.52 | 0.82 (0.74–0.87) | −1.99 | 160 | +0 | 0.728 |
| −Diffusion | 1.27 | 0.91 (0.84–0.98) | −2.14 | 160 | +0 | 0.744 |
| −both | 0.02 | 0.79 (0.70–0.83) | −1.78 | 160 | +0 | 0.714 |

Only the delivered chain and the fully-bypassed chain pass `parity.py`'s limits; either stage
alone is a level failure (1.27 and 1.52 dB against a 0.5 dB limit).
The delivered chain passes, but the sweep's numbers say it passed more closely before.

## 2. Per-band difference from Kontakt (median removed, dB)

Mean over the 6 clean captures; positive means KONTRA is louder in that band.

| Hz | full chain | −Choral | −Diffusion | −both |
|---:|---:|---:|---:|---:|
| 50 | +0.76 | −0.17 | +1.00 | +0.04 |
| 63 | +0.70 | +0.50 | +0.80 | +0.60 |
| 80 | −0.60 | −0.24 | −0.53 | −0.20 |
| 101 | −0.85 | −1.05 | −0.63 | −0.86 |
| 127 | −1.36 | −1.14 | −1.14 | −0.94 |
| 160 | −2.20 | −1.87 | −2.01 | −1.69 |
| 202 | −0.02 | −0.59 | +0.11 | −0.52 |
| 254 | +0.21 | −0.01 | −0.49 | −0.69 |
| 320 | −1.28 | −0.60 | −0.74 | −0.08 |
| 403 | −1.37 | −0.90 | −1.00 | −0.59 |
| 508 | +0.24 | +0.56 | +0.02 | +0.30 |
| 640 | −1.16 | −0.73 | −0.46 | −0.06 |
| 806 | +0.83 | +0.09 | +0.86 | +0.13 |
| 1016 | +0.50 | +1.04 | +0.53 | +1.04 |
| 1280 | +0.67 | +0.87 | +1.00 | +1.18 |
| 1613 | +0.42 | +0.99 | +0.91 | +1.45 |
| 2032 | +0.45 | +0.72 | +0.51 | +0.74 |
| 2560 | +0.51 | +0.62 | +0.41 | +0.52 |
| 3225 | −0.39 | −0.24 | −0.18 | −0.06 |
| 4064 | −0.00 | +0.24 | +0.14 | +0.35 |
| 5120 | +0.56 | +0.92 | +0.63 | +0.96 |
| 6451 | +0.46 | +0.58 | +0.58 | +0.67 |
| 8127 | −0.09 | +0.06 | +0.00 | +0.13 |
| 10240 | −1.07 | −0.93 | −0.97 | −0.85 |
| 12902 | −1.37 | −1.22 | −1.28 | −1.14 |

The pattern is the same in all four columns: KONTRA ~1.6–2.3 dB low at 160 Hz, ~0.5–1.3 dB high
from 1 to 2.5 kHz, ~1.2–1.5 dB low from 10 kHz up. Moving between configurations changes a single
band by at most 1.20 dB (at 320 Hz) — comparable to the 1.38 dB
worst-band floor between captures and far under the 0.84 dB round-robin spread,
so no individual band separates the four columns. The shape error is pre-existing; neither stage causes it and
neither fixes it.

## 3. What each stage adds, on KONTRA's own side

Absolute band level per configuration, and the difference between the full chain and the chain
with that stage bypassed (negative means the stage costs level).

| Hz | full chain | −Choral | −Diffusion | −both | Choral adds | Diffusion adds |
|---:|---:|---:|---:|---:|---:|---:|
| 50 | 0.9 | 3.3 | −0.6 | 1.8 | −2.40 | +1.46 |
| 63 | 10.2 | 11.8 | 8.9 | 10.5 | −1.67 | +1.32 |
| 80 | 9.3 | 10.4 | 8.0 | 9.1 | −1.12 | +1.29 |
| 101 | 14.8 | 16.5 | 13.3 | 15.0 | −1.67 | +1.44 |
| 127 | 38.4 | 39.6 | 36.9 | 38.2 | −1.26 | +1.44 |
| 160 | 15.7 | 16.9 | 14.3 | 15.4 | −1.14 | +1.42 |
| 202 | 30.2 | 32.3 | 28.9 | 30.9 | −2.05 | +1.35 |
| 254 | 28.7 | 30.4 | 28.2 | 29.8 | −1.70 | +0.52 |
| 320 | 30.5 | 31.3 | 28.7 | 29.5 | −0.80 | +1.76 |
| 403 | 29.2 | 30.2 | 27.6 | 28.7 | −1.01 | +1.59 |
| 508 | 9.7 | 10.9 | 8.7 | 9.9 | −1.15 | +1.00 |
| 640 | 30.8 | 31.9 | 28.9 | 29.9 | −1.05 | +1.92 |
| 806 | 10.3 | 12.5 | 9.0 | 11.2 | −2.21 | +1.26 |
| 1016 | 22.2 | 23.1 | 20.9 | 21.9 | −0.93 | +1.25 |
| 1280 | 10.9 | 12.2 | 9.4 | 10.7 | −1.28 | +1.54 |
| 1613 | 13.9 | 14.8 | 12.2 | 13.1 | −0.91 | +1.71 |
| 2032 | 15.4 | 16.6 | 14.1 | 15.4 | −1.21 | +1.28 |
| 2560 | 9.8 | 11.1 | 8.7 | 10.0 | −1.36 | +1.12 |
| 3225 | 12.7 | 14.0 | 11.3 | 12.6 | −1.32 | +1.43 |
| 4064 | 4.6 | 5.8 | 3.2 | 4.5 | −1.24 | +1.36 |
| 5120 | −3.1 | −1.9 | −4.3 | −3.2 | −1.12 | +1.28 |
| 6451 | −13.9 | −12.5 | −15.2 | −13.8 | −1.35 | +1.34 |
| 8127 | −21.1 | −19.7 | −22.4 | −21.1 | −1.33 | +1.31 |
| 10240 | −26.8 | −25.4 | −28.1 | −26.8 | −1.34 | +1.32 |
| 12902 | −36.7 | −35.4 | −38.0 | −36.7 | −1.33 | +1.31 |

Choral's contribution is **flat: -1.28 dB across 500 Hz–16 kHz, never outside
-2.21…-0.91** — a broadband gain loss, not a timbre difference (the −2.2 dB dip at
806 Hz is a band ~12 dB below its neighbours in both signals, near a null). A chorus changes the spectrum by sweeping a comb, and
a 2 s band average washes that out, so this metric cannot certify that Choral modulates correctly.
What it can say is that Choral takes 1.25 dB out of KONTRA's chain, and that the
uneffected chain is the one that matches Kontakt's full chain to 0.02 dB — so the
level Choral removes is level Kontakt's chain does not remove overall. Whether Kontakt's Choral
slot does it and its Replika slot gives it back is not observable from KONTRA's IR (§Answer).

Diffusion's contribution is likewise flat, at about +1.54 dB. It does de-transient
the signal: the 100 ms band-level deviation over the note falls from 8.87 dB with it bypassed to
6.34 dB with it active, so the network is doing something real. Its gain is the problem.

The two slots' own records, as the instrument asks for them:

```
insert 4: Diffusion, output 0.299, dry 1.000
insert 6: Chorus,    output 1.000, dry 0.000
```

Choral is 100 % wet (`dry 0.000`), so its -1.25 dB is the chorus kernel's own gain and
not a dry/wet mix error. Diffusion is 0.299 wet over a unity dry and still gains
+1.54 dB overall.

## 4. The Kontakt noise floor

Batch 1 was two sessions of three renders (4 of 6 rejected for the defect in §6). Batch 2 was the
same with re-renders on rejection (6 of 10 rejected); then four single-render sessions for the
index-0 floor (1 of 4 rejected), and two more on 10-31 to confirm the reference had not drifted
(0 of 2 rejected). Of **22 renders, 11 were discarded**, leaving 11 clean; the floor below is
measured on clean captures only.

| comparison | pair | \|level Δ\| dB | band RMS dB | \|worst band\| dB | lag ms |
|---|---|---:|---:|---:|---:|
| across sessions, round-robin 0 | A-C | 0.00 | 0.27 | 0.91 | +0 |
| across sessions, round-robin 0 | A-D | 0.01 | 0.21 | 0.54 | +0 |
| across sessions, round-robin 0 | A-E | 0.01 | 0.16 | 0.39 | +0 |
| across sessions, round-robin 0 | A-F | 0.03 | 0.42 | 1.18 | +0 |
| across sessions, round-robin 0 | A-G | 0.01 | 0.28 | 0.59 | +0 |
| across sessions, round-robin 0 | C-D | 0.00 | 0.27 | 0.77 | +0 |
| across sessions, round-robin 0 | C-E | 0.00 | 0.25 | 0.67 | +0 |
| across sessions, round-robin 0 | C-F | 0.03 | 0.35 | 1.38 | +0 |
| across sessions, round-robin 0 | C-G | 0.01 | 0.31 | 0.87 | +0 |
| across sessions, round-robin 0 | D-E | 0.00 | 0.23 | 0.57 | +0 |
| across sessions, round-robin 0 | D-F | 0.04 | 0.38 | 1.02 | +0 |
| across sessions, round-robin 0 | D-G | 0.00 | 0.18 | 0.47 | +0 |
| across sessions, round-robin 0 | E-F | 0.03 | 0.40 | 1.15 | +0 |
| across sessions, round-robin 0 | E-G | 0.00 | 0.28 | 0.61 | +0 |
| across sessions, round-robin 0 | F-G | 0.04 | 0.32 | 0.91 | +0 |
| within a session | A0-A1 | 0.19 | 0.67 | 1.67 | +0 |
| within a session | A0-A3 | 0.13 | 0.84 | 2.77 | +0 |
| within a session | A0-B1 | 0.21 | 0.64 | 1.44 | +0 |
| within a session | A1-A3 | 0.06 | 0.58 | 1.40 | +0 |
| within a session | A1-B1 | 0.02 | 0.21 | 0.49 | +0 |
| within a session | A3-B1 | 0.09 | 0.60 | 1.44 | +0 |

Round-robin index 0 across sessions — the floor parity's verdict stands on — is level ≤
0.04 dB, band RMS ≤ 0.42 dB, worst band ≤ 1.38 dB.
Different round-robin samples within one session reach 0.84 dB band RMS and
0.21 dB level. That reproduces `parity.py`'s stated floor (0.08–0.12 dB level,
0.31–0.39 dB band RMS) — the level floor is tighter here and the band floor slightly wider, which
will be two of the six captures coming from a second day — and confirms the caveat: a single render
is not a level, and a sweep row whose error is under ~0.8 dB is not evidence of
anything.

## 5. Verdict per stage

| stage | verdict | the number that decides it |
|---|---|---|
| **Choral** (slot 6) | **revert**, or fix its gain first — it moves parity away | `+ Choral only` vs `−both`: level error 0.02 → 1.27 dB, because Choral takes 1.25 dB out, against a 0.04 dB floor; band RMS 0.79 → 0.91 (+0.12, inside the 0.42 dB floor). No shape benefit to weigh against the loss. |
| **Diffusion** (slot 4) | **revert**, or fix its gain first — it moves parity away | `+ Diffusion only` vs `−both`: level error 0.02 → 1.52 dB, because Diffusion adds 1.54 dB, again against a 0.04 dB floor; band RMS 0.79 → 0.82 (+0.02, inside the floor). The kernel genuinely de-transients; the level it adds is what costs parity. |
| **the pair** | only the pair is at issue: they cancel | delivered 0.27 dB / 0.90 dB band RMS versus 0.02 / 0.79 with both out. Two ~1.3 dB errors summing to 0.27 dB is not a correct chain. |

**Reverting either one alone makes the chain worse, not better**: `−Choral` alone is
1.52 dB off and `−Diffusion` alone is 1.27 dB off, both level
failures against the 0.5 dB limit, while reverting both returns the instrument to
0.02 dB / 0.79 dB — the state the sweep measured at 0.85 / −0.01. So the
choice is between reverting the pair and resolving their gains against Kontakt's own slots; there
is no evidence here for keeping one of them.

## 6. The Kontakt capture defect this measurement hit

**11 of 20 oracle renders** carried a ~0.4 s broadband burst above 6 kHz, landing 0.8–1.2 s into
the note. In such a capture the 12.9 kHz band reads up to **+46 dB** over the note's own top octave,
and the burst appears in silence as well, so it is not instrument content.

Effect on a verdict, measured by accident: this same delivered chain, compared against a bursty
capture, reads **6.38 dB band RMS and worst +24.8 dB at 12.9 kHz** — a fail — versus 0.92 dB and
−2.28 dB at 160 Hz against a clean one. That is the whole distance between "fine" and "broken", so
every capture used here was scored (`12.9 kHz over 1 kHz` per 200 ms window, worst window minus
the note's median, > 20 dB rejected) and re-rendered on a hit.

The first batch overlapped KONTRA renders on the same machine and lost 4 of 6; the second ran on
an idle machine and lost 6 of 10. So it is not load contention — it is in the oracle's capture path
or Kontakt's audio thread, and it needs fixing at that end. Until then any single-render number
from this oracle can be wrong by ~25 dB in the top octave, and a sweep row that does not reject it
will be wrong in exactly that way.

## 7. What was not measured, and what would change the answer

- **Choral's modulation.** A 2 s band average cannot see a chorus sweep. "Choral costs
  1.25 dB and changes nothing else measurable" is all this says; it does not say
  the chorus law is right. That needs a short-time per-band variance against Kontakt's own Choral,
  which means isolating Kontakt's Choral on its side — one capture per slot bypassed in the
  instrument UI. The oracle's `dump.lua` records no effects, so KONTRA's parse is the only chain
  available in the IR.
- **The 40 Hz band.** It has no energy on either side and `compare.py`'s argmax can pick it (the
  `−Diffusion` column's raw worst band is −2.09 dB at 40 Hz, arithmetic on silence). Bands below
  50 Hz are excluded above.
- **The window.** 2 s from the onset, as the sweep. Choral's LFO and Diffusion's tail past 2 s are
  outside the metric.

## Reproducing

```
git worktree add -f /tmp/opencode/wt-measure -b measure/electric-mint feat/integration
CARGO_TARGET_DIR=/var/tmp/tgt-measure cargo build --release -p kontra-host --bin kontra-render

# Kontakt: first render after load per session, rejected if it carries the HF burst
python3 kontakt_sessions2.py --out DIR --sessions 2 --clean 3

# One configuration, one capture
KONTRA_RENDER=bin/choral.sh python3 tools/oracle-mcp/parity.py \
    --kontakt-wav CAPTURE.wav --out OUT --json OUT.json
```

`KONTRA_BYPASS=<kind>,...` is a **measurement knob added to `crates/app/host/src/render.rs` in the
worktree only and deliberately not committed** (36 lines, mirroring `KONTRA_SCRIPT_SOURCE`). It sets
`bypass` on the instrument's own insert records whose slot is one of the named kinds, before the
plan is built — measuring each stage's contribution as the instrument asks for it, without touching
KONTRA's DSP. Unset, the binary is byte-identical to stock (verified: same SHA-256 for the same
input with the variable unset and set to `none`).

Sample data for this record: `/var/tmp/measure-em/` — `analysis2.json` holds every per-band number,
`k2/sessions.json` and `rr0/rr0.json` the capture ledger with every rejection.
