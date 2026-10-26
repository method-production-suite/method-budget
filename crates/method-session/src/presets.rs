//! Tag presets and line blocks, kept in the budget (decision 15; docs/APP.md,
//! "Presets").
//!
//! - **Tag presets** bundle a line's tags (fringes, groups, set, location,
//!   currency) under a name. One is made from a line ([`tag_preset_from_row`])
//!   or edited part by part ([`store_tag_preset`]), and put on a selection in
//!   one of two modes ([`put_tag_preset`]):
//!   - **add**: the preset's fringes and groups join the line's own; its set,
//!     location and currency are filled in only where the line has none (a
//!     line in the base currency has none). Add never takes anything off a
//!     line or changes a value it already has.
//!   - **replace**: the line's fringes, groups, set, location and currency
//!     become exactly the preset's; an empty part clears that tag.
//!
//!   Which rows take what follows the apply panel (src/apply.rs): fringes and
//!   a currency go on lines, groups, a set and a location on lines and
//!   adjustments; other rows are passed over.
//! - **Line blocks** keep copies of rows ([`store_line_block`]), with a
//!   checklist of what to keep, and insert fresh copies above a row
//!   ([`insert_block`]): new ids, fringe ranges renumbered for the
//!   account, as a paste of copied rows does (src/clipboard.rs, whose logic
//!   this reuses). A block also keeps the definitions of the setup items its
//!   rows use; one the budget no longer has is listed before inserting
//!   ([`block_preview`]) and added back with the rows. The budget's own
//!   definition of an item always wins over the block's.
//!
//! Every edit checks everything first and changes the budget only when all
//! of it can be done, so a refusal leaves it as it was; run through
//! `Session::edit` (as the commands do), each one is a single undo step.

use std::collections::{BTreeMap, BTreeSet};

use method_core::{
    Budget, Detail, LinePreset, RowId, RowKind, SetupItems, TagPreset, TopsheetRow, MAX_BLOCK_ROWS,
    MAX_LINE_PRESETS, MAX_TAG_PRESETS, MAX_TOOL_PRESET_NAME_CHARS,
};
use serde::Deserialize;

use crate::apply::rows_under;
use crate::clipboard::{self, Clip, Content, Pasted, Preview, Setup, Target};
use crate::edit::{details, row_gone, EditResult};
use crate::messages::{msg, word};
use crate::session::Session;

/// Lines take a currency.
fn is_line(d: &Detail) -> bool {
    d.kind == RowKind::Line
}

/// Lines and adjustments take fringes (decisions.md, entry 56), groups, a
/// set and a location.
fn is_costed(d: &Detail) -> bool {
    matches!(d.kind, RowKind::Line | RowKind::Adjustment)
}

// ---------------------------------------------------------------------------
// Names and the two lists

/// Which list: tag presets or line blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Tags,
    Lines,
}

impl Which {
    pub fn parse(s: &str) -> EditResult<Which> {
        match s {
            "tags" => Ok(Which::Tags),
            "lines" => Ok(Which::Lines),
            other => Err(msg("engine.presets.unknown_kind", &[("kind", &other)])),
        }
    }

    fn what(self) -> String {
        word(match self {
            Which::Tags => "engine.what.tag_preset",
            Which::Lines => "engine.what.line_block",
        })
    }

    fn len(self, b: &Budget) -> usize {
        match self {
            Which::Tags => b.tag_presets.len(),
            Which::Lines => b.line_presets.len(),
        }
    }

    fn names(self, b: &Budget) -> Vec<&str> {
        match self {
            Which::Tags => b.tag_presets.iter().map(|p| p.name.as_str()).collect(),
            Which::Lines => b.line_presets.iter().map(|p| p.name.as_str()).collect(),
        }
    }

    fn gone(self) -> String {
        msg("engine.presets.gone", &[("what", &self.what())])
    }
}

/// `name` trimmed, when it is a usable name for an item of `which` other
/// than item `except`: not empty, not too long, not taken (ignoring case).
fn check_name(b: &Budget, which: Which, name: &str, except: Option<usize>) -> EditResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(msg("engine.presets.give_name", &[("what", &which.what())]));
    }
    if name.chars().count() > MAX_TOOL_PRESET_NAME_CHARS {
        return Err(msg(
            "engine.presets.name_long",
            &[
                ("what", &which.what()),
                ("max", &MAX_TOOL_PRESET_NAME_CHARS),
            ],
        ));
    }
    let key = name.to_lowercase();
    let taken = which
        .names(b)
        .iter()
        .enumerate()
        .any(|(i, n)| Some(i) != except && n.trim().to_lowercase() == key);
    if taken {
        return Err(msg(
            "engine.presets.name_taken",
            &[("what", &which.what()), ("name", &name)],
        ));
    }
    Ok(name.to_string())
}

fn check_room(b: &Budget, which: Which) -> EditResult {
    let max = match which {
        Which::Tags => MAX_TAG_PRESETS,
        Which::Lines => MAX_LINE_PRESETS,
    };
    if which.len(b) >= max {
        let many = word(match which {
            Which::Tags => "engine.what.tag_presets",
            Which::Lines => "engine.what.line_blocks",
        });
        return Err(msg(
            "engine.presets.too_many",
            &[("max", &max), ("what", &many)],
        ));
    }
    Ok(())
}

fn check_index(b: &Budget, which: Which, index: usize) -> EditResult {
    if index < which.len(b) {
        Ok(())
    } else {
        Err(which.gone())
    }
}

/// Renames preset or block `index`.
pub fn rename_preset(b: &mut Budget, which: Which, index: usize, name: &str) -> EditResult {
    check_index(b, which, index)?;
    let name = check_name(b, which, name, Some(index))?;
    match which {
        Which::Tags => b.tag_presets[index].name = name,
        Which::Lines => b.line_presets[index].name = name,
    }
    Ok(())
}

