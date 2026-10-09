# Decisions

Short records of choices that shape the code. Newest last.

## 1. Money and decimals (2026-10-08)

- **Money is `Cents = i64`.** Never floats.
- **Everything else numeric is `Dec`**: fixed-point, 9 decimal places, stored in an `i128`.
  Quantities, rates, percentages, exchange rates and globals are all `Dec`.
- **Rounding rule: half away from zero**, everywhere (0.005 becomes 0.01, -0.005 becomes -0.01).
  It's the rule people expect from payroll and spreadsheets, and it's symmetric for credits.
- `Dec` multiply and divide round to 9 places. That can matter for repeating fractions:
  `35 / 3` is stored as `11.666666667`.

## 2. Where rounding happens (2026-10-08)

Money is rounded to cents **once per detail line**, never in the rollups:

1. `quantity = amount x multiplier` (Dec)
2. `local total = round(quantity x rate)` in the line's currency
3. `base total = round(local total x exchange rate)` when the currency is foreign
4. each fringe = `round(base total x percent / 100)` or `round(per-unit cents x units)`

Accounts, categories, the subtotal, fringe totals and the grand total are integer sums of
those cents, so they always agree exactly. Contractual charges round once each, on their base.

## 3. Fringe cutoffs are per line (2026-10-08)

A percent fringe's `wage_cap` caps the fringed amount **on each detail line**. A cap that
tracks one person across several lines needs a notion of "person" the model doesn't have yet.
Revisit once the feature research is done.

## 4. Groups (2026-10-08)

Group filtering is a recalc option, not budget data: `exclude_groups` drops tagged lines,
`only_groups` keeps only tagged lines. A filtered-out line still evaluates (so bad formulas
are still reported) but contributes 0. Property-tested: excluding a group gives the same
totals as deleting its lines.

## 5. Globals and formulas (2026-10-08)

Amount, multiplier and rate are formula strings: numbers, global names, `+ - * /`, unary
minus, parentheses. Hand-written parser plus AST walker, no `eval`. Globals can reference
each other; they're evaluated in dependency order and cycles are reported with their path.

## 6. No dependencies in the engine yet (2026-10-08)

Property tests use a small seeded generator in `tests/properties.rs` instead of `proptest`.
It keeps `fwb-core` dependency-free until we actually need something (serde, for the format).
