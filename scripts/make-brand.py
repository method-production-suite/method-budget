#!/usr/bin/env python3
"""Draw Method Budget's mark and wordmark as SVG files. Standard library only.

    python3 scripts/make-brand.py          write the files below
    python3 scripts/make-brand.py --check  change nothing; exit 1 if a file differs

Writes, under site/:

    favicon.svg                    the small mark on a 16-unit grid, light and dark
    favicon.ico                    the same at 16 and 32 pixels, for browsers that want one
    apple-touch-icon.png           180 pixels, the white mark on teal, for phone home screens
    assets/brand/mark.svg          the mark, for light backgrounds
    assets/brand/mark-dark.svg     the mark, for dark backgrounds
    assets/brand/wordmark.svg      mark and name, for light backgrounds
    assets/brand/wordmark-dark.svg mark and name, for dark backgrounds

The mark is a budget page with its corner turned back: a column of right-aligned
figures and the accountant's double rule under the total. The figures and the gap
around the turned corner are cut out of the page, so the mark sits on any background.
The name is drawn from strokes on a 100-unit x-height (lowercase, geometric, one
weight), not set in a font, so it looks the same everywhere and ships no font file.

The app icon is drawn from the same geometry by crates/method-app/icons/source/make_icon.py;
change both together. Colors are the brand tokens in site/assets/tokens.css; rules for
using all of this are in docs/BRAND.md.
"""

from __future__ import annotations

import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "site" / "assets" / "brand"

# Brand colors (site/assets/tokens.css): the app's teal, and the ink of each scheme.
TEAL = "#0E7468"
TEAL_DARK = "#2CC7B2"
INK = "#1C2228"
INK_DARK = "#E4E8EC"

