#!/usr/bin/env python3
"""Generate the boon icon SVGs in `crates/axilog-html/assets/icons/boons/`.

GW2 boon art is only ever published at 32x32 (the official render service)
or 40x40 (the wiki's `Category:Boon_icons`), so tracing a whole icon --
frame bevel, body gradient and glyph together -- cannot produce clean
vectors. It also produces the classic artefact: the frame border and the
glyph come out as a single path with the glyph as a *hole*, so stroking
the result outlines the glyph's inner boundary too.

This script sidesteps both problems by separating the two layers:

- the **frame** is hand-authored vector geometry, measured off the art
  (apex at x=20 y=0, shoulders at y=10, sides at x=2.5/37.5, flat bottom)
  and shared byte-for-byte across every icon;
- the **glyph** is a flat near-white silhouette, i.e. a clean binary mask,
  which is the one part of the source that vectorises well at this size.
  It is thresholded out and traced here, so no stroke is ever involved.

Two glyphs are hand-authored instead of traced. Alacrity and Resolution
are newer, painterly art published at 32x32 only; their masks fragment at
every threshold (Alacrity's clock face never separates from its crescent,
Resolution speckles). They are redrawn to match the family.

Usage:  python3 scripts/gen_boon_svgs.py [--cache DIR] [--out DIR]
"""

from __future__ import annotations

import argparse
import math
import pathlib
import sys
import urllib.parse
import urllib.request

# --- source art -------------------------------------------------------

WIKI_API = "https://wiki.guildwars2.com/api.php"
RENDER = "https://render.guildwars2.com/file/"

# Official render-service art, keyed by the buff id in
# `crates/axilog-core/src/analysis/buff_icons.rs`. Always 32x32.
OFFICIAL = {
    "might": (740, "2FA9DF9D6BC17839BBEA14723F1C53D645DDB5E1/102852"),
    "fury": (725, "96D90DF84CAFE008233DD1C2606A12C1A0E68048/102842"),
    "quickness": (1187, "D4AB6401A6D6917C3D4F230764452BCCE1035B0D/1012835"),
    "alacrity": (30328, "4FDAC2113B500104121753EF7E026E45C141E94D/1938787"),
    "protection": (717, "CD77D1FAB7B270223538A8F8ECDA1CFB044D65F4/102834"),
    "regeneration": (718, "F69996772B9E18FD18AD0AABAB25D7E3FC42F261/102835"),
    "swiftness": (719, "20CFC14967E67F7A3FD4A4B8722B4CF5B8565E11/102836"),
    "vigor": (726, "58E92EBAF0DB4DA7C4AC04D9B22BCA5ECF0100DE/102843"),
    "aegis": (743, "DFB4D1B50AE4D6A275B349E15B179261EE3EB0AF/102854"),
    "stability": (1122, "3D3A1C2D6D791C05179AB871902D28782C65C244/415959"),
    "resistance": (26980, "50BAC1B8E10CFAB9E749A5D910D4A9DCF29EBB7C/961398"),
    "resolution": (873, "D104A6B9344A2E2096424A3C300E46BC2926E4D7/2440718"),
}

# Boons with a 40x40 wiki variant, which is the better trace source.
# Alacrity and Resolution have no 40px upload; they are hand-authored
# anyway, so the gap costs nothing.
HAS_40PX = {
    "aegis", "fury", "might", "protection", "quickness",
    "regeneration", "resistance", "stability", "swiftness", "vigor",
}

# --- palette, sampled from the art -----------------------------------
#
# Every boon shares one frame in one colour; only the glyph differs.
# `#A74806` appears as exactly 205 border pixels in nine separate icons,
# i.e. the identical ring. The body is a top-left to bottom-right ramp.

BORDER = "#A74806"
BODY_LIGHT = "#E9C68F"
BODY_MID = "#D89440"
BODY_DARK = "#BF700A"
GLYPH = "#FEFBEF"

# --- frame geometry, measured off the 40x40 art ----------------------

VIEW = 40
SUPER = 8      # mask is thresholded at 8x, then low-passed
BLUR = 0.55    # gaussian radius, in source pixels
FRAME_OUTER = "M20 0L37.5 10V40H2.5V10Z"
FRAME_INNER = "M20 2.07L35.7 11.04V38.2H4.3V11.04Z"
# GW2 lights the upper-left of the body across a hard diagonal running
# from the right shoulder to the bottom-left corner. Without it the frame
# reads flat next to the original.
FRAME_SHEEN = "M20 2.07L35.7 11.04L4.3 38.2V11.04Z"


# --- mask extraction --------------------------------------------------

# Glyphs are near-white on orange, so one cutoff serves almost all of
# them. Quickness is the exception: its motion streaks are drawn dimmer
# than the runner and vanish at the shared threshold, leaving three
# specks where the art has three streaks.
THRESHOLD = {}

# Rectangles (in source pixels) zeroed out of a mask before tracing.
# Quickness' three motion streaks are drawn too dim to survive any
# threshold intact; the fragments that do survive are debris, so they
# are cut and redrawn as EXTRA geometry.
MASK_CLIP = {"quickness": (0, 19, 16, 25)}

