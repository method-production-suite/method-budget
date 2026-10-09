# Film Wizard Budgeting

An open-source, clean-room film and TV budgeting app, written in Rust. A Darkwell project.

**Status:** scaffold. Nothing usable yet.

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
