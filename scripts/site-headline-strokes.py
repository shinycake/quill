#!/usr/bin/env python3
"""Build the landing page's headline fonts and the strokes its quill writes.

The headline is set in Inter Display at weight 750. On load a quill writes
it out by hand in Sacramento, a thin cursive, and then each handwritten
letter flows into the Inter one (site/quill-write.js). The nib follows each
handwritten letter's centerline in the order a hand would form it. This
script makes the files that depend on the fonts:

  site/fonts/headline.woff2       Inter Display 750, Basic Latin, kerning only
  site/fonts/hand.woff2           Sacramento, Basic Latin, kerning only,
                                  renamed Quill Hand (its license reserves
                                  the name Sacramento)
  site/data/headline-strokes.json the handwriting's strokes, per character

Get InterVariable.ttf from https://github.com/rsms/inter/releases (v4.1) and
Sacramento-Regular.ttf from https://github.com/google/fonts/tree/main/ofl/sacramento,
then run, with fonttools, brotli, numpy, scipy, scikit-image and pillow:

  python3 scripts/site-headline-strokes.py path/to/InterVariable.ttf path/to/Sacramento-Regular.ttf

Each stroke comes from a plan below: waypoints in the letter's ink box. The
script draws the letter large, thins it to its centerline, snaps each
waypoint to that line and follows it from one waypoint to the next. So a
plan picks the order and direction of the strokes, and the font decides
their exact shape. Pass --debug DIR to draw every letter with its strokes
numbered and the ink tinted by when the pen reaches it.

Output, per character: the time the letter takes, and its strokes as flat
[x, y, r, t, ...] lists. x and y are thousandths of an em from the glyph's
origin on the baseline, y pointing down; r is the stroke's half width there;
t is when the nib gets there, in milliseconds from the letter's start.
"""
import argparse
import heapq
import json
import math
import os
import subprocess
import tempfile

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage
from skimage.morphology import skeletonize

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UNICODES = "U+0020-007E,U+00A0,U+2018,U+2019,U+201C,U+201D,U+2013,U+2014,U+2026"

