#!/usr/bin/env python3
"""Pixel diff + mask utilities for the openmix design-diff harness.

Invoked by run.mjs as `python3 -I diff.py ...`. The `-I` flag strips user
site-packages from sys.path, so this preamble re-adds it before importing
Pillow / numpy (both live in the user site on this machine).
"""
import sys
import os

try:
    from PIL import Image
    import numpy as np
except ImportError:
    _cands = []
    try:
        import site as _site

        _cands.append(_site.getusersitepackages())
    except Exception:
        pass
    import glob as _glob

    _cands += _glob.glob(os.path.expanduser("~/.local/lib/python*/site-packages"))
    for _p in _cands:
        if _p and os.path.isdir(_p) and _p not in sys.path:
            sys.path.append(_p)
    from PIL import Image
    import numpy as np

import argparse
import json


def load_rgb(path):
    return np.asarray(Image.open(path).convert("RGB")).astype(np.int16)


def load_mask(path, shape):
    """shape = (H, W). Returns bool array True = ignore."""
    h, w = shape
    m = np.zeros((h, w), dtype=bool)
    if not path:
        return m
    with open(path) as f:
        data = json.load(f)
    for r in data.get("rects", []):
        x, y, rw, rh = (int(v) for v in r)
        x0, y0 = max(0, x), max(0, y)
        x1, y1 = min(w, x + rw), min(h, y + rh)
        if x1 > x0 and y1 > y0:
            m[y0:y1, x0:x1] = True
    return m


def heat_color(v):
    """v in [0,1] -> (r,g,b). 0 = dark, small = yellow, large = red."""
    v = max(0.0, min(1.0, float(v)))
    if v <= 0.0:
        return (24, 28, 36)
    if v < 0.5:
        t = v / 0.5
        return (int(24 + t * (250 - 24)), int(28 + t * (210 - 28)), int(36 + t * (40 - 36)))
    t = (v - 0.5) / 0.5
    return (250, int(210 - t * 190), int(40 - t * 30))


def cmd_diff(a):
    ref = load_rgb(a.ref)
    ours_img = Image.open(a.ours).convert("RGB")
    ours = np.asarray(ours_img).astype(np.int16)
    os.makedirs(a.out_dir, exist_ok=True)
    if ref.shape != ours.shape:
        out = {
            "mismatchPct": 100.0,
            "threshold": a.threshold,
            "delta": a.delta,
            "pass": False,
            "error": "sizeMismatch",
            "refSize": [int(ref.shape[1]), int(ref.shape[0])],
            "oursSize": [int(ours.shape[1]), int(ours.shape[0])],
        }
        print(json.dumps(out), flush=True)
        return 1
    h, w, _ = ref.shape
    mask = load_mask(a.mask, (h, w))
    d = np.abs(ref - ours).max(axis=2)
    bad = d > a.delta
    valid = ~mask
    n_valid = int(valid.sum())
    n_bad = int((bad & valid).sum())
    pct = (n_bad / n_valid * 100.0) if n_valid else 0.0
    masked_pct = float(mask.sum()) / mask.size * 100.0

    # 8x8 grid heatmap over unmasked pixels.
    cell = 8
    gh, gw = h // cell, w // cell
    heat = np.zeros((gh, gw), dtype=np.float64)
    for gy in range(gh):
        y0, y1 = gy * cell, (gy + 1) * cell
        bv = valid[y0:y1, :]
        bb = (bad & valid)[y0:y1, :]
        for gx in range(gw):
            x0, x1 = gx * cell, (gx + 1) * cell
            denom = int(bv[:, x0:x1].sum())
            heat[gy, gx] = (int(bb[:, x0:x1].sum()) / denom) if denom else 0.0
    worst_cells = []
    if n_bad:
        flat = [ (float(heat[gy, gx]), gx, gy) for gy in range(gh) for gx in range(gw) if heat[gy, gx] > 0]
        flat.sort(reverse=True)
        worst_cells = [{"cell": [gx, gy], "pct": round(v * 100.0, 2)} for v, gx, gy in flat[:12]]

    # diff.png: ours with mismatching unmasked pixels painted red.
    vis = np.asarray(ours_img).copy()
    hit = bad & valid
    vis[hit] = (255, 30, 30)
    Image.fromarray(vis).save(os.path.join(a.out_dir, "diff.png"))

    # heat.png: 4x upscale of the grid with colormap.
    hs = 4
    heat_img = np.zeros((gh * hs, gw * hs, 3), dtype=np.uint8)
    for gy in range(gh):
        for gx in range(gw):
            heat_img[gy * hs:(gy + 1) * hs, gx * hs:(gx + 1) * hs] = heat_color(heat[gy, gx])
    Image.fromarray(heat_img).save(os.path.join(a.out_dir, "heat.png"))

    # compare.png: ref | ours | diff side by side.
    comp = np.zeros((h, w * 3, 3), dtype=np.uint8)
    comp[:, :w] = np.asarray(Image.open(a.ref).convert("RGB"))
    comp[:, w:2 * w] = np.asarray(ours_img)
    comp[:, 2 * w:] = vis
    Image.fromarray(comp).save(os.path.join(a.out_dir, "compare.png"))

    out = {
        "mismatchPct": round(pct, 4),
        "threshold": a.threshold,
        "delta": a.delta,
        "pass": bool(pct <= a.threshold),
        "badPixels": n_bad,
        "validPixels": n_valid,
        "maskedPct": round(masked_pct, 3),
        "worstCells": worst_cells,
    }
    print(json.dumps(out), flush=True)
    return 0 if out["pass"] else 1


