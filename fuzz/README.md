# Fuzzing

Fuzz targets for the parsers Method Budget writes by hand, run with
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer). This
directory is its own Cargo workspace: the main workspace, its pinned compiler
and its CI never build it. Only fuzzing uses a nightly compiler.

| Target          | What it reads |
|-----------------|---------------|
| `inflate`       | DEFLATE streams (`import/inflate.rs`) |
| `zip`           | ZIP directories, then every entry (`import/zipread.rs`) |
| `xlsx`          | Whole Excel workbooks (`import/xlsxread.rs`) |
| `csv`           | Delimited text; the first byte picks the delimiter hint (`import/csvread.rs`) |
| `xml`           | The XML tokenizer, event by event (`import/xml.rs`) |
| `exchange_json` | Budget exchange JSON, then build and reconcile (`import/exchange`) |
| `mthd`          | Native budgets (`from_str`); what reads must write and read back the same |
| `delta`         | `[doc, other]`: `other` as a version-delta patch on `doc`, and the differ's patch from `doc` to `other` must rebuild `other` (`delta.rs`) |

## Running

Once:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --version 0.13.2 --locked
```

From the repository root:

```sh
python3 fuzz/seed.py                      # seed fuzz/corpus/ from tracked fixtures
cargo +nightly fuzz list --fuzz-dir fuzz
cargo +nightly fuzz run --fuzz-dir fuzz xlsx -- -max_total_time=300
```

The build keeps debug assertions and overflow checks on (`[profile.release]` in
`Cargo.toml`), so an overflow is a crash, not a wrapped number.

A crash leaves its input in `fuzz/artifacts/<target>/`. Shrink and replay it:

```sh
cargo +nightly fuzz tmin --fuzz-dir fuzz xlsx fuzz/artifacts/xlsx/crash-...
cargo +nightly fuzz run  --fuzz-dir fuzz xlsx fuzz/artifacts/xlsx/minimized-from-...
```

Fix the cause in the parser and add the shrunk input, as inline bytes, to a
unit test in the parser's own module.

`.github/workflows/fuzz.yml` runs every target for five minutes each Thursday (and
on demand), uploading any crashing inputs as artifacts.

## Dependencies

`fuzz/Cargo.lock` is separate from the workspace's and holds to the same
14-day cooldown: `python3 scripts/check-crate-ages.py --lock fuzz/Cargo.lock`
(CI checks both locks).