# Hand-drawn geometry appended to a traced glyph.
EXTRA = {"quickness": "M8.60 14.90Q12.35 15.19 15.80 13.70Q12.05 13.41 8.60 14.90ZM7.40 21.50Q12.03 21.89 16.40 20.30Q11.77 19.91 7.40 21.50ZM6.60 26.80Q9.87 27.08 12.80 25.60Q9.53 25.32 6.60 26.80Z"}


# Glyphs whose art is left-right symmetric. The 40px source is not: its
# anti-aliasing differs side to side, so a trace of it comes out with one
# limb fatter or a corner clipped. Folding the mask against its mirror
# before tracing makes the two halves identical by construction. The
# value is the mirror axis, in source pixels (the art is not centred on
# 20 for every icon).
SYMMETRIC = {
    "aegis": 20.5,
    "protection": 20.5,
    "resistance": 20.5,
    "stability": 20.5,
    "vigor": 20.5,
}


def _fold(f, axis):
    """Mirror one half of `f` onto the other, about column `axis`.

    Averaging the two halves would be the obvious move, but the axis is
    only known to a fraction of a pixel, so averaging smears every thin
    feature -- Protection's visor slot lands half on itself and washes
    out below the threshold. Copying a whole half across keeps that
    half's contrast intact. Whichever choice stays closer to the
    unfolded mask wins.
    """
    import numpy as np

    c = int(round(axis))
    mirror = np.roll(f[:, ::-1], 2 * c - f.shape[1], axis=1)
    left, right = f.copy(), f.copy()
    left[:, c:] = mirror[:, c:]
    right[:, :c] = mirror[:, :c]
    ref = f > 127
    score = lambda g: (
        ((g > 127) & ref).sum() / max(((g > 127) | ref).sum(), 1)
    )
    return left if score(left) >= score(right) else right


def default_spec(name: str) -> dict:
    """The one-layer selection every icon gets unless LAYERS overrides it."""
    val, satmax = THRESHOLD.get(name, DEFAULT_THRESHOLD)
    if (POLARITY == "light") != (name in LIGHT_GLYPH):
        return {"mode": "light", "val": val, "sat": satmax}
    return {"mode": "dark", "val": val}


def layer_specs(name: str) -> list:
    return LAYERS.get(name) or [default_spec(name)]


def select(a, spec: dict):
    """One layer's pixels, chosen from an RGBA float array.

    Three ways to tell glyph from body, because no one of them covers the
    set. `light`/`dark` cut on value, which is what separates a white eye
    or a black flame from the tile under it. `hue` cuts on colour, which
    is the only thing that separates Chilled's blue snowflake or Slow's
    green turtle from red -- those glyphs span the same value range as
    the body they sit on, so no cutoff can split them.
    """
    import numpy as np

    rgb, alpha = a[..., :3], a[..., 3]
    mx, mn = rgb.max(-1), rgb.min(-1)
    sat = np.where(mx > 0, (mx - mn) / np.maximum(mx, 1e-6), 0.0)
    mode = spec.get("mode", "dark")
    if mode == "light":
        m = (mx > spec["val"]) & (sat < spec.get("sat", 1.0))
    elif mode == "hue":
        # Sixth-of-a-turn hue, in degrees, computed the usual way.
        d = np.maximum(mx - mn, 1e-6)
        r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        h = np.where(
            mx == r, ((g - b) / d) % 6,
            np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4),
        ) * 60.0
        lo, hi = spec["hue"]
        m = (h >= lo) & (h <= hi) & (sat > spec.get("satmin", 0.18))
        if "val" in spec:
            m &= mx > spec["val"]
        if "valmax" in spec:
            m &= mx < spec["valmax"]
    else:
        # Condition glyphs are dark *and* strongly coloured -- blue for
        # Chilled, green for Poison -- so saturation says nothing about
        # what is glyph and what is body. Value alone separates them:
        # the body ramp never falls below #D8351D.
        m = mx < spec["val"]
    return m & (alpha > 0.5)


def glyph_mask(png: pathlib.Path, name: str = "", spec: dict | None = None):
    """One glyph layer's silhouette, as a boolean array."""
    import numpy as np
    from PIL import Image, ImageFilter

    spec = spec or default_spec(name)
    img = Image.open(png).convert("RGBA")
    src = img.size[0]
    # Threshold at SUPER x resolution and low-pass the result. Thresholding
    # at 40px yields a mask whose boundary IS the pixel grid, so the trace
    # reproduces every stair step; interpolating first lets the boundary
    # fall between source pixels and come out curved.
    img = img.resize((src * SUPER, src * SUPER), Image.LANCZOS)
    a = np.asarray(img, dtype=float) / 255.0
    m = select(a, spec)
    # Per-layer, because the right amount of low-pass depends on the
    # feature size. The default rounds off stair steps on a solid glyph;
    # art built from 1px strokes with 1px gaps -- Immobile's chain links --
    # is closed solid by it, so those layers ask for far less.
    soft = Image.fromarray((m * 255).astype("uint8"), "L").filter(
        ImageFilter.GaussianBlur(SUPER * spec.get("blur", BLUR))
    )
    f = np.asarray(soft, dtype=float)
    if name in SYMMETRIC:
        f = _fold(f, SYMMETRIC[name] * SUPER)
    m = f > 127
    if name in MASK_CLIP:
        x0, y0, x1, y1 = (v * SUPER for v in MASK_CLIP[name])
        m[y0:y1, x0:x1] = False
    return m & _inside_frame(m.shape), src


