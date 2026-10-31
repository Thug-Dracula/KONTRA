#!/usr/bin/env python3
"""Cut the Sound Preset panel out of a real Kontakt capture, in our page's pixels.

    tools/kontakt-reference/panel-reference.py SHOT.png
    tools/kontakt-reference/panel-reference.py SHOT.png --out /tmp/panel.png

`SHOT.png` is an `oracle_shot` capture (docs/re/kontakt-headless-gui.md): a
window-relative grab of Kontakt 8.14.1's window, 1010x698 with an instrument
page loaded. The output is `electric-mint-panel-kontakt.png`, the panel region
(see PANEL below) cropped from Kontakt and nothing else -- the reference the
`pixel diff vs Kontakt panel` row in tools/verify.sh compares our own render to.

## Why this file exists

The whole-page row diffs our render against **our own** earlier render, so it can
only see that the page *changed*, never that it was wrong to begin with. That is
how a preset field drew a stock frame over the plate through three review
rounds: every round agreed with the last wrong picture. This reference comes
from Kontakt, so the row that uses it can say "wrong".

## How the alignment is found (no per-run coordinates)

Kontakt draws its page 633x500 inside a window that also holds a library pane
and a keyboard, so the page is not at (0,0) of the capture. The offset is
*measured*, not assumed:

1. our own page render (`electric-mint-page.png`, 633x500) is template-matched
   against the capture, block-averaged 8x8 for the coarse pass and then refined
   at full resolution around the best block;
2. the best offset that comes back is the page's origin in the capture.

That is robust to the page being wrong in places (it is: the plate's frame sits
one pixel off) because the match is over 316,500 pixels of which the residue is
under 1%, and it is deterministic -- the same capture always yields the same
offset. Both sides are 1:1 (Kontakt's page is 633x500, our render is 633x500),
so nothing is rescaled.

The panel window inside the page is a constant, because it is in *page*
coordinates and both images share those: PANEL is the preset plate's own rect
(the `Menu` at (446,32,171,21), plan.rs's measured geometry) grown to take in the
"Sound Preset" header row above it and the "Link to Song" row below, 2 px of
margin either side. The script prints the offset it found so a capture that does
not line up says so instead of producing a quietly shifted reference.
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
DEFAULT_PAGE = HERE / "electric-mint-page.png"
DEFAULT_OUT = HERE / "electric-mint-panel-kontakt.png"

# The panel, in page coordinates: the preset plate (446,32,171,21) plus the
# header row above and the Link to Song row below, 2 px clear either side.
PANEL = (444, 12, 177, 69)

# Coarse match granularity. A multiple that divides both sizes keeps the block
# average exact, which keeps the coarse pass deterministic.
BLOCK = 8
# A page that matched this badly means the capture is not an instrument page,
# or the window is scaled: say so instead of writing a reference.
MAX_RESIDUAL = 24.0


def load_luma(path):
    try:
        im = Image.open(path)
        im.load()
    except OSError as e:
        sys.exit(f"panel-reference: cannot read {path}: {e}")
    if im.mode != "RGB":
        im = im.convert("RGBA")
        flat = Image.new("RGB", im.size, (0, 0, 0))
        flat.paste(im, mask=im.split()[-1])
        im = flat
    return np.asarray(im, dtype=np.float32).mean(axis=2)


def blocks(a, k):
    """`a` averaged over k x k blocks, cropped to whole blocks."""
    h, w = (a.shape[0] // k) * k, (a.shape[1] // k) * k
    return a[:h, :w].reshape(h // k, k, w // k, k).mean(axis=(1, 3))


def match_coarse(shot, page, k=BLOCK):
    """Best (dx, dy) at 1/k resolution by mean absolute difference."""
    s, p = blocks(shot, k), blocks(page, k)
    sh, sw = s.shape
    ph, pw = p.shape
    if ph > sh or pw > sw:
        sys.exit(f"panel-reference: page {pw*k}x{ph*k} is larger than the capture "
                 f"{sw*k}x{sh*k}; is this an instrument-page capture?")
    best = (float("inf"), 0, 0)
    for y in range(sh - ph + 1):
        # One strip of candidate rows at a time: cheaper than a loop per pixel.
        win = s[y:y + ph]
        for x in range(sw - pw + 1):
            d = np.abs(win[:, x:x + pw] - p).mean()
            if d < best[0]:
                best = (float(d), x, y)
    return best


def match_fine(shot, page, coarse, k=BLOCK, slack=BLOCK + 1):
    """Refine the coarse offset at full resolution within +-slack px."""
    cx, cy = coarse[1] * k, coarse[2] * k
    ph, pw = page.shape
    sh, sw = shot.shape
    best = (float("inf"), cx, cy)
    for dy in range(cy - slack, cy + slack + 1):
        if dy < 0 or dy + ph > sh:
            continue
        for dx in range(cx - slack, cx + slack + 1):
            if dx < 0 or dx + pw > sw:
                continue
            d = np.abs(shot[dy:dy + ph, dx:dx + pw] - page).mean()
            if d < best[0]:
                best = (float(d), dx, dy)
    return best


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("shot", help="an oracle_shot capture of Kontakt's instrument page")
    ap.add_argument("--page", default=str(DEFAULT_PAGE),
                    help="our own page render, used only to locate the page in the capture")
    ap.add_argument("--out", default=str(DEFAULT_OUT))
    args = ap.parse_args()

    shot = load_luma(args.shot)
    page = load_luma(args.page)
    coarse = match_coarse(shot, page)
    residual, x, y = match_fine(shot, page, coarse)
    print(f"capture   {Path(args.shot).name}: {shot.shape[1]}x{shot.shape[0]}")
    print(f"our page  {Path(args.page).name}: {page.shape[1]}x{page.shape[0]}")
    print(f"page origin in capture: ({x},{y})  residual {residual:.2f}/channel, mean")
    if residual > MAX_RESIDUAL:
        sys.exit(f"panel-reference: the page matched badly ({residual:.1f} > {MAX_RESIDUAL}); "
                 "not writing a reference from this capture")

    px, py, pw, ph = PANEL
    crop = (x + px, y + py, x + px + pw, y + py + ph)
    if crop[0] < 0 or crop[1] < 0 or crop[2] > shot.shape[1] or crop[3] > shot.shape[0]:
        sys.exit(f"panel-reference: panel window {crop} falls outside the capture")

    # Re-read in colour: the index above is only good for locating the page.
    im = Image.open(args.shot)
    im.load()
    im = im.convert("RGB") if im.mode != "RGBA" else im
    before = Path(args.out).exists()
    old = None
    if before:
        old = np.asarray(Image.open(args.out).convert("RGB"), dtype=np.int16)
    crop_im = im.crop(crop)
    crop_im.save(args.out, optimize=True)
    print(f"panel     {'x'.join(str(v) for v in crop)} -> {args.out} ({crop_im.size[0]}x{crop_im.size[1]})")
    if old is not None:
        new = np.asarray(crop_im.convert("RGB"), dtype=np.int16)
        if old.shape == new.shape:
            print(f"replaces  {int((np.abs(old - new).max(axis=2) > 12).sum())} px differ from the committed file")


if __name__ == "__main__":
    main()