# Waypoints per stroke, in writing order, in the letter's ink box: (0, 0) is
# its top left and (1, 1) its bottom right. A stroke with one waypoint is a
# dab, like a full stop. These are the handwriting's letters (Sacramento):
# most run as one stroke from the top or the left out to the joining stroke
# on the right, the way the cursive is written.
PLANS = {
    "A": [[(0.02, 0.87), (0.2, 0.95), (0.55, 0.5), (0.85, 0.03), (0.86, 0.6), (0.86, 0.95)], [(0.19, 0.37), (0.14, 0.5), (0.3, 0.66), (0.6, 0.6), (0.98, 0.57)]],
    "T": [[(0.33, 0.31), (0.15, 0.48), (0.03, 0.2), (0.4, 0.05), (0.7, 0.18), (0.98, 0.1)], [(0.75, 0.12), (0.66, 0.4), (0.55, 0.95), (0.25, 0.81)]],
    "a": [[(0.5, 0.2), (0.2, 0.08), (0.05, 0.6), (0.25, 0.88), (0.44, 0.63), (0.44, 0.46), (0.5, 0.85), (0.7, 0.88), (0.92, 0.4)]],
    "c": [[(0.45, 0.36), (0.45, 0.08), (0.08, 0.5), (0.4, 0.9), (0.9, 0.4)]],
    "d": [[(0.38, 0.62), (0.1, 0.8), (0.2, 0.95), (0.42, 0.8), (0.5, 0.3), (0.6, 0.03), (0.66, 0.2), (0.45, 0.7), (0.6, 0.95), (0.92, 0.76)]],
    "e": [[(0.17, 0.6), (0.45, 0.38), (0.45, 0.05), (0.1, 0.3), (0.1, 0.7), (0.45, 0.92), (0.9, 0.36)]],
    "f": [[(0.15, 0.43), (0.5, 0.2), (0.35, 0.02), (0.12, 0.2), (0.1, 0.6), (0.12, 0.9), (0.35, 0.97), (0.6, 0.85), (0.18, 0.64), (0.88, 0.48)]],
    "g": [[(0.55, 0.07), (0.15, 0.05), (0.1, 0.25), (0.4, 0.3), (0.49, 0.15), (0.5, 0.6), (0.3, 0.97), (0.12, 0.75), (0.45, 0.48), (0.9, 0.13)]],
    "h": [[(0.09, 0.7), (0.35, 0.35), (0.2, 0.03), (0.07, 0.4), (0.07, 0.95), (0.45, 0.66), (0.5, 0.9), (0.7, 0.96), (0.92, 0.77)]],
    "i": [[(0.18, 0.44), (0.25, 0.85), (0.45, 0.92), (0.88, 0.62)], [(0.25, 0.0), (0.08, 0.1), (0.25, 0.2), (0.38, 0.1), (0.25, 0.0)]],
    "k": [[(0.1, 0.02), (0.08, 0.95)], [(0.1, 0.87), (0.45, 0.65), (0.58, 0.72), (0.3, 0.84), (0.5, 0.95), (0.92, 0.76)]],
    "l": [[(0.15, 0.7), (0.45, 0.3), (0.35, 0.03), (0.12, 0.3), (0.15, 0.8), (0.4, 0.95), (0.88, 0.77)]],
    "m": [[(0.03, 0.4), (0.13, 0.07), (0.15, 0.75), (0.3, 0.25), (0.38, 0.2), (0.4, 0.75), (0.55, 0.25), (0.62, 0.2), (0.65, 0.75), (0.78, 0.8), (0.95, 0.38)]],
    "n": [[(0.05, 0.4), (0.2, 0.07), (0.22, 0.75), (0.5, 0.2), (0.6, 0.25), (0.65, 0.75), (0.75, 0.8), (0.95, 0.4)]],
    "o": [[(0.25, 0.15), (0.05, 0.5), (0.35, 0.9), (0.55, 0.55), (0.38, 0.2), (0.5, 0.05), (0.58, 0.3), (0.55, 0.55), (0.9, 0.36)]],
    "p": [[(0.33, 0.04), (0.3, 0.4), (0.15, 0.97), (0.05, 0.85), (0.3, 0.35)], [(0.27, 0.2), (0.35, 0.22), (0.6, 0.1), (0.68, 0.25), (0.55, 0.37), (0.42, 0.31)], [(0.55, 0.37), (0.93, 0.2)]],
    "r": [[(0.17, 0.45), (0.2, 0.28), (0.12, 0.08), (0.22, 0.03), (0.25, 0.25), (0.4, 0.3), (0.42, 0.8), (0.6, 0.88), (0.92, 0.5)]],
    "s": [[(0.13, 0.48), (0.22, 0.33), (0.22, 0.1), (0.42, 0.55), (0.3, 0.85), (0.1, 0.8)], [(0.28, 0.87), (0.55, 0.88), (0.9, 0.48)]],
    "t": [[(0.38, 0.7), (0.5, 0.3), (0.45, 0.03), (0.35, 0.3), (0.36, 0.75), (0.5, 0.97), (0.75, 0.75)], [(0.03, 0.18), (0.97, 0.18)]],
    "u": [[(0.17, 0.07), (0.08, 0.6), (0.25, 0.9), (0.44, 0.62), (0.47, 0.15), (0.45, 0.6), (0.6, 0.9), (0.95, 0.38)]],
    "v": [[(0.1, 0.17), (0.12, 0.6), (0.25, 0.9), (0.5, 0.12), (0.52, 0.33), (0.75, 0.55), (0.92, 0.45)]],
    "w": [[(0.13, 0.17), (0.08, 0.6), (0.2, 0.88), (0.35, 0.7), (0.37, 0.22), (0.36, 0.7), (0.5, 0.88), (0.63, 0.12), (0.66, 0.4), (0.8, 0.55), (0.94, 0.45)]],
    "y": [[(0.18, 0.05), (0.05, 0.2), (0.25, 0.33), (0.45, 0.25), (0.53, 0.08), (0.48, 0.5), (0.3, 0.97), (0.1, 0.8), (0.48, 0.38), (0.92, 0.15)]],
    ",": [[(0.9, 0.1), (0.1, 0.9)]],
    ".": [[(0.5, 0.5)]],
}