# The frame's own bevel highlight is bright and desaturated too, so any
# threshold loose enough for Quickness' streaks also picks up an arc
# tracking the pentagon. Glyphs never touch the border, so clipping to
# the frame interior removes that whole class of false positive.
_FRAME_INSET = [(20, 3.8), (34.2, 12.8), (34.2, 36.7), (5.8, 36.7), (5.8, 12.8)]


def _inside_frame(shape):
    import numpy as np

    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    x = (xs + 0.5) * (VIEW / w)
    y = (ys + 0.5) * (VIEW / h)
    inside = np.zeros(shape, dtype=bool)
    n = len(_FRAME_INSET)
    for i in range(n):
        x0, y0 = _FRAME_INSET[i]
        x1, y1 = _FRAME_INSET[(i + 1) % n]
        crosses = ((y0 > y) != (y1 > y)) & (
            x < (x1 - x0) * (y - y0) / (y1 - y0 + 1e-12) + x0
        )
        inside ^= crosses
    return inside


# --- tracing ----------------------------------------------------------

def boundary_loops(mask):
    """Closed pixel-edge loops around `mask`, in pixel-corner coordinates.

    Each foreground pixel contributes a directed edge for every side whose
    neighbour is background, oriented so foreground stays on the right.
    Outer loops therefore wind clockwise and holes anticlockwise, which is
    what `fill-rule="evenodd"` needs to punch the holes back out.
    """
    import numpy as np

    m = np.pad(mask, 1, constant_values=False)
    edges: dict[tuple[int, int], list[tuple[int, int]]] = {}
    for r, c in zip(*np.nonzero(m)):
        r, c = int(r), int(c)
        if not m[r - 1, c]:
            edges.setdefault((c, r), []).append((c + 1, r))
        if not m[r, c + 1]:
            edges.setdefault((c + 1, r), []).append((c + 1, r + 1))
        if not m[r + 1, c]:
            edges.setdefault((c + 1, r + 1), []).append((c, r + 1))
        if not m[r, c - 1]:
            edges.setdefault((c, r + 1), []).append((c, r))

    loops = []
    while edges:
        start = next(iter(edges))
        loop, cur = [start], start
        while True:
            outs = edges.get(cur)
            if not outs:
                break
            nxt = outs.pop()
            if not outs:
                del edges[cur]
            if nxt == start:
                break
            loop.append(nxt)
            cur = nxt
        if len(loop) >= 4:
            # undo the 1px pad
            loops.append([(x - 1.0, y - 1.0) for x, y in loop])
    return loops


def _perp(p, a, b):
    (px, py), (ax, ay), (bx, by) = p, a, b
    dx, dy = bx - ax, by - ay
    n = math.hypot(dx, dy)
    if n == 0:
        return math.hypot(px - ax, py - ay)
    return abs(dy * px - dx * py + bx * ay - by * ax) / n


def simplify(pts, eps):
    """Douglas-Peucker. Collapses pixel staircases into clean diagonals."""
    if len(pts) < 3:
        return pts
    worst, idx = 0.0, 0
    for i in range(1, len(pts) - 1):
        d = _perp(pts[i], pts[0], pts[-1])
        if d > worst:
            worst, idx = d, i
    if worst <= eps:
        return [pts[0], pts[-1]]
    return simplify(pts[: idx + 1], eps)[:-1] + simplify(pts[idx:], eps)


def _corner(prev, cur, nxt, limit):
    ax, ay = cur[0] - prev[0], cur[1] - prev[1]
    bx, by = nxt[0] - cur[0], nxt[1] - cur[1]
    na, nb = math.hypot(ax, ay), math.hypot(bx, by)
    if na == 0 or nb == 0:
        return True
    cos = max(-1.0, min(1.0, (ax * bx + ay * by) / (na * nb)))
    return math.degrees(math.acos(cos)) > limit


def to_path(loop, eps=0.30, corner_deg=58.0, smooth=0.34):
    """One closed subpath: straight through corners, curved between them.

    Vertices whose turn exceeds `corner_deg` stay sharp -- icon glyphs are
    mostly hard-edged, and rounding a sword tip or a cross arm reads as
    blur. Everything else gets Catmull-Rom tangents so traced circles come
    out round rather than faceted.
    """
    pts = simplify(loop + [loop[0]], eps)[:-1]
    n = len(pts)
    if n < 3:
        return ""
    sharp = [
        _corner(pts[(i - 1) % n], pts[i], pts[(i + 1) % n], corner_deg)
        for i in range(n)
    ]

    def tangent(i):
        if sharp[i]:
            return (0.0, 0.0)
        px, py = pts[(i - 1) % n]
        nx, ny = pts[(i + 1) % n]
        return ((nx - px) * smooth, (ny - py) * smooth)

    out = [f"M{_n(pts[0][0])} {_n(pts[0][1])}"]
    for i in range(n):
        a, b = pts[i], pts[(i + 1) % n]
        ta, tb = tangent(i), tangent((i + 1) % n)
        if ta == (0.0, 0.0) and tb == (0.0, 0.0):
            out.append(f"L{_n(b[0])} {_n(b[1])}")
        else:
            c1 = (a[0] + ta[0], a[1] + ta[1])
            c2 = (b[0] - tb[0], b[1] - tb[1])
            out.append(
                f"C{_n(c1[0])} {_n(c1[1])} {_n(c2[0])} {_n(c2[1])}"
                f" {_n(b[0])} {_n(b[1])}"
            )
    return "".join(out) + "Z"


