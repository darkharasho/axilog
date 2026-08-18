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
THRESHOLD = {"quickness": (0.70, 0.32)}


def glyph_mask(png: pathlib.Path, name: str = ""):
    """The glyph silhouette as a boolean array: bright and desaturated."""
    import numpy as np
    from PIL import Image

    a = np.asarray(Image.open(png).convert("RGBA"), dtype=float) / 255.0
    rgb, alpha = a[..., :3], a[..., 3]
    mx, mn = rgb.max(-1), rgb.min(-1)
    sat = np.where(mx > 0, (mx - mn) / np.maximum(mx, 1e-6), 0.0)
    val, satmax = THRESHOLD.get(name, (0.80, 0.22))
    m = (mx > val) & (sat < satmax) & (alpha > 0.5)
    return m & _inside_frame(m.shape)


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


def to_path(loop, eps=0.42, corner_deg=52.0, smooth=0.30):
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


def trace_glyph(png: pathlib.Path, name: str = "") -> str:
    mask = glyph_mask(png, name)
    scale = VIEW / mask.shape[0]
    loops = boundary_loops(mask)
    if scale != 1.0:
        loops = [[(x * scale, y * scale) for x, y in lp] for lp in loops]
    # Drop specks, and drop pinholes: lowering a threshold far enough to
    # catch Quickness' dim motion streaks also catches the anti-aliased
    # halo inside the runner, which would otherwise punch it hollow.
    unit = scale * scale
    loops = [
        lp for lp in loops
        if _area(lp) >= 0.6 * unit
        and not (_signed_area(lp) > 0 and _area(lp) < 5.0 * unit)
    ]
    return "".join(to_path(lp) for lp in sorted(loops, key=_area, reverse=True))


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


# --- hand-authored glyphs --------------------------------------------
#
# Alacrity and Resolution only exist as 32x32 painterly art whose masks
# fragment at every threshold, so they are drawn to match the traced set.

HAND_DRAWN = {
    # Crescent moon behind a clock face reading past the hour.
    "alacrity": (
        "M22.6 8.2A12.4 12.4 0 1 0 22.6 32.6"
        "A9.9 9.9 0 1 1 22.6 8.2Z"
        "M24.4 10.6A9.8 9.8 0 1 1 24.4 30.2A9.8 9.8 0 1 1 24.4 10.6Z"
        "M23.3 13.4H25.5V21.4H31.4V23.6H23.3Z"
    ),
    # Heater shield carrying a four-point star.
    "resolution": (
        "M20 7.4L31.6 11.2V21.6C31.6 27.9 26.8 31.6 20 34.2"
        "C13.2 31.6 8.4 27.9 8.4 21.6V11.2Z"
        "M20 12.8L22.4 19.2L28.8 21.6L22.4 24L20 30.4"
        "L17.6 24L11.2 21.6L17.6 19.2Z"
    ),
}


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
  <path fill="#fff" fill-opacity=".13" d="{sheen}"/>
  <path fill="{glyph}" fill-rule="evenodd" d="{d}"/>
</svg>
"""


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
    title = f"File:{name.capitalize()} 40px.png"
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
    ap.add_argument("--cache", default="/tmp/axilog-boon-src", type=pathlib.Path)
    ap.add_argument(
        "--out",
        default=pathlib.Path("crates/axilog-html/assets/icons/boons"),
        type=pathlib.Path,
    )
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    for name in sorted(OFFICIAL):
        if name in HAND_DRAWN:
            d, how = HAND_DRAWN[name], "hand-authored"
        else:
            src = (
                wiki_40px(name, args.cache)
                if name in HAND_DRAWN or name in HAS_40PX
                else fetch(RENDER + OFFICIAL[name][1] + ".png",
                           args.cache / f"{name}-32.png")
            )
            d, how = trace_glyph(src, name), f"traced {src.name}"
        (args.out / f"{name}.svg").write_text(
            SVG.format(
                v=VIEW, label=name, light=BODY_LIGHT, mid=BODY_MID,
                dark=BODY_DARK, border=BORDER, glyph=GLYPH,
                outer=FRAME_OUTER, inner=FRAME_INNER, sheen=FRAME_SHEEN, d=d,
            )
        )
        print(f"{name:<13} {how:<22} {len(d):5d} chars")
    return 0


if __name__ == "__main__":

    sys.exit(main())
