# The command line's edits through method-session

Status: done on the `cli-through-session` branch. method-cli depends on method-session, and
the MCP server's and `method set` / `method apply`'s edits call the app's edits
(`method_session::edit`) instead of their own copies of its checks. The MCP tools' JSON and
error kinds are unchanged. The owner then settled the differences found (2026-10-15,
"Decisions" below); the tests that changed with those decisions are listed there.

## How kinds survive

The app's edits return `Result<_, String>`. The MCP server tells `invalid_input`,
`not_found` and `refused` apart, so the edits it needs moved to `edit::typed` and return an
`EditError` (`Invalid`, `NotFound`, `Refused`) whose text is the app's message. The functions
the app calls keep their names and `String` errors and call those. `From<EditError> for
ToolError` keeps the kind.

## The map

| CLI | Session function now used | Differences found, and what was done |
|---|---|---|
| MCP `add_setup` / `update_setup` | `typed::set_fringe`, `set_group`, `set_set`, `set_location`, `set_unit`, `typed::set_currency`, `rowstyle::apply_group_color` | MCP fills what an update leaves out from the item as it is, then hands the whole item over. Short-code and color messages are now the app's (wording only). Money is passed as typed and rounded by the app (D1); an unknown flat fringe unit is refused by the app's edit (D2). |
| MCP `delete_setup` | `delete_global`, `delete_fringe`, `typed::delete_group`, `delete_set`, `delete_location`, `delete_unit_with`, `typed::delete_currency_with`; refusals `typed::check_group_delete`, `check_currency_delete` | The "used by the credit", "totals are shown in", "a charge is in" refusals were copies; now one each. The use count and `force` stay MCP-only. Deleting a unit with its lines blanks it on flat fringes and global info in the app's edit (D3). |
| MCP `set_global`, script `set global` | `typed::set_global` (+ `display::apply_global_info` for MCP's description and unit) | Script didn't refuse an empty value; neither refused a global that divides by zero or loops (MCP refused it later, as "wouldn't add up", kind `refused`; the app refuses it at entry, now also kind `refused`). A global's description now has the app's length limit. |
| MCP `set_charge` / `set_credit` | `typed::set_charge` / `typed::set_credit` | Name-addressed partial updates are built into a full `ChargeInput` / `CreditInput`. Messages for an empty name, a bad percent or discount, an unknown currency and an unknown credit source are now the app's. A credit row may keep a group, category or account its old rows used (as charge exclusions already could). Money is passed as typed; the app's edit refuses a negative excluded amount (D4). |
| MCP `delete_charge`, `apply_charge` | `delete_charge` / `delete_credit` by index | Same behaviour; `apply_charge` no longer goes through the script. |
| MCP `add_category`, `update_category`, `add_production_total`, `delete_category`, `delete_production_total` | `typed::add_category`, `update_topsheet_row`, `add_production_total`, `delete_topsheet_row` | **Drift bug fixed:** MCP inserted and removed topsheet rows without moving the budget's fringe row, so it could end up above the wrong row. Number messages lose MCP's `21x: ` prefix. |
| MCP `add_account`, `update_account`, `delete_account` | `typed::add_account`, `update_account`, `delete_account` | "there is no category X" is now the app's "category X no longer exists" (still `not_found`). The row count and `force` stay MCP-only. |
| MCP `set_title`, script `set title` | `edit::set_title` | The script now trims the title, as MCP and the app did. |
| MCP `add_lines` | `edit::copy_from_neighbor` | MCP's copy skipped fields the row sets; it then set them anyway, so the result is the same. |
| MCP `delete_lines` | `edit::delete_details` | Same. |
| script `set fringe_row` | `edit::set_fringe_row` | The script took any account text and any description length; it now has the app's limits. Its own message for a position past the end stays (it says the bounds). |
| script `set line .. color` | `rowstyle::check_color` | Message wording only. |
| `parse_money` (script, MCP `cents`) | `edit::parse_money` | One parser, `parse_money_as` with a rounding or exact option. The CLI rounds as the app does (D1) and reads `$`, parentheses and `+` as it does. |
| script `set line`, MCP `update_lines` / `add_lines` fields | `edit::set_detail_field` / `set_rows_field`, `rowstyle::apply_row_style` | The script's own field code is gone (D5). `fringe_range` stays the script's: the app numbers ranges itself and has no edit that sets one. |
| script `set fringe .. rate\|cutoff\|basis\|max_hours_per_day` | `typed::set_fringe` | The fringe is rebuilt as the app's input with one field changed, so the app's checks apply (a negative cutoff is refused, D1). |
| money formatters (`calc/explain.rs`, `setup/copy.rs`, `cli.rs`) | `CurrencyFormat::number` | Same text, tests pin it. The CLI keeps parentheses for negatives. |

Not routed: tags, renames (both already call `method_core::rename`), and the settings
switches (arithmetic, group rules, cutoff factor), which only parse a core enum. MCP keeps
its own check that a row's kind uses a field, since it covers the style fields too and says
which row.

## Decisions (owner, 2026-10-15)

1. **Money rounds everywhere.** Scripts and MCP round as the app does (`SubCents::Round`):
   `1.005` is $1.01. A negative cutoff is refused, as the app refuses it.
2. **A flat fringe's unit must exist**, in the app and MCP alike: `typed::set_fringe`
   refuses it (`not_found`).
3. **Deleting a unit with its lines** (`delete_unit_with(.., true)`, MCP's `force`) also blanks
   it on flat fringes and global info, in the app too.
4. **A charge's excluded amount can't be negative**, in the app and MCP alike.
5. **Line fields follow the app.** `set line` and MCP's line tools set fields with
   `set_detail_field` (formulas trimmed, a formula that divides by zero refused, an empty Unit
   filled from the global an Amt names) and `apply_row_style`. A unit, currency, set or
   location a line names must exist, now in the app too. Quick add adds the new item before
   setting the cell, so the cell can name it.
6. **The actuals page's money** (`actuals::parse_money`) stays separate: it reads amounts as a
   sheet import does (`€` and `£` signs, `1.234,56`, `1,23` refused).

Tests changed with these decisions:

- `method-app/tests/commands.rs`, `charges_are_added_edited_moved_and_deleted`: the excluded
  amount `(1,000)` is now refused (D4); the test uses `$1,000` and checks the refusal.
- `method-session/src/apply.rs`, `eligibility_matches_the_grid_cells`: the budget gets a set,
  location and currency named `X` first, since a cell may only name what exists (D5).
- `method-cli/src/mcp/tools/tests.rs`, charges: `1.005` no longer fails with "two decimal
  places" (D1); the case is now `(1.005)`, refused as negative.
- `method-cli/src/script.rs`, `parses_every_command`: `cutoff 1.005` no longer fails to parse
  (D1); `money_values` checks it rounds.

Tests added: `edit::tests::names_on_lines_and_fringes_must_exist`,
`deleting_a_unit_with_its_lines_clears_it_from_settings`, and
`script::tests::fringe_and_line_edits_are_the_apps`.