def _n(v):
    """Two decimals, trailing zeros stripped -- keeps the files small."""
    return f"{v:.2f}".rstrip("0").rstrip(".") or "0"


def trace_glyph(png: pathlib.Path, name: str = "",
                spec: dict | None = None) -> str:
    spec = spec or default_spec(name)
    mask, src = glyph_mask(png, name, spec)
    scale = VIEW / mask.shape[0]
    unit = (VIEW / src) ** 2  # area of one source pixel, in view units
    loops = boundary_loops(mask)
    if scale != 1.0:
        loops = [[(x * scale, y * scale) for x, y in lp] for lp in loops]
    # Drop specks, and drop pinholes: lowering a threshold far enough to
    # catch Quickness' dim motion streaks also catches the anti-aliased
    # halo inside the runner, which would otherwise punch it hollow.
    loops = [
        lp for lp in loops
        if _area(lp) >= spec.get("speck", 1.2) * unit
        and not (_signed_area(lp) > 0
                 and _area(lp) < spec.get("pinhole", 2.0) * unit)
    ]
    d = "".join(to_path(lp) for lp in sorted(loops, key=_area, reverse=True))
    return d + EXTRA.get(name, "")


def _signed_area(loop):
    """Positive for holes, negative for outer loops (y-down, see
    `boundary_loops`). Lets tiny anti-aliasing holes be dropped without
    touching the real ones."""
    s = 0.0
    for i in range(len(loop)):
        x0, y0 = loop[i]
        x1, y1 = loop[(i + 1) % len(loop)]
        s += x0 * y1 - x1 * y0
    return s / 2.0


def _area(loop):
    s = 0.0
    for i in range(len(loop)):
        x0, y0 = loop[i]
        x1, y1 = loop[(i + 1) % len(loop)]
        s += x0 * y1 - x1 * y0
    return abs(s) / 2.0


# --- drawing primitives ----------------------------------------------
#
# The hand-authored glyphs are built from these rather than written out
# as path literals, so `scripts/fit_boon_glyphs.py` can fit their
# parameters numerically instead of the shapes being nudged by eye.


def _p(*xy) -> str:
    return "".join("%.2f %.2f " % (x, y) for x, y in xy).strip()


def poly(pts) -> str:
    return ("M%.2f %.2f" % pts[0]
            + "".join("L%.2f %.2f" % q for q in pts[1:]) + "Z")


def ell(cx, cy, rx, ry, rot=0.0, cw=True) -> str:
    """A rotated ellipse as four cubics. `cw=False` reverses the winding,
    which is what makes it a hole under fill-rule="evenodd"."""
    k = 0.5522847498
    pts = [(0, -ry), (rx, 0), (0, ry), (-rx, 0)]
    tan = [(rx * k, 0), (0, ry * k), (-rx * k, 0), (0, -ry * k)]
    if not cw:
        pts = [pts[0], pts[3], pts[2], pts[1]]
        tan = [(-rx * k, 0), (0, ry * k), (rx * k, 0), (0, -ry * k)]
    a = math.radians(rot)
    ca, sa = math.cos(a), math.sin(a)
    T = lambda q: (cx + q[0] * ca - q[1] * sa, cy + q[0] * sa + q[1] * ca)
    R = lambda v: (v[0] * ca - v[1] * sa, v[0] * sa + v[1] * ca)
    P = [T(q) for q in pts]
    V = [R(v) for v in tan]
    d = "M%.2f %.2f" % P[0]
    for i in range(4):
        p0, v0 = P[i], V[i]
        p1, v1 = P[(i + 1) % 4], V[(i + 1) % 4]
        d += "C" + _p((p0[0] + v0[0], p0[1] + v0[1]),
                      (p1[0] - v1[0], p1[1] - v1[1]), p1)
    return d + "Z"


def bar(x0, y0, x1, y1, w) -> str:
    """A rectangle of half-width `w` from one point to another."""
    dx, dy = x1 - x0, y1 - y0
    L = math.hypot(dx, dy) or 1e-9
    nx, ny = -dy / L * w, dx / L * w
    return poly([(x0 + nx, y0 + ny), (x1 + nx, y1 + ny),
                 (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)])


def dash(x0, y0, x1, y1, t, bow=0.0) -> str:
    """A thin tapered motion line: two mirrored quadratics meeting at
    points, half-thickness `t` at the midpoint. Short and fat reads as a
    leaf; long and thin reads as speed."""
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    dx, dy = x1 - x0, y1 - y0
    L = math.hypot(dx, dy) or 1e-9
    nx, ny = -dy / L, dx / L
    return ("M%.2f %.2fQ%.2f %.2f %.2f %.2fQ%.2f %.2f %.2f %.2fZ" % (
        x0, y0, mx + nx * (t + bow) * 2, my + ny * (t + bow) * 2, x1, y1,
        mx - nx * (t - bow) * 2, my - ny * (t - bow) * 2, x0, y0))


