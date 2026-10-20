#!/usr/bin/env python3
"""Fast screenshot classifier for launch measurement (no third-party deps).

Downscales via sips to 320px wide, converts to BMP, counts saturated green
(CH buttons/meters — mixer signature) and overall brightness.

Usage: classify.py <png> [<png> ...]  -> one JSON object per line.
"""
import json
import os
import struct
import subprocess
import sys
import tempfile


def png_to_stats(png_path, width=320):
    with tempfile.NamedTemporaryFile(suffix=".bmp", delete=False) as f:
        bmp_path = f.name
    small = png_path + ".small.png"
    try:
        r = subprocess.run(
            ["sips", "-Z", str(width), png_path, "--out", small],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            return {"file": png_path, "error": r.stderr.strip()[-200:]}
        r = subprocess.run(
            ["sips", "-s", "format", "bmp", small, "--out", bmp_path],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            return {"file": png_path, "error": r.stderr.strip()[-200:]}
        with open(bmp_path, "rb") as f:
            data = f.read()
        if data[:2] != b"BM":
            return {"file": png_path, "error": "not a BMP"}
        off = struct.unpack("<I", data[10:14])[0]
        w = struct.unpack("<i", data[18:22])[0]
        h = struct.unpack("<i", data[22:26])[0]
        bpp = struct.unpack("<H", data[28:30])[0]
        if w <= 0 or h == 0 or bpp not in (24, 32):
            return {"file": png_path, "error": f"unexpected BMP w={w} h={h} bpp={bpp}"}
        h = abs(h)
        ch = bpp // 8
        stride = ((w * ch + 3) // 4) * 4
        px = data[off:]
        n = green = bright = 0
        for y in range(0, h, 2):
            row = y * stride
            for x in range(0, w, 2):
                i = row + x * ch
                if i + 2 >= len(px):
                    break
                b, g, r_ = px[i], px[i + 1], px[i + 2]
                n += 1
                if g > 150 and r_ < 110 and b < 110:
                    green += 1
                if (r_ + g + b) // 3 > 60:
                    bright += 1
        gf, bf = green / n, bright / n
        if bf > 0.95:
            screen = "white"  # sim boot logo / springboard loading
        elif bf < 0.02:
            screen = "black"  # launch storyboard / empty webview
        elif gf > 0.01:
            screen = "mixer"
        else:
            screen = "other"  # home screen etc.
        return {"file": png_path, "screen": screen,
                "green_frac": round(gf, 4), "bright_frac": round(bf, 4)}
    finally:
        for p in (bmp_path, small):
            try:
                os.unlink(p)
            except OSError:
                pass


if __name__ == "__main__":
    for p in sys.argv[1:]:
        print(json.dumps(png_to_stats(p)))