S = 600  # pixels per em while tracing
PAD = 200
STEP = 0.012  # em between output samples

# Pen speed. Along a straight line the nib runs at VMAX; in a bend it slows
# with the two-thirds power law of handwriting, speed growing with the cube
# root of the bend's radius, down to BEND_FLOOR of VMAX. It eases in at the
# start of a stroke and out at the end.
VMAX = 0.0285  # em of the handwriting per ms; set 1.25 times its size, that is about 0.036 em of the headline
BEND_RADIUS = 0.3  # em; bends wider than this run at full speed
BEND_FLOOR = 0.45
DAB_MS = 45
# Pixels this close (in em) to two strokes take the earlier one, so a loop
# that closes on itself, like an o, inks its seam with the first pass.
TIE = 0.0015


# Lifting the pen, moving it d em and setting it down takes
# HOP[0] + HOP[1] * sqrt(d) ms. The page uses the same rule between letters.
HOP = (8, 29)


def hop_ms(dist):
    return HOP[0] + HOP[1] * math.sqrt(dist)


NB = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)]


def render(font, ch):
    W, H = int(S * 1.6) + 2 * PAD, int(S * 2) + 2 * PAD
    base = PAD + int(S * 1.1)
    im = Image.new("L", (W, H), 0)
    ImageDraw.Draw(im).text((PAD, base), ch, font=font, fill=255, anchor="ls")
    return np.array(im) > 127, base


def shortest(pts, a, b):
    """The shortest way from a to b along the centerline's pixels."""
    dist = {a: 0.0}
    prev = {}
    q = [(0.0, a)]
    while q:
        d, p = heapq.heappop(q)
        if p == b:
            break
        if d > dist[p]:
            continue
        for dy, dx in NB:
            n = (p[0] + dy, p[1] + dx)
            if n not in pts:
                continue
            nd = d + (1.4142 if dy and dx else 1.0)
            if nd < dist.get(n, math.inf):
                dist[n] = nd
                prev[n] = p
                heapq.heappush(q, (nd, n))
    path = [b]
    while path[-1] != a:
        path.append(prev[path[-1]])
    return path[::-1]


def extend(path, ink, at_end):
    """Carry a stroke that stops at a terminal on to the edge of the ink."""
    p = path if at_end else path[::-1]
    k = min(len(p) - 1, 12)
    d = np.array(p[-1], float) - np.array(p[-1 - k], float)
    n = np.hypot(*d)
    if n == 0:
        return path
    d /= n
    extra = []
    cur = np.array(p[-1], float)
    while True:
        cur = cur + d
        if not ink[int(round(cur[0])), int(round(cur[1]))]:
            break
        extra.append((cur[0], cur[1]))
    return path + extra if at_end else extra[::-1] + path


def smooth(p, sigma):
    if len(p) < 3:
        return p
    out = ndimage.gaussian_filter1d(p, sigma, axis=0, mode="nearest")
    # Pin the ends.
    for i in range(min(len(p), int(sigma * 2))):
        w = i / (sigma * 2)
        out[i] = p[i] * (1 - w) + out[i] * w
        out[-1 - i] = p[-1 - i] * (1 - w) + out[-1 - i] * w
    return out


def resample(p, step):
    s = np.concatenate([[0], np.cumsum(np.hypot(*np.diff(p, axis=0).T))])
    t = np.linspace(0, s[-1], max(2, int(round(s[-1] / step)) + 1))
    return np.stack([np.interp(t, s, p[:, 0]), np.interp(t, s, p[:, 1])], 1)


def arc_of(s):
    return np.concatenate([[0], np.cumsum(np.hypot(*np.diff(s[:, :2], axis=0).T))])