/// Removes preset or block `index`.
pub fn delete_preset(b: &mut Budget, which: Which, index: usize) -> EditResult {
    check_index(b, which, index)?;
    match which {
        Which::Tags => drop(b.tag_presets.remove(index)),
        Which::Lines => drop(b.line_presets.remove(index)),
    }
    Ok(())
}

/// Moves preset or block `index` to position `to` (both in the current list).
pub fn move_preset(b: &mut Budget, which: Which, index: usize, to: usize) -> EditResult {
    check_index(b, which, index)?;
    if to >= which.len(b) {
        return Err(msg("engine.presets.cant_move", &[("what", &which.what())]));
    }
    match which {
        Which::Tags => {
            let p = b.tag_presets.remove(index);
            b.tag_presets.insert(to, p);
        }
        Which::Lines => {
            let p = b.line_presets.remove(index);
            b.line_presets.insert(to, p);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tag presets

/// The setup items a tag preset names that `b` doesn't define, in words
/// (`fringe PENSION`), in the preset's order.
fn missing_items(b: &Budget, p: &TagPreset) -> Vec<String> {
    let mut out = Vec::new();
    for f in &p.fringes {
        if !b.fringes.contains_key(f) {
            out.push(format!("fringe {f}"));
        }
    }
    for g in &p.groups {
        if !b.groups.contains_key(g) {
            out.push(format!("group {g}"));
        }
    }
    if let Some(s) = p.set.as_ref().filter(|s| !b.sets.contains_key(*s)) {
        out.push(format!("set {s}"));
    }
    if let Some(l) = p
        .location
        .as_ref()
        .filter(|l| !b.locations.contains_key(*l))
    {
        out.push(format!("location {l}"));
    }
    if let Some(c) = p
        .currency
        .as_ref()
        .filter(|c| **c != b.base_currency && !b.currencies.contains_key(*c))
    {
        out.push(format!("currency {c}"));
    }
    out
}

fn refuse_missing(b: &Budget, p: &TagPreset) -> EditResult {
    let missing = missing_items(b, p);
    if missing.is_empty() {
        return Ok(());
    }
    let code = if missing.len() == 1 {
        "engine.presets.missing_one"
    } else {
        "engine.presets.missing_many"
    };
    Err(msg(code, &[("items", &missing.join(", "))]))
}

/// `p` as it is stored: names trimmed, blank values as none, the base
/// currency as none.
fn tidy_tags(b: &Budget, mut p: TagPreset) -> TagPreset {
    let names = |s: BTreeSet<String>| -> BTreeSet<String> {
        s.into_iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect()
    };
    let one = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    p.fringes = names(p.fringes);
    p.groups = names(p.groups);
    p.set = one(p.set);
    p.location = one(p.location);
    p.currency = one(p.currency).filter(|c| *c != b.base_currency);
    p
}

/// Saves `preset` as a new tag preset at the end of the list (`index`
/// `None`), or in place of preset `index` (its name and every part: an edit,
/// or a rename). Every item it names must be defined. Returns where it is.
pub fn store_tag_preset(
    b: &mut Budget,
    index: Option<usize>,
    preset: TagPreset,
) -> EditResult<usize> {
    match index {
        Some(i) => check_index(b, Which::Tags, i)?,
        None => check_room(b, Which::Tags)?,
    }
    let mut p = tidy_tags(b, preset);
    p.name = check_name(b, Which::Tags, &p.name, index)?;
    refuse_missing(b, &p)?;
    Ok(match index {
        Some(i) => {
            b.tag_presets[i] = p;
            i
        }
        None => {
            b.tag_presets.push(p);
            b.tag_presets.len() - 1
        }
    })
}

/// Which of a line's tags a new tag preset takes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TagParts {
    #[serde(default = "yes")]
    pub fringes: bool,
    #[serde(default = "yes")]
    pub groups: bool,
    #[serde(default = "yes")]
    pub set: bool,
    #[serde(default = "yes")]
    pub location: bool,
    #[serde(default = "yes")]
    pub currency: bool,
}

fn yes() -> bool {
    true
}

impl Default for TagParts {
    fn default() -> TagParts {
        TagParts {
            fringes: true,
            groups: true,
            set: true,
            location: true,
            currency: true,
        }
    }
}

/// A new tag preset named `name` from row `id`'s tags (the parts `parts`
/// picks), at the end of the list. A row's tags that name items the budget
/// doesn't define are refused, like any preset's. Returns its position.
pub fn tag_preset_from_row(
    b: &mut Budget,
    id: RowId,
    name: &str,
    parts: TagParts,
) -> EditResult<usize> {
    let d = details(b)
        .find(|d| d.id == id)
        .ok_or_else(|| row_gone(id))?;
    if !is_costed(d) {
        return Err(word("engine.presets.no_tags_to_save"));
    }
    let preset = TagPreset {
        name: name.to_string(),
        fringes: if parts.fringes {
            d.fringes.clone()
        } else {
            BTreeSet::new()
        },
        groups: if parts.groups {
            d.groups.clone()
        } else {
            BTreeSet::new()
        },
        set: if parts.set { d.set.clone() } else { None },
        location: if parts.location {
            d.location.clone()
        } else {
            None
        },
        currency: if parts.currency {
            d.currency.clone()
        } else {
            None
        },
    };
    store_tag_preset(b, None, preset)
}

/// How a tag preset goes on the lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Add,
    Replace,
}

impl Mode {
    pub fn parse(s: &str) -> EditResult<Mode> {
        match s {
            "add" => Ok(Mode::Add),
            "replace" => Ok(Mode::Replace),
            other => Err(msg("engine.presets.unknown_mode", &[("mode", &other)])),
        }
    }
}

/// Puts tag preset `index` on rows `ids` (see the module notes for what add
/// and replace do). Refused, changing nothing, when a row is gone, none of
/// the rows can take tags, or the preset names an item the budget no longer
/// has. Returns how many rows changed (0: they already carry it, and there
/// is no undo step).
pub fn put_tag_preset(
    b: &mut Budget,
    index: usize,
    mode: Mode,
    ids: &[RowId],
) -> EditResult<usize> {
    check_index(b, Which::Tags, index)?;
    let p = b.tag_presets[index].clone();
    put_tags(b, &p, mode, ids)
}

/// Makes the tags of rows `ids` exactly `tags` (its name is not used), as a
/// tag preset's replace does: the apply panel's "Replace tags". Names are
/// tidied as a saved preset's are, and each must be defined.
pub fn replace_tags(b: &mut Budget, tags: TagPreset, ids: &[RowId]) -> EditResult<usize> {
    let p = tidy_tags(b, tags);
    let missing = missing_items(b, &p);
    if !missing.is_empty() {
        return Err(msg(
            "engine.presets.missing",
            &[("items", &missing.join(", "))],
        ));
    }
    put_tags(b, &p, Mode::Replace, ids)
}

/// Puts the tags `p` carries on rows `ids` (see the module notes).
fn put_tags(b: &mut Budget, p: &TagPreset, mode: Mode, ids: &[RowId]) -> EditResult<usize> {
    if ids.is_empty() {
        return Err(word("engine.presets.select_lines"));
    }
    let want: BTreeSet<RowId> = ids.iter().copied().collect();
    let have: BTreeSet<RowId> = details(b)
        .map(|d| d.id)
        .filter(|id| want.contains(id))
        .collect();
    if let Some(gone) = want.difference(&have).next() {
        return Err(row_gone(*gone));
    }
    if !details(b).any(|d| want.contains(&d.id) && is_costed(d)) {
        return Err(word("engine.presets.none_take_tags"));
    }
    refuse_missing(b, p)?;
    let base = b.base_currency.clone();
    let currency = p.currency.clone().filter(|c| *c != base);
    let mut changed = 0;
    for d in b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .filter(|d| want.contains(&d.id))
    {
        let before = d.clone();
        let line = is_line(d);
        if !is_costed(d) {
            continue;
        }
        match mode {
            Mode::Add => {
                d.fringes.extend(p.fringes.iter().cloned());
                d.groups.extend(p.groups.iter().cloned());
                if d.set.is_none() {
                    d.set.clone_from(&p.set);
                }
                if d.location.is_none() {
                    d.location.clone_from(&p.location);
                }
                let own = d.currency.as_ref().filter(|c| **c != base);
                if line && own.is_none() && currency.is_some() {
                    d.currency.clone_from(&currency);
                }
            }
            Mode::Replace => {
                d.fringes.clone_from(&p.fringes);
                if line {
                    d.currency.clone_from(&currency);
                }
                d.groups.clone_from(&p.groups);
                d.set.clone_from(&p.set);
                d.location.clone_from(&p.location);
            }
        }
        changed += usize::from(*d != before);
    }
    Ok(changed)
}

/// The rows a selection means, checked: rows by id (each must exist),
/// every row in the accounts numbered in `accounts`, every row in the
/// categories at topsheet positions `categories` (each must be there).
pub fn selection_rows(
    b: &Budget,
    row_ids: &[RowId],
    accounts: &[String],
    categories: &[usize],
) -> EditResult<Vec<RowId>> {
    let have: BTreeSet<RowId> = details(b).map(|d| d.id).collect();
    if let Some(gone) = row_ids.iter().find(|id| !have.contains(id)) {
        return Err(row_gone(*gone));
    }
    for code in accounts {
        let found = b
            .categories()
            .flat_map(|c| &c.accounts)
            .any(|a| a.code == *code);
        if !found {
            return Err(msg("engine.edit.account_gone", &[("code", code)]));
        }
    }
    for &ti in categories {
        if !matches!(b.topsheet.get(ti), Some(TopsheetRow::Category(_))) {
            return Err(word("engine.presets.category_gone"));
        }
    }
    Ok(rows_under(b, row_ids, accounts, categories))
}

// ---------------------------------------------------------------------------
// Line blocks

/// Which fields a new line block keeps. The description, the row's kind and
/// its text style always come along.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockFields {
    /// Amount, X, rate and 4X.
    #[serde(default = "yes")]
    pub formulas: bool,
    /// The four unit columns.
    #[serde(default = "yes")]
    pub units: bool,
    /// Fringes, groups, set, location, currency and fringe ranges.
    #[serde(default = "yes")]
    pub tags: bool,
    #[serde(default = "yes")]
    pub notes: bool,
}

