#!/usr/bin/env python3
"""Makes the spreadsheet-import test fixtures (crates/fwb-format/tests/fixtures).

Python standard library only (zlib, zipfile): no openpyxl. Run from anywhere:

    python3 -I make_fixtures.py <out-dir>

inflate/: raw DEFLATE streams made by zlib at several levels and strategies,
and manifest.txt ("name length crc32") for checking what they decode to.
xlsx/: small workbooks written by hand (zipfile + XML), each testing one
thing the reader must handle or refuse.

Everything is deterministic (seeded random), so running it again gives the
same inputs; zlib's output can differ between zlib versions, which is fine:
the manifest checks the decoded bytes, not the compressed ones.
"""

import io
import os
import random
import sys
import zipfile
import zlib


def words_text(n_words, seed):
    rng = random.Random(seed)
    words = ("budget line account fringe rate day week crew camera grip "
             "electric location travel per diem overtime set unit total "
             "producer director cast extras wardrobe makeup editorial sound").split()
    out = []
    for i in range(n_words):
        out.append(rng.choice(words))
        if i % 13 == 12:
            out.append(".\n")
    return " ".join(out).encode()


def sheet_xml(rows, seed):
    rng = random.Random(seed)
    parts = ['<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n',
             '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>']
    for r in range(1, rows + 1):
        parts.append(f'<row r="{r}">')
        parts.append(f'<c r="A{r}" t="s"><v>{rng.randrange(40)}</v></c>')
        parts.append(f'<c r="B{r}"><v>{rng.randrange(1, 60)}</v></c>')
        parts.append(f'<c r="C{r}" s="2"><v>{rng.randrange(10000, 900000) / 100:.2f}</v></c>')
        parts.append('</row>')
    parts.append('</sheetData></worksheet>')
    return "".join(parts).encode()


def deflate(data, level, strategy=zlib.Z_DEFAULT_STRATEGY):
    c = zlib.compressobj(level, zlib.DEFLATED, -15, 9, strategy)
    return c.compress(data) + c.flush()


def make_inflate(out):
    os.makedirs(out, exist_ok=True)
    rnd = random.Random(1)
    inputs = {
        "empty": b"",
        "text": words_text(1000, 2),
        "xml": sheet_xml(400, 3),
        "random": bytes(rnd.randrange(256) for _ in range(3000)),
        "zeros": bytes(1 << 20),
        "mixed": words_text(300, 4) + bytes(rnd.randrange(256) for _ in range(1500))
        + bytes(5000) + sheet_xml(50, 5),
    }
    cases = []
    for name, data in inputs.items():
        for level in ((6, 9) if name == "xml" else (1, 6, 9)):
            cases.append((f"{name}-l{level}", data, level, zlib.Z_DEFAULT_STRATEGY))
    cases.append(("text-l0", inputs["text"], 0, zlib.Z_DEFAULT_STRATEGY))
    cases.append(("empty-l0", b"", 0, zlib.Z_DEFAULT_STRATEGY))
    for sname, strat in (("fixed", zlib.Z_FIXED), ("huffman", zlib.Z_HUFFMAN_ONLY),
                         ("rle", zlib.Z_RLE), ("filtered", zlib.Z_FILTERED)):
        cases.append((f"text-{sname}", inputs["text"], 6, strat))
        if sname == "filtered":
            cases.append((f"xml-{sname}", inputs["xml"], 6, strat))
        if sname != "huffman":
            cases.append((f"mixed-{sname}", inputs["mixed"], 6, strat))
    manifest = []
    for name, data, level, strat in cases:
        z = deflate(data, level, strat)
        assert zlib.decompress(z, -15) == data
        with open(os.path.join(out, name + ".deflate"), "wb") as f:
            f.write(z)
        manifest.append(f"{name} {len(data)} {zlib.crc32(data):08x}")
    with open(os.path.join(out, "manifest.txt"), "w") as f:
        f.write("# name, decoded length, CRC-32 of the decoded bytes (make_fixtures.py)\n")
        f.write("\n".join(manifest) + "\n")


NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
NS_R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
NS_PR = "http://schemas.openxmlformats.org/package/2006/relationships"
HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'

CONTENT_TYPES = HEAD + (
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
    '</Types>')