def distance(q, s):
    """Distance from point q to stroke s's centerline, and the arc length
    along s of the nearest point."""
    a, b = s[:-1, :2], s[1:, :2]
    ab = b - a
    u = np.clip(((q - a) * ab).sum(1) / np.maximum((ab * ab).sum(1), 1e-12), 0, 1)
    d = np.hypot(*(a + ab * u[:, None] - q).T)
    i = int(np.argmin(d))
    return d[i], arc_of(s)[i] + u[i] * np.hypot(*ab[i])


def trace(font, ch, plan):
    """The plan's strokes as arrays of (x, y, r) in em."""
    ink, base = render(font, ch)
    dt = ndimage.distance_transform_edt(ink)
    pts = set(zip(*np.nonzero(skeletonize(ink))))
    deg = {p: sum((p[0] + a, p[1] + b) in pts for a, b in NB) for p in pts}
    arr = np.array(sorted(pts))
    ys, xs = np.nonzero(ink)
    x0, x1, y0, y1 = xs.min(), xs.max(), ys.min(), ys.max()

    def snap(w):
        y, x = y0 + w[1] * (y1 - y0), x0 + w[0] * (x1 - x0)
        return tuple(arr[np.argmin((arr[:, 0] - y) ** 2 + (arr[:, 1] - x) ** 2)])

    strokes = []
    for way in plan:
        snapped = [snap(w) for w in way]
        path = [snapped[0]]
        for a, b in zip(snapped, snapped[1:]):
            path += shortest(pts, a, b)[1:]
        if len(snapped) > 1:
            if deg[snapped[0]] <= 1:
                path = extend(path, ink, False)
            if deg[snapped[-1]] <= 1:
                path = extend(path, ink, True)
            p = resample(smooth(np.array(path, float), 5), STEP * S)
        else:
            p = np.array(path, float)
        r = np.array([max(dt[int(round(y)), int(round(x))], 1.0) for y, x in p])
        if len(r) > 3:
            r = ndimage.gaussian_filter1d(r, 1.5, mode="nearest")
        strokes.append(np.stack([(p[:, 1] - PAD) / S, (p[:, 0] - base) / S, r / S], 1))

    # The pen is as wide as the letter's strokes. Where strokes cross, the
    # centerline sits in a wider patch of ink; cap the width near the
    # letter's usual one. Where a stroke runs out to the ink's edge the width
    # falls to nothing; hold it.
    usual = np.median(np.concatenate([s[:, 2] for s in strokes]))
    for s in strokes:
        s[:, 2] = np.minimum(s[:, 2], usual * 1.15)
        if len(s) >= 8:
            for seq in (s, s[::-1]):
                seq[:5, 2] = np.maximum(seq[:5, 2], seq[5:8, 2].max())
    # A stroke that starts or ends on another, like the bowl of a d on its
    # stem, stops at that stroke's edge. Run on to its centerline and it
    # would ink a round patch of the other stroke before the pen gets there.
    # The same goes for a start that a later part of its own stroke passes
    # over, like the crossbar of an e where the bowl comes round.
    def inside(q, others):
        for o in others:
            d, at = distance(q[:2], o)
            if d < np.interp(at, arc_of(o), o[:, 2]) * 0.95:
                return True
        return False

    for k, s in enumerate(strokes):
        if len(s) < 3:
            continue
        others = [o for j, o in enumerate(strokes) if j != k and len(o) > 1]
        arc = arc_of(s)
        a, b = 0, len(s)
        while b - a > 2:
            later = s[arc > arc[a] + 3 * s[a, 2]]
            if not inside(s[a], others + ([later] if len(later) > 1 else [])):
                break
            a += 1
        while b - a > 2 and inside(s[b - 1], others):
            b -= 1
        strokes[k] = s[a:b]
    return strokes, ink, base


