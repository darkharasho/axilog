#!/usr/bin/env python3
"""Fit the hand-authored boon glyphs to the source art numerically.

The hand-drawn glyphs (Alacrity, Resolution) used to be path literals
tuned by eye against a 32x32 reference, which is slow and unreliable --
at that size the difference between "the diamond is too squished" and
"the diamond is right" is two or three pixels.

This turns fit into a number. A candidate glyph is rasterised with
cairosvg at the same scale as the reference, and scored against the
reference's thresholded mask; Nelder-Mead then moves the parameters in
`gen_boon_svgs.GLYPH_PARAMS` to maximise that score.

    python3 scripts/fit_boon_glyphs.py --report
    python3 scripts/fit_boon_glyphs.py --fit alacrity resolution --write

`--report` scores all twelve icons, traced ones included, so a change to
the tracer shows up as a column of numbers rather than something you
have to catch by eye in a render.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

import cairosvg
import numpy as np
from PIL import Image
from scipy.optimize import minimize

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import gen_icon_svgs as gen  # noqa: E402

CACHE = pathlib.Path("/tmp/axilog-boons-src")
BIG = 160          # comparison resolution, 4x the 40-unit view
W_BIG = 0.55       # silhouette agreement
W_NATIVE = 0.30    # agreement at the reference's own resolution
W_EDGE = 0.15      # boundary agreement -- keeps IoU from rewarding blobs
P_OUTSIDE = 6.0    # penalty for glyph ink outside the frame interior


# --- rasterising ------------------------------------------------------

def render(paths, size: int) -> np.ndarray:
    """Coverage in [0, 1] of the glyph alone, no frame."""
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d">%s</svg>'
        % (gen.VIEW, gen.VIEW, gen.glyph_paths(paths))
    )
    png = cairosvg.svg2png(
        bytestring=svg.encode(), output_width=size, output_height=size
    )
    import io

    a = np.asarray(Image.open(io.BytesIO(png)).convert("RGBA"), dtype=float)
    return a[..., 3] / 255.0


def source_png(name: str) -> pathlib.Path:
    """The art the glyph is measured against -- 40px where the wiki has
    it, the 32px render service otherwise."""
    if name in gen.HAS_40PX:
        return gen.wiki_40px(name, CACHE)
    return gen.fetch(gen.RENDER + gen.OFFICIAL[name][1] + ".png",
                     CACHE / f"{name}-32.png")


def reference(name: str):
    """(coverage at BIG, coverage at native, native size).

    Same near-white threshold the tracer uses, but applied at the source
    resolution: this is the target, so it must not be smoothed first.
    """
    img = Image.open(source_png(name)).convert("RGBA")
    n = img.size[0]
    a = np.asarray(img, dtype=float) / 255.0
    # The union of every layer: the score asks whether the SVG covers the
    # glyph, and a two-ink glyph is still one glyph.
    m = np.zeros(a.shape[:2], dtype=bool)
    for spec in gen.layer_specs(name):
        m |= gen.select(a, spec)
    m = m & gen._inside_frame(m.shape)
    native = m.astype(float)
    big = np.asarray(
        Image.fromarray((native * 255).astype("uint8"), "L").resize(
            (BIG, BIG), Image.LANCZOS
        ),
        dtype=float,
    ) / 255.0
    return np.clip(big, 0, 1), native, n


# --- scoring ----------------------------------------------------------

def soft_iou(a: np.ndarray, b: np.ndarray) -> float:
    """IoU generalised to coverage maps. Smooth in the parameters, which
    a hard boolean IoU is not -- Nelder-Mead needs the gradient signal."""
    union = np.maximum(a, b).sum()
    return float(np.minimum(a, b).sum() / union) if union else 0.0


def _edges(a: np.ndarray) -> np.ndarray:
    gy, gx = np.gradient(a)
    return np.hypot(gx, gy)


def score(paths, ref, name: str = "") -> float:
    big_ref, native_ref, n = ref
    cand_big = render(paths, BIG)
    cand_native = render(paths, n)
    s = (
        W_BIG * soft_iou(cand_big, big_ref)
        + W_NATIVE * soft_iou(cand_native, native_ref)
        + W_EDGE * soft_iou(_edges(cand_big), _edges(big_ref))
    )
    # Ink outside the frame interior is the shield-clipping-the-bevel
    # bug. Make it impossible rather than something to spot in a render.
    inside = gen._inside_frame(cand_big.shape)
    ink = cand_big.sum()
    if ink:
        s -= P_OUTSIDE * float(cand_big[~inside].sum() / ink)
    return s


# --- bounds and relations ---------------------------------------------
#
# Left free, the optimiser deletes whatever the reference does not
# strongly reward: it drove the sword tips, one clock hand and the motion
# dashes to negative width, which renders them as nothing and costs no
# overlap. These say what a *plausible* glyph looks like, so the fit
# chooses among real glyphs rather than degenerate ones.

BOUNDS = {
    "alacrity": {
        "r": (8.0, 11.4), "ecc": (0.90, 1.10), "tilt": (-35.0, 35.0),
        "hr": (5.0, 9.0), "hdx": (-3.0, 3.0), "hdy": (-3.0, 3.0),
        "dr": (10.5, 13.4),
        "h1l": (3.5, 8.5), "h1w": (0.38, 0.85),
        "h2l": (3.0, 8.0), "h2w": (0.34, 0.80),
        "dlen": (2.5, 5.0), "dt": (0.22, 0.55),
        "dspread": (14.0, 40.0),
    },
    "resolution": {
        "shw": (8.0, 13.2), "sty": (8.6, 13.0), "dhw": (5.0, 12.4),
        "dhh": (6.0, 14.0), "sw": (0.75, 1.70), "sgh": (1.50, 3.30),
        "stip": (1.30, 3.40), "hspread": (2.0, 6.0), "tipx": (2.0, 6.5),
    },
}

# (a, b, gap): a must stay at least `gap` below b.
RELATIONS = {
    "alacrity": [
        ("hr", "r", 1.6),     # the ring must stay a ring, not a disc
        ("r", "dr", 0.4),     # dashes fly outside the face, not across it
    ],
    "resolution": [
        ("dhw", "shw", 0.9),  # the diamond leaves a rim of shield
        ("sw", "sgh", 0.5),   # the crossguard outreaches the blade
    ],
}


def sanitize(name, p):
    """Clamp into the plausible region and repair ordering. Returns the
    repaired dict and how far it had to move, which the objective charges
    for so Nelder-Mead is steered back rather than walled off."""
    q, moved = dict(p), 0.0
    for k, (lo, hi) in BOUNDS.get(name, {}).items():
        v = min(max(q[k], lo), hi)
        moved += abs(v - q[k])
        q[k] = v
    if name == "alacrity":
        # The bright pixels in the source are a crescent -- the right of
        # the clock rim is too dim to threshold -- so an unconstrained
        # fit slides the hole off-centre until the band pinches out and
        # the glyph reads as a moon. Cap the offset at a fraction of the
        # band width so the ring always closes.
        import math as _m

        band = q["r"] - q["hr"]
        off = _m.hypot(q["hdx"], q["hdy"])
        cap = 0.40 * band
        if off > cap:
            moved += off - cap
            q["hdx"] *= cap / off
            q["hdy"] *= cap / off
    for a, b, gap in RELATIONS.get(name, []):
        if q[b] - q[a] < gap:
            mid = (q[a] + q[b]) / 2
            moved += gap - (q[b] - q[a])
            q[a], q[b] = mid - gap / 2, mid + gap / 2
    return q, moved


# --- fitting ----------------------------------------------------------

def fit(name: str, restarts: int = 3, maxiter: int = 1200, seed: int = 0):
    build = gen.BUILDERS[name]
    p0 = gen.GLYPH_PARAMS[name]
    keys = list(p0)
    x0 = np.array([p0[k] for k in keys], dtype=float)
    ref = reference(name)
    # Angles vary over degrees, lengths over view units; give Nelder-Mead
    # a step proportional to each parameter's own magnitude.
    step = np.maximum(np.abs(x0) * 0.08, 0.25)

    def objective(x):
        q, moved = sanitize(name, dict(zip(keys, x)))
        return -score(build(q), ref, name) + 0.05 * moved

    best_x, best = x0, -objective(x0)
    rng = np.random.default_rng(seed)
    for i in range(restarts):
        start = best_x if i == 0 else best_x + rng.normal(0, step)
        simplex = np.vstack([start] + [start + np.eye(len(x0))[j] * step[j]
                                       for j in range(len(x0))])
        r = minimize(objective, start, method="Nelder-Mead",
                     options={"initial_simplex": simplex, "maxiter": maxiter,
                              "xatol": 1e-3, "fatol": 1e-5})
        if -r.fun > best:
            best, best_x = -r.fun, r.x
        print(f"  restart {i}: {-r.fun:.4f} (best {best:.4f})")
    q, _ = sanitize(name, dict(zip(keys, map(float, best_x))))
    return {k: round(v, 2) for k, v in q.items()}, best


def write_params(name: str, params: dict) -> None:
    """Rewrite one entry of GLYPH_PARAMS in place, preserving the rest of
    the generator byte for byte."""
    path = pathlib.Path(__file__).resolve().parent / "gen_icon_svgs.py"
    s = path.read_text()
    body = "".join(
        '        "%s": %.2f,\n' % (k, v) for k, v in params.items()
    )
    pat = re.compile(r'(?s)(    "%s": \{\n).*?(    \},\n)' % re.escape(name))
    s, n = pat.subn(lambda m: m.group(1) + body + m.group(2), s, count=1)
    if n != 1:
        raise SystemExit(f"could not locate GLYPH_PARAMS[{name!r}]")
    path.write_text(s)


# --- reporting --------------------------------------------------------

def glyph_for(name: str):
    if name in gen.HAND_DRAWN:
        return gen.HAND_DRAWN[name]
    return gen.trace_glyph(source_png(name), name)


def report() -> None:
    print(f"{'icon':<13} {'score':>7} {'IoU@160':>8} {'IoU@src':>8}")
    for name in sorted(gen.OFFICIAL):
        ref = reference(name)
        d = glyph_for(name)
        print("%-13s %7.4f %8.4f %8.4f" % (
            name, score(d, ref, name),
            soft_iou(render(d, BIG), ref[0]),
            soft_iou(render(d, ref[2]), ref[1]),
        ))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fit", nargs="*", metavar="NAME")
    ap.add_argument("--report", action="store_true")
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--restarts", type=int, default=3)
    ap.add_argument("--family", default="boons", choices=sorted(gen.FAMILIES))
    args = ap.parse_args()
    gen.use_family(args.family)
    global CACHE
    CACHE = pathlib.Path(f"/tmp/axilog-{args.family}-src")

    if args.fit is not None:
        for name in args.fit or sorted(gen.BUILDERS):
            before = score(gen.BUILDERS[name](gen.GLYPH_PARAMS[name]),
                           reference(name), name)
            print(f"{name}: start {before:.4f}")
            params, best = fit(name, restarts=args.restarts)
            print(f"{name}: {before:.4f} -> {best:.4f}")
            for k, v in params.items():
                print(f"    {k:<10} {gen.GLYPH_PARAMS[name][k]:>7.2f}"
                      f" -> {v:>7.2f}")
            if args.write:
                write_params(name, params)
                print(f"  written to gen_boon_svgs.py")
    if args.report:
        report()
    return 0


if __name__ == "__main__":
    sys.exit(main())
