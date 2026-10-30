//! Engine messages with stable codes (decisions.md, entry 94, fifth pass;
//! docs/APP.md, "Languages").
//!
//! A message the engine gives a person (an edit refused, a value that
//! doesn't read) is made at its source from a code and its parameters with
//! [`msg`]: `msg("engine.edit.account_gone", &[("code", &code)])`. The
//! English for each code is in [`MESSAGES`], so the command line, the MCP
//! server and the browser build show the same English as before: the error
//! is still that text, and nothing here reads a table.
//!
//! The desktop app shows the words in the person's language: it hands the
//! text to [`recognize`], which gives back the code and the parameters the
//! English was made from, and looks the code up in `ui/i18n/en.js` (whose
//! entry for each code is this English, tested), falling back to the
//! English. A parameter named in [`NESTED`] is a message itself (a noun
//! such as `engine.what.fringe`, or a reason) and is recognized in turn.
//!
//! Codes are `engine.<area>.<name>`, letters, digits, dots and underscores;
//! parameters are `{name}`. A message about a count is plural: an English
//! form for one and one for more, picked by its `count` parameter.
//!
//! Generic templates make recognition ambiguous, so two rules keep it
//! honest (decision 94, sixth pass). A count phrase put into a sentence
//! (`engine.phrase.*`: "3 comments", "added 2 accounts") is only looked for
//! inside a nested parameter ([`recognize_nested`]), never as a whole
//! message, since "{count} accounts" would match any text ending that way.
//! And a summary of several sentences is several messages joined by a
//! space, or a list of phrases joined by ", ", which the desktop translates
//! piece by piece (`i18n::engine()`), rather than one template per mix.
//!
//! Undo labels (`engine.undo.*`, decision 94, seventh pass) are made here
//! too, so the History tab, the Activity tab and the rest can show them in
//! the person's language. A label is stored: in the session's history, in a
//! shared budget's change log as the change's `label`, and from there in
//! what other computers and older versions read. So it is always this
//! English, and only its display is translated, on this computer, through
//! [`recognize_label`]. Labels are only looked for there, never as a
//! message (an English error starting "Edit " is not a step), and a
//! message is never looked for as a label. What the person named a thing
//! (a version, a fringe, an account) is a parameter, kept as they wrote it.

use std::fmt::Display;

/// One message: its code and its English, with a second form when it
/// counts something (`one` for a count of 1, `other` for the rest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub code: &'static str,
    /// The English for a count of 1, when the message counts something.
    pub one: Option<&'static str>,
    /// The English (for a count other than 1, when it counts something).
    pub other: &'static str,
}

const fn m(code: &'static str, english: &'static str) -> Message {
    Message {
        code,
        one: None,
        other: english,
    }
}

const fn plural(code: &'static str, one: &'static str, other: &'static str) -> Message {
    Message {
        code,
        one: Some(one),
        other,
    }
}

/// Parameters whose value is a message too: a noun the engine names
/// (`engine.what.*`), a reason given after a colon, or a count phrase or a
/// list of them (`engine.phrase.*`), or a role in words (`engine.role.*`).
/// Others are data.
pub const NESTED: &[&str] = &[
    "accounts",
    "added",
    "categories",
    "changes",
    "dismissed",
    "merged",
    "role",
    "rows",
    "source",
    "what",
    "why",
];

/// Codes only recognized inside a nested parameter.
pub const PHRASE: &str = "engine.phrase.";

/// A role in words ("Edits everything"), only recognized inside a nested
/// parameter, and the only thing a `role` parameter can be.
pub const ROLE: &str = "engine.role.";

/// Undo labels' codes, only recognized by [`recognize_label`].
pub const UNDO: &str = "engine.undo.";

/// Names the engine gives things it saves (the automatic version before a
/// restore), only recognized by [`recognize_name`].
pub const NAME: &str = "engine.name.";

/// The fixed words of Explain's working (`engine.working.*`, decision 94,
/// twelfth pass). Each step carries its code and values
/// (`method_core::explain::Said`), so they are never recognized: a line
/// the person called "Fringes" is theirs. The English is method-core's
/// `explain::WORDING` (tested).
pub const WORKING: &str = "engine.working.";