def timed(strokes):
    """Add when the nib reaches each point. Returns strokes and duration."""
    out = []
    t = 0.0
    for i, s in enumerate(strokes):
        if i:
            t += hop_ms(float(np.hypot(*(s[0, :2] - strokes[i - 1][-1, :2]))))
        if len(s) == 1:
            # A dab: the ink spreads from a point as the nib presses down.
            x, y, r = s[0]
            ts = np.linspace(0, DAB_MS, 6)
            out.append(np.array([[x, y, r * (0.35 + 0.65 * k / 5), t + tk] for k, tk in enumerate(ts)]))
            t += DAB_MS
            continue
        seg = np.hypot(*np.diff(s[:, :2], axis=0).T)
        arc = np.concatenate([[0], np.cumsum(seg)])
        if len(s) < 3:
            # Too short to bend: a tick at half speed.
            ts = t + np.concatenate([[0], np.cumsum(seg / (VMAX / 2))])
            out.append(np.concatenate([s, ts[:, None]], 1))
            t = ts[-1]
            continue
        heading = np.unwrap(np.arctan2(*np.diff(s[:, :2], axis=0).T[::-1]))
        bend = np.abs(np.gradient(heading)) / np.maximum(seg, 1e-6)
        bend = np.concatenate([bend[:1], ndimage.gaussian_filter1d(bend, 2)])
        radius = 1 / np.maximum(bend, 1e-6)
        v = VMAX * np.clip((radius / BEND_RADIUS) ** (1 / 3), BEND_FLOOR, 1)
        smoothstep = lambda e, x: np.clip(x / e, 0, 1) ** 2 * (3 - 2 * np.clip(x / e, 0, 1))
        v *= 0.3 + 0.7 * smoothstep(0.1, arc)
        v *= 0.4 + 0.6 * smoothstep(0.08, arc[-1] - arc)
        dts = np.concatenate([[0], seg / ((v[1:] + v[:-1]) / 2)])
        ts = t + np.cumsum(dts)
        out.append(np.concatenate([s, ts[:, None]], 1))
        t = ts[-1]
    return out, t


def reveal_times(strokes, ink, base):
    """When each ink pixel appears, worked out the way site/quill-write.js
    does it. Each stroke inks a band: its centerline, as wide as the pen,
    with square ends. The ink's leading edge is round with its tip under the
    nib, so a pixel on the centerline appears as the nib reaches it and one
    at the band's edge a pen's half width later. A pixel in several bands,
    where strokes cross, takes the earliest. A pixel in none, like the
    corner of a square terminal, takes the time of the nearest pixel that
    has one."""
    ys, xs = np.nonzero(ink)
    px = np.stack([(xs - PAD) / S, (ys - base) / S], 1)
    when = np.full(len(px), np.inf)
    slack = 0.6 / S
    for s in strokes:
        arc = arc_of(s)
        if arc[-1] == 0:
            # A dab: the ink spreads out from under the nib.
            d = np.hypot(px[:, 0] - s[0, 0], px[:, 1] - s[0, 1])
            for x, y, r, t in s:
                when = np.where(d <= r + slack, np.minimum(when, t), when)
            continue
        n = len(s) - 1
        for i in range(n):
            a, b = s[i, :2], s[i + 1, :2]
            ab = b - a
            L2 = max(float(ab @ ab), 1e-12)
            u = ((px - a) @ ab) / L2
            lo = 0 if i == 0 else -0.5
            hi = 1 if i == n - 1 else 1.5
            uc = np.clip(u, 0, 1)
            d = np.hypot(*(a + np.outer(uc, ab) - px).T)
            r = s[i, 2] + (s[i + 1, 2] - s[i, 2]) * uc
            at = arc[i] + uc * np.sqrt(L2)
            band = (d <= r + slack) & (u >= lo) & (u <= hi)
            tc = np.interp(at + r - np.sqrt(np.maximum(r * r - d * d, 0)), arc, s[:, 3])
            when = np.where(band, np.minimum(when, tc), when)
    grid = np.full(ink.shape, np.inf)
    grid[ys, xs] = when
    _, (iy, ix) = ndimage.distance_transform_edt(~np.isfinite(grid), return_indices=True)
    return ys, xs, grid[iy, ix][ys, xs]


