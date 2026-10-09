# Film Wizard Budgeting

An open-source, clean-room film and TV budgeting app, written in Rust. A Darkwell project.

**Status:** engine in progress (`crates/fwb-core`): budget model, formulas and globals,
fringes with cutoffs and ranges, groups, currencies, charges, credits and topsheet totals.
Native `.fwb` file format, a small `fwb` command line, and a first desktop app
(`crates/fwb-app`, Tauri). Design choices live in [docs/decisions.md](docs/decisions.md).

## Scope (v1)
1. **Budgeting engine** (`crates/fwb-core`): topsheet, account and detail levels, fringes,
   globals, groups, subtotals, contractual charges, currencies. Integer-cent money.
2. **Import from exports**: bring budgets over from other tools via the formats they export
   (Excel/CSV), no proprietary file parsing.
3. **Own open format**: a documented, diff-friendly native file format.

See [CLEAN-ROOM.md](CLEAN-ROOM.md) for the rules every contribution follows.

## Build
```bash
cargo test
```

## Try it
The desktop app (local files only, works offline; see [docs/APP.md](docs/APP.md)):
```bash
cargo run -p fwb-app
```
On Linux, install the WebKitGTK build packages first (listed in docs/APP.md).

The command line:
```bash
cargo run -p fwb-cli -- new my-film.fwb
cargo run -p fwb-cli -- show my-film.fwb
```
Budgets are saved as `.fwb` files: plain, documented JSON ([docs/FORMAT.md](docs/FORMAT.md)).

## License
Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