def sword(hx, hy, tx, ty, w, gh, gt, gat, grip, tip) -> str:
    """One closed, non-self-overlapping outline: grip, crossguard,
    tapered blade, point. (hx, hy) is the hilt butt, (tx, ty) the tip."""
    dx, dy = tx - hx, ty - hy
    L = math.hypot(dx, dy) or 1e-9
    ux, uy = dx / L, dy / L
    nx, ny = -uy, ux
    P = lambda a, o: (hx + ux * a + nx * o, hy + uy * a + ny * o)
    d1 = gat * L
    d2 = d1 + gt
    return poly([
        P(0, grip), P(d1, grip), P(d1, gh), P(d2, gh), P(d2, w),
        P(L - tip, w), P(L, 0), P(L - tip, -w), P(d2, -w), P(d2, -gh),
        P(d1, -gh), P(d1, -grip), P(0, -grip),
    ])


# --- parametric hand-authored glyphs ---------------------------------
#
# Fitted by `scripts/fit_boon_glyphs.py`, which rasterises the candidate
# and maximises overlap against the source art. Edit the numbers here
# only to seed a fit; the fitter rewrites this dict in place.

GLYPH_PARAMS = {
    "alacrity": {
        "cx": 20.23,
        "cy": 21.92,
        "r": 11.40,
        "ecc": 1.10,
        "tilt": -21.55,
        "hr": 8.22,
        "hdx": 1.20,
        "hdy": -0.43,
        "h1a": -10.99,
        "h1l": 8.50,
        "h1w": 0.78,
        "h2a": 191.38,
        "h2l": 4.25,
        "h2w": 0.80,
        "dr": 12.04,
        "dlen": 3.08,
        "dt": 0.33,
        "dbase": 81.26,
        "dspread": 33.55,
    },
    "resolution": {
        "shw": 12.43,
        "sty": 11.18,
        "ssy": 24.13,
        "sby": 36.09,
        "dhw": 11.53,
        "dhh": 14.00,
        "dcy": 22.21,
        "hspread": 4.91,
        "hilty": 27.73,
        "tipx": 4.17,
        "tipy": 17.44,
        "sw": 1.02,
        "sgh": 1.55,
        "stip": 1.53,
    },
}


def build_alacrity(p) -> list:
    """A clock: a ring whose hole sits up and to the right so the band
    reads heavy at the bottom-left, two hands, and motion dashes off the
    top-right and bottom-left. Only 32px art exists, hence drawn."""
    hub = (p["cx"] + p["hdx"], p["cy"] + p["hdy"])
    ring = (ell(p["cx"], p["cy"], p["r"], p["r"] * p["ecc"], p["tilt"], True)
            + ell(hub[0], hub[1], p["hr"], p["hr"] * p["ecc"], p["tilt"],
                  False))
    hands = ""
    for a, L, w in ((p["h1a"], p["h1l"], p["h1w"]),
                    (p["h2a"], p["h2l"], p["h2w"])):
        t = math.radians(a)
        hands += bar(hub[0], hub[1],
                     hub[0] + L * math.sin(t), hub[1] - L * math.cos(t), w)
    dashes = ""
    for side in (0.0, 180.0):
        for k in (-1, 0, 1):
            t = math.radians(p["dbase"] + side + k * p["dspread"])
            ux, uy = math.cos(t), -math.sin(t)
            x0, y0 = p["cx"] + ux * p["dr"], p["cy"] + uy * p["dr"]
            dashes += dash(x0, y0, x0 + ux * p["dlen"], y0 + uy * p["dlen"],
                           p["dt"])
    return [ring, hands, dashes]


def build_resolution(p) -> list:
    """A shield with a four-point diamond punched out of it and crossed
    swords over the void. Each sword is its own path: concatenated,
    fill-rule="evenodd" punches a hole where they cross."""
    hw, ty, sy, by = p["shw"], p["sty"], p["ssy"], p["sby"]
    shield = (
        "M%.2f %.2fH%.2fV%.2f" % (20 - hw, ty, 20 + hw, sy)
        + "C" + _p((20 + hw, sy + (by - sy) * 0.42),
                   (20 + hw * 0.60, by - (by - sy) * 0.30), (20.0, by))
        + "C" + _p((20 - hw * 0.60, by - (by - sy) * 0.30),
                   (20 - hw, sy + (by - sy) * 0.42), (20 - hw, sy))
        + "Z"
        + poly([(20, p["dcy"] - p["dhh"]), (20 + p["dhw"], p["dcy"]),
                (20, p["dcy"] + p["dhh"]), (20 - p["dhw"], p["dcy"])])
    )
    kw = dict(w=p["sw"], gh=p["sgh"], gt=0.90, gat=0.22, grip=0.50,
              tip=p["stip"])
    a = sword(20 - p["hspread"], p["hilty"], 20 + p["tipx"], p["tipy"], **kw)
    b = sword(20 + p["hspread"], p["hilty"], 20 - p["tipx"], p["tipy"], **kw)
    return [shield, a, b]


BUILDERS = {"alacrity": build_alacrity, "resolution": build_resolution}


# --- hand-authored glyphs --------------------------------------------
#
# Alacrity and Resolution only exist as 32x32 painterly art whose masks
# fragment at every threshold, so they are drawn to match the traced set.