def debug(ch, strokes, ink, base, path):
    H, W = ink.shape
    end = strokes[-1][-1, 3]
    ys, xs, when = reveal_times(strokes, ink, base)
    f = when / end
    rgb = np.zeros((H, W, 3), np.uint8)
    rgb[ys, xs, 0] = (40 + 200 * f).astype(np.uint8)
    rgb[ys, xs, 1] = (200 - 150 * f).astype(np.uint8)
    rgb[ys, xs, 2] = 120
    img = Image.fromarray(rgb)
    d = ImageDraw.Draw(img)
    label = ImageFont.load_default(36)
    for i, s in enumerate(strokes):
        pts = [(x * S + PAD, y * S + base) for x, y, _, _ in s]
        if len(pts) > 1:
            d.line(pts, fill=(255, 255, 255), width=3)
        d.ellipse((pts[0][0] - 9, pts[0][1] - 9, pts[0][0] + 9, pts[0][1] + 9), fill=(255, 255, 0))
        d.text((pts[0][0] + 10, pts[0][1] - 30), str(i + 1), fill=(255, 255, 0), font=label)
        if len(pts) > 3:
            a, b = pts[-4], pts[-1]
            ang = math.atan2(b[1] - a[1], b[0] - a[0])
            for da in (2.6, -2.6):
                d.line([b, (b[0] + 22 * math.cos(ang + da), b[1] + 22 * math.sin(ang + da))], fill=(255, 255, 255), width=3)
    d.text((10, 10), f"{ch}  {end:.0f} ms", fill=(255, 255, 255), font=ImageFont.load_default(30))
    img.save(path)


def subset_hand(src, out_dir, sub):
    """Sacramento, Basic Latin, kerning only, renamed: its license reserves
    the name Sacramento for the original font."""
    from fontTools.ttLib import TTFont

    subprocess.run(["pyftsubset", src, f"--unicodes={UNICODES}", "--layout-features=kern", f"--output-file={sub}"], check=True)
    font = TTFont(sub)
    for rec in font["name"].names:
        if rec.nameID in (1, 3, 4, 16):
            rec.string = "Quill Hand"
        elif rec.nameID == 6:
            rec.string = "QuillHand-Regular"
    font.save(sub)
    font.flavor = "woff2"
    font.save(os.path.join(out_dir, "hand.woff2"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("inter_variable")
    ap.add_argument("sacramento")
    ap.add_argument("--debug")
    args = ap.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        inst = os.path.join(tmp, "inst.ttf")
        sub = os.path.join(tmp, "hand.ttf")
        subprocess.run(["fonttools", "varLib.instancer", args.inter_variable, "wght=750", "opsz=32", "-q", "-o", inst], check=True)
        common = [inst, f"--unicodes={UNICODES}", "--layout-features=kern"]
        subprocess.run(["pyftsubset", *common, "--flavor=woff2", f"--output-file={ROOT}/site/fonts/headline.woff2"], check=True)
        subset_hand(args.sacramento, f"{ROOT}/site/fonts", sub)
        font = ImageFont.truetype(sub, S)
        glyphs = {}
        for ch, plan in PLANS.items():
            strokes, ink, base = trace(font, ch, plan)
            strokes, ms = timed(strokes)
            glyphs[ch] = {
                "ms": round(ms),
                "strokes": [[int(round(v)) for x, y, r, t in s for v in (x * 1000, y * 1000, r * 1000, t)] for s in strokes],
            }
            if args.debug:
                os.makedirs(args.debug, exist_ok=True)
                debug(ch, strokes, ink, base, os.path.join(args.debug, f"{ord(ch):04x}.png"))
    with open(f"{ROOT}/site/data/headline-strokes.json", "w") as f:
        json.dump({"font": "Sacramento", "hop": HOP, "glyphs": glyphs}, f, separators=(",", ":"))
        f.write("\n")


if __name__ == "__main__":
    main()