PACKAGE_RELS = HEAD + (
    f'<Relationships xmlns="{NS_PR}">'
    f'<Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/>'
    '</Relationships>')

WORKBOOK = HEAD + (
    f'<workbook xmlns="{NS}" xmlns:r="{NS_R}">'
    '<!-- sheets in order, one hidden -->'
    '<bookViews><workbookView/></bookViews>'
    '<sheets>'
    '<sheet name="Crew &amp; Cast" sheetId="1" r:id="rId1"/>'
    '<sheet name="Hidden" sheetId="2" state="hidden" r:id="rId2"/>'
    '<sheet name="Prefixed" sheetId="3" r:id="rId3"/>'
    '</sheets></workbook>')

WORKBOOK_RELS = HEAD + (
    f'<Relationships xmlns="{NS_PR}">'
    f'<Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/>'
    f'<Relationship Id="rId2" Type="{NS_R}/worksheet" Target="/xl/worksheets/sheet2.xml"/>'
    f'<Relationship Id="rId3" Type="{NS_R}/worksheet" Target="worksheets/../worksheets/sheet3.xml"/>'
    f'<Relationship Id="rId4" Type="{NS_R}/sharedStrings" Target="strings.xml"/>'
    f'<Relationship Id="rId5" Type="{NS_R}/styles" Target="styles.xml"/>'
    '</Relationships>')

SHARED = HEAD + (
    f'<sst xmlns="{NS}" count="7" uniqueCount="7">'
    '<si><t>Account</t></si>'
    '<si><t>Description</t></si>'
    '<si><r><rPr><b/></rPr><t>Rate</t></r><r><t xml:space="preserve"> (USD)</t></r></si>'
    '<si><t>Caf&#233; &amp; crew &lt;day&gt; &#x1F3AC;</t></si>'
    '<si><t>Line_x000D_break</t><rPh sb="0" eb="1"><t>ignored</t></rPh></si>'
    '<si><t/></si>'
    '<si><t><![CDATA[a<b]]></t></si>'
    '</sst>')

SHEET1 = HEAD + (
    f'<worksheet xmlns="{NS}"><dimension ref="A1:F5"/><sheetData>'
    '<row r="1" spans="1:5"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c>'
    '<c r="C1" t="s"><v>2</v></c><c r="D1" t="inlineStr"><is><t>Amount</t></is></c>'
    '<c r="E1" t="inlineStr"><is><r><t>To</t></r><r><t>tal</t></r></is></c></row>'
    '<row r="2"><c r="A2"><v>1100</v></c><c r="B2" t="s"><v>3</v></c><c r="C2" s="1"><v>1234.5</v></c>'
    '<c r="D2"><v>1.5E-3</v></c><c r="E2"><f>C2*D2</f><v>1.85175</v></c></row>'
    '<row r="4"><c r="A4" t="str"><f>"11"&amp;"-01"</f><v>11-01</v></c><c t="s"><v>4</v></c>'
    '<c><v>0.30000000000000004</v></c><c t="b"><v>1</v></c><c t="e"><v>#DIV/0!</v></c></row>'
    '<row><c r="A5" t="s"><v>6</v></c><c r="F5" s="1"/><c r="C5" t="s"><v>5</v></c></row>'
    '<row r="7"/>'
    '</sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>')

SHEET2 = HEAD + f'<worksheet xmlns="{NS}"><sheetData><row r="1"><c r="A1"><v>42</v></c></row></sheetData></worksheet>'

SHEET3 = HEAD + (
    '<x:worksheet xmlns:x="http://purl.oclc.org/ooxml/spreadsheetml/main">'
    '<x:sheetData><x:row r="1"><x:c r="B1" t="inlineStr"><x:is><x:t>Strict</x:t></x:is></x:c>'
    '<x:c r="AA1"><x:v>-7.25</x:v></x:c></x:row></x:sheetData></x:worksheet>')

STYLES = HEAD + f'<styleSheet xmlns="{NS}"/>'


def basic_parts():
    return [
        ("[Content_Types].xml", CONTENT_TYPES, zipfile.ZIP_DEFLATED),
        ("_rels/.rels", PACKAGE_RELS, zipfile.ZIP_DEFLATED),
        ("xl/workbook.xml", WORKBOOK, zipfile.ZIP_STORED),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS, zipfile.ZIP_DEFLATED),
        ("xl/strings.xml", SHARED, zipfile.ZIP_DEFLATED),
        ("xl/styles.xml", STYLES, zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet1.xml", SHEET1, zipfile.ZIP_DEFLATED),
        ("xl/worksheets/sheet2.xml", SHEET2, zipfile.ZIP_STORED),
        ("xl/worksheets/sheet3.xml", SHEET3, zipfile.ZIP_DEFLATED),
    ]