def merge_rects(rects, gap=6):
    rects = [list(r) for r in rects]
    changed = True
    while changed:
        changed = False
        out, used = [], [False] * len(rects)
        for i in range(len(rects)):
            if used[i]:
                continue
            x, y, w, h = rects[i]
            for j in range(i + 1, len(rects)):
                if used[j]:
                    continue
                x2, y2, w2, h2 = rects[j]
                if x - gap <= x2 + w2 and x2 - gap <= x + w and y - gap <= y2 + h2 and y2 - gap <= y + h:
                    x = min(x, x2)
                    y = min(y, y2)
                    w = max(x + w, x2 + w2) - x
                    h = max(y + h, y2 + h2) - y
                    used[j] = True
                    changed = True
            out.append([x, y, w, h])
            used[i] = True
        rects = out
    return rects


def cmd_mask(a):
    shots = list(a.shots or [])
    if a.a and a.b:
        shots = [a.a, a.b]
    if len(shots) < 2:
        print(json.dumps({"error": "need at least 2 shots"}), flush=True)
        return 2
    imgs = [np.asarray(Image.open(p).convert("RGB")).astype(np.int16) for p in shots]
    h, w, _ = imgs[0].shape
    for im in imgs[1:]:
        if im.shape != imgs[0].shape:
            print(json.dumps({"error": "sizeMismatch"}), flush=True)
            return 2
    binary = np.zeros((h, w), dtype=bool)
    for prev, cur in zip(imgs, imgs[1:]):
        d = np.abs(prev - cur).max(axis=2)
        binary |= d > a.sensitivity
    base_img = shots[0]
    # Aggregate to 4px cells, dilate by 2 cells, connected components.
    cs = 4
    gh, gw = (h + cs - 1) // cs, (w + cs - 1) // cs
    grid = np.zeros((gh, gw), dtype=bool)
    for gy in range(gh):
        for gx in range(gw):
            grid[gy, gx] = bool(binary[gy * cs:min(h, (gy + 1) * cs), gx * cs:min(w, (gx + 1) * cs)].any())
    dil = np.zeros_like(grid)
    R = 2
    for dy in range(-R, R + 1):
        for dx in range(-R, R + 1):
            dil |= np.roll(np.roll(grid, dy, axis=0), dx, axis=1)
    # Undo wraparound pollution from roll on the borders.
    dil[:R, :] |= grid[:R, :]
    dil[-R:, :] |= grid[-R:, :]
    dil[:, :R] |= grid[:, :R]
    dil[:, -R:] |= grid[:, -R:]
    seen = np.zeros_like(dil)
    rects = []
    for gy in range(gh):
        for gx in range(gw):
            if dil[gy, gx] and not seen[gy, gx]:
                stack = [(gy, gx)]
                seen[gy, gx] = True
                x0 = x1 = gx
                y0 = y1 = gy
                while stack:
                    cy, cx = stack.pop()
                    x0, x1 = min(x0, cx), max(x1, cx)
                    y0, y1 = min(y0, cy), max(y1, cy)
                    for ny in (cy - 1, cy, cy + 1):
                        for nx in (cx - 1, cx, cx + 1):
                            if 0 <= ny < gh and 0 <= nx < gw and dil[ny, nx] and not seen[ny, nx]:
                                seen[ny, nx] = True
                                stack.append((ny, nx))
                px0 = max(0, x0 * cs - 3)
                py0 = max(0, y0 * cs - 3)
                px1 = min(w, (x1 + 1) * cs + 3)
                py1 = min(h, (y1 + 1) * cs + 3)
                if px1 - px0 >= 4 and py1 - py0 >= 4:
                    rects.append([px0, py0, px1 - px0, py1 - py0])
    rects = merge_rects(rects)
    total = sum(r[2] * r[3] for r in rects)
    mask = {
        "version": 1,
        "generatedFrom": {"shots": [os.path.basename(s) for s in shots], "sensitivity": a.sensitivity},
        "rects": rects,
        "coveragePct": round(total / (w * h) * 100.0, 3),
        "notes": "Animated regions (meters, level bars, GR, input level) differ between live captures taken at different times. Review before use.",
    }
    with open(a.out, "w") as f:
        json.dump(mask, f, indent=2)
    if a.preview:
        base = np.asarray(Image.open(base_img).convert("RGB")).copy()
        # Dim everything outside rects, outline rects in cyan.
        dim = (base * 0.45).astype(np.uint8)
        for x, y, rw, rh in rects:
            dim[y:y + rh, x:x + rw] = base[y:y + rh, x:x + rw]
            dim[y:y + 2, x:x + rw] = (0, 220, 255)
            dim[max(0, y + rh - 2):y + rh, x:x + rw] = (0, 220, 255)
            dim[y:y + rh, x:x + 2] = (0, 220, 255)
            dim[y:y + rh, max(0, x + rw - 2):x + rw] = (0, 220, 255)
        Image.fromarray(dim).save(a.preview)
    print(json.dumps({"rects": len(rects), "coveragePct": mask["coveragePct"]}), flush=True)
    return 0