impl Default for BlockFields {
    fn default() -> BlockFields {
        BlockFields {
            formulas: true,
            units: true,
            tags: true,
            notes: true,
        }
    }
}

fn to_items(s: Setup) -> SetupItems {
    let Setup {
        fringes,
        groups,
        globals,
        units,
        sets,
        locations,
        currencies,
    } = s;
    SetupItems {
        fringes,
        groups,
        globals,
        units,
        sets,
        locations,
        currencies,
    }
}

fn to_setup(s: &SetupItems) -> Setup {
    let SetupItems {
        fringes,
        groups,
        globals,
        units,
        sets,
        locations,
        currencies,
    } = s.clone();
    Setup {
        fringes,
        groups,
        globals,
        units,
        sets,
        locations,
        currencies,
    }
}

/// Saves copies of rows `ids` (in budget order) as a new line block named
/// `name`, keeping the fields `keep` asks for, at the end of the list. Rows
/// lose their ids and any person, a line in the base currency keeps no
/// currency, and the block's fringe ranges are numbered 1, 2... in order.
/// Returns its position.
pub fn store_line_block(
    b: &mut Budget,
    name: &str,
    ids: &[RowId],
    keep: BlockFields,
) -> EditResult<usize> {
    check_room(b, Which::Lines)?;
    let name = check_name(b, Which::Lines, name, None)?;
    let want: BTreeSet<RowId> = ids.iter().copied().collect();
    if want.is_empty() {
        return Err(word("engine.presets.select_lines_to_save"));
    }
    let mut rows: Vec<Detail> = details(b)
        .filter(|d| want.contains(&d.id))
        .cloned()
        .collect();
    let have: BTreeSet<RowId> = rows.iter().map(|d| d.id).collect();
    if let Some(gone) = want.difference(&have).next() {
        return Err(row_gone(*gone));
    }
    if rows.len() > MAX_BLOCK_ROWS {
        return Err(msg(
            "engine.presets.block_too_big",
            &[("max", &MAX_BLOCK_ROWS)],
        ));
    }
    let mut ranges: BTreeMap<u32, u32> = BTreeMap::new();
    for d in &mut rows {
        d.id = 0;
        // A block is reusable: its rows never join a person's cutoff
        // (decisions.md, entry 79), nor say they were filled from one of
        // this budget's rate tables (entry 90).
        d.person = None;
        d.rate_source = None;
        if !keep.formulas {
            for f in [&mut d.amount, &mut d.x, &mut d.rate, &mut d.x4] {
                f.0.clear();
            }
        }
        if !keep.units {
            for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
                u.clear();
            }
        }
        if keep.tags {
            if d.currency.as_deref() == Some(b.base_currency.as_str()) {
                d.currency = None;
            }
            if let Some(n) = d.fringe_range {
                let next = u32::try_from(ranges.len() + 1).unwrap_or(u32::MAX);
                d.fringe_range = Some(*ranges.entry(n).or_insert(next));
            }
        } else {
            d.fringes.clear();
            d.groups.clear();
            d.set = None;
            d.location = None;
            d.currency = None;
            d.fringe_range = None;
        }
        if !keep.notes {
            d.notes.clear();
        }
    }
    let setup = to_items(Setup::used_by(b, &rows));
    b.line_presets.push(LinePreset { name, rows, setup });
    Ok(b.line_presets.len() - 1)
}