class NoSeek(io.RawIOBase):
    """A write-only stream that can't seek, so zipfile writes data descriptors."""

    def __init__(self):
        self.buf = bytearray()

    def writable(self):
        return True

    def write(self, b):
        self.buf += b
        return len(b)


def write_zip(path, parts, comment=b"", stream=False):
    raw = NoSeek() if stream else io.BytesIO()
    with zipfile.ZipFile(raw, "w") as zf:
        for name, text, method in parts:
            info = zipfile.ZipInfo(name, date_time=(2026, 10, 9, 12, 0, 0))
            info.compress_type = method
            zf.writestr(info, text.encode() if isinstance(text, str) else text)
        zf.comment = comment
    data = bytes(raw.buf) if stream else raw.getvalue()
    with open(path, "wb") as f:
        f.write(data)
    return data


def replace_part(parts, name, text):
    return [(n, text if n == name else t, m) for n, t, m in parts]


def make_xlsx(out):
    os.makedirs(out, exist_ok=True)
    parts = basic_parts()
    write_zip(os.path.join(out, "basic.xlsx"), parts)
    # Same package, written to a stream: every entry has a data descriptor
    # (sizes and CRC after the data, zeros in the local header).
    dd = write_zip(os.path.join(out, "descriptors.xlsx"), parts, stream=True)
    assert zipfile.ZipFile(io.BytesIO(dd)).testzip() is None
    # An archive comment after the end record.
    write_zip(os.path.join(out, "comment.xlsx"), parts, comment=b"Made by hand for a test.")
    # A DTD with an external entity: must be refused, never resolved.
    doctype = HEAD + (
        '<!DOCTYPE worksheet [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>'
        f'<worksheet xmlns="{NS}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>&xxe;</t></is></c></row></sheetData></worksheet>')
    write_zip(os.path.join(out, "doctype.xlsx"), replace_part(parts, "xl/worksheets/sheet1.xml", doctype))
    # An entity no DTD declares.
    entity = SHARED.replace("<si><t>Account</t></si>", "<si><t>&xxe;</t></si>")
    write_zip(os.path.join(out, "entity.xlsx"), replace_part(parts, "xl/strings.xml", entity))
    # Mismatched tags.
    broken = SHEET1.replace("</sheetData>", "</sheetDta>")
    write_zip(os.path.join(out, "badxml.xlsx"), replace_part(parts, "xl/worksheets/sheet1.xml", broken))
    # A damaged entry: the CRC in both headers no longer matches the data.
    data = bytearray(write_zip(os.path.join(out, "badcrc.xlsx"), parts))
    zf = zipfile.ZipFile(io.BytesIO(bytes(data)))
    info = zf.getinfo("xl/worksheets/sheet2.xml")
    wrong = (info.CRC ^ 0x5A5A5A5A).to_bytes(4, "little")
    data[info.header_offset + 14:info.header_offset + 18] = wrong
    # Find the central directory record for sheet2 by its name.
    name = info.filename.encode()
    at = 0
    while True:
        at = data.index(b"PK\x01\x02", at)
        n = int.from_bytes(data[at + 28:at + 30], "little")
        if bytes(data[at + 46:at + 46 + n]) == name:
            data[at + 16:at + 20] = wrong
            break
        at += 4
    with open(os.path.join(out, "badcrc.xlsx"), "wb") as f:
        f.write(data)
    # Not a ZIP at all.
    with open(os.path.join(out, "notzip.xlsx"), "wb") as f:
        f.write(b"Account,Description\r\n1100,Writer\r\n")


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "fixtures"
    make_inflate(os.path.join(out, "inflate"))
    make_xlsx(os.path.join(out, "xlsx"))
    total = 0
    for root, _, files in os.walk(out):
        for f in files:
            total += os.path.getsize(os.path.join(root, f))
    print(f"wrote fixtures to {out}: {total} bytes")


if __name__ == "__main__":
    main()