HAND_DRAWN = {
    "alacrity": build_alacrity(GLYPH_PARAMS["alacrity"]),
    "resolution": build_resolution(GLYPH_PARAMS["resolution"]),
    "might": (
        "M20 7.4L24.5 12.9V25.0H15.5V12.9ZM12.7 25.9H27.3L26.4 28.1H13.6ZM18.7 28.1H21.3V34.3L20 35.9L18.7 34.3Z"
    ),
}


# --- families ---------------------------------------------------------
#
# Boons and conditions share every mechanism: one hand-authored frame,
# one thresholded glyph mask, one tracer. They differ in three things --
# the frame's orientation (the condition pentagon is the boon pentagon
# flipped, point down), its palette, and which way the glyph contrasts
# with the body. A family is exactly that difference; `use_family` swaps
# it into the module globals the rest of the file already reads.

CONDITIONS = {
    "might": None,  # placeholder, replaced below
}

# Condition art, keyed by the buff id in
# `crates/axilog-core/src/analysis/buff_icons.rs`.
COND_OFFICIAL = {
    "bleeding": (736, "79FF0046A5F9ADA3B4C4EC19ADB4CB124D5F0021/102848"),
    "blinded": (720, "09770136BB76FD0DBE1CC4267DEED54774CB20F6/102837"),
    "burning": (737, "B47BF5803FED2718D7474EAF9617629AD068EE10/102849"),
    "chilled": (722, "28C4EC547A3516AF0242E826772DA43A5EAC3DF3/102839"),
    "confusion": (861, "289AA0A4644F0E044DED3D3F39CED958E1DDFF53/102880"),
    "crippled": (721, "070325E519C178D502A8160523766070D30C0C19/102838"),
    "fear": (791, "30307A6E766D74B6EB09EDA12A4A2DE50E4D76F4/102869"),
    "immobile": (727, "397A613651BFCA2832B6469CE34735580A2C120E/102844"),
    "poison": (723, "559B0AF9FB5E1243D2649FAAE660CCB338AACC19/102840"),
    "slow": (26766, "F60D1EF5271D7B9319610855676D320CD25F01C6/961397"),
    "taunt": (27705, "02EED459AD65FAF7DF32A260E479C625070841B9/1228472"),
    "torment": (19426, "10BABF2708CA3575730AC662A2E72EC292565B08/598887"),
    "vulnerability": (738, "3A394C1A0A3257EB27A44842DDEEF0DF000E1241/102850"),
    "weakness": (742, "6CB0E64AF9AA292E332A38C1770CE577E2CDE0E8/102853"),
}

# Everything but Taunt and Torment has a 40px wiki upload.
COND_HAS_40PX = set(COND_OFFICIAL) - {"taunt", "torment"}