/// Every engine message, by code (sorted, tested).
pub const MESSAGES: &[Message] = &[
    m("engine.actuals.account_number_long", "An account number is too long."),
    m("engine.actuals.amount_too_large", "The amount is too large."),
    m("engine.actuals.not_a_date", "{text} isn't a date written YYYY-MM-DD."),
    m("engine.actuals.not_a_percent", "{text} isn't a percent."),
    m("engine.actuals.not_an_amount", "{text} isn't an amount."),
    m("engine.actuals.too_large", "The actuals add up to more than the app can hold."),
    m("engine.actuals.type_amount", "Type an amount."),
    m("engine.actuals.unknown_date_order", "unknown date order `{order}`"),
    m("engine.actuals.unknown_method", "unknown method `{method}`"),
    m("engine.apply.already_one_range", "these lines are already one fringe range"),
    m("engine.apply.no_such", "there is no {what} `{name}`"),
    m("engine.apply.none_in_range", "none of the selected lines are in a fringe range"),
    m("engine.apply.none_take_currency", "currencies only go on lines, and none of the selected rows is a line"),
    m("engine.apply.none_take_fringes", "fringes only go on lines and adjustment rows, and none of the selected rows is one"),
    m("engine.apply.none_take_groups", "none of the selected rows can be in a group"),
    m("engine.apply.none_take_location", "none of the selected rows can take a location"),
    m("engine.apply.none_take_set", "none of the selected rows can take a set"),
    m("engine.apply.range_adjacent", "select lines next to each other to make a fringe range"),
    m("engine.apply.range_numbers_used", "this account has no fringe range numbers left"),
    m("engine.apply.range_one_account", "a fringe range has to stay inside one account"),
    m("engine.apply.range_two_lines", "a fringe range needs at least two lines"),
    m("engine.apply.select_lines", "select one or more lines first"),
    m("engine.apply.select_lines_first", "select lines first"),
    m("engine.apply.unknown_pick", "`{field}` can't be assigned to rows; use set, location or currency"),
    m("engine.apply.unknown_tag", "`{field}` can't be applied to rows; use fringes or groups"),
    m("engine.compare.already_in", "That budget is in the comparison already."),
    m("engine.compare.choose_one", "Choose at least one budget to compare with."),
    m("engine.compare.file_gone", "That file is no longer at hand; add it again."),
    m("engine.compare.source_closed", "A budget in the comparison has been closed."),
    m("engine.compare.too_many", "Up to {max} budgets can be compared with this one at a time."),
    m("engine.core.bad_separator", "separator {char} is not allowed (use none, space, - . , / : or ;)"),
    m("engine.core.category_width", "category width must be at least 1"),
    m("engine.core.divide_by_zero", "divide by zero"),
    m("engine.core.empty_suffix", "fringe account suffix is empty"),
    m("engine.core.invalid_char", "character {char} is not allowed in an account number"),
    m("engine.core.no_account_segment", "fringe accounts need an account segment (account width is 0)"),
    m("engine.core.not_a_number", "not a number"),
    m("engine.core.number_too_large", "number too large"),
    m("engine.core.segment_gap", "{what} segment is used but the segment before it is not"),
    m("engine.core.too_many_segments", "account number has {found} segments but the pattern has only {max}"),
    m("engine.core.too_precise", "more than {places} decimal places"),
    m("engine.core.too_precise_exact", "more than {places} decimal places (exact arithmetic keeps {kept})"),
    m("engine.core.unknown_global", "unknown global `{name}`"),
    m("engine.core.width_too_large", "{what} width {width} is too large (at most {max})"),
    plural("engine.days.all_match", "The globals already match the schedule; {count} global is now linked to it.", "The globals already match the schedule; {count} globals are now linked to it."),
    plural("engine.days.already_matched", "{count} global already matched.", "{count} globals already matched."),
    m("engine.days.ambiguous", "{global} was set from a figure that now matches more than one figure in this file ({keys}): pick the right one to keep it linked."),
    m("engine.days.bad_global", "{name} can't be a global name: use letters, digits and _, with at least one letter or _."),
    m("engine.days.bad_global_of", "{label}: {name} can't be a global name: use letters, digits and _, with at least one letter or _."),
    m("engine.days.claimed", "{globals} were all set from this figure before; only {kept} is offered here, the others are left as they are."),
    m("engine.days.found_by_words", "Found by its description only (set before keys were recorded); setting it again records its key."),
    m("engine.days.goes_too", "{other} goes to {global} too."),
    m("engine.days.goes_too_of", "{label}: {other} goes to {global} too."),
    m("engine.days.gone", "Not in this schedule file any more."),
    m("engine.days.no_figure", "The file has no figure {key}."),
    m("engine.days.no_such_key", "the file has no figure {key} (keys are total, unit:NAME, set:NAME, location:NAME, week:YYYY-MM-DD and element:ID, as the review lists them)"),
    m("engine.days.not_a_days_file", "Couldn't read {file}: it isn't a schedule days file (Method Schedule writes one with File > Export for Budget)."),
    m("engine.days.nothing_picked", "Nothing is picked to set."),
    m("engine.days.renamed", "Set from {category} {element} before; the schedule's element with that id is now named and numbered differently. Tick it to keep the link."),
    plural("engine.days.set", "Set {count} global from the schedule.", "Set {count} globals from the schedule."),
    plural("engine.days.set_new", "Set {count} global from the schedule ({new} new).", "Set {count} globals from the schedule ({new} new)."),
    m("engine.days.tick_for_zero", "Tick it to set the global to 0."),
    m("engine.docs.closed", "That budget has been closed."),
    m("engine.docs.last_budget", "The last budget can't be closed; close the window instead."),
    m("engine.docs.last_in_pane", "A pane's only tab can't move to the other pane."),
    m("engine.docs.no_such_pane", "There is no such pane."),
    m("engine.docs.not_in_pane", "That budget isn't in that pane."),
    m("engine.docs.not_split", "The window isn't split."),
    m("engine.docs.split_already", "The window is split already."),
    m("engine.edit.account_exists", "account {code} already exists"),
    m("engine.edit.account_gone", "account {code} no longer exists"),
    m("engine.edit.account_not_in_category", "account {code} is not in category {category}"),
    m("engine.edit.account_number_empty", "account number is empty"),
    m("engine.edit.amount_sign_after", "put the sign before the $"),
    m("engine.edit.amount_sign_twice", "use a minus sign or parentheses, not both"),
    m("engine.edit.base_currency", "{code} is the base currency"),
    m("engine.edit.base_currency_listed", "{code} is in the currency list; delete it there first"),
    m("engine.edit.cant_be_negative", "{what} can't be negative"),
    m("engine.edit.cant_move", "can't move the {what} there"),
    m("engine.edit.category_exists", "category {code} already exists"),
    m("engine.edit.category_gone", "category {code} no longer exists"),
    m("engine.edit.category_number_empty", "category number is empty"),
    m("engine.edit.charge_exists", "there is already a charge named {name}"),
    m("engine.edit.charge_in_currency", "a charge is in {code}; change it first"),
    m("engine.edit.charge_name_empty", "charge name is empty"),
    m("engine.edit.credit_exists", "there is already a credit named {name}"),
    m("engine.edit.credit_name_empty", "credit name is empty"),
    m("engine.edit.credit_no_account", "there is no account `{code}`"),
    m("engine.edit.credit_no_category", "there is no category `{code}`"),
    m("engine.edit.credit_no_group", "there is no group `{name}`"),
    m("engine.edit.currency_gone", "currency {code} no longer exists"),
    m("engine.edit.discount_over_100", "the discount can't be more than 100%"),
    m("engine.edit.exchange_rate_zero", "an exchange rate must be more than 0"),
    m("engine.edit.field_gone", "field {name} no longer exists"),
    m("engine.edit.field_needs_label", "the field needs a label"),
    m("engine.edit.flat_charge_needs_amount", "a flat charge needs an amount"),
    m("engine.edit.formula_refused", "{what} can't be {value}: {why}"),
    m("engine.edit.fringe_gone", "fringe {name} no longer exists"),
    m("engine.edit.fringe_row_account_char", "the fringe row's account number can only use letters, digits and separators, not {char}"),
    m("engine.edit.fringe_row_description_long", "the fringe row's description is longer than {max} characters"),
    m("engine.edit.fringe_suffix_char", "the suffix can only use letters and digits, not {char}"),
    m("engine.edit.fringe_suffix_empty", "enter the fringe account suffix, like 99"),
    m("engine.edit.global_gone", "global {name} no longer exists"),
    m("engine.edit.global_name", "`{name}` can't be a global name: use letters, digits and _, with at least one letter or _"),
    m("engine.edit.global_needs_value", "global {name} needs a value"),
    m("engine.edit.global_refused", "global {name} can't be {formula}: {why}"),
    m("engine.edit.group_gone", "group {name} no longer exists"),
    m("engine.edit.group_used_by_credit", "group {name} is used by the credit {credit}; change the credit first"),
    m("engine.edit.hours_negative", "hours can't be negative"),
    m("engine.edit.item_gone", "{what} {name} no longer exists"),
    plural("engine.edit.lines_use_currency", "{count} line uses {code}; change them first", "{count} lines use {code}; change them first"),
    m("engine.edit.location_gone", "location {name} no longer exists"),
    m("engine.edit.lock_needs_fix", "fix this first, then lock the totals: {why}"),
    m("engine.edit.max_hours_zero", "max hours per day must be more than 0"),
    m("engine.edit.name_comma", "{what} names can't contain commas"),
    m("engine.edit.name_empty", "{what} name is empty"),
    m("engine.edit.needs_value", "{what} needs a value"),
    m("engine.edit.negative_cutoff", "a cutoff can't be negative"),
    m("engine.edit.no_baseline_to_clear", "there are no original totals to clear"),
    m("engine.edit.no_baseline_to_freeze", "there are no original totals to freeze"),
    m("engine.edit.no_exchange_rate", "there is no exchange rate for {code}; add it first"),
    m("engine.edit.no_exchange_rate_listed", "there is no exchange rate for {code}; add it under Currencies first"),
    m("engine.edit.no_row_uses_column", "none of the selected rows use this column"),
    m("engine.edit.no_short_code", "a {what} has no short code"),
    m("engine.edit.no_such", "there is no {what} {name}"),
    m("engine.edit.not_a_date", "`{text}` is not a date; write it as YYYY-MM-DD, like 2027-03-01"),
    m("engine.edit.not_a_number", "{what} `{text}` is not a number: {why}"),
    m("engine.edit.not_an_amount", "`{text}` is not an amount: {why}"),
    m("engine.edit.not_binary_exact", "`{value}` is not binary or exact"),
    m("engine.edit.not_cutoff_factor", "`{value}` is not a cutoff factor: use 1.0, 1.5, 2.0, 2.5 or 3.0"),
    m("engine.edit.not_group_rule", "`{value}` is not a group rule"),
    m("engine.edit.not_include_exclude", "`{value}` is not include or exclude"),
    m("engine.edit.not_keep_drop", "`{value}` is not keep or drop"),
    m("engine.edit.past_cents", "`{text}` has more than two decimal places"),
    m("engine.edit.production_total_name_empty", "production total name is empty"),
    m("engine.edit.row_gone", "row {id} no longer exists"),
    m("engine.edit.row_needs_account", "row {row} needs an account"),
    m("engine.edit.row_needs_category", "row {row} needs a category"),
    m("engine.edit.row_needs_group", "row {row} needs a group"),
    m("engine.edit.row_not_in_account", "row {id} is not in account {code}"),
    m("engine.edit.rows_dont_use_column", "{what} don't use this column"),
    m("engine.edit.rows_gone", "those rows no longer exist"),
    m("engine.edit.separator_one_char", "the separator is one character, or none"),
    m("engine.edit.separators", "separators should be 1,234.56 or 1.234,56, not `{text}`"),
    m("engine.edit.set_gone", "set {name} no longer exists"),
    m("engine.edit.shared_cap_needs_amount", "the shared cap {name} is new, so it needs an amount"),
    m("engine.edit.shared_cap_needs_name", "a shared cap amount needs a shared cap name"),
    m("engine.edit.short_code_comma", "short codes can't contain commas"),
    m("engine.edit.short_code_is_name", "{code} is the name of another {what}"),
    m("engine.edit.short_code_long", "that short code is {count} characters; short codes can be at most {max}"),
    m("engine.edit.short_code_taken", "{code} is already the short code of the {what} {name}"),
    m("engine.edit.that_item_gone", "that {what} no longer exists"),
    m("engine.edit.too_large", "`{text}` is too large"),
    m("engine.edit.too_many_cells", "that's {count} cells; at most {max} can change at once"),
    m("engine.edit.topsheet_row_gone", "that topsheet row no longer exists"),
    m("engine.edit.totals_frozen", "the original totals are frozen; unfreeze them first"),
    m("engine.edit.totals_in_currency", "totals are shown in {code}; change that first"),
    m("engine.edit.totals_too_large", "the totals are too large to keep"),
    m("engine.edit.unit_gone", "unit {name} no longer exists"),
    m("engine.edit.unknown_charge_kind", "unknown charge kind `{kind}`"),
    m("engine.edit.unknown_field", "unknown field `{field}`"),
    m("engine.edit.unknown_field_kind", "unknown field kind `{kind}`"),
    m("engine.edit.unknown_flat_basis", "unknown flat fringe basis `{basis}`"),
    m("engine.edit.unknown_fringe_kind", "unknown fringe kind `{kind}`"),
    m("engine.edit.unknown_fringe_posting", "unknown fringe posting `{mode}`"),
    m("engine.edit.unknown_row_kind", "unknown row kind `{kind}`"),
    m("engine.explain.totals_unworkable", "the totals can't be worked out"),
    m("engine.explain.totals_unworkable_why", "the totals can't be worked out: {why}"),
    m("engine.find.account_numbers", "account numbers aren't replaced here; change them in the account list"),
    m("engine.find.cant_search", "can't search for `{scope}`"),
    m("engine.find.check_results", "check one or more results first"),
    m("engine.find.field_not_searched", "a {what} search doesn't look in `{field}`"),
    m("engine.find.global_unworkable", "global {name} can't be worked out ({why}), so its value can't take its place"),
    m("engine.find.no_global_add", "there is no global {name}; add it under Globals first"),
    m("engine.find.no_global_value", "there is no global {name}, so it has no value to put in its place"),
    m("engine.find.no_such_add", "there is no {what} `{name}`; add it in the tools panel first"),
    m("engine.find.not_a_formula", "`{text}` isn't a formula: {why}"),
    m("engine.find.not_a_global_name", "`{name}` isn't a global's name: names are letters, digits and _, with at least one letter or _"),
    m("engine.find.note_too_long", "a note would be {len} characters; notes can be at most {max}"),
    m("engine.find.nothing_there", "nothing to replace there"),
    m("engine.find.nothing_would_change", "nothing would change: the replacement is what is there already"),
    m("engine.find.pick_replacement", "pick the {what} to put in its place, or remove it"),
    m("engine.find.replaced_by_itself", "{name} would be replaced by itself"),
    m("engine.find.stale", "some of the checked results no longer match (the budget changed); search again"),
    m("engine.find.type_replacement", "type a global, a number or a formula to put in its place"),
    m("engine.find.type_what", "type what to find first"),
    m("engine.import.bad_pattern", "That numbering pattern can't be used: {why}."),
    m("engine.import.cant_read", "Couldn't read {file}: {why}."),
    m("engine.import.chart_applied", "Applied the chart: {changes}."),
    plural("engine.import.chart_kept", "Kept {count} account the chart doesn't list (they have lines).", "Kept {count} accounts the chart doesn't list (they have lines)."),
    m("engine.import.chart_matches", "The chart matches this budget: nothing to change."),
    m("engine.import.chart_new", "New budget with {categories} and {accounts}, and no lines."),
    m("engine.import.chart_unreadable", "The {name} chart template can't be read: {why}."),
    m("engine.import.column_past_end", "The {what} column is past the sheet's last column."),
    m("engine.import.default_account_long", "The default account number is too long."),
    m("engine.import.export_imported", "Imported {categories}, {accounts} and {what}: check the notes in the review before relying on it."),
    m("engine.import.export_no_categories", "The export has no categories to import."),
    m("engine.import.header_past_end", "The header row is past the end of the sheet."),
    m("engine.import.imported", "Imported {what} into {accounts}."),
    m("engine.import.imported_adding", "Imported {what} into {accounts} ({added})."),
    m("engine.import.imported_unassigned", "Imported {what} into {accounts} ({unassigned} unassigned)."),
    m("engine.import.new_currencies", "New currencies start at a rate of 1: {names}. Set their rates in Currencies."),
    m("engine.import.new_fringes", "New fringes start at 0%: {names}. Set their rates in Fringes."),
    m("engine.import.no_categories", "No categories were found."),
    m("engine.import.no_such_sheet", "That sheet isn't in the file."),
    m("engine.import.not_bundled", "That chart template isn't bundled with this version."),
    m("engine.import.not_loaded", "{name} hasn't loaded yet. Check the connection and pick it again."),
    m("engine.import.not_openable", "{file} isn't a budget, a budget export or a spreadsheet. Open reads Method Budget files (.mthd), budget exports (budget exchange JSON or XML, or the export's .xlsx spreadsheet or tab-delimited file) and spreadsheets (.xlsx, .csv, .tsv, .txt)."),
    m("engine.import.not_text", "Couldn't read {file}: it isn't text."),
    m("engine.import.numbers_dont_fit", "The numbers don't fit that pattern: {problems}."),
    m("engine.import.picked_since", "Another file was picked since. Import again."),
    plural("engine.import.skipped", "{count} row was skipped.", "{count} rows were skipped."),
    m("engine.import.template_unreadable", "The {name} template can't be read: {why}."),
    m("engine.import.too_large", "The file is too large to import (more than {max} MB)."),
    m("engine.import.unknown_layout", "Unknown export layout `{layout}`."),
    m("engine.library.added", "Added from the library: {changes}."),
    m("engine.library.folder_deleted", "Deleted the folder {name}."),
    m("engine.library.folder_deleted_item", "Deleted the folder {name}; its item is unfiled now."),
    plural("engine.library.folder_deleted_items", "Deleted the folder {name}; its {count} item is unfiled now.", "Deleted the folder {name}; its {count} items are unfiled now."),
    m("engine.library.folder_made", "Made the folder {name}."),
    m("engine.library.folder_renamed", "Renamed the folder {from} to {to}."),
    m("engine.library.in_tab", "Your library is open as the budget tab \u{201c}{name}\u{201d}. Make the change there, or close that tab and try again."),
    m("engine.library.not_kept", "A {kind} isn't kept in the library."),
    m("engine.library.nothing_to_add", "Nothing to add: this budget already has what was picked."),
    m("engine.library.nothing_to_copy", "Nothing to copy: that item never copies between budgets."),
    m("engine.library.nothing_to_save", "Nothing to save: the library already has what was picked."),
    m("engine.library.saved", "Saved to the library: {changes}."),
    m("engine.merge.budget_closed", "The budget has been closed."),
    m("engine.merge.choose_base", "Choose the base: the budget both copies started from (a version of this budget, or the file that was sent)."),
    m("engine.merge.choose_base_first", "Choose the base first."),
    m("engine.merge.file_gone", "That file is no longer at hand; choose it again."),
    m("engine.merge.from_itself", "A budget can't merge changes from itself."),
    m("engine.merge.front_not_base", "The budget in front can't be the base."),
    m("engine.merge.never_saved", "This budget hasn't been saved to a file yet."),
    m("engine.merge.not_a_conflict", "That choice isn't one of the conflicts."),
    m("engine.merge.not_a_side", "\"{side}\" isn't ours or theirs."),
    m("engine.merge.not_from_version", "This copy was saved from the budget as it stood, not from a version. Choose the base: the file that was sent."),
    m("engine.merge.other_budget", "This copy came from \"{title}\", not this budget. Choose the base: the budget both started from."),
    m("engine.merge.same_budget", "That is the budget being compared."),
    m("engine.merge.saved_file", "The saved file"),
    m("engine.merge.too_many_choices", "Too many choices."),
    m("engine.merge.version_deleted", "That version has been deleted."),
    m("engine.merge.version_gone", "The version \"{name}\" this copy was shared from is no longer in this budget. Choose the base."),
    m("engine.merge.version_label", "Version \"{name}\""),
    m("engine.merge.version_shared_label", "Version \"{name}\" (what was shared)"),
    m("engine.move.accounts_one_category", "accounts move within their own category: select accounts in one category"),
    m("engine.move.one_place", "rows move one place at a time (by -1 or 1)"),
    m("engine.move.rows_one_account", "rows move within their own account: select rows in one account"),
    m("engine.move.select_accounts", "select one or more accounts first"),
    m("engine.move.select_rows", "select one or more rows first"),
    m("engine.move.select_topsheet_rows", "select one or more topsheet rows first"),
    m("engine.name.before_restoring", "Before restoring \"{name}\""),
    m("engine.notes.author_too_long", "author names can be at most {max} characters"),
    m("engine.notes.gone", "that note no longer exists"),
    m("engine.notes.needs_text", "a note needs some text"),
    m("engine.notes.time_too_long", "that time is too long"),
    m("engine.notes.too_long", "that note is {len} characters; notes can be at most {max}"),
    m("engine.packs.imported", "Imported: {changes}."),
    m("engine.packs.imported_from", "Imported from {title}: {changes}."),
    m("engine.packs.no_settings", "A setup pack doesn't bring the budget setting {setting}."),
    m("engine.packs.nothing_budget", "Nothing to import: this budget already has what was picked."),
    m("engine.packs.nothing_library", "Nothing to import: the library already has what was picked."),
    plural("engine.packs.saved", "Saved the setup pack {name}: {count} item.", "Saved the setup pack {name}: {count} items."),
    m("engine.paste.accounts_into_category", "accounts are pasted into a category"),
    plural("engine.paste.base_currency_differs", "{count} line in {from}, the base currency where they were copied, will count in {to}, this budget's base currency.", "{count} lines in {from}, the base currency where they were copied, will count in {to}, this budget's base currency."),
    m("engine.paste.cant_copy", "can't copy `{what}`"),
    m("engine.paste.copied_since", "Something else was copied since. Paste again."),
    m("engine.paste.lines_into_account", "lines are pasted into an account"),
    m("engine.paste.no_account_number", "there is no free account number left in category {category}"),
    m("engine.paste.no_category_number", "there is no free category number left"),
    m("engine.paste.no_new_name", "a {what} can't be added under a new name"),
    m("engine.paste.no_number_after", "there is no free number left after {code}"),
    m("engine.paste.select_accounts", "select the accounts to copy first"),
    m("engine.paste.select_categories", "select the categories to copy first"),
    m("engine.paste.select_rows", "select the rows to copy first"),
    m("engine.paste.unknown_choice", "unknown choice `{choice}`"),
    plural("engine.phrase.accounts", "{count} account", "{count} accounts"),
    plural("engine.phrase.added_accounts", "added {count} account", "added {count} accounts"),
    plural("engine.phrase.added_categories", "added {count} category", "added {count} categories"),
    plural("engine.phrase.categories", "{count} category", "{count} categories"),
    plural("engine.phrase.comments", "{count} comment", "{count} comments"),
    plural("engine.phrase.items_added", "{count} item added", "{count} items added"),
    plural("engine.phrase.items_kept_budget", "{count} item kept as this budget had it", "{count} items kept as this budget had them"),
    plural("engine.phrase.items_kept_library", "{count} item kept as the library had it", "{count} items kept as the library had them"),
    plural("engine.phrase.items_replaced", "{count} item replaced", "{count} items replaced"),
    m("engine.phrase.line_at", "{code} line {line}"),
    plural("engine.phrase.lines", "{count} line", "{count} lines"),
    plural("engine.phrase.lines_in", "{count} line in {code}", "{count} lines in {code}"),
    plural("engine.phrase.new_accounts", "{count} new account", "{count} new accounts"),
    plural("engine.phrase.new_categories", "{count} new category", "{count} new categories"),
    plural("engine.phrase.new_setup_items", "{count} new setup item", "{count} new setup items"),
    plural("engine.phrase.removed_empty_accounts", "removed {count} empty account", "removed {count} empty accounts"),
    m("engine.phrase.renamed", "renamed {count}"),
    plural("engine.phrase.transactions", "{count} transaction", "{count} transactions"),
    m("engine.presets.block_empty", "That line block has no rows."),
    m("engine.presets.block_too_big", "A line block can hold at most {max} rows."),
    m("engine.presets.cant_move", "Can't move the {what} there."),
    m("engine.presets.cant_save", "That tag preset can't be saved: {why}"),
    m("engine.presets.category_gone", "That category no longer exists."),
    m("engine.presets.give_name", "Give the {what} a name."),
    m("engine.presets.gone", "That {what} no longer exists."),
    m("engine.presets.missing", "This budget has no {items}."),
    m("engine.presets.missing_many", "This budget has no {items}. Edit the preset, or add them first."),
    m("engine.presets.missing_one", "This budget has no {items}. Edit the preset, or add it first."),
    m("engine.presets.name_long", "A {what} name can be at most {max} characters."),
    m("engine.presets.name_taken", "There is already a {what} called \"{name}\"."),
    m("engine.presets.no_tags_to_save", "Only lines and adjustments have tags to save."),
    m("engine.presets.none_take_tags", "None of the selected rows can take tags."),
    m("engine.presets.select_lines", "Select one or more lines first."),
    m("engine.presets.select_lines_to_save", "Select the lines to save first."),
    m("engine.presets.too_many", "A budget can keep at most {max} {what}."),
    m("engine.presets.unknown_kind", "unknown kind of preset `{kind}`"),
    m("engine.presets.unknown_mode", "unknown way to apply a preset `{mode}`; use add or replace"),
    m("engine.rates.edited", "That line's rate was changed by hand since it was filled: pick a classification again to fill it."),
    m("engine.rates.line_gone", "That line is no longer in the budget."),
    m("engine.rates.no_sheets", "That file has no sheets to read."),
    m("engine.rates.no_unit", "The line has no unit: pick one for the rate."),
    m("engine.rates.not_a_number", "Row {row} ({classification}): the rate per {unit}, \u{201c}{value}\u{201d}, isn't a number."),
    m("engine.rates.not_filled", "That line's rate wasn't filled from a rate table."),
    m("engine.rates.nothing_changed", "No line's table says anything different now."),
    m("engine.rates.only_lines", "Only a line has a rate to fill."),
    m("engine.rates.sheet_gone", "That sheet is no longer in the file."),
    m("engine.rates.up_to_date", "That line already has its table's rate."),
    m("engine.reports.cant_read_layout", "That bid layout can't be read: {why}"),
    m("engine.reports.cant_save", "That report setup can't be saved: {why}"),
    m("engine.review.account_gone", "There is no account {code} in the budget now (deleted or renumbered)."),
    m("engine.review.already_reviewing", "This budget is open for review. Its comments are already showing."),
    m("engine.review.budget_no_notes", "The whole budget has no notes of its own, so it stays in the review file."),
    m("engine.review.cant_read", "{name} can't be read: {why}."),
    m("engine.review.category_gone", "There is no category {code} in the budget now (deleted or renumbered)."),
    m("engine.review.comment_gone", "That comment is no longer in the review."),
    m("engine.review.line_gone", "Its line is no longer in the budget."),
    m("engine.review.none_open", "No review file is open on this budget."),
    m("engine.review.not_a_place", "\"{text}\" is no account or category in the budget; write row:ID, account:NUMBER, category:CODE, fringe:NAME (or another setup kind) or budget"),
    m("engine.review.not_a_place_kind", "\"{kind}\" isn't something a comment can be on (row, account, category, budget, or a setup kind: global, fringe, group, unit, set, location, currency, charge, credit)"),
    m("engine.review.not_a_row_id", "\"{text}\" isn't a row id"),
    m("engine.review.not_in_review", "Comment {id} isn't in this review."),
    m("engine.review.own_comments", "A budget open for review can't take its own comments; open the review file on the author's budget."),
    m("engine.review.place_gone", "{place} isn't in the budget."),
    plural("engine.review.settled_already", "{count} was in the budget already.", "{count} were in the budget already."),
    m("engine.review.settled_both", "Merged {merged} as notes and dismissed {dismissed}."),
    plural("engine.review.settled_dismissed", "Dismissed {count} comment.", "Dismissed {count} comments."),
    plural("engine.review.settled_kept", "{count} comment stays open: its place is gone or has no notes.", "{count} comments stay open: their place is gone or has no notes."),
    plural("engine.review.settled_merged", "Merged {count} comment as notes.", "Merged {count} comments as notes."),
    m("engine.review.settled_nothing", "Nothing changed."),
    m("engine.review.setup_gone", "There is no {what} {name} in the budget now."),
    m("engine.review.setup_no_notes", "Setup items don't have notes, so it stays in the review file."),
    m("engine.review.shared_folder", "A budget shared through a folder can't be opened for review. Save a copy for sharing and review that."),
    m("engine.review.too_many", "A review file can have at most {max} comments."),
    m("engine.review.write_while_reviewing", "Comments are written while reviewing (File > Open for Review). Here you can merge or dismiss them."),
    m("engine.role.actuals", "Actuals and notes"),
    m("engine.role.admin", "Edits everything and sets what people may change"),
    m("engine.role.co_owner", "Co-owner: edits everything and sets what people may change, admins included"),
    m("engine.role.comment", "Notes only"),
    m("engine.role.department", "Edits {scope}"),
    m("engine.role.editor", "Edits everything"),
    m("engine.role.owner", "Owner: edits everything and sets what people may change"),
    m("engine.role.read_only", "Read only"),
    m("engine.role.removed", "Removed: reads only, and later changes are left out"),
    m("engine.session.held_read_only", "This budget is open read-only here, so it can't be changed."),
    m("engine.session.read_only", "This is a copy saved for sharing, so it opens read-only. Make an editable copy to change it."),
    m("engine.session.reviewing", "This budget is open for review, so it can't be changed. Comments go in the review file beside it."),
    m("engine.session.shared_stays", "A shared budget stays in its shared folder. To give someone a copy, use File > Save a Copy for Sharing."),
    m("engine.session.shared_versions_now", "While a budget is shared, versions are saved from the budget as it is now."),
    m("engine.session.step_gone", "That step is no longer in the history."),
    m("engine.setup.cant_move_further", "{name} can't move further"),
    m("engine.setup.unknown_kind", "unknown kind of setup item `{kind}`"),
    m("engine.undo.account_numbering", "Change account numbering"),
    m("engine.undo.account_total_rounding", "Change account total rounding"),
    m("engine.undo.add_account", "Add account {code} {name}"),
    m("engine.undo.add_account_unnamed", "Add account {code}"),
    m("engine.undo.add_category", "Add category {code} {name}"),
    m("engine.undo.add_category_unnamed", "Add category {code}"),
    m("engine.undo.add_charge", "Add a charge"),
    m("engine.undo.add_credit", "Add a credit"),
    m("engine.undo.add_currency", "Add currency {code}"),
    m("engine.undo.add_fringe", "Add fringe {name}"),
    m("engine.undo.add_fringe_to_rows", "Add fringe {name} to {rows}"),
    m("engine.undo.add_from_library", "Add from library"),
    m("engine.undo.add_global", "Add global {name} = {formula}"),
    m("engine.undo.add_group", "Add group {name}"),
    m("engine.undo.add_group_to_rows", "Add group {name} to {rows}"),
    m("engine.undo.add_location", "Add location {name}"),
    m("engine.undo.add_note", "Add a note"),
    m("engine.undo.add_production_total", "Add production total {name}"),
    m("engine.undo.add_set", "Add set {name}"),
    m("engine.undo.add_tag_to_rows", "Add tag {name} to {rows}"),
    m("engine.undo.add_unit", "Add unit {name}"),
    m("engine.undo.apply_chart", "Apply a chart of accounts"),
    m("engine.undo.apply_tag_preset", "Apply a tag preset"),
    m("engine.undo.assign_rows", "Set {what} {value} on {rows}"),
    m("engine.undo.cancel_invite", "Cancelled an invite: {role}"),
    m("engine.undo.change", "Change"),
    m("engine.undo.check_note", "Check off a note"),
    m("engine.undo.clear_baseline", "Clear the baseline"),
    m("engine.undo.clear_cell", "Clear {what}"),
    m("engine.undo.clear_cell_at", "Clear {what} on {code} line {line}"),
    plural("engine.undo.clear_cells", "Clear {count} cell", "Clear {count} cells"),
    plural("engine.undo.clear_cells_in", "Clear {count} cell in {code}", "Clear {count} cells in {code}"),
    m("engine.undo.clear_fringe_range", "Remove the fringe range from {rows}"),
    m("engine.undo.clear_rows", "Clear {what} on {rows}"),
    m("engine.undo.color_group", "Color group {name}"),
    plural("engine.undo.cut_accounts", "Cut {count} account", "Cut {count} accounts"),
    plural("engine.undo.cut_categories", "Cut {count} category", "Cut {count} categories"),
    m("engine.undo.cut_cell", "Cut {what}"),
    m("engine.undo.cut_cell_at", "Cut {what} on {code} line {line}"),
    plural("engine.undo.cut_cells", "Cut {count} cell", "Cut {count} cells"),
    plural("engine.undo.cut_cells_in", "Cut {count} cell in {code}", "Cut {count} cells in {code}"),
    plural("engine.undo.cut_rows", "Cut {count} row", "Cut {count} rows"),
    m("engine.undo.cutoff_factor", "Change the cutoff factor"),
    m("engine.undo.delete_a_version", "Delete a version"),
    m("engine.undo.delete_account", "Delete account {code}"),
    plural("engine.undo.delete_actuals", "Delete {count} actual", "Delete {count} actuals"),
    m("engine.undo.delete_category", "Delete category {code}"),
    m("engine.undo.delete_charge", "Delete a charge"),
    m("engine.undo.delete_credit", "Delete a credit"),
    m("engine.undo.delete_currency", "Delete currency {code}"),
    m("engine.undo.delete_fringe", "Delete fringe {name}"),
    m("engine.undo.delete_global", "Delete global {name}"),
    m("engine.undo.delete_group", "Delete group {name}"),
    m("engine.undo.delete_info", "Delete budget info {name}"),
    m("engine.undo.delete_line_at", "Delete line {line} of {code}"),
    plural("engine.undo.delete_lines", "Delete {count} line", "Delete {count} lines"),
    plural("engine.undo.delete_lines_from", "Delete {count} line from {code}", "Delete {count} lines from {code}"),
    m("engine.undo.delete_location", "Delete location {name}"),
    m("engine.undo.delete_note", "Delete a note"),
    m("engine.undo.delete_overtime_pattern", "Delete overtime pattern {name}"),
    m("engine.undo.delete_preset", "Delete a preset"),
    m("engine.undo.delete_production_total", "Delete production total {name}"),
    m("engine.undo.delete_rate_table", "Delete rate table {name}"),
    m("engine.undo.delete_report_preset", "Delete a report preset"),
    m("engine.undo.delete_set", "Delete set {name}"),
    m("engine.undo.delete_topsheet_row", "Delete a topsheet row"),
    m("engine.undo.delete_unit", "Delete unit {name}"),
    m("engine.undo.delete_version", "Delete version {name}"),
    plural("engine.undo.edit_actuals", "Edit {count} actual", "Edit {count} actuals"),
    m("engine.undo.edit_category", "Edit category {code}"),
    m("engine.undo.edit_cell", "Edit {what}"),
    m("engine.undo.edit_cell_at", "Edit {what} on {code} line {line}"),
    plural("engine.undo.edit_cells", "Edit {count} cell", "Edit {count} cells"),
    plural("engine.undo.edit_cells_in", "Edit {count} cell in {code}", "Edit {count} cells in {code}"),
    m("engine.undo.edit_charge", "Edit a charge"),
    m("engine.undo.edit_credit", "Edit a credit"),
    m("engine.undo.edit_currency", "Edit currency {code}"),
    m("engine.undo.edit_fringe", "Edit fringe {name}"),
    m("engine.undo.edit_global_details", "Edit the details of global {name}"),
    m("engine.undo.edit_group", "Edit group {name}"),
    m("engine.undo.edit_info", "Edit budget info {name}"),
    m("engine.undo.edit_location", "Edit location {name}"),
    m("engine.undo.edit_note", "Edit a note"),
    m("engine.undo.edit_production_total", "Edit production total {name}"),
    m("engine.undo.edit_set", "Edit set {name}"),
    m("engine.undo.edit_topsheet_row", "Edit a topsheet row"),
    m("engine.undo.edit_unit", "Edit unit {name}"),
    m("engine.undo.fill_cell", "Fill down {what}"),
    m("engine.undo.fill_cell_at", "Fill down {what} on {code} line {line}"),
    plural("engine.undo.fill_cells", "Fill down {count} cell", "Fill down {count} cells"),
    plural("engine.undo.fill_cells_in", "Fill down {count} cell in {code}", "Fill down {count} cells in {code}"),
    m("engine.undo.fill_rate", "Fill rate from {table}"),
    m("engine.undo.fill_rate_at", "Fill rate on {code} line {line} from {table}"),
    m("engine.undo.format_lines", "Format lines"),
    m("engine.undo.freeze_totals", "Freeze the original totals"),
    m("engine.undo.fringe_posting", "Change fringe posting"),
    m("engine.undo.fringe_row", "Change the fringe row"),
    m("engine.undo.group_rule", "Change a group rule"),
    m("engine.undo.import_actuals", "Import actuals from {file}"),
    m("engine.undo.import_chart", "Import a chart of accounts"),
    m("engine.undo.import_export", "Import a budget export"),
    m("engine.undo.import_lines", "Import lines from a spreadsheet"),
    plural("engine.undo.import_setup_items", "Import {count} item of setup", "Import {count} items of setup"),
    m("engine.undo.import_setup_pack", "Import setup pack"),
    m("engine.undo.insert_adjustment", "Insert adjustment in {code}"),
    m("engine.undo.insert_line", "Insert line in {code}"),
    m("engine.undo.insert_line_block", "Insert a line block in {code}"),
    m("engine.undo.insert_separator", "Insert separator in {code}"),
    m("engine.undo.insert_subtotal", "Insert subtotal in {code}"),
    m("engine.undo.invite", "Invited someone: {role}"),
    m("engine.undo.join_from_invite", "Joined from an invite: {role}"),
    m("engine.undo.load_file_version", "Load the file's version"),
    m("engine.undo.lock_baseline", "Lock the baseline"),
    m("engine.undo.make_fringe_range", "Make a fringe range of {rows}"),
    m("engine.undo.merge_changes", "Merge changes from {source}"),
    plural("engine.undo.merge_review_comments", "Merge {count} review comment as notes", "Merge {count} review comments as notes"),
    plural("engine.undo.move_accounts_down", "Move {count} account down", "Move {count} accounts down"),
    plural("engine.undo.move_accounts_up", "Move {count} account up", "Move {count} accounts up"),
    plural("engine.undo.move_actuals", "Move {count} actual to {code}", "Move {count} actuals to {code}"),
    m("engine.undo.move_charge", "Move a charge"),
    m("engine.undo.move_credit", "Move a credit"),
    m("engine.undo.move_item_down", "Move {what} {name} down"),
    m("engine.undo.move_item_up", "Move {what} {name} up"),
    plural("engine.undo.move_lines_down", "Move {count} line down", "Move {count} lines down"),
    plural("engine.undo.move_lines_up", "Move {count} line up", "Move {count} lines up"),
    m("engine.undo.move_preset", "Move a preset"),
    m("engine.undo.move_rows_to", "Move {rows} to {place}"),
    m("engine.undo.move_rows_to_no_location", "Move {rows} to no location"),
    plural("engine.undo.move_topsheet_rows_down", "Move {count} topsheet row down", "Move {count} topsheet rows down"),
    plural("engine.undo.move_topsheet_rows_up", "Move {count} topsheet row up", "Move {count} topsheet rows up"),
    m("engine.undo.new_budget", "New budget"),
    m("engine.undo.new_line_defaults", "Change new line defaults"),
    m("engine.undo.numbers_on_screen", "Change numbers on screen"),
    m("engine.undo.overtime", "Overtime"),
    m("engine.undo.overtime_at", "Overtime on {code} line {line}"),
    plural("engine.undo.paste_accounts", "Paste {count} account", "Paste {count} accounts"),
    plural("engine.undo.paste_categories", "Paste {count} category", "Paste {count} categories"),
    m("engine.undo.paste_cell", "Paste {what}"),
    m("engine.undo.paste_cell_at", "Paste {what} on {code} line {line}"),
    plural("engine.undo.paste_cells", "Paste {count} cell", "Paste {count} cells"),
    plural("engine.undo.paste_cells_in", "Paste {count} cell in {code}", "Paste {count} cells in {code}"),
    plural("engine.undo.paste_rows", "Paste {count} row", "Paste {count} rows"),
    m("engine.undo.put_back_field", "Put back {field} on {place}"),
    m("engine.undo.put_back_field_budget", "Put back {field} on the budget"),
    m("engine.undo.put_back_value", "Put back a value"),
    m("engine.undo.redo", "Redo"),
    m("engine.undo.remove_actuals_import", "Remove the import of {file}"),
    m("engine.undo.remove_an_actuals_import", "Remove an import of actuals"),
    m("engine.undo.remove_fringe_from_rows", "Remove fringe {name} from {rows}"),
    m("engine.undo.remove_group_from_rows", "Remove group {name} from {rows}"),
    m("engine.undo.remove_person", "Removed {name}"),
    m("engine.undo.remove_tag_from_rows", "Remove tag {name} from {rows}"),
    m("engine.undo.rename_account", "Rename account {code}"),
    m("engine.undo.rename_budget", "Rename budget to {title}"),
    m("engine.undo.rename_item", "Rename {what} {from} to {to}"),
    m("engine.undo.rename_preset", "Rename a preset to {name}"),
    m("engine.undo.rename_version", "Rename a version to {name}"),
    m("engine.undo.renumber_account", "Renumber account {code} to {new}"),
    m("engine.undo.renumber_accounts", "Renumber accounts to the new pattern"),
    m("engine.undo.reorder", "Reorder {what}"),
    plural("engine.undo.replace", "Replace {count} match with {text}", "Replace {count} matches with {text}"),
    plural("engine.undo.replace_with_nothing", "Replace {count} match with nothing", "Replace {count} matches with nothing"),
    m("engine.undo.restore_a_version", "Restore a version"),
    m("engine.undo.restore_version", "Restore version {name}"),
    m("engine.undo.revert_to_backup", "Revert to a backup"),
    m("engine.undo.rounding", "Change rounding"),
    m("engine.undo.save_a_tag_preset", "Save a tag preset"),
    m("engine.undo.save_line_block", "Save line block {name}"),
    m("engine.undo.save_overtime_pattern", "Save overtime pattern {name}"),
    m("engine.undo.save_rate_table", "Save rate table {name}"),
    m("engine.undo.save_report_preset", "Save a report preset"),
    m("engine.undo.save_tag_preset", "Save tag preset {name}"),
    m("engine.undo.save_version", "Save version {name}"),
    m("engine.undo.save_version_from_history", "Save version \u{201c}{name}\u{201d} from the history"),
    m("engine.undo.schedule_days", "Set globals from the schedule"),
    m("engine.undo.schedule_days_named", "Set globals from the schedule {name}"),
    m("engine.undo.set_etc", "Set the estimate to complete of {account}"),
    m("engine.undo.set_field_rows", "Set {what} on {rows}"),
    m("engine.undo.set_global", "Set global {name} = {formula}"),
    m("engine.undo.set_role", "{name}: {role}"),
    m("engine.undo.set_tags_rows", "Set the tags on {rows}"),
    m("engine.undo.short_code", "Set the short code of {name}"),
    m("engine.undo.store_returned_copy", "Store a returned copy as version \"{name}\""),
    m("engine.undo.totals_currency", "Change the totals currency"),
    m("engine.undo.trailing_fringes", "Change fringes after the last production total"),
    plural("engine.undo.unassign_actuals", "Unassign {count} actual", "Unassign {count} actuals"),
    m("engine.undo.uncheck_note", "Uncheck a note"),
    m("engine.undo.undo", "Undo"),
    m("engine.undo.unfreeze_totals", "Unfreeze the original totals"),
    m("engine.undo.update_a_tag_preset", "Update a tag preset"),
    m("engine.undo.update_rate", "Update rate to the table"),
    m("engine.undo.update_rates", "Update {count} rates to their tables"),
    m("engine.undo.update_report_preset", "Update a report preset"),
    m("engine.versions.name_one_line", "a version name is one line, with no tabs"),
    m("engine.versions.name_too_long", "that name is {len} characters; version names can be at most {max}"),
    m("engine.versions.needs_name", "a version needs a name"),
    m("engine.what.a_cell", "a cell"),
    m("engine.what.account", "account"),
    m("engine.what.adjustment_rows", "adjustment rows"),
    m("engine.what.amount", "amount"),
    m("engine.what.amt", "Amt"),
    m("engine.what.an_excluded_amount", "an excluded amount"),
    m("engine.what.category", "category"),
    m("engine.what.charge", "charge"),
    m("engine.what.column_account", "Account number"),
    m("engine.what.column_amount", "Amount"),
    m("engine.what.column_credit", "Credit"),
    m("engine.what.column_date", "Date"),
    m("engine.what.column_debit", "Debit"),
    m("engine.what.column_description", "Description"),
    m("engine.what.column_reference", "Reference"),
    m("engine.what.column_tags", "Tags"),
    m("engine.what.column_vendor", "Vendor"),
    m("engine.what.credit", "credit"),
    m("engine.what.currency", "currency"),
    m("engine.what.description", "description"),
    m("engine.what.formula", "formula"),
    m("engine.what.fringe", "fringe"),
    m("engine.what.fringes", "fringes"),
    m("engine.what.global", "global"),
    m("engine.what.group", "group"),
    m("engine.what.groups", "groups"),
    m("engine.what.hours", "hours"),
    m("engine.what.item", "item"),
    m("engine.what.line_block", "line block"),
    m("engine.what.line_blocks", "line blocks"),
    m("engine.what.lines", "lines"),
    m("engine.what.location", "location"),
    m("engine.what.max_hours_per_day", "max hours per day"),
    m("engine.what.note", "note"),
    m("engine.what.rate", "rate"),
    m("engine.what.rate_column", "Rate"),
    m("engine.what.report_preset", "report preset"),
    m("engine.what.report_presets", "report presets"),
    m("engine.what.row_amount", "row {row}'s amount"),
    m("engine.what.row_cap", "row {row}'s cap"),
    m("engine.what.row_percent", "row {row}'s percent"),
    m("engine.what.separator_rows", "separator rows"),
    m("engine.what.set", "set"),
    m("engine.what.sub_account", "sub-account"),
    m("engine.what.subtotal_rows", "subtotal rows"),
    m("engine.what.tag", "tag"),
    m("engine.what.tag_preset", "tag preset"),
    m("engine.what.tag_presets", "tag presets"),
    m("engine.what.the_amount", "the amount"),
    m("engine.what.the_brokers_fee", "the broker's fee"),
    m("engine.what.the_cap", "the cap"),
    m("engine.what.the_discount", "the discount"),
    m("engine.what.the_minimum_spend", "the minimum spend"),
    m("engine.what.the_percent", "the percent"),
    m("engine.what.the_shared_cap", "the shared cap"),
    m("engine.what.this_cell", "This cell"),
    m("engine.what.unit", "unit"),
    m("engine.what.x", "X"),
    m("engine.what.x4", "4X"),
    m("engine.working.a_flat_fee", "a flat fee"),
    m("engine.working.account_fringes", "fringes earned by the account's lines"),
    m("engine.working.account_total", "account total"),
    m("engine.working.adjustment_base", "the lines above, back to the previous adjustment (subtotal rows don't restart it)"),
    m("engine.working.adjustment_base_doubles", "the lines above, back to the previous adjustment (subtotal rows don't restart it), added up in doubles"),
    m("engine.working.after_last_level_neither", "categories after the last production total are neither above nor below the line"),
    m("engine.working.applied_credit", "an applied credit"),
    m("engine.working.at_least_minimum", "the rows' net spend {spend} is at least the minimum of {minimum}"),
    m("engine.working.at_most", "at most {cap}: {capped}"),
    m("engine.working.atl_btl_is", "above-the-line plus below-the-line"),
    m("engine.working.atl_is", "the first production level, when a production total closes it"),
    m("engine.working.below_minimum", "the rows' net spend {spend} is below the minimum of {minimum}, so the credit is 0"),
    m("engine.working.blank_counts_as_0", "blank, counts as 0"),
    m("engine.working.btl_is", "every later production level a production total closes"),
    m("engine.working.cap_on_spend", "the spend the percent applies to is at most {cap}: {spend}"),
    m("engine.working.category_fringe_account", "the category's fringe account (fringe posting: by category)"),
    m("engine.working.category_fringes", "fringes earned in the category"),
    m("engine.working.category_total", "category total"),
    m("engine.working.charge_base", "every category with its fringes, wherever they post, plus the budget's fringe row: the grand total before charges (charges never apply to each other)"),
    m("engine.working.charge_percent_of", "{percent}% of ({base} - {excluded}) = {exact}"),
    m("engine.working.counted_lines_tagged", "counted lines tagged {group}, with their fringes"),
    m("engine.working.currency", "{amount} {currency} x {rate} {base} per {currency} = {exact} {base}"),
    m("engine.working.currency_unrounded", "{amount} {currency} (not rounded yet) x {rate} {base} per {currency} = {exact} {base}"),
    m("engine.working.cutoff_factor", "the budget multiplies every percent cutoff by {factor}: {typed} x {factor} = {cutoff}"),
    m("engine.working.cutoff_used_by_lines_above", "{fringe} stops at {cutoff} per earner, and the lines above in the range used it all, so this line takes nothing"),
    m("engine.working.discount", "less {percent}%: {capped} x {keep} = {exact}"),
    m("engine.working.earns_none", "this line earns none"),
    m("engine.working.earns_nothing_from_it", "this line earns nothing from it"),
    m("engine.working.every_applied_credit", "every applied credit"),
    m("engine.working.every_line_of", "every line of {account}"),
    m("engine.working.fee_converted", "{amount} {currency} x {rate} {base} per {currency} = {exact}"),
    m("engine.working.fee_less", "less {fee}: {amount}"),
    m("engine.working.fee_less_floor", "less {fee}, but never below zero: {taken} taken off, {amount}"),
    m("engine.working.flat_cutoff_unused", "{cutoff} is kept on {fringe} but not used: cutoffs apply to percent fringes only"),
    m("engine.working.flat_once_per_line", "flat, once per line"),
    m("engine.working.flat_per_unit_amount", "flat, per {unit}, amount only"),
    m("engine.working.flat_per_unit_counts", "flat, per {unit}, amount plus counts in X and 4X"),
    m("engine.working.flat_per_unit_quantity", "flat, per {unit}, full quantity"),
    m("engine.working.fringe_kind_flat", "a flat fringe, over every counted line"),
    m("engine.working.fringe_kind_percent", "a percent fringe, no cutoff, over every counted line"),
    m("engine.working.fringe_kind_percent_cutoff", "a percent fringe with a {cutoff} cutoff per earner, over every counted line"),
    m("engine.working.fringe_kind_percent_factor", "a percent fringe with a {cutoff} cutoff per earner ({typed} x the budget's cutoff factor {factor}), over every counted line"),
    m("engine.working.fringe_on_account_lines", "{fringe} on the account's lines"),
    m("engine.working.fringe_range", "range {range}: lines {lines} earn as one person, so they share one cutoff, used up in line order"),
    m("engine.working.fringes_by_line", "fringes earned by line {line}"),
    m("engine.working.fringes_on_top", "earned on top of this total, shown in Fringe $ (not in it)"),
    m("engine.working.grand_total_is", "above and below-the-line, the categories after the last production total, budget fringes and charges"),
    m("engine.working.gross_account", "account {account} with its fringes, wherever fringes post"),
    m("engine.working.gross_category", "category {category} with its fringes, wherever fringes post"),
    m("engine.working.gross_no_account", "account {account} is not in the budget, so 0"),
    m("engine.working.gross_no_category", "category {category} is not in the budget, so 0"),
    m("engine.working.label.adjustment", "Adjustment"),
    m("engine.working.label.after_last_level", "Categories after the last production total"),
    m("engine.working.label.amount", "Amount"),
    m("engine.working.label.atl", "Total above-the-line"),
    m("engine.working.label.atl_btl", "Total above and below-the-line"),
    m("engine.working.label.base", "Base"),
    m("engine.working.label.brokers_fee", "Broker's fee"),
    m("engine.working.label.btl", "Total below-the-line"),
    m("engine.working.label.cap_on_spend", "Cap on spend"),
    m("engine.working.label.charge", "Charge"),
    m("engine.working.label.count", "Count"),
    m("engine.working.label.credit", "Credit"),
    m("engine.working.label.credit_cap", "Credit cap"),
    m("engine.working.label.credits", "Credits"),
    m("engine.working.label.currency", "Currency"),
    m("engine.working.label.cutoff", "Cutoff"),
    m("engine.working.label.cutoff_factor", "Cutoff factor"),
    m("engine.working.label.cutoff_reached", "Cutoff reached"),
    m("engine.working.label.cutoff_used_up", "Cutoff used up"),
    m("engine.working.label.discount", "Discount"),
    m("engine.working.label.earnings", "Earnings"),
    m("engine.working.label.excluded", "Excluded"),
    m("engine.working.label.excluded_amount", "Excluded amount"),
    m("engine.working.label.excluded_fringes", "Excluded fringes"),
    m("engine.working.label.fee", "Fee"),
    m("engine.working.label.fringe", "Fringe"),
    m("engine.working.label.fringe_range", "Fringe range"),
    m("engine.working.label.fringes", "Fringes"),
    m("engine.working.label.fringes_earned_here", "Fringes earned here"),
    m("engine.working.label.grand_total", "Grand total"),
    m("engine.working.label.gross", "Gross"),
    m("engine.working.label.left_out", "Left out"),
    m("engine.working.label.less_credits", "Less credits"),
    m("engine.working.label.line", "Line"),
    m("engine.working.label.line_n", "Line {line}"),
    m("engine.working.label.lines", "Lines"),
    m("engine.working.label.locations", "Locations"),
    m("engine.working.label.minimum_spend", "Minimum spend"),
    m("engine.working.label.net_spend", "Net spend"),
    m("engine.working.label.net_total", "Net total"),
    m("engine.working.label.no_fringes", "No fringes"),
    m("engine.working.label.no_production_totals", "No production totals"),
    m("engine.working.label.not_counted_here", "Not counted here"),
    m("engine.working.label.not_rounded_yet", "Not rounded yet"),
    m("engine.working.label.numbered_fringes", "{number} Fringes"),
    m("engine.working.label.person", "Person"),
    m("engine.working.label.quantity", "Quantity"),
    m("engine.working.label.rate", "Rate"),
    m("engine.working.label.rate_source", "Rate source"),
    m("engine.working.label.rounding", "Rounding"),
    m("engine.working.label.row_n", "Row {row}"),
    m("engine.working.label.rows_added", "Rows added"),
    m("engine.working.label.share", "Share"),
    m("engine.working.label.shared_cap", "Shared cap"),
    m("engine.working.label.subtotal", "Subtotal"),
    m("engine.working.label.total_fringes", "Total fringes"),
    m("engine.working.label.where_fringes_post", "Where fringes post"),
    m("engine.working.label.x", "X"),
    m("engine.working.label.x4", "4X"),
    m("engine.working.left_out_adjustment_ungrouped", "the adjustment row has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    m("engine.working.left_out_by_report", "the report leaves out group {groups}; counted, it would be {total}"),
    m("engine.working.left_out_group_off", "group {groups} has In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    m("engine.working.left_out_group_off_conflict", "group {groups} has In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    m("engine.working.left_out_groups_off", "groups {groups} have In budget total off, so the line shows but counts as 0; counted, it would be {total}"),
    m("engine.working.left_out_groups_off_conflict", "groups {groups} have In budget total off (it is also in a counted group, and the group rule leaves out such conflicts), so the line shows but counts as 0; counted, it would be {total}"),
    m("engine.working.left_out_no_fringes", "the line doesn't count, so it earns no fringes"),
    m("engine.working.left_out_not_shown", "the report shows only some groups, and this line has none of them; counted, it would be {total}"),
    m("engine.working.left_out_ungrouped", "the line has no group and the group rule leaves out lines with no group, so it shows but counts as 0; counted, it would be {total}"),
    m("engine.working.level_adds", "the categories since the previous production total"),
    m("engine.working.lines_tagged", "lines tagged {group} plus their fringes"),
    m("engine.working.net_total_is", "grand total less credits"),
    m("engine.working.no_production_totals", "nothing is above or below the line until a production total closes the first level"),
    m("engine.working.not_in_total_account", "not in this total: fringes post to each account's total"),
    m("engine.working.not_in_total_budget", "not in this total: fringes post to the budget's one Total Fringes row"),
    m("engine.working.not_in_total_category", "not in this total: fringes post to each category's fringe account"),
    m("engine.working.not_in_total_level", "not in this total: fringes post above each production total"),
    m("engine.working.only_lines_at", "only lines at {locations} count, with their fringes"),
    m("engine.working.per_earner_left_not_reached", "{cutoff} per earner; {left} of it left after the lines above in the range, not reached"),
    m("engine.working.per_earner_not_reached", "{cutoff} per earner, not reached"),
    m("engine.working.per_person_not_reached", "{cutoff} per person, not reached: {person} is paid {pay} with {fringe}, so each line earns its own percent"),
    m("engine.working.percent_of", "{percent}% of {base} = {exact}"),
    m("engine.working.percent_of_budget", "{percent}% of the budget"),
    m("engine.working.percent_of_capped", "{percent}% of {base} = {exact}, capped at {cap}: {credit}"),
    m("engine.working.percent_of_rate", "{percent}% of {base} = {exact} (rate {formula})"),
    m("engine.working.percent_with_cutoff", "{percent}% with a {cutoff} cutoff"),
    m("engine.working.percent_with_cutoff_per_person", "{percent}% with a {cutoff} cutoff per person"),
    plural("engine.working.person_paid_account", "{person} is paid on {count} line in account {accounts}, so they share one cutoff", "{person} is paid on {count} lines in account {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_account_no_cutoff", "{person} is paid on {count} line in account {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in account {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    plural("engine.working.person_paid_accounts", "{person} is paid on {count} line in accounts {accounts}, so they share one cutoff", "{person} is paid on {count} lines in accounts {accounts}, so they share one cutoff"),
    plural("engine.working.person_paid_accounts_no_cutoff", "{person} is paid on {count} line in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent", "{person} is paid on {count} lines in accounts {accounts}; {fringe} has no cutoff, so each line earns its own percent"),
    m("engine.working.person_past_cutoff", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact}"),
    m("engine.working.person_past_cutoff_rate", "{person} is paid {pay} with {fringe}, past the {cutoff} cutoff, so {person} earns {percent}% of {cutoff} = {exact} (rate {formula})"),
    m("engine.working.posted_above_level", "posted above the production total (fringe posting: by production level)"),
    m("engine.working.posted_to_account", "posted to the account (fringe posting: by account)"),
    m("engine.working.posting_account", "fringes post to each account's total (a budget setting)"),
    m("engine.working.posting_budget", "fringes post to the budget's one Total Fringes row (a budget setting)"),
    m("engine.working.posting_category", "fringes post to each category's fringe account (a budget setting)"),
    m("engine.working.posting_level", "fringes post above each production total (a budget setting)"),
    m("engine.working.rounded_once", "rows are added, capped and discounted before the one rounding"),
    m("engine.working.rounds_double", "{amount} rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    m("engine.working.rounds_double_held", "{amount} is {held} as a binary double, so it rounds to {rounded} (spreadsheet rounding: half up on the binary value)"),
    m("engine.working.rounds_exact", "{amount} rounds half away from zero to {rounded}"),
    m("engine.working.row_adjustment", "{code} line {line}, adjustment"),
    m("engine.working.row_line", "{code} line {line}, line"),
    m("engine.working.row_line_left_out", "{code} line {line}, line, left out"),
    m("engine.working.share", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down"),
    m("engine.working.share_extra_cent", "this line's part of {person}'s {amount}, by pay: {amount} x {total} / {pay}, {floor} rounded down, plus one of the cents left over (shares rounded down; the left-over cents go to the largest remainders, then the lowest row id)"),
    m("engine.working.shared_cap_missing", "{name} is not in the budget's shared caps, so it holds nothing down"),
    m("engine.working.shared_cap_share", "{name} caps this credit and {others} at {cap} together; its share, in proportion to {capped}, is {share}"),
    m("engine.working.shared_cap_within", "{name} caps the credits sharing it at {cap} together, which they are within"),
    m("engine.working.stops_per_earner", "{fringe} stops at {cutoff} per earner"),
    m("engine.working.stops_per_earner_left", "{fringe} stops at {cutoff} per earner; {left} of it left after the lines above in the range"),
    m("engine.working.stops_per_person", "{fringe} stops at {cutoff} per person, so nothing is earned"),
    m("engine.working.subtotal_adds", "the lines and adjustments since the previous subtotal row"),
    m("engine.working.taken_off_base", "taken off the base"),
    m("engine.working.taken_off_fee", "taken off the fee (typed amounts, and the lines of excluded groups, accounts and categories with their fringes)"),
    m("engine.working.this_lines_total", "this line's total"),
    m("engine.working.title.account_fringes", "Account {account}: fringes"),
    m("engine.working.title.account_lines", "Account {account}: lines"),
    m("engine.working.title.account_total", "Account {account}: total"),
    m("engine.working.title.after_last_level_fringes", "Categories after the last production total: fringes"),
    m("engine.working.title.after_last_level_total", "Categories after the last production total: total"),
    m("engine.working.title.category_fringes", "Category {category}: fringes"),
    m("engine.working.title.category_total", "Category {category}: total"),
    m("engine.working.title.charge", "Charge {name}"),
    m("engine.working.title.credit", "Credit {name}"),
    m("engine.working.title.fringe_total", "Fringe {fringe}: total"),
    m("engine.working.title.level_fringes", "{level}: fringes"),
    m("engine.working.title.level_total", "{level}: total"),
    m("engine.working.title.line_described_fringe", "{code} line {line} ({description}): {fringe}"),
    m("engine.working.title.line_described_fringes", "{code} line {line} ({description}): fringes"),
    m("engine.working.title.line_described_total", "{code} line {line} ({description}): total"),
    m("engine.working.title.line_fringe", "{code} line {line}: {fringe}"),
    m("engine.working.title.line_fringes", "{code} line {line}: fringes"),
    m("engine.working.title.line_total", "{code} line {line}: total"),
    m("engine.working.total_fringes_is", "every fringe in the budget, outside above and below-the-line (fringe posting: by budget)"),
    m("engine.working.trailing_fringes_dropped", "no production total follows these categories, so their fringes post nowhere and count in no total (trailing level fringes: drop)"),
    m("engine.working.typed_in", "typed in"),
    m("engine.working.whole_units_half_away", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half away from zero to {whole}"),
    m("engine.working.whole_units_half_up", "the budget rounds account totals to whole units: the lines come to {lines}, which rounds to the nearest whole unit, a half up to {whole}"),
];

/// The message with this code.
pub fn find(code: &str) -> Option<&'static Message> {
    MESSAGES
        .binary_search_by(|m| m.code.cmp(code))
        .ok()
        .map(|i| &MESSAGES[i])
}

/// `code`'s English with `params` filled in: the form for one when its
/// `count` is 1. An unknown code gives the code itself (the tests keep
/// that from happening).
pub fn msg(code: &str, params: &[(&str, &dyn Display)]) -> String {
    let Some(m) = find(code) else {
        debug_assert!(false, "{code} is not in method_session::messages::MESSAGES");
        return code.to_string();
    };
    let params: Vec<(&str, String)> = params.iter().map(|(n, v)| (*n, v.to_string())).collect();
    let count_is_one = params.iter().any(|(n, v)| *n == "count" && v == "1");
    let english = match m.one {
        Some(one) if count_is_one => one,
        _ => m.other,
    };
    fill(english, &params)
}

/// A noun or other message with no parameters, as text.
pub fn word(code: &str) -> String {
    msg(code, &[])
}

/// `template` with each `{name}` replaced by its value, in one pass (a
/// value that holds `{other}` stays as it is). A name with no value stays
/// written as `{name}`.
pub fn fill<V: AsRef<str>>(template: &str, params: &[(&str, V)]) -> String {
    let mut out = String::with_capacity(template.len());
    for piece in pieces(template) {
        match piece {
            Piece::Text(t) => out.push_str(t),
            Piece::Param(name) => match params.iter().find(|(n, _)| *n == name) {
                Some((_, v)) => out.push_str(v.as_ref()),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            },
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Param(&'a str),
}

/// A template's text and `{name}` parameters, in order. A brace that
/// doesn't open a parameter name is text.
fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut text_from = 0;
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let rest = &template[i + 1..];
            if let Some(end) = rest.find('}') {
                let name = &rest[..end];
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    if text_from < i {
                        out.push(Piece::Text(&template[text_from..i]));
                    }
                    out.push(Piece::Param(name));
                    i += end + 2;
                    text_from = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if text_from < template.len() {
        out.push(Piece::Text(&template[text_from..]));
    }
    out
}

/// What [`recognize`] found: the code a message was made from and its
/// parameters' values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recognized {
    pub code: &'static str,
    pub params: Vec<(&'static str, String)>,
}

/// The code and parameters `text` was made from by [`msg`], when it is one
/// of the engine's messages: the message whose English it matches, the one
/// with the most fixed words when several do (`a cutoff can't be negative`
/// over `{what} can't be negative`). Filling the code's English with the
/// parameters gives `text` back exactly. A count phrase (`engine.phrase.*`)
/// is not a message on its own: see [`recognize_nested`].
pub fn recognize(text: &str) -> Option<Recognized> {
    recognize_among(text, Among::Messages)
}

/// [`recognize`] for a nested parameter's value, which may also be a count
/// phrase (`engine.phrase.*`).
pub fn recognize_nested(text: &str) -> Option<Recognized> {
    recognize_among(text, Among::Nested)
}

/// The code and parameters a name the engine gave something was made from
/// (`engine.name.*`), when `text` is one: the Versions dialog translates
/// the automatic version's name on display. The name itself stays this
/// English.
pub fn recognize_name(text: &str) -> Option<Recognized> {
    recognize_among(text, Among::Names)
}

/// The code and parameters an undo label was made from (`engine.undo.*`),
/// when `text` is one: what the History list, the Activity tab and the
/// rest translate on display. The label itself stays this English.
pub fn recognize_label(text: &str) -> Option<Recognized> {
    recognize_among(text, Among::Labels)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Among {
    /// Messages: not phrases, roles or labels.
    Messages,
    /// A nested parameter: messages, phrases and roles.
    Nested,
    /// Undo labels only.
    Labels,
    /// Names the engine gave things only.
    Names,
}

fn recognize_among(text: &str, among: Among) -> Option<Recognized> {
    let mut best: Option<(usize, Recognized)> = None;
    for m in MESSAGES {
        let label = m.code.starts_with(UNDO);
        let name = m.code.starts_with(NAME);
        if m.code.starts_with(WORKING) {
            continue;
        }
        let skip = match among {
            Among::Messages => {
                label || name || m.code.starts_with(PHRASE) || m.code.starts_with(ROLE)
            }
            Among::Nested => label || name,
            Among::Labels => !label,
            Among::Names => !name,
        };
        if skip {
            continue;
        }

        for english in m.one.into_iter().chain([m.other]) {
            // The text before the first brace is where it has to start (a
            // quick test before the full one).
            let head = english.split('{').next().unwrap_or("");
            if !text.starts_with(head) {
                continue;
            }
            let pieces = pieces(english);
            let fixed: usize = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(t) => t.len(),
                    Piece::Param(_) => 0,
                })
                .sum();
            if best.as_ref().is_some_and(|(f, _)| *f >= fixed) {
                continue;
            }
            let mut params = Vec::new();
            if match_pieces(&pieces, text, &mut params) {
                let params = params
                    .into_iter()
                    .map(|(n, v)| (n, v.to_string()))
                    .collect();
                best = Some((
                    fixed,
                    Recognized {
                        code: m.code,
                        params,
                    },
                ));
            }
        }
    }
    best.map(|(_, r)| r)
}

/// Whether `text` is `pieces` with some value for each parameter, trying
/// every place the text after a parameter could start (a `count` is
/// digits). The values found are pushed onto `params`.
fn match_pieces<'p, 't>(
    pieces: &[Piece<'p>],
    text: &'t str,
    params: &mut Vec<(&'p str, &'t str)>,
) -> bool {
    match pieces.split_first() {
        None => text.is_empty(),
        Some((Piece::Text(t), rest)) => {
            text.starts_with(t) && match_pieces(rest, &text[t.len()..], params)
        }
        Some((Piece::Param(name), rest)) => {
            let ends: Vec<usize> = match rest.first() {
                None => vec![text.len()],
                Some(Piece::Text(next)) => text.match_indices(next).map(|(i, _)| i).collect(),
                // Two parameters side by side can't be told apart (tested
                // not to happen).
                Some(Piece::Param(_)) => return false,
            };
            for end in ends {
                // A count is a number ("Edit a cell" isn't "Edit {count}
                // cell").
                let value = &text[..end];
                if *name == "count"
                    && (value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()))
                {
                    continue;
                }
                // A role is one of the roles ("{name}: {role}" isn't any
                // sentence with a colon in it).
                if *name == "role" && !is_role(value) {
                    continue;
                }
                let mark = params.len();
                params.push((name, &text[..end]));
                if match_pieces(rest, &text[end..], params) {
                    return true;
                }
                params.truncate(mark);
            }
            false
        }
    }
}

/// Whether `text` is a role in words (`engine.role.*`).
fn is_role(text: &str) -> bool {
    MESSAGES
        .iter()
        .filter(|m| m.code.starts_with(ROLE))
        .any(|m| match_pieces(&pieces(m.other), text, &mut Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn the_catalog_is_sorted_and_well_formed() {
        for pair in MESSAGES.windows(2) {
            assert!(
                pair[0].code < pair[1].code,
                "{} is out of order",
                pair[1].code
            );
        }
        for m in MESSAGES {
            assert!(
                m.code.starts_with("engine.")
                    && m.code
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
                "{}: engine.<area>.<name>",
                m.code
            );
            for english in m.one.into_iter().chain([m.other]) {
                assert!(!english.contains('\u{2014}'), "{}: an em dash", m.code);
                assert!(!english.contains('!'), "{}: an exclamation mark", m.code);
                let ps = pieces(english);
                assert!(
                    !matches!(ps.as_slice(), [Piece::Param(_)]),
                    "{}: a parameter alone",
                    m.code
                );
                for w in ps.windows(2) {
                    assert!(
                        !matches!(w, [Piece::Param(_), Piece::Param(_)]),
                        "{}: two parameters side by side",
                        m.code
                    );
                }
            }
            if m.one.is_some() {
                assert!(
                    m.other.contains("{count}"),
                    "{}: a plural counts {{count}}",
                    m.code
                );
            }
        }
    }

    /// Every message, made with sample values, is recognized as itself.
    #[test]
    fn each_message_is_recognized_with_its_parameters() {
        for m in MESSAGES {
            for (english, count) in m.one.map(|o| (o, "1")).into_iter().chain([(m.other, "7")]) {
                let names: Vec<&str> = pieces(english)
                    .into_iter()
                    .filter_map(|p| match p {
                        Piece::Param(n) => Some(n),
                        Piece::Text(_) => None,
                    })
                    .collect();
                let values: Vec<(&str, String)> = names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        let v = if *n == "count" {
                            count.to_string()
                        } else if *n == "role" {
                            word("engine.role.editor")
                        } else {
                            format!("Qz{i}")
                        };
                        (*n, v)
                    })
                    .collect();
                let refs: Vec<(&str, &dyn Display)> = values
                    .iter()
                    .map(|(n, v)| (*n, v as &dyn Display))
                    .collect();
                let text = msg(m.code, &refs);
                assert_eq!(text, fill(english, &values));
                if m.code.starts_with(WORKING) {
                    // Carried with its code, never recognized.
                    for r in [
                        recognize(&text),
                        recognize_nested(&text),
                        recognize_label(&text),
                        recognize_name(&text),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        assert!(
                            !r.code.starts_with(WORKING),
                            "{text:?}: the working's words are carried, not recognized"
                        );
                    }
                    continue;
                }
                let phrase = m.code.starts_with(PHRASE) || m.code.starts_with(ROLE);
                let label = m.code.starts_with(UNDO);
                let name = m.code.starts_with(NAME);
                if phrase {
                    assert!(
                        recognize(&text).is_none_or(|r| {
                            !r.code.starts_with(PHRASE) && !r.code.starts_with(ROLE)
                        }),
                        "{text:?}: a phrase is only recognized nested"
                    );
                }
                if label {
                    for r in [recognize(&text), recognize_nested(&text)]
                        .into_iter()
                        .flatten()
                    {
                        assert!(
                            !r.code.starts_with(UNDO),
                            "{text:?}: a label is only recognized as one"
                        );
                    }
                } else if name {
                    for r in [
                        recognize(&text),
                        recognize_nested(&text),
                        recognize_label(&text),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        assert!(
                            !r.code.starts_with(NAME),
                            "{text:?}: a name is only recognized as one"
                        );
                    }
                } else {
                    assert!(
                        recognize_label(&text).is_none(),
                        "{text:?}: a message is never a label"
                    );
                }
                let r = if phrase {
                    recognize_nested(&text)
                } else if label {
                    recognize_label(&text)
                } else if name {
                    recognize_name(&text)
                } else {
                    recognize(&text)
                }
                .unwrap_or_else(|| panic!("{text:?} isn't recognized"));
                assert_eq!(r.code, m.code, "{text:?}");
                let got: Vec<(&str, String)> = r.params.into_iter().collect();
                assert_eq!(got, values, "{text:?}");
            }
        }
    }

    #[test]
    fn the_more_specific_message_wins_and_nouns_nest() {
        let r = recognize("a cutoff can't be negative").unwrap();
        assert_eq!(r.code, "engine.edit.negative_cutoff");
        let r = recognize("row 3's cap can't be negative").unwrap();
        assert_eq!(r.code, "engine.edit.cant_be_negative");
        assert_eq!(r.params, [("what", "row 3's cap".to_string())]);
        let r = recognize(&r.params[0].1).unwrap();
        assert_eq!(r.code, "engine.what.row_cap");
        let r = recognize("there is no group `Crew`").unwrap();
        assert_eq!(r.code, "engine.edit.credit_no_group");
        let r = recognize("account 1100 no longer exists").unwrap();
        assert_eq!(r.code, "engine.edit.account_gone");
        assert_eq!(recognize("Something else entirely."), None);
        assert_eq!(
            msg(
                "engine.edit.lines_use_currency",
                &[("count", &1), ("code", &"EUR")]
            ),
            "1 line uses EUR; change them first"
        );
        assert_eq!(
            fill("{a} and {b}", &[("a", "{b}"), ("b", "x")]),
            "{b} and x",
            "filled in one pass"
        );
    }

    /// The second slice's generic templates: a count phrase alone is not a
    /// message, and the longer of two overlapping templates wins.
    #[test]
    fn phrases_stay_nested_and_overlaps_go_to_the_longer_template() {
        assert_eq!(recognize("3 accounts"), None);
        let r = recognize_nested("3 accounts").unwrap();
        assert_eq!(r.code, "engine.phrase.accounts");
        let r = recognize("Imported 4 lines into 2 accounts (1 new account).").unwrap();
        assert_eq!(r.code, "engine.import.imported_adding");
        assert_eq!(r.params[2], ("added", "1 new account".to_string()));
        let r = recognize("Couldn't read a.csv: it isn't text.").unwrap();
        assert_eq!(r.code, "engine.import.not_text");
        let r = recognize("Couldn't read a.csv: the sheet is empty.").unwrap();
        assert_eq!(r.code, "engine.import.cant_read");
        let r =
            recognize("This budget has no fringe X. Edit the preset, or add it first.").unwrap();
        assert_eq!(r.code, "engine.presets.missing_one");
        assert_eq!(recognize("Edits everything"), None);
        assert_eq!(
            recognize_nested("Edits 2000 Production").unwrap().code,
            "engine.role.department"
        );
        assert_eq!(recognize_label("Note: something else"), None);
        let r = recognize("Days: Qz goes to Days too.").unwrap();
        assert_eq!(r.code, "engine.days.goes_too_of");
        for m in MESSAGES {
            assert!(
                !(m.code.starts_with(PHRASE)
                    && m.one.is_none()
                    && !m.other.contains("{count}")
                    && m.code != "engine.phrase.line_at"),
                "{}: a phrase counts something (or is a line's place)",
                m.code
            );
        }
    }

    /// Undo labels: the same English as before the catalog (byte for
    /// byte, as stored in histories and shared change logs), recognized as
    /// labels with the person's own words as plain parameters.
    #[test]
    fn undo_labels_keep_their_english_and_are_recognized() {
        let rows = msg(
            "engine.phrase.lines_in",
            &[("count", &4), ("code", &"21-01")],
        );
        let cases: Vec<(String, &str)> = vec![
            (
                msg(
                    "engine.undo.edit_cell_at",
                    &[
                        ("what", &word("engine.what.rate")),
                        ("code", &"21-01"),
                        ("line", &3),
                    ],
                ),
                "Edit rate on 21-01 line 3",
            ),
            (
                msg(
                    "engine.undo.paste_cells_in",
                    &[("count", &12), ("code", &"21-01")],
                ),
                "Paste 12 cells in 21-01",
            ),
            (
                msg("engine.undo.fill_cells", &[("count", &1)]),
                "Fill down 1 cell",
            ),
            (
                msg(
                    "engine.undo.set_field_rows",
                    &[("what", &word("engine.what.x4")), ("rows", &rows)],
                ),
                "Set 4X on 4 lines in 21-01",
            ),
            (
                msg(
                    "engine.undo.add_fringe_to_rows",
                    &[("name", &"FICA"), ("rows", &rows)],
                ),
                "Add fringe FICA to 4 lines in 21-01",
            ),
            (
                msg(
                    "engine.undo.assign_rows",
                    &[
                        ("what", &word("engine.what.set")),
                        ("value", &"Stage A"),
                        ("rows", &rows),
                    ],
                ),
                "Set set Stage A on 4 lines in 21-01",
            ),
            (
                msg(
                    "engine.undo.save_version_from_history",
                    &[("name", &"Rate pass")],
                ),
                "Save version \u{201c}Rate pass\u{201d} from the history",
            ),
            (
                msg("engine.undo.store_returned_copy", &[("name", &"From Dana")]),
                "Store a returned copy as version \"From Dana\"",
            ),
            (
                msg("engine.undo.move_lines_down", &[("count", &1)]),
                "Move 1 line down",
            ),
            (
                msg(
                    "engine.undo.rename_item",
                    &[
                        ("what", &word("engine.what.fringe")),
                        ("from", &"FICA"),
                        ("to", &"Payroll"),
                    ],
                ),
                "Rename fringe FICA to Payroll",
            ),
            (
                msg("engine.undo.replace_with_nothing", &[("count", &2)]),
                "Replace 2 matches with nothing",
            ),
        ];
        for (made, english) in &cases {
            assert_eq!(made, english);
            let r = recognize_label(made).unwrap_or_else(|| panic!("{made:?}"));
            assert!(r.code.starts_with(UNDO), "{made:?}");
        }
        // The more specific label wins; the person's words stay parameters.
        let r = recognize_label("Add fringe FICA to 4 lines in 21-01").unwrap();
        assert_eq!(r.code, "engine.undo.add_fringe_to_rows");
        assert_eq!(r.params[0], ("name", "FICA".to_string()));
        assert_eq!(
            recognize_nested(&r.params[1].1).unwrap().code,
            "engine.phrase.lines_in"
        );
        let r = recognize_label("Set set Stage A on 4 lines in 21-01").unwrap();
        assert_eq!(r.code, "engine.undo.assign_rows");
        assert_eq!(r.params[1], ("value", "Stage A".to_string()));
        let r = recognize_label("Edit a cell").unwrap();
        assert_eq!(r.code, "engine.undo.edit_cell");
        assert_eq!(
            recognize_nested(&r.params[0].1).unwrap().code,
            "engine.what.a_cell"
        );
        let r = recognize_label("Edit fringe FICA").unwrap();
        assert_eq!(r.code, "engine.undo.edit_fringe");
        let r = recognize_label("Renumber accounts to the new pattern").unwrap();
        assert_eq!(r.code, "engine.undo.renumber_accounts");
        // Not a step, and not the other way round.
        assert_eq!(recognize_label("account 1100 no longer exists"), None);
        assert_eq!(recognize("Edit rate on 21-01 line 3"), None);
        // A label cut short to fit the history is left as it is.
        assert_eq!(recognize_label("Rename fringe x\u{2026}"), None);
    }

    /// Every `"engine.…"` code written in the crate's sources is in the
    /// catalog, and every message in the catalog is written somewhere.
    #[test]
    fn every_code_used_is_in_the_catalog_and_every_message_is_used() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = here.join("src");
        let mut used = BTreeSet::new();
        // The desktop app makes a few undo labels of its own.
        let mut dirs = vec![src, here.join("../method-app/src")];
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    dirs.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs")
                    || p.ends_with("messages.rs")
                    || p.ends_with("i18n.rs")
                {
                    continue;
                }
                let rs = std::fs::read_to_string(&p).unwrap();
                for (at, _) in rs.match_indices("\"engine.") {
                    let rest = &rs[at + 1..];
                    let end = rest.find('"').unwrap();
                    used.insert(rest[..end].to_string());
                }
            }
        }
        // Explain's working is worded in method-core (tested below to
        // agree with this catalog).
        used.extend(
            method_core::explain::WORDING
                .iter()
                .map(|w| w.code.to_string()),
        );
        for code in &used {
            assert!(find(code).is_some(), "{code} is used but not in MESSAGES");
        }
        let unused: Vec<&str> = MESSAGES
            .iter()
            .map(|m| m.code)
            .filter(|c| !used.contains(*c))
            .collect();
        assert!(unused.is_empty(), "messages nothing makes: {unused:?}");
    }

    /// Explain's working: method-core's `WORDING` and this catalog's
    /// `engine.working.*` entries are the same codes with the same words.
    #[test]
    fn the_working_matches_the_engines_words() {
        let ours: Vec<(&str, Option<&str>, &str)> = MESSAGES
            .iter()
            .filter(|m| m.code.starts_with(WORKING))
            .map(|m| (m.code, m.one, m.other))
            .collect();
        let core: Vec<(&str, Option<&str>, &str)> = method_core::explain::WORDING
            .iter()
            .map(|w| (w.code, w.one, w.other))
            .collect();
        assert_eq!(WORKING, method_core::explain::WORDING_PREFIX);
        assert!(!core.is_empty());
        assert_eq!(ours, core);
    }
}
