#!/usr/bin/env python3
"""Generate the Method Budget app icon (1024x1024 RGBA PNG), stdlib only.

Design: deep-indigo rounded-square tile; white film frame with sprocket holes
on both side rails; three ascending white bars (a budget chart) inside.
Antialiasing uses signed-distance coverage. Usage: make_icon.py OUT.png
"""
import math, struct, sys, zlib

N = 1024

def rbox(px, py, cx, cy, hx, hy, r):
    qx = abs(px - cx) - hx + r
    qy = abs(py - cy) - hy + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

def cov(d):
    return 0.0 if d >= 0.5 else 1.0 if d <= -0.5 else 0.5 - d

HOLES = [(sx * 262, y) for sx in (-1, 1) for y in (-170, -57, 57, 170)]
BARS = [(-110, 120), (0, 220), (110, 330)]  # (x center, height)
BASE = 205  # bar baseline (y offset from center)

def glyph(x, y):
    d = max(rbox(x, y, 0, 0, 310, 300, 70), -rbox(x, y, 0, 0, 212, 236, 28))
    for hx, hy in HOLES:
        d = max(d, -rbox(x, y, hx, hy, 22, 26, 8))
    for bx, h in BARS:
        d = min(d, rbox(x, y, bx, BASE - h / 2, 38, h / 2, 10))
    return d

def render():
    rows = []
    top, bot = (58, 52, 140), (34, 30, 92)  # indigo vertical gradient
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
            g = cov(glyph(x, y)) if abs(x) < 330 and abs(y) < 320 else 0.0
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
