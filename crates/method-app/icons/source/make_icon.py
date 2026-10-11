#!/usr/bin/env python3
"""Generate the Method Budget app icon (1024x1024 RGBA PNG), stdlib only.

Design: the brand mark (docs/BRAND.md) in white on a teal rounded-square tile. A budget
page with its corner turned back, a column of right-aligned figures and the double rule
under the total. The figures are cut out of the page, so the tile's teal shows through
them; the turned corner is the page at half strength. The geometry is the 64-unit mark
in scripts/make-brand.py; change both together.

Antialiasing uses signed-distance coverage. Usage: make_icon.py OUT.png

Then rebuild the bundle's icon set from it, from crates/method-app (the CLI writes more
sizes than the app bundles; keep the files listed in docs/BRAND.md):

    cargo tauri icon icons/source/icon-1024.png -o <scratch dir>
"""
import math, struct, sys, zlib

N = 1024
K = 11.0  # pixels per mark unit: the 52-unit page is 572 px tall
SQ2 = math.sqrt(2)

def rbox(px, py, cx, cy, hx, hy, r):
    qx = abs(px - cx) - hx + r
    qy = abs(py - cy) - hy + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

def cov(d):
    return 0.0 if d >= 0.5 else 1.0 if d <= -0.5 else 0.5 - d

def rrect(u, v, x, y, w, h, r):
    return rbox(u, v, x + w / 2, y + h / 2, w / 2, h / 2, r)

# Cut-outs in mark units (x, y, w, h, radius): two figures, then the double rule.
HOLES = [(25, 27, 19, 5, 1.5), (31, 36, 13, 5, 1.5),
         (20, 45.5, 24, 2.75, 1.2), (20, 50.5, 24, 2.75, 1.2)]

def page(u, v):
    """Distance (mark units) to the page: the sheet less its turned corner and the
    cut-outs."""
    d = rbox(u, v, 32, 32, 20, 26, 5)
    d = max(d, -rbox(u, v, 88, -30, 50, 50, 2))  # the notch the corner leaves
    for x, y, w, h, r in HOLES:
        d = max(d, -rrect(u, v, x, y, w, h, r))
    return d

def flap(u, v):
    """Distance to the turned corner: a right triangle, 2 units clear of the page."""
    d = rbox(u, v, 90, -32, 50, 50, 2)  # right angle at (40, 18), rounded
    return max(d, (u - v - 31.6) / SQ2)

def render():
    rows = []
    top, bot = (20, 135, 124), (10, 90, 81)  # teal vertical gradient
    for j in range(N):
        y = j + 0.5 - N / 2
        t = j / (N - 1)
        bg = [top[k] + (bot[k] - top[k]) * t for k in range(3)]
        row = bytearray(b"\x00")
        for i in range(N):
            x = i + 0.5 - N / 2
            a = cov(rbox(x, y, 0, 0, 432, 432, 190))
            if a == 0:
                row += b"\x00\x00\x00\x00"; continue
            u, v = 32 + x / K, 32 + y / K
            g = 0.0
            if 10 < u < 54 and 4 < v < 60:
                g = max(cov(page(u, v) * K), 0.5 * cov(flap(u, v) * K))
            c = [bg[k] + (255 - bg[k]) * g for k in range(3)]
            row += bytes([round(c[0]), round(c[1]), round(c[2]), round(255 * a)])
        rows.append(bytes(row))
    return b"".join(rows)

def png(data):
    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(data, 9)) + chunk(b"IEND", b""))

if __name__ == "__main__":
    with open(sys.argv[1] if len(sys.argv) > 1 else "icon-1024.png", "wb") as f:
        f.write(png(render()))