FAMILIES = {
    "boons": {
        "OFFICIAL": OFFICIAL,
        "HAS_40PX": HAS_40PX,
        "WIKI_FILE": {},
        "BORDER": "#A74806",
        "BODY_LIGHT": "#E9C68F",
        "BODY_MID": "#D89440",
        "BODY_DARK": "#BF700A",
        "GLYPH": "#FEFBEF",
        "FRAME_OUTER": FRAME_OUTER,
        "FRAME_INNER": FRAME_INNER,
        "FRAME_SHEEN": FRAME_SHEEN,
        "_FRAME_INSET": _FRAME_INSET,
        "POLARITY": "light",
        "LIGHT_GLYPH": set(),
        "DEFAULT_THRESHOLD": (0.80, 0.22),
        "THRESHOLD": THRESHOLD,
        "MASK_CLIP": MASK_CLIP,
        "EXTRA": EXTRA,
        "SYMMETRIC": SYMMETRIC,
        "HAND_DRAWN": HAND_DRAWN,
        "LAYERS": {},
        "SAMPLE_GLYPH": False,
        "OUT": "boons",
    },
    "conditions": {
        "OFFICIAL": COND_OFFICIAL,
        "HAS_40PX": COND_HAS_40PX,
        # The wiki files the icon "Blinded" under the effect's other name.
        "WIKI_FILE": {"blinded": "Blind"},
        # Sampled the same way as the boon palette: the border is one
        # exact colour repeated around every icon, the body a top-left to
        # bottom-right ramp.
        "BORDER": "#5A100C",
        "BODY_LIGHT": "#F1B4AB",
        "BODY_MID": "#E26856",
        "BODY_DARK": "#D8351D",
        "GLYPH": "#3A1410",
        # The boon pentagon flipped in y: flat top, point at the bottom.
        "FRAME_OUTER": "M20 40L37.5 30V0H2.5V30Z",
        "FRAME_INNER": "M20 37.93L35.7 28.96V1.8H4.3V28.96Z",
        # The condition body ramps smoothly from corner to corner; there
        # is no hard-edged sheen to reproduce, so the gradient carries it.
        "FRAME_SHEEN": "",
        "_FRAME_INSET": [(20, 36.2), (34.2, 27.2), (34.2, 3.3),
                         (5.8, 3.3), (5.8, 27.2)],
        # Condition glyphs are dark on a bright body, the reverse of the
        # boons. The body never falls below #D8351D (max channel 216), so
        # 0.80 clears it with room for anti-aliasing.
        "POLARITY": "dark",
        # Blinded is the family's one bright glyph: a white eye, not a
        # dark one. Traced dark it comes out as the eye's *outline*.
        "LIGHT_GLYPH": {"blinded"},
        # The body ramp bottoms out at #D8351D (max channel 216), but a
        # cutoff just under that also swallows each glyph's outer glow
        # and the icons read as blobs. 0.55 takes the glyph proper.
        "DEFAULT_THRESHOLD": (0.55, 0.0),
        # Taunt and Torment have no 40px upload, and the 32px render-service
        # art is dimmer overall -- the shared cutoff would call the whole
        # tile glyph.
        "THRESHOLD": {
            # No 40px upload; the 32px render-service art is dimmer
            # overall, so the shared cutoff would call the whole tile
            # glyph.
            "taunt": (0.42, 0.0),
            "torment": (0.40, 0.0),
            # The spiral itself ramps from 0.29 at the bottom right to
            # 0.50 at the top left, under a glow field that ramps from
            # 0.65 to 0.85. A cutoff inside the glyph's own range cuts
            # the spiral in half along that diagonal; 0.60 clears it.
            "confusion": (0.60, 0.0),
            "blinded": (0.85, 0.12),
            "immobile": (0.76, 0.0),
        },
        # Icons the single-layer model cannot express, either because
        # the art uses two inks or because value alone cannot find the
        # glyph. Layers are drawn in order, first at the bottom.
        "LAYERS": {
            # A white eye ringed in near-black. One layer gives you one
            # or the other; the eye alone floats, so draw the ring under
            # it and let the eye punch the middle back out.
            "blinded": [
                {"mode": "dark", "val": 0.68},
                {"mode": "light", "val": 0.85, "sat": 0.12},
            ],
            # The flame is two inks: a near-black body and an orange
            # core. The core is *brighter* than the tile it sits on, so
            # only hue finds it -- 33 degrees against the body's 8.
            "burning": [
                {"mode": "dark", "val": 0.55},
                {"mode": "hue", "hue": (18, 70), "satmin": 0.4,
                 "val": 0.55, "keep": "light"},
            ],
            # Blue on red. The snowflake's spikes fade to within a few
            # percent of the body's value at their tips, so a value
            # cutoff either loses the top spike or floods; hue does not
            # care how faint they are.
            # Hue looks like the obvious pick here -- blue on red -- but
            # the snowflake's spikes fade to grey before they end, and
            # the glow around it stays blue, so a hue mask drops the
            # spikes and fills the gaps between them: a hexagram. Value
            # keeps them; 0.62 is above the spike tips (0.50) and below
            # the glow (0.72).
            "chilled": [{"mode": "dark", "val": 0.62}],
        # Two draped chains, and a chain is holes: every link is a ring
        # about two source pixels across with a one-pixel eye. The
        # default low-pass closes every one of those eyes and welds
        # neighbouring links together, which is why it read as a blob.
        # A quarter of the usual blur keeps the links apart, and the
        # pinhole floor has to come down with it or the trace drops the
        # eyes right after the mask preserved them.
        "immobile": [
            {"mode": "dark", "val": 0.66, "blur": 0.1,
             "pinhole": 0.3, "speck": 0.5},
        ],
            # Green on red, same argument, plus a gold rim along the
            # shell that reads as a highlight rather than a second glyph.
            "slow": [
                {"mode": "hue", "hue": (55, 175), "satmin": 0.15},
                # The shell is a lighter green than the outline and the
                # legs -- 0.28 against 0.19 -- and drawing it as its own
                # layer is what keeps the turtle from reading as a blob.
                {"mode": "hue", "hue": (55, 175), "satmin": 0.15,
                 "val": 0.245, "keep": "light", "speck": 2.0},
                # The gold band along the shell's leading edge. Green
                # meeting red anti-aliases through orange, so the same
                # cut picks up a scatter of single pixels all over the
                # turtle -- hence the much larger speck floor.
                {"mode": "hue", "hue": (14, 58), "satmin": 0.35,
                 "val": 0.45, "keep": "light", "speck": 4.0},
            ],
        },
        "MASK_CLIP": {},
        "EXTRA": {},
        "SYMMETRIC": {},
        "HAND_DRAWN": {},
        # Boon glyphs are all one white; condition glyphs each have their
        # own colour, so it is read off the art rather than tabulated.
        "SAMPLE_GLYPH": True,
        "OUT": "conditions",
    },
}
del CONDITIONS


def use_family(name: str) -> None:
    globals().update(FAMILIES[name])


