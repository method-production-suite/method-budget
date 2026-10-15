#!/usr/bin/env python3
"""Fills fuzz/corpus/<target>/ with seed inputs from files the repository tracks.

Usage: python3 fuzz/seed.py

Seeds come only from tracked templates and test fixtures (never the untracked
scratch files at the repository root). Existing corpus files are kept, so a
corpus grown by earlier runs survives; seeds are named by their content's hash.
Stdlib only.
"""

import hashlib
import io
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "fuzz" / "corpus"
FIX = ROOT / "crates" / "method-format" / "tests" / "fixtures"
CHARTS = ROOT / "templates" / "charts"

# Native budgets bigger than this aren't worth mutating byte by byte.
MAX_SEED = 40_000


def put(target, data):
    d = CORPUS / target
    d.mkdir(parents=True, exist_ok=True)
    (d / hashlib.sha1(data).hexdigest()).write_bytes(data)


def files(directory, *patterns):
    for pattern in patterns:
        yield from sorted(directory.glob(pattern))


def workbooks():
    return list(files(FIX / "xlsx", "*.xlsx")) + list(files(FIX / "exchange", "*.xlsx"))


def main():
    for f in files(FIX / "inflate", "*.deflate"):
        put("inflate", f.read_bytes())

    for f in workbooks():
        data = f.read_bytes()
        put("zip", data)
        put("xlsx", data)
        # The raw DEFLATE streams and XML parts inside each workbook.
        try:
            z = zipfile.ZipFile(io.BytesIO(data))
        except zipfile.BadZipFile:
            continue
        for info in z.infolist():
            if info.compress_type == zipfile.ZIP_DEFLATED:
                start = info.header_offset + 30
                head = data[info.header_offset : start]
                start += int.from_bytes(head[26:28], "little") + int.from_bytes(head[28:30], "little")
                put("inflate", data[start : start + info.compress_size])
            try:
                part = z.read(info)
            except Exception:  # a fixture that is damaged on purpose
                continue
            if info.filename.endswith((".xml", ".rels")):
                put("xml", part)

    for f in files(FIX / "exchange", "*.xml"):
        put("xml", f.read_bytes())

    # The csv target's first byte picks the delimiter hint (0: none).
    for f in files(CHARTS / "src", "*.csv"):
        put("csv", b"\x00" + f.read_bytes())
        put("csv", b"\x01" + f.read_bytes())
    for f in files(FIX / "exchange", "*.txt"):
        put("csv", b"\x02" + f.read_bytes())

    for f in files(FIX / "exchange", "*.json"):
        put("exchange_json", f.read_bytes())

    golden = ROOT / "crates" / "method-format" / "tests" / "golden" / "small.json"
    put("mthd", golden.read_bytes())
    for f in files(CHARTS, "*.mthd"):
        if f.stat().st_size <= MAX_SEED:
            put("mthd", f.read_bytes())
    # The small budget with versions written as deltas (docs/FORMAT.md,
    # "Version deltas"), so the patcher is reached from the start.
    doc = json.loads(golden.read_text())
    doc["budget"]["compact_versions"] = True
    doc["budget"]["versions"] = [
        {
            "id": 1,
            "name": "Studio draft",
            "created": "2026-10-09T14:30:00Z",
            "delta": {
                "base": "next",
                "patch": [{"op": "replace", "path": "/title", "value": "Draft"}],
            },
        },
        {
            "id": 2,
            "name": "Locked",
            "created": "2026-10-12T09:00:00Z",
            "delta": {
                "base": "budget",
                "patch": [
                    {"op": "add", "path": "/globals/WEEKS", "value": "2"},
                    {"op": "remove", "path": "/globals/DAYS"},
                ],
            },
        },
    ]
    put("mthd", (json.dumps(doc, indent=2) + "\n").encode())

    # [document, other]: other is a patch for document, and the differ's
    # target from it.
    base = doc["budget"]
    for pair in [
        [{"a": 1, "b": [1, 2, 3]}, [{"op": "add", "path": "/b/-", "value": 4}]],
        [{"a": [1, 2, 3, 4]}, {"a": [1, 3, 4, 5], "c": {"d~/e": None}}],
        [[1, [2, 3], {"x": "y"}], [[2, 3], 1, {"x": "z"}, 4]],
        [base, [{"op": "replace", "path": "/title", "value": "x"}, {"op": "remove", "path": "/info"}]],
        [base, doc["budget"]["versions"][1]["delta"]["patch"]],
    ]:
        put("delta", json.dumps(pair).encode())

    for d in sorted(CORPUS.iterdir()):
        print(f"{d.name:15} {len(list(d.iterdir())):4} seeds")


if __name__ == "__main__":
    main()
