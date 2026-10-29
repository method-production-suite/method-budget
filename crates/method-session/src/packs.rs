//! Setup packs (decisions.md, entry 50; docs/APP.md, "Setup packs"): the
//! review of importing a pack into a budget or the library, importing it,
//! the items a pack can be made from, and making one.
//!
//! - **Import.** Import Setup's review and rules (src/import_setup.rs): per
//!   item keep, take or add under a new name, dependencies ticked along, one
//!   undo step into a budget. The review also carries the pack's details
//!   and each item's source, and what comes in is stamped with the pack's
//!   title, version and effective date. Budget settings never come from a
//!   pack.
//! - **Export.** The page lists the source's items by kind and folder (the
//!   Library tab's own view of any budget) and sends the picks and the
//!   pack's details; the dependencies are walked again
//!   (method_format::setup::pack::build).
//!
//! The Packs list (the user's Packs folder and the bundled packs), the
//! dialogs, reading and writing pack files and the library's file are the
//! desktop's (method-app's src/packs.rs).

use method_core::Budget;
use method_format::setup::import::What;
use method_format::setup::pack::{self, PackDetails, PackSummary};
use serde::Serialize;

use crate::docs::{DocId, Docs};
use crate::import_setup::{self, ImportPreview, ImportSources, ItemRef, Resolution};
use crate::library::{self, LibraryKind};
use crate::messages::{msg, word};
use crate::session::Session;
use crate::view::View;

// ---------------------------------------------------------------------------
// The review, into a budget or the library

/// A review of importing a pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackReview {
    /// Import Setup's review; its `token` is what the import takes.
    #[serde(flatten)]
    pub preview: ImportPreview,
    /// The pack's details and each item's source.
    pub pack: PackSummary,
    /// `budget` or `library`.
    pub into: &'static str,
    /// The pack, kept for a library import (0 for a budget import, whose
    /// `token` names the pack itself).
    pub source_token: u64,
}

/// `p` without the budget settings, which never come from a pack.
pub fn without_settings(mut p: ImportPreview) -> ImportPreview {
    p.items
        .retain(|it| What::parse(it.kind).map(pack::pack_kind).unwrap_or(false));
    p
}

/// The review of importing `src` (named `label`) into budget `target`; the
/// pack is kept under the review's token.
pub fn review_into_budget(
    docs: &Docs,
    sources: &ImportSources,
    target: DocId,
    label: String,
    src: Budget,
) -> Result<PackReview, String> {
    let here = docs
        .get(target)
        .ok_or_else(|| word("engine.docs.closed"))?
        .budget();
    let preview = without_settings(import_setup::preview(here, &src, 0, &label, ""));
    let summary = pack::summary(&src);
    let token = sources.keep(label, src);
    Ok(PackReview {
        preview: ImportPreview { token, ..preview },
        pack: summary,
        into: "budget",
        source_token: 0,
    })
}

/// Refuses picks of budget settings.
pub fn check_picks(picks: &[ItemRef]) -> Result<(), String> {
    for p in picks {
        if !pack::pack_kind(What::parse(&p.kind)?) {
            return Err(msg(
                "engine.packs.no_settings",
                &[("setting", &p.kind.replace('_', " "))],
            ));
        }
    }
    Ok(())
}

/// The sentence after a pack import.
pub fn imported_message(c: &import_setup::Imported, title: &str, into_library: bool) -> String {
    if c.added == 0 && c.replaced == 0 {
        return word(if into_library {
            "engine.packs.nothing_library"
        } else {
            "engine.packs.nothing_budget"
        });
    }
    let changes = library::copied_changes(c, into_library);
    if title.is_empty() {
        msg("engine.packs.imported", &[("changes", &changes)])
    } else {
        msg(
            "engine.packs.imported_from",
            &[("title", &title), ("changes", &changes)],
        )
    }
}

// ---------------------------------------------------------------------------
// Export

/// The items a pack can be made from, by kind with their folders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackItems {
    /// The budget's or the library's name.
    pub source: String,
    /// Its title, for the pack's title to start from.
    pub title: String,
    pub kinds: Vec<LibraryKind>,
}

/// What `pack_items` shows for `b`.
pub fn items_of(name: String, b: &Budget) -> PackItems {
    PackItems {
        source: name,
        title: b.title.clone(),
        kinds: library::view_of(b)
            .into_iter()
            .filter(|k| What::parse(k.kind).map(pack::pack_kind).unwrap_or(false))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/packs.rs and the host's src/host/packs.rs call
// these)

/// What `pack_export` wrote.
#[derive(Debug, Serialize)]
pub struct PackExported {
    pub name: String,
    pub message: String,
}

/// What `pack_import` did: the budget's view, and a sentence.
#[derive(Debug, Serialize)]
pub struct PackImported {
    pub view: View,
    pub message: String,
}

/// What `pack_import_library` did: the library, and a sentence.
#[derive(Debug, Serialize)]
pub struct PackIntoLibrary {
    pub library: library::LibraryView,
    pub message: String,
}

/// A setup pack of `picks` (and what they need) from budget `b`, with
/// `details` and `created` (the page's clock), and how many items it holds.
pub fn pack_build(
    b: &Budget,
    picks: &[ItemRef],
    details: &PackDetails,
    created: &str,
) -> Result<(Budget, usize), String> {
    let (built, done) = pack::build(b, picks, details, created, crate::sharing::APP_VERSION)?;
    Ok((built, done.written.len()))
}

/// The sentence after a pack named `name` is saved with `items` items.
pub fn exported_message(name: &str, items: usize) -> String {
    msg("engine.packs.saved", &[("name", &name), ("count", &items)])
}

/// Imports into budget `s` what review `token` showed, stamped with the
/// pack, as one undo step. The sentence saying what came in.
pub fn pack_import(
    s: &mut Session,
    sources: &ImportSources,
    token: u64,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<String, String> {
    check_picks(picks)?;
    let (_, src) = sources
        .get(token)
        .ok_or_else(|| word("engine.import.picked_since"))?;
    let title = method_core::pack::info(&src)
        .map(|i| i.title.clone())
        .unwrap_or_default();
    let counts = s.edit(
        crate::messages::word("engine.undo.import_setup_pack"),
        |b| pack::import_pack(b, &src, picks, resolutions),
    )?;
    Ok(imported_message(&counts, &title, false))
}
