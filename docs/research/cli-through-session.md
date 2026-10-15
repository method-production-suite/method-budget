# The command line's edits through method-session

Status: done on the `cli-through-session` branch. method-cli depends on method-session, and
the MCP server's and `method set` / `method apply`'s edits call the app's edits
(`method_session::edit`) instead of their own copies of its checks. The MCP tools' JSON and
error kinds are unchanged; `crates/method-cli/src/mcp/tools/tests.rs` passes as it was.

## How kinds survive

The app's edits return `Result<_, String>`. The MCP server tells `invalid_input`,
`not_found` and `refused` apart, so the edits it needs moved to `edit::typed` and return an
`EditError` (`Invalid`, `NotFound`, `Refused`) whose text is the app's message. The functions
the app calls keep their names and `String` errors and call those. `From<EditError> for
ToolError` keeps the kind.

## The map

| CLI | Session function now used | Differences found, and what was done |
|---|---|---|
| MCP `add_setup` / `update_setup` | `edit::set_fringe`, `set_group`, `set_set`, `set_location`, `set_unit`, `typed::set_currency`, `rowstyle::apply_group_color` | MCP fills what an update leaves out from the item as it is, then hands the whole item over. Short-code and color messages are now the app's (wording only). MCP keeps its own checks for things the app doesn't check: a flat fringe's unit must exist (Q2), money is exact (Q1). |
| MCP `delete_setup` | `delete_global`, `delete_fringe`, `typed::delete_group`, `delete_set`, `delete_location`, `delete_unit_with`, `typed::delete_currency_with`; refusals `typed::check_group_delete`, `check_currency_delete` | The "used by the credit", "totals are shown in", "a charge is in" refusals were copies; now one each. The use count and `force` stay MCP-only. Deleting a unit also blanks flat fringes' and globals' units in MCP; the app's delete leaves them (Q3). |
| MCP `set_global`, script `set global` | `typed::set_global` (+ `display::apply_global_info` for MCP's description and unit) | Script didn't refuse an empty value; neither refused a global that divides by zero or loops (MCP refused it later, as "wouldn't add up", kind `refused`; the app refuses it at entry, now also kind `refused`). A global's description now has the app's length limit. |
| MCP `set_charge` / `set_credit` | `typed::set_charge` / `typed::set_credit` | Name-addressed partial updates are built into a full `ChargeInput` / `CreditInput`. Messages for an empty name, a bad percent or discount, an unknown currency and an unknown credit source are now the app's. A credit row may keep a group, category or account its old rows used (as charge exclusions already could). Excluded amounts stay non-negative in MCP; the app allows negative ones (Q4). |
| MCP `delete_charge`, `apply_charge` | `delete_charge` / `delete_credit` by index | Same behaviour; `apply_charge` no longer goes through the script. |
| MCP `add_category`, `update_category`, `add_production_total`, `delete_category`, `delete_production_total` | `typed::add_category`, `update_topsheet_row`, `add_production_total`, `delete_topsheet_row` | **Drift bug fixed:** MCP inserted and removed topsheet rows without moving the budget's fringe row, so it could end up above the wrong row. Number messages lose MCP's `21x: ` prefix. |
| MCP `add_account`, `update_account`, `delete_account` | `typed::add_account`, `update_account`, `delete_account` | "there is no category X" is now the app's "category X no longer exists" (still `not_found`). The row count and `force` stay MCP-only. |
| MCP `set_title`, script `set title` | `edit::set_title` | The script now trims the title, as MCP and the app did. |
| MCP `add_lines` | `edit::copy_from_neighbor` | MCP's copy skipped fields the row sets; it then set them anyway, so the result is the same. |
| MCP `delete_lines` | `edit::delete_details` | Same. |
| script `set fringe_row` | `edit::set_fringe_row` | The script took any account text and any description length; it now has the app's limits. Its own message for a position past the end stays (it says the bounds). |
| script `set line .. color` | `rowstyle::check_color` | Message wording only. |
| `parse_money` (script, MCP `cents`) | `edit::parse_money_as(.., SubCents::Refuse)` | One parser with a rounding or exact option. The CLI's strictness (refuses digits past the cent) is kept; it now also reads `$`, parentheses and `+` as the app does. |
| money formatters (`calc/explain.rs`, `setup/copy.rs`, `cli.rs`) | `CurrencyFormat::number` | Same text, tests pin it. The CLI keeps parentheses for negatives. |

Not routed: script and MCP field edits on lines (`set line`, `update_lines`), tags, the
fringe ops `set fringe .. rate|cutoff|basis|max_hours_per_day`, renames (both already call
`method_core::rename`), and the settings switches (arithmetic, group rules, cutoff factor),
which only parse a core enum. Line fields differ on purpose or by drift (Q5).

## Open questions for the owner

1. **Money in the CLI is exact, in the app rounded.** `1.005` is an error in a script or MCP
   call and $1.01 in the app. Kept as is (`SubCents::Refuse` for the CLI). A script cutoff
   may still be negative; the app refuses a negative cutoff.
2. **A flat fringe's unit.** MCP refuses a unit the budget doesn't have; the app's
   `set_fringe` stores any text. Which should the app do?
3. **Deleting a unit.** MCP (with `force`) also blanks it on flat fringes and global info;
   the app's `delete_unit_with(.., true)` only blanks it on lines. Should the app do the same?
4. **Negative excluded amounts on a charge.** The app takes them, MCP refuses them.
5. **Line fields.** `set line` and MCP's `update_lines` check that a unit, set, location and
   currency exist and store formulas as typed; the app's `set_detail_field` doesn't check
   those names, trims formulas, refuses one that divides by zero and fills an empty Unit from
   the global an Amt names. Which is meant for the command line?
6. **The actuals page's money** (`actuals::parse_money`) reads amounts as a sheet import does
   (`€` and `£` signs, `1.234,56`, `1,23` refused). It is left alone: folding it into
   `parse_money` would change what that page accepts.