def cmd_shift(a):
    src = np.asarray(Image.open(a.src).convert("RGB"))
    h, w, _ = src.shape
    dx = a.dx
    out = np.zeros_like(src)
    if dx >= 0:
        out[:, dx:] = src[:, :w - dx]
    else:
        out[:, :w + dx] = src[:, -dx:]
    Image.fromarray(out).save(a.dst)
    print(json.dumps({"shifted": [dx, a.dy], "dst": a.dst}), flush=True)
    return 0


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("diff")
    d.add_argument("--ref", required=True)
    d.add_argument("--ours", required=True)
    d.add_argument("--mask", default="")
    d.add_argument("--out-dir", required=True)
    d.add_argument("--threshold", type=float, default=1.0)
    d.add_argument("--delta", type=int, default=24)
    m = sub.add_parser("mask")
    m.add_argument("--shots", nargs="+", default=[])
    m.add_argument("--a", default="")
    m.add_argument("--b", default="")
    m.add_argument("--out", required=True)
    m.add_argument("--preview", default="")
    m.add_argument("--sensitivity", type=int, default=8)
    s = sub.add_parser("shift")
    s.add_argument("--src", required=True)
    s.add_argument("--dst", required=True)
    s.add_argument("--dx", type=int, default=2)
    s.add_argument("--dy", type=int, default=0)
    a = p.parse_args()
    if a.cmd == "diff":
        return cmd_diff(a)
    if a.cmd == "mask":
        return cmd_mask(a)
    return cmd_shift(a)


if __name__ == "__main__":
    sys.exit(main())