def glyph_color(png: pathlib.Path, name: str = "",
                spec: dict | None = None) -> str:
    """A single flat fill for a traced glyph, read off the source.

    The art shades each glyph, but the boon set is flat white and these
    have to sit beside it, so one colour it is: the median of the glyph
    interior, which ignores both the dark outline and the highlight.

    The mask is eroded first. Its border pixels are anti-aliased against
    the red body, and including them pulls every colour towards the
    frame -- it is what turned Chilled's blue snowflake mauve.
    """
    import numpy as np
    from PIL import Image, ImageFilter

    spec = spec or default_spec(name)
    m, _ = glyph_mask(png, name, spec)
    img = Image.open(png).convert("RGB")
    core = Image.fromarray((m * 255).astype("uint8"), "L").resize(
        img.size, Image.LANCZOS
    ).filter(ImageFilter.MinFilter(3))
    sel = np.asarray(core) > 200
    if not sel.any():
        # Confusion's spiral is a pixel wide in places, so eroding it
        # leaves nothing. Fall back to the mask itself.
        sel = np.asarray(
            Image.fromarray((m * 255).astype("uint8"), "L").resize(
                img.size, Image.LANCZOS
            )
        ) > 127
    a = np.asarray(img)[sel]
    if not len(a):
        return GLYPH
    # Take the median of the extreme two-fifths, not of everything: a
    # glyph's characteristic colour lives in its darkest pixels (its
    # brightest, for a light one), and the rest is shading towards the
    # body. Averaged in, it turned Poison's green skull grey-brown.
    v = a.max(1)
    keep = np.argsort(v)[: max(1, len(a) * 2 // 5)]
    if spec.get("keep", spec.get("mode")) == "light":
        keep = np.argsort(-v)[: max(1, len(a) * 2 // 5)]
    return "#%02X%02X%02X" % tuple(
        int(x) for x in np.median(a[keep], axis=0)
    )


# --- assembly ---------------------------------------------------------

SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {v} {v}" \
role="img" aria-label="{label}">
  <defs>
    <linearGradient id="b" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="{light}"/>
      <stop offset=".55" stop-color="{mid}"/>
      <stop offset="1" stop-color="{dark}"/>
    </linearGradient>
  </defs>
  <path fill="{border}" d="{outer}"/>
  <path fill="url(#b)" d="{inner}"/>
{sheenpath}{glyphpaths}</svg>
"""


def glyph_paths(d, fill: str | None = None) -> str:
    """Render a glyph as one <path> per subpath group.

    Overlapping shapes have to live in separate elements: within one path
    fill-rule="evenodd" XORs them, so two crossed swords would punch a
    diamond hole where they meet instead of merging.
    """
    parts = [d] if isinstance(d, str) else list(d)
    fill = fill or GLYPH
    return "".join(
        '  <path fill="%s" fill-rule="evenodd" d="%s"/>\n' % (fill, x)
        for x in parts if x
    )


def fetch(url: str, dest: pathlib.Path) -> pathlib.Path:
    if not dest.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        req = urllib.request.Request(
            url, headers={"User-Agent": "axilog-icon-tooling/0.1"}
        )
        with urllib.request.urlopen(req, timeout=30) as r:
            dest.write_bytes(r.read())
    return dest


def wiki_40px(name: str, cache: pathlib.Path) -> pathlib.Path:
    import json

    dest = cache / f"{name}-40.png"
    if dest.exists():
        return dest
    title = f"File:{WIKI_FILE.get(name, name.capitalize())} 40px.png"
    q = (
        f"{WIKI_API}?action=query&titles={urllib.parse.quote(title)}"
        "&prop=imageinfo&iiprop=url&format=json"
    )
    req = urllib.request.Request(q, headers={"User-Agent": "axilog-icon-tooling/0.1"})
    with urllib.request.urlopen(req, timeout=30) as r:
        page = next(iter(json.load(r)["query"]["pages"].values()))
    return fetch(page["imageinfo"][0]["url"], dest)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--family", default="boons", choices=sorted(FAMILIES))
    ap.add_argument("--cache", default=None, type=pathlib.Path)
    ap.add_argument("--out", default=None, type=pathlib.Path)
    args = ap.parse_args()
    use_family(args.family)

    cache = args.cache or pathlib.Path(f"/tmp/axilog-{args.family}-src")
    out = args.out or pathlib.Path("crates/axilog-html/assets/icons") / OUT
    out.mkdir(parents=True, exist_ok=True)

    for name in sorted(OFFICIAL):
        if name in HAND_DRAWN:
            drawn, how = [(HAND_DRAWN[name], None)], "hand-authored"
        else:
            src = (
                wiki_40px(name, cache)
                if name in HAS_40PX
                else fetch(RENDER + OFFICIAL[name][1] + ".png",
                           cache / f"{name}-32.png")
            )
            how = f"traced {src.name}"
            drawn = [
                (trace_glyph(src, name, spec),
                 spec.get("fill") or (glyph_color(src, name, spec)
                                      if SAMPLE_GLYPH else None))
                for spec in layer_specs(name)
            ]
        sheen = (
            '  <path fill="#fff" fill-opacity=".13" d="%s"/>\n' % FRAME_SHEEN
            if FRAME_SHEEN else ""
        )
        (out / f"{name}.svg").write_text(
            SVG.format(
                v=VIEW, label=name, light=BODY_LIGHT, mid=BODY_MID,
                dark=BODY_DARK, border=BORDER,
                outer=FRAME_OUTER, inner=FRAME_INNER, sheenpath=sheen,
                glyphpaths="".join(
                    glyph_paths(d, fill) for d, fill in drawn
                ),
            )
        )
        n = sum(len(d) if isinstance(d, str) else sum(map(len, d))
                for d, _ in drawn)
        fills = " ".join(f or "" for _, f in drawn)
        print(f"{name:<15} {how:<22} {n:5d} chars {fills}")
    return 0


if __name__ == "__main__":

    sys.exit(main())