def fmt(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


# ----------------------------------------------------------------------------- mark
# 64-unit grid, y down. The page runs x 12..52, y 6..58; the corner is turned back 14
# units. Figures align right at x 44, as numbers do in a budget column.

PAGE = "M17 6H38V18a2 2 0 0 0 2 2H52V53a5 5 0 0 1-5 5H17a5 5 0 0 1-5-5V11a5 5 0 0 1 5-5Z"
FLAP = "M38 6L52 20H41a3 3 0 0 1-3-3Z"


def rrect(x: float, y: float, w: float, h: float, r: float) -> str:
    """A rounded rectangle as a closed subpath. The page uses fill-rule evenodd, so
    any subpath inside it is a hole whichever way it winds."""
    return (
        f"M{fmt(x + r)} {fmt(y)}H{fmt(x + w - r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)} {fmt(r)}"
        f"V{fmt(y + h - r)}a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)} {fmt(r)}H{fmt(x + r)}"
        f"a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)}-{fmt(r)}V{fmt(y + r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)}-{fmt(r)}Z"
    )


# The cut-outs: two figures, then the double rule under the total.
HOLES = [
    rrect(25, 27, 19, 5, 1.5),
    rrect(31, 36, 13, 5, 1.5),
    rrect(20, 45.5, 24, 2.75, 1.2),
    rrect(20, 50.5, 24, 2.75, 1.2),
]
# The flap sits in the turned corner, 2 units clear of the page on both legs.
FLAP_INSET = "M40 8.4L49.6 18H42a2 2 0 0 1-2-2Z"


def mark_paths(color: str, cls: str = "") -> str:
    c = f' class="{cls}"' if cls else f' fill="{color}"'
    body = PAGE + "".join(HOLES)
    return (
        f'<path{c} fill-rule="evenodd" d="{body}"/>'
        f'<path{c} opacity=".5" d="{FLAP_INSET}"/>'
    )


# 16-unit favicon grid: the same idea with whole-unit rows, so it stays sharp at 16 and
# 32 pixels. One figure instead of two; the double rule keeps its gap.
FAV_PAGE = "M3 1H10V4.5a.5.5 0 0 0 .5.5H14V14a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1Z"
FAV_HOLES = ["M6 7h6v2H6Z", "M4 10.5h8v1H4Z", "M4 12.5h8v1H4Z"]
FAV_FLAP = "M11 1.6L13.4 4H11.5a.5.5 0 0 1-.5-.5Z"

# ----------------------------------------------------------------------------- name
# Strokes on a y-up grid: baseline 0, x-height 100, ascender 140, stroke 18. Round
# letters overshoot the baseline and x-height by 1.5 so they look as tall as the stems.

W = 18.0
R = 43.5  # bowl radius at the stroke's center
C = 50.0  # bowl center height
ASC = 140.0
TASC = 126.0  # t is shorter than b, d and h
BOWL = 2 * R + W  # 105: width of a bowl letter


def bowl(x0: float) -> list[str]:
    cx = x0 + W / 2 + R
    return [f"M{cx - R} {C}A{R} {R} 0 1 1 {cx + R} {C}A{R} {R} 0 1 1 {cx - R} {C}Z"]


def g_m(x0):
    r = 34.0
    c = 100 - W / 2 - r + 1.5  # arch tops overshoot like the bowls
    s1, s2, s3 = x0 + W / 2, x0 + W / 2 + 2 * r, x0 + W / 2 + 4 * r
    return [
        f"M{s1} 0V100",
        f"M{s1} {c}A{r} {r} 0 0 1 {s2} {c}V0",
        f"M{s2} {c}A{r} {r} 0 0 1 {s3} {c}V0",
    ], 4 * r + W


def g_e(x0):
    cx = x0 + W / 2 + R
    a = math.radians(-38)
    ex, ey = cx + R * math.cos(a), C + R * math.sin(a)
    return [f"M{cx - R} {C}H{cx + R}A{R} {R} 0 1 0 {ex:.2f} {ey:.2f}"], BOWL


def g_t(x0):
    stem = x0 + 22
    return [f"M{stem} {TASC}V0", f"M{x0} {100 - W / 2}H{x0 + 54}"], 54.0


def g_h(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 0V{ASC}", f"M{l} {C}A{R} {R} 0 0 1 {r} {C}V0"], BOWL


def g_o(x0):
    return bowl(x0), BOWL


def g_d(x0):
    return bowl(x0) + [f"M{x0 + W / 2 + 2 * R} 0V{ASC}"], BOWL


def g_b(x0):
    return bowl(x0) + [f"M{x0 + W / 2} 0V{ASC}"], BOWL


def g_u(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 100V{C}A{R} {R} 0 0 0 {r} {C}", f"M{r} 100V0"], BOWL


def g_g(x0):
    cx = x0 + W / 2 + R
    r = cx + R
    top = -12.0  # where the stem turns into the hook
    a = math.radians(-158)
    ex, ey = cx + R * math.cos(a), top + R * math.sin(a)
    return bowl(x0) + [f"M{r} 100V{top}A{R} {R} 0 0 1 {ex:.2f} {ey:.2f}"], BOWL


GLYPHS = {"m": g_m, "e": g_e, "t": g_t, "h": g_h, "o": g_o, "d": g_d, "b": g_b, "u": g_u, "g": g_g}
# Space between neighbours, outer edge to outer edge: tighter beside a bowl.
GAP = {"me": 13, "et": 9, "th": 14, "ho": 13, "od": 12, "bu": 13, "ud": 13, "dg": 12, "ge": 11}
WORD_SPACE = 52.0


def set_name(text: str) -> tuple[list[str], float]:
    """Paths (y up) and the advance width for a lowercase phrase."""
    x, paths, prev = 0.0, [], ""
    for ch in text:
        if ch == " ":
            x += WORD_SPACE
            prev = ""
            continue
        if prev:
            x += GAP.get(prev + ch, 13)
        p, w = GLYPHS[ch](x)
        paths += p
        x += w
        prev = ch
    return paths, x


def flip(path: str, base: float) -> str:
    """Turn a y-up path into SVG's y-down space with the baseline at `base`. Arc sweep
    flags are written as they look on screen (1 turns clockwise), so they stay."""
    out, tokens = [], tokenize(path)
    i = 0
    cmd = ""
    while i < len(tokens):
        t = tokens[i]
        if t.isalpha():
            cmd = t
            out.append(t)
            i += 1
            continue
        if cmd in "ML":
            x, y = float(tokens[i]), float(tokens[i + 1])
            out.append(f"{fmt(x)} {fmt(base - y)}")
            i += 2
        elif cmd == "H":
            out.append(fmt(float(t)))
            i += 1
        elif cmd == "V":
            out.append(fmt(base - float(t)))
            i += 1
        elif cmd == "A":
            rx, ry, rot, large, sweep, x, y = tokens[i : i + 7]
            out.append(
                f"{fmt(float(rx))} {fmt(float(ry))} {rot} {large} {sweep} "
                f"{fmt(float(x))} {fmt(base - float(y))}"
            )
            i += 7
        else:
            raise ValueError(f"unsupported command {cmd!r}")
    return " ".join(out).replace(" Z", "Z")


def tokenize(path: str) -> list[str]:
    toks, num = [], ""
    for ch in path:
        if ch.isalpha():
            if num:
                toks.append(num)
                num = ""
            toks.append(ch)
        elif ch in " ,":
            if num:
                toks.append(num)
                num = ""
        elif ch == "-" and num and num[-1] not in "eE":
            toks.append(num)
            num = ch
        else:
            num += ch
    if num:
        toks.append(num)
    return toks


# ----------------------------------------------------------------------------- pixels
# The two raster files browsers still ask for, drawn here with 8x8 supersampling so no
# image tool is needed: favicon.ico (16 and 32 px, from the 16-unit favicon geometry)
# and apple-touch-icon.png (180 px, the white mark on a full-bleed teal square; iOS
# rounds the corners itself).


def in_rrect(u, v, x, y, w, h, r):
    if not (x <= u <= x + w and y <= v <= y + h):
        return False
    cx = min(max(u, x + r), x + w - r)
    cy = min(max(v, y + r), y + h - r)
    return (u - cx) ** 2 + (v - cy) ** 2 <= r * r


def in_notch(u, v, cx, cy, r):
    """The square the turned corner leaves, right of x=cx-r and above y=cy+r, with
    its inner corner rounded about (cx, cy)."""
    if not (u > cx - r and v < cy + r):
        return False
    if u < cx and v > cy:
        return (u - cx) ** 2 + (v - cy) ** 2 <= r * r
    return True


def fav_cover(u, v):
    """(page, flap) at a point of the 16-unit favicon."""
    page = (in_rrect(u, v, 2, 1, 12, 14, 1) and not in_notch(u, v, 10.5, 4.5, 0.5)
            and not (6 <= u <= 12 and 7 <= v <= 9)
            and not (4 <= u <= 12 and (10.5 <= v <= 11.5 or 12.5 <= v <= 13.5)))
    flap = u >= 11 and v <= 4 and u - v <= 9.4
    return page, flap


MARK_HOLES = [(25, 27, 19, 5, 1.5), (31, 36, 13, 5, 1.5),
              (20, 45.5, 24, 2.75, 1.2), (20, 50.5, 24, 2.75, 1.2)]


def mark_cover(u, v):
    """(page, flap) at a point of the 64-unit mark."""
    page = (in_rrect(u, v, 12, 6, 40, 52, 5) and not in_notch(u, v, 40, 18, 2)
            and not any(in_rrect(u, v, *h) for h in MARK_HOLES))
    flap = (u >= 40 and v <= 18 and u - v <= 31.6
            and not (u < 42 and v > 16 and (u - 42) ** 2 + (v - 16) ** 2 > 4))
    return page, flap


def hex_rgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def raster(size, units, cover, fg, bg=None, inset=0.0):
    """RGBA rows. `units` is the drawing's grid size, shown across size minus inset on
    each side. fg is the page color; the flap is fg at half strength. bg None means
    transparent."""
    ss = 8
    scale = units / (size - 2 * inset)
    fr, fgc, fb = hex_rgb(fg)
    out = bytearray()
    for j in range(size):
        out.append(0)
        for i in range(size):
            acc = 0.0
            for sy in range(ss):
                for sx in range(ss):
                    u = (i - inset + (sx + 0.5) / ss) * scale
                    v = (j - inset + (sy + 0.5) / ss) * scale
                    page, flap = cover(u, v)
                    acc += 1.0 if page else (0.5 if flap else 0.0)
            a = acc / (ss * ss)
            if bg is None:
                out += bytes((fr, fgc, fb, round(255 * a)))
            else:
                br, bgc, bb = hex_rgb(bg)
                out += bytes((round(br + (fr - br) * a), round(bgc + (fgc - bgc) * a),
                              round(bb + (fb - bb) * a), 255))
    return bytes(out)


def png(size, rows):
    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def ico(images):
    """An .ico holding PNG images: [(size, png bytes)]."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, data = b"", b""
    for size, blob in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset + len(data))
        data += blob
    return head + entries + data


def favicon_ico() -> bytes:
    return ico([(n, png(n, raster(n, 16, fav_cover, TEAL))) for n in (16, 32)])


def touch_icon() -> bytes:
    return png(180, raster(180, 64, mark_cover, "#FFFFFF", bg=TEAL, inset=14))


# ----------------------------------------------------------------------------- files

HEADER = "<!-- Method Budget {what}. Drawn by scripts/make-brand.py; edit that, not this file. -->"


def mark_svg(color: str, label: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img"'
        f' aria-label="{label}">\n{HEADER.format(what="mark")}\n'
        f"<title>{label}</title>\n{mark_paths(color)}\n</svg>\n"
    )


def wordmark_svg(mark: str, ink: str) -> str:
    """The mark, as tall as the ascenders, then the name, on one baseline. Drawn in the
    name's own units: baseline at y 150, ascenders reach y 10, the g's tail y 214."""
    paths, width = set_name("method budget")
    base = 150.0
    d = " ".join(flip(p, base) for p in paths)
    k = (base - 10) / 52  # the page (52 units tall) spans ascender to baseline
    tx, ty = -12 * k, 10 - 6 * k
    name_x = 40 * k + 40  # page width, then a gap
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(name_x + width)} 224"'
        ' role="img" aria-label="Method Budget">\n'
        f"{HEADER.format(what='wordmark')}\n<title>Method Budget</title>\n"
        f'<g transform="translate({fmt(tx)} {fmt(ty)}) scale({k:.4f})">{mark_paths(mark)}</g>\n'
        f'<path transform="translate({fmt(name_x)} 0)" fill="none" stroke="{ink}"'
        f' stroke-width="{fmt(W)}" d="{d}"/>\n</svg>\n'
    )


def favicon_svg() -> str:
    body = FAV_PAGE + "".join(FAV_HOLES)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">\n'
        f"{HEADER.format(what='favicon')}\n"
        f"<style>path{{fill:{TEAL}}}@media (prefers-color-scheme:dark){{path{{fill:{TEAL_DARK}}}}}</style>\n"
        f'<path fill-rule="evenodd" d="{body}"/><path opacity=".5" d="{FAV_FLAP}"/>\n</svg>\n'
    )


def outputs() -> dict[Path, str | bytes]:
    return {
        ROOT / "site" / "favicon.ico": favicon_ico(),
        ROOT / "site" / "apple-touch-icon.png": touch_icon(),
        ROOT / "site" / "favicon.svg": favicon_svg(),
        BRAND / "mark.svg": mark_svg(TEAL, "Method Budget"),
        BRAND / "mark-dark.svg": mark_svg(TEAL_DARK, "Method Budget"),
        BRAND / "wordmark.svg": wordmark_svg(TEAL, INK),
        BRAND / "wordmark-dark.svg": wordmark_svg(TEAL_DARK, INK_DARK),
    }


def canonical(data: bytes) -> bytes:
    """What a file means rather than its exact bytes: a PNG's pixels (zlib versions
    compress differently, so CI on Linux and a Mac never write the same bytes), each
    image of an .ico likewise, anything else as is."""
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        pos, idat, head = 8, b"", b""
        while pos < len(data):
            n, kind = struct.unpack(">I4s", data[pos : pos + 8])
            body = data[pos + 8 : pos + 8 + n]
            if kind == b"IHDR":
                head = body
            elif kind == b"IDAT":
                idat += body
            pos += 12 + n
        return head + zlib.decompress(idat)
    if data[:4] == b"\x00\x00\x01\x00":
        count = struct.unpack("<H", data[4:6])[0]
        out = b""
        for k in range(count):
            size, off = struct.unpack("<II", data[6 + 16 * k + 8 : 6 + 16 * k + 16])
            out += canonical(data[off : off + size])
        return out
    return data


def main(argv: list[str]) -> int:
    check = "--check" in argv
    stale = []
    for path, content in outputs().items():
        data = content.encode("utf-8") if isinstance(content, str) else content
        old = path.read_bytes() if path.exists() else None
        if old is None or canonical(old) != canonical(data):
            stale.append(path)
            if not check:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
                print(f"wrote {path.relative_to(ROOT)}")
    if check and stale:
        for p in stale:
            sys.stderr.write(f"make-brand: {p.relative_to(ROOT)} is out of date\n")
        return 1
    if not stale:
        print("make-brand: nothing to change")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