/// Block `index` as a copy ready to paste into `b`: its rows, and its own
/// definitions of the setup items they use.
fn block_clip(b: &Budget, index: usize) -> EditResult<Clip> {
    check_index(b, Which::Lines, index)?;
    let p = &b.line_presets[index];
    if p.rows.is_empty() {
        return Err(word("engine.presets.block_empty"));
    }
    let mut clip = Clip::new(b, Content::Rows(p.rows.clone()), None);
    clip.setup = to_setup(&p.setup);
    clip.source = p.name.clone();
    Ok(clip)
}

/// What inserting block `index` into `b` would do: the setup items it would
/// add back (the budget's own definitions win, so items set up differently
/// here are not listed). `review` is true when there is something to show.
pub fn block_preview(b: &Budget, index: usize) -> EditResult<Preview> {
    let clip = block_clip(b, index)?;
    let mut p = clipboard::preview(b, &clip);
    p.items.retain(|i| i.status == "add");
    p.review = !p.items.is_empty() || !p.notes.is_empty();
    Ok(p)
}

/// Inserts copies of block `index` above row `before_row` in account
/// `account` (at its end when `None`), adding back the setup items the
/// budget no longer has. Returns the new rows.
pub fn insert_block(
    b: &mut Budget,
    index: usize,
    account: &str,
    before_row: Option<RowId>,
) -> EditResult<Pasted> {
    let clip = block_clip(b, index)?;
    let target = Target {
        account: Some(account.to_string()),
        before_row,
        ..Target::default()
    };
    // No resolutions: an item set up differently here keeps this budget's
    // version, and a missing one is added.
    clipboard::paste(b, &clip, &target, &[])
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/presets.rs and the host's src/host/presets.rs
// call these)

/// Saves `preset` (in the file format's shape, docs/FORMAT.md, "Tag
/// preset") as tag preset `index`, or as a new one when `None`.
pub fn save_tag_preset(
    s: &mut Session,
    index: Option<usize>,
    preset: serde_json::Value,
) -> EditResult<usize> {
    let preset = method_format::tag_preset_from_value(preset)
        .map_err(|e| msg("engine.presets.cant_save", &[("why", &e)]))?;

    let label = word(if index.is_some() {
        "engine.undo.update_a_tag_preset"
    } else {
        "engine.undo.save_a_tag_preset"
    });
    s.edit(label, |b| store_tag_preset(b, index, preset))
}

/// A new tag preset from a line's tags.
pub fn tag_preset_from_line(
    s: &mut Session,
    row_id: RowId,
    name: &str,
    parts: Option<TagParts>,
) -> EditResult<usize> {
    let label = msg("engine.undo.save_tag_preset", &[("name", &name.trim())]);
    s.edit(label, |b| {
        tag_preset_from_row(b, row_id, name, parts.unwrap_or_default())
    })
}

/// Puts tag preset `index` on the selection (rows by id, whole accounts by
/// number, whole categories by topsheet position), `mode` `add` or
/// `replace`. One undo step.
pub fn apply_tag_preset(
    s: &mut Session,
    index: usize,
    mode: &str,
    row_ids: &[RowId],
    accounts: &[String],
    categories: &[usize],
) -> EditResult<usize> {
    let mode = Mode::parse(mode)?;
    s.edit(word("engine.undo.apply_tag_preset"), |b| {
        let rows = selection_rows(b, row_ids, accounts, categories)?;
        put_tag_preset(b, index, mode, &rows)
    })
}

/// A new line block from the rows.
pub fn save_line_preset(
    s: &mut Session,
    name: &str,
    row_ids: &[RowId],
    keep: Option<BlockFields>,
) -> EditResult<usize> {
    let label = msg("engine.undo.save_line_block", &[("name", &name.trim())]);
    s.edit(label, |b| {
        store_line_block(b, name, row_ids, keep.unwrap_or_default())
    })
}

/// What inserting block `index` would add back, before inserting it.
pub fn line_preset_preview(s: &Session, index: usize) -> EditResult<Preview> {
    block_preview(s.budget(), index)
}

/// Inserts copies of block `index` above the row (the account's end when
/// `None`), as one undo step. The first new row, for the view's
/// `created_row`.
pub fn insert_line_preset(
    s: &mut Session,
    index: usize,
    account: &str,
    before_row: Option<RowId>,
) -> EditResult<Option<RowId>> {
    let label = msg(
        "engine.undo.insert_line_block",
        &[("code", &account.trim())],
    );
    let pasted = s.edit(label, |b| insert_block(b, index, account, before_row))?;
    Ok(pasted.rows.first().copied())
}

/// `which` is `tags` or `lines`.
pub fn rename_tool_preset(s: &mut Session, which: &str, index: usize, name: &str) -> EditResult {
    let which = Which::parse(which)?;
    let label = msg("engine.undo.rename_preset", &[("name", &name.trim())]);
    s.edit(label, |b| rename_preset(b, which, index, name))
}

pub fn delete_tool_preset(s: &mut Session, which: &str, index: usize) -> EditResult {
    let which = Which::parse(which)?;
    s.edit(word("engine.undo.delete_preset"), |b| {
        delete_preset(b, which, index)
    })
}

pub fn move_tool_preset(s: &mut Session, which: &str, index: usize, to: usize) -> EditResult {
    let which = Which::parse(which)?;
    s.edit(word("engine.undo.move_preset"), |b| {
        move_preset(b, which, index, to)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit;
    use crate::session::Session;
    use method_core::{Fringe, FringeKind, Group, Location, SetDef};

    fn ids(b: &Budget, account: &str) -> Vec<RowId> {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == account)
            .unwrap()
            .details
            .iter()
            .map(|d| d.id)
            .collect()
    }

    fn row(b: &Budget, id: RowId) -> Detail {
        details(b).find(|d| d.id == id).cloned().unwrap()
    }

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The starter budget with a second fringe, two groups, two sets, two
    /// locations and a currency.
    fn budget() -> Budget {
        let mut b = method_core::starter::budget_with_separator('-');
        b.fringes.insert(
            "PENSION".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "5".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        for g in ["Crew", "Union"] {
            b.groups.insert(
                g.into(),
                Group {
                    short_code: None,
                    description: String::new(),
                    in_budget_total: true,
                    color: None,
                },
            );
        }
        for s in ["Stage A", "Stage B"] {
            b.sets.insert(s.into(), SetDef::default());
        }
        for l in ["Toronto", "Vancouver"] {
            b.locations.insert(l.into(), Location::default());
        }
        edit::set_currency(
            &mut b,
            "CAD",
            &serde_json::from_value(serde_json::json!({"rate": "0.75"})).unwrap(),
        )
        .unwrap();
        b
    }

    fn crew() -> TagPreset {
        TagPreset {
            name: "Union crew".into(),
            fringes: names(&["PENSION"]),
            groups: names(&["Union"]),
            set: Some("Stage A".into()),
            location: Some("Toronto".into()),
            currency: Some("CAD".into()),
        }
    }

    /// Account 20-01 as [line, subtotal, line, adjustment], the second line
    /// already tagged (group Crew, set Stage B, currency CAD).
    fn mixed(s: &mut Session) -> Vec<RowId> {
        let first = ids(s.budget(), "20-01")[0];
        s.edit("Test edit", |b| {
            edit::add_detail(b, "20-01", Some(first), RowKind::Subtotal)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            edit::add_detail(b, "20-01", None, RowKind::Adjustment)
        })
        .unwrap();
        let rows = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| {
            let d = b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
                .find(|d| d.id == rows[2])
                .unwrap();
            d.groups = names(&["Crew"]);
            d.set = Some("Stage B".into());
            d.currency = Some("CAD".into());
            Ok(())
        })
        .unwrap();
        rows
    }

    #[test]
    fn tag_presets_are_saved_checked_and_tidied() {
        let mut b = budget();
        let mut p = crew();
        p.name = "  Union crew  ".into();
        p.groups.insert(" Crew ".into());
        assert_eq!(store_tag_preset(&mut b, None, p), Ok(0));
        let saved = &b.tag_presets[0];
        assert_eq!(saved.name, "Union crew");
        assert_eq!(saved.groups, names(&["Crew", "Union"]));

        // The base currency is stored as none.
        let mut base = crew();
        base.name = "Base".into();
        base.currency = Some("USD".into());
        store_tag_preset(&mut b, None, base).unwrap();
        assert_eq!(b.tag_presets[1].currency, None);

        let before = b.clone();
        let refused = |b: &mut Budget, index: Option<usize>, p: TagPreset, want: &str| {
            let err = store_tag_preset(b, index, p).unwrap_err();
            assert!(err.contains(want), "{err}");
        };
        let named = |n: &str| TagPreset {
            name: n.into(),
            ..crew()
        };
        refused(&mut b, None, named(" "), "name");
        refused(&mut b, None, named("UNION CREW"), "already");
        refused(&mut b, None, named(&"x".repeat(81)), "80");
        refused(&mut b, Some(9), named("New"), "no longer exists");
        let mut ghost = named("Ghost");
        ghost.fringes.insert("NOPE".into());
        ghost.location = Some("Mars".into());
        ghost.currency = Some("EUR".into());
        refused(
            &mut b,
            None,
            ghost,
            "fringe NOPE, location Mars, currency EUR",
        );
        assert_eq!(b, before);
        // Renaming in place to its own name in other case is fine.
        assert_eq!(
            store_tag_preset(&mut b, Some(0), named("UNION CREW")),
            Ok(0)
        );
        assert_eq!(b.tag_presets[0].name, "UNION CREW");
    }

    #[test]
    fn a_preset_comes_from_a_lines_tags() {
        let mut s = Session::with_budget(budget(), None);
        let rows = mixed(&mut s);
        s.edit("Test edit", |b| {
            tag_preset_from_row(b, rows[2], "From line", TagParts::default())
        })
        .unwrap();
        let p = s.budget().tag_presets[0].clone();
        assert_eq!(p.fringes, names(&["PAYROLL"]));
        assert_eq!(p.groups, names(&["Crew"]));
        assert_eq!(p.set.as_deref(), Some("Stage B"));
        assert_eq!(p.location, None);
        assert_eq!(p.currency.as_deref(), Some("CAD"));

        let parts = TagParts {
            fringes: false,
            currency: false,
            ..TagParts::default()
        };
        s.edit("Test edit", |b| {
            tag_preset_from_row(b, rows[2], "Some", parts)
        })
        .unwrap();
        let p = &s.budget().tag_presets[1];
        assert!(p.fringes.is_empty() && p.currency.is_none());
        assert_eq!(p.groups, names(&["Crew"]));

        // A subtotal has no tags; a gone row is refused.
        let before = s.budget().clone();
        assert!(s
            .edit("Test edit", |b| tag_preset_from_row(
                b,
                rows[1],
                "Sub",
                TagParts::default()
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| tag_preset_from_row(
                b,
                999_999,
                "Gone",
                TagParts::default()
            ))
            .unwrap_err()
            .contains("999999"));
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn add_joins_and_fills_only_empty_tags_in_one_undo_step() {
        let mut s = Session::with_budget(budget(), None);
        let rows = mixed(&mut s); // [line, subtotal, tagged line, adjustment]
        s.edit("Test edit", |b| store_tag_preset(b, None, crew()))
            .unwrap();
        let before = s.budget().clone();

        let n = s
            .edit("Test edit", |b| put_tag_preset(b, 0, Mode::Add, &rows))
            .unwrap();
        assert_eq!(n, 3, "two lines and the adjustment");
        let b = s.budget().clone();
        let first = row(&b, rows[0]);
        assert_eq!(first.fringes, names(&["PAYROLL", "PENSION"]));
        assert_eq!(first.groups, names(&["Union"]));
        assert_eq!(first.set.as_deref(), Some("Stage A"));
        assert_eq!(first.location.as_deref(), Some("Toronto"));
        assert_eq!(
            first.currency.as_deref(),
            Some("CAD"),
            "a base line has none"
        );
        // The tagged line keeps its own set and currency, gains the rest.
        let tagged = row(&b, rows[2]);
        assert_eq!(tagged.groups, names(&["Crew", "Union"]));
        assert_eq!(tagged.set.as_deref(), Some("Stage B"));
        assert_eq!(tagged.location.as_deref(), Some("Toronto"));
        // The adjustment takes fringes (decisions.md, entry 56), groups, set
        // and location; never a currency. The subtotal takes nothing.
        let adj = row(&b, rows[3]);
        assert_eq!(adj.fringes, names(&["PENSION"]));
        assert!(adj.currency.is_none());
        assert_eq!(adj.groups, names(&["Union"]));
        assert_eq!(adj.set.as_deref(), Some("Stage A"));
        assert_eq!(row(&b, rows[1]), row(&before, rows[1]));

        // Once undone, the budget is as it was: one step.
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
        assert!(s.redo());
        // Again: nothing changes, and there is no new undo step.
        let at = s.budget().clone();
        assert_eq!(
            s.edit("Test edit", |b| put_tag_preset(b, 0, Mode::Add, &rows)),
            Ok(0)
        );
        assert_eq!(s.budget(), &at);
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn replace_makes_the_tags_exactly_the_presets() {
        let mut s = Session::with_budget(budget(), None);
        let rows = mixed(&mut s);
        let mut p = crew();
        p.location = None; // an empty part clears the tag
        p.currency = None; // the base currency
        s.edit("Test edit", |b| store_tag_preset(b, None, p))
            .unwrap();
        s.edit("Test edit", |b| {
            let d = b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
                .find(|d| d.id == rows[0])
                .unwrap();
            d.location = Some("Vancouver".into());
            Ok(())
        })
        .unwrap();
        let before = s.budget().clone();
        let n = s
            .edit("Test edit", |b| put_tag_preset(b, 0, Mode::Replace, &rows))
            .unwrap();
        assert_eq!(n, 3);
        let b = s.budget().clone();
        for id in [rows[0], rows[2]] {
            let d = row(&b, id);
            assert_eq!(d.fringes, names(&["PENSION"]), "PAYROLL is gone");
            assert_eq!(d.groups, names(&["Union"]));
            assert_eq!(d.set.as_deref(), Some("Stage A"));
            assert_eq!(d.location, None);
            assert_eq!(d.currency, None);
        }
        assert_eq!(row(&b, rows[3]).groups, names(&["Union"]));
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
    }

    #[test]
    fn a_refused_apply_changes_nothing() {
        let mut s = Session::with_budget(budget(), None);
        let rows = mixed(&mut s);
        s.edit("Test edit", |b| store_tag_preset(b, None, crew()))
            .unwrap();
        // The preset's fringe is deleted afterwards (it comes off the lines,
        // not off the preset).
        s.edit("Test edit", |b| edit::delete_fringe(b, "PENSION"))
            .unwrap();
        let before = s.budget().clone();
        let refused = |s: &mut Session, index: usize, ids: &[RowId], want: &str| {
            let err = s
                .edit("Test edit", |b| put_tag_preset(b, index, Mode::Add, ids))
                .unwrap_err();
            assert!(err.contains(want), "{err}");
            assert_eq!(s.budget(), &before);
        };
        refused(&mut s, 0, &rows, "fringe PENSION");
        s.edit("Test edit", |b| {
            b.tag_presets[0].fringes.clear();
            Ok(())
        })
        .unwrap();
        let before = s.budget().clone();
        let refused = |s: &mut Session, index: usize, ids: &[RowId], want: &str| {
            let err = s
                .edit("Test edit", |b| {
                    put_tag_preset(b, index, Mode::Replace, ids)
                })
                .unwrap_err();
            assert!(err.contains(want), "{err}");
            assert_eq!(s.budget(), &before);
        };
        // A gone row sinks the whole request, even after good rows.
        refused(&mut s, 0, &[rows[0], 999_999], "999999");
        refused(&mut s, 0, &[], "Select");
        refused(&mut s, 0, &[rows[1]], "can take tags");
        refused(&mut s, 5, &rows, "no longer exists");
        assert!(Mode::parse("merge").is_err());
    }

    #[test]
    fn a_selection_of_accounts_and_categories_means_their_rows() {
        let b = budget();
        let cast = ids(&b, "13-01");
        let staff = ids(&b, "20-01");
        // 13 Cast is at topsheet position 3.
        assert_eq!(selection_rows(&b, &[], &[], &[3]), Ok(cast.clone()));
        assert_eq!(
            selection_rows(&b, &[staff[1]], &["13-01".into()], &[]),
            Ok([cast, vec![staff[1]]].concat())
        );
        assert!(selection_rows(&b, &[], &["99-99".into()], &[]).is_err());
        assert!(
            selection_rows(&b, &[], &[], &[4]).is_err(),
            "a production total"
        );
        assert!(selection_rows(&b, &[999_999], &[], &[]).is_err());
    }

    #[test]
    fn presets_are_renamed_moved_and_deleted() {
        let mut b = budget();
        for n in ["A", "B", "C"] {
            store_tag_preset(
                &mut b,
                None,
                TagPreset {
                    name: n.into(),
                    ..TagPreset::default()
                },
            )
            .unwrap();
        }
        let order =
            |b: &Budget| -> Vec<String> { b.tag_presets.iter().map(|p| p.name.clone()).collect() };
        move_preset(&mut b, Which::Tags, 2, 0).unwrap();
        assert_eq!(order(&b), ["C", "A", "B"]);
        assert!(move_preset(&mut b, Which::Tags, 0, 3).is_err());
        rename_preset(&mut b, Which::Tags, 1, " Z ").unwrap();
        assert_eq!(order(&b), ["C", "Z", "B"]);
        assert!(rename_preset(&mut b, Which::Tags, 1, "b").is_err());
        delete_preset(&mut b, Which::Tags, 0).unwrap();
        assert_eq!(order(&b), ["Z", "B"]);
        assert!(delete_preset(&mut b, Which::Tags, 2).is_err());
        assert!(delete_preset(&mut b, Which::Lines, 0).is_err());
        assert!(Which::parse("blocks").is_err());
    }

    #[test]
    fn a_block_keeps_the_checked_fields_and_the_setup_it_uses() {
        let mut s = Session::with_budget(budget(), None);
        let rows = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| crate::apply::make_range(b, &rows))
            .unwrap();
        s.edit("Test edit", |b| {
            let on = crate::notes::NoteOwner::Row(rows[0]);
            crate::notes::add(b, &on, "Prep and shoot", "", None).map(|_| ())
        })
        .unwrap();
        s.edit("Test edit", |b| {
            store_line_block(b, "Staff", &rows, BlockFields::default())
        })
        .unwrap();
        let p = s.budget().line_presets[0].clone();
        assert_eq!(p.rows.len(), 2);
        assert!(p.rows.iter().all(|d| d.id == 0));
        assert_eq!(p.rows[0].description, "Line producer");
        assert_eq!(p.rows[0].notes[0].text, "Prep and shoot");
        assert_eq!(p.rows[0].fringe_range, Some(1));
        assert_eq!(p.rows[1].fringe_range, Some(1));
        // Every setup item the rows use, with what those use in turn.
        assert!(p.setup.fringes.contains_key("PAYROLL"));
        assert!(p.setup.units.contains_key("Week"));
        let globals: Vec<&str> = p.setup.globals.keys().map(String::as_str).collect();
        assert_eq!(
            globals,
            ["PREP_WEEKS", "SHOOT_DAYS", "SHOOT_WEEKS", "WRAP_WEEKS"]
        );

        let bare = BlockFields {
            formulas: false,
            units: false,
            tags: false,
            notes: false,
        };
        s.edit("Test edit", |b| store_line_block(b, "Bare", &rows, bare))
            .unwrap();
        let p = &s.budget().line_presets[1];
        for d in &p.rows {
            assert!(d.amount.is_blank() && d.rate.is_blank() && d.unit.is_empty());
            assert!(d.fringes.is_empty() && d.fringe_range.is_none() && d.notes.is_empty());
            assert!(!d.description.is_empty());
        }
        assert!(p.setup.is_empty());

        let before = s.budget().clone();
        for (name, ids, want) in [
            ("Staff", rows.clone(), "already"),
            ("New", vec![], "Select"),
            ("New", vec![rows[0], 999_999], "999999"),
            ("", rows.clone(), "name"),
        ] {
            let err = s
                .edit("Test edit", |b| {
                    store_line_block(b, name, &ids, BlockFields::default())
                })
                .unwrap_err();
            assert!(err.contains(want), "{err}");
            assert_eq!(s.budget(), &before);
        }
    }

    /// A block is reusable, so it never keeps a person (decisions.md, entry
    /// 79): pasted or inserted from the library, its rows would join that
    /// person's cutoff without anyone saying so.
    #[test]
    fn a_stored_block_never_keeps_a_person() {
        let mut s = Session::with_budget(budget(), None);
        let rows = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| {
            crate::edit::set_rows_field(b, &rows, crate::edit::Field::Person, "Mara Lee")
        })
        .unwrap();
        s.edit("Test edit", |b| {
            store_line_block(b, "Staff", &rows, BlockFields::default())
        })
        .unwrap();
        let bare = BlockFields {
            formulas: false,
            units: false,
            tags: false,
            notes: false,
        };
        s.edit("Test edit", |b| store_line_block(b, "Bare", &rows, bare))
            .unwrap();
        for p in &s.budget().line_presets {
            assert!(p.rows.iter().all(|d| d.person.is_none()), "{}", p.name);
        }
        // The lines themselves keep theirs.
        assert!(crate::edit::details(s.budget())
            .filter(|d| rows.contains(&d.id))
            .all(|d| d.person.as_deref() == Some("Mara Lee")));
    }

    #[test]
    fn a_block_goes_in_above_the_cursor_with_new_ids_as_one_step() {
        let mut s = Session::with_budget(budget(), None);
        let staff = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| crate::apply::make_range(b, &staff))
            .unwrap();
        s.edit("Test edit", |b| {
            store_line_block(b, "Staff", &staff, BlockFields::default())
        })
        .unwrap();
        let camera = ids(s.budget(), "21-01");
        // Camera already has a range numbered 1.
        s.edit("Test edit", |b| crate::apply::make_range(b, &camera))
            .unwrap();
        let total_before = s.selection(&camera).lines.details_total;
        let before = s.budget().clone();

        // Nothing is missing: no review.
        let p = block_preview(s.budget(), 0).unwrap();
        assert!(!p.review && p.items.is_empty());
        assert_eq!((p.what, p.count, p.source.as_str()), ("rows", 2, "Staff"));

        let pasted = s
            .edit("Test edit", |b| {
                insert_block(b, 0, "21-01", Some(camera[1]))
            })
            .unwrap();
        let now = ids(s.budget(), "21-01");
        assert_eq!(now.len(), 4);
        assert_eq!(now[0], camera[0]);
        assert_eq!(&now[1..3], &pasted.rows[..]);
        assert_eq!(now[3], camera[1]);
        let highest = before.highest_row_id().unwrap();
        assert!(pasted.rows.iter().all(|&id| id > highest));
        let b = s.budget().clone();
        assert_eq!(row(&b, pasted.rows[0]).description, "Line producer");
        assert_eq!(
            row(&b, pasted.rows[0]).fringe_range,
            Some(2),
            "new in 21-01"
        );
        assert_eq!(row(&b, pasted.rows[1]).fringe_range, Some(2));
        assert_eq!(row(&b, camera[0]).fringe_range, Some(1));
        assert!(s.selection(&pasted.rows).lines.details_total > 0);
        assert_eq!(s.selection(&camera).lines.details_total, total_before);
        // One undo step takes all of it out.
        assert!(s.undo());
        assert_eq!(s.budget(), &before);

        // At the end of an account, and refusals that change nothing.
        let pasted = s
            .edit("Test edit", |b| insert_block(b, 0, "50-01", None))
            .unwrap();
        assert_eq!(ids(s.budget(), "50-01")[1..], pasted.rows[..]);
        s.undo();
        for (index, account, at, want) in [
            (3, "21-01", None, "no longer exists"),
            (0, "99-99", None, "99-99"),
            (0, "21-01", Some(staff[0]), "not in account"),
        ] {
            let err = s
                .edit("Test edit", |b| insert_block(b, index, account, at))
                .unwrap_err();
            assert!(err.contains(want), "{err}");
            assert_eq!(s.budget(), &before);
        }
    }

    #[test]
    fn views_list_blocks_without_their_rows() {
        let mut s = Session::with_budget(budget(), None);
        let staff = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| store_tag_preset(b, None, crew()))
            .unwrap();
        s.edit("Test edit", |b| {
            store_line_block(b, "Staff", &staff, BlockFields::default())
        })
        .unwrap();
        for v in [
            serde_json::to_value(s.view_with(None)).unwrap(),
            serde_json::to_value(s.full_view()).unwrap(),
        ] {
            let b = &v["budget"];
            assert_eq!(
                b["tag_presets"][0],
                method_format::tag_preset_to_value(&crew())
            );
            assert_eq!(
                b["line_presets"],
                serde_json::json!([{"name": "Staff", "count": 2}])
            );
        }
        // Taken out for the view and put back.
        assert_eq!(s.budget().line_presets[0].rows.len(), 2);
    }

    #[test]
    fn missing_setup_is_listed_and_added_back_while_the_budgets_own_wins() {
        let mut s = Session::with_budget(budget(), None);
        let staff = ids(s.budget(), "20-01");
        s.edit("Test edit", |b| {
            store_line_block(b, "Staff", &staff, BlockFields::default())
        })
        .unwrap();
        // Since then: PAYROLL deleted (it comes off the lines), the Week unit
        // deleted, and a global changed.
        s.edit("Test edit", |b| edit::delete_fringe(b, "PAYROLL"))
            .unwrap();
        s.edit("Test edit", |b| {
            b.units.remove("Week");
            b.globals.insert("PREP_WEEKS".into(), "6".into());
            Ok(())
        })
        .unwrap();
        let p = block_preview(s.budget(), 0).unwrap();
        assert!(p.review);
        let listed: Vec<(&str, &str, &str)> = p
            .items
            .iter()
            .map(|i| (i.kind, i.name.as_str(), i.status))
            .collect();
        assert_eq!(
            listed,
            [("fringe", "PAYROLL", "add"), ("unit", "Week", "add")],
            "the changed global is not listed: this budget's own wins"
        );

        let before = s.budget().clone();
        let pasted = s
            .edit("Test edit", |b| insert_block(b, 0, "21-01", None))
            .unwrap();
        let b = s.budget().clone();
        assert!(b.fringes.contains_key("PAYROLL"));
        assert!(b.units.contains_key("Week"));
        assert_eq!(b.globals["PREP_WEEKS"].0, "6");
        assert_eq!(row(&b, pasted.rows[0]).fringes, names(&["PAYROLL"]));
        // Rows and setup items, one undo step.
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
    }
}
