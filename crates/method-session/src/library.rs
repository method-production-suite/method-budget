//! The personal setup library's review and copying (roadmap 2.15;
//! decisions.md, entries 15, 29 and 37): what the Library tab shows of a
//! library, the review of copying items between a budget and the library
//! (Import Setup's review without the budget settings), the copying both
//! ways, and the library's folders. A library is an ordinary budget with
//! setup and no lines.
//!
//! Which file is the library, reading and writing it, and refusing changes
//! while it is open as a tab are the desktop's (method-app's
//! src/library.rs): a library lives in a file the user picked.

use method_core::{folders, Budget};
use serde::Serialize;

use crate::docs::{DocId, Docs};
use crate::import_setup::{
    self, all_items, closure, describe, item_ref, ImportPreview, Imported, ItemRef, Resolution,
    What,
};
use crate::session::Session;
use crate::view::View;

/// The file a new library gets, in the folder the user picks.
pub const LIBRARY_FILE_NAME: &str = "My Method Budget Library.mthd";
/// A new library's title.
pub const LIBRARY_TITLE: &str = "My Method Budget Library";

/// A new, empty library: no lines, no units, no setup.
pub fn new_library(base_currency: &str) -> Budget {
    let mut b = Budget::new(LIBRARY_TITLE, base_currency);
    b.units.clear();
    b
}

/// Whether kind `w` can be a library item (everything but the settings).
pub fn library_kind(w: What) -> bool {
    !w.is_setting()
}

// ---------------------------------------------------------------------------
// A library open as a tab (the desktop refuses changes to it there)

/// What a tab is called, for messages.
pub fn tab_name(docs: &Docs, id: DocId) -> String {
    docs.get(id)
        .map(|s| s.tab_label())
        .unwrap_or_else(|| "it".into())
}

/// The message for a change refused because the library is open as a tab.
pub fn in_tab_refusal(name: &str) -> String {
    format!(
        "Your library is open as the budget tab \u{201c}{name}\u{201d}. Make the change there, or close that tab and try again."
    )
}

// ---------------------------------------------------------------------------
// What the Library tab shows

/// One item in the library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryItem {
    pub name: String,
    /// Its folder, or none (unfiled).
    pub folder: Option<String>,
    /// Its definition, in words.
    pub definition: String,
    /// Every other library item it needs, however deep: they come along.
    pub needs: Vec<ItemRef>,
}

/// One kind of item, with its folders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryKind {
    /// `global`, `fringe`, `group`, `unit`, `set`, `location`, `currency`,
    /// `charge`, `credit`, `tag_preset`, `line_preset` or `report_preset`.
    pub kind: &'static str,
    /// Folder names, in name order (empty ones too).
    pub folders: Vec<String>,
    pub items: Vec<LibraryItem>,
}

/// The library, for the Library tab.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LibraryView {
    /// A library file has been picked.
    pub configured: bool,
    /// Its file name without its extension.
    pub name: String,
    /// The folder it is in.
    pub location: String,
    /// Why it couldn't be read (moved, deleted, not a budget).
    pub problem: Option<String>,
    /// Open as a budget tab: read from there, and changed only there.
    pub in_tab: bool,
    pub kinds: Vec<LibraryKind>,
}

/// The view of library `lib`.
pub fn view_of(lib: &Budget) -> Vec<LibraryKind> {
    let items = all_items(lib);
    What::ALL
        .into_iter()
        .filter(|w| library_kind(*w))
        .map(|w| {
            let kind = w.key();
            let folders = lib.setup_folders.of(kind).map(|(f, _)| f.clone()).collect();
            let items = items
                .iter()
                .filter(|(iw, _)| *iw == w)
                .map(|(_, name)| {
                    let key = (w, name.clone());
                    let mut deps = closure(lib, [key.clone()]);
                    deps.remove(&key);
                    LibraryItem {
                        name: name.clone(),
                        folder: folders::folder_of(lib, kind, name).map(str::to_string),
                        definition: describe(lib, w, name),
                        needs: deps.iter().map(item_ref).collect(),
                    }
                })
                .collect();
            LibraryKind {
                kind,
                folders,
                items,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The review, both ways

/// A review of copying items between a budget and the library.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryPreview {
    #[serde(flatten)]
    pub preview: ImportPreview,
    /// The items to tick at the start (what the user picked to copy).
    pub picked: Vec<ItemRef>,
}

/// The review for copying from `src` into `here`, without the budget
/// settings. With `picks`, only those items and what they need, the picks
/// ticked; without, every item and none ticked.
pub fn review(
    here: &Budget,
    src: &Budget,
    token: u64,
    source: &str,
    picks: Option<&[ItemRef]>,
) -> Result<LibraryPreview, String> {
    let mut p = import_setup::preview(here, src, token, source, "");
    p.items
        .retain(|it| What::parse(it.kind).map(library_kind).unwrap_or(false));
    let Some(picks) = picks else {
        return Ok(LibraryPreview {
            preview: p,
            picked: Vec::new(),
        });
    };
    let mut keep: std::collections::BTreeSet<(String, String)> = Default::default();
    let mut picked = Vec::new();
    for pick in picks {
        let w = What::parse(&pick.kind)?;
        if !library_kind(w) {
            return Err(format!(
                "A {} isn't kept in the library.",
                w.key().replace('_', " ")
            ));
        }
        let Some(it) = p
            .items
            .iter()
            .find(|it| it.kind == pick.kind && it.name == pick.name)
        else {
            continue; // e.g. the base currency there, which never copies
        };
        keep.insert((it.kind.to_string(), it.name.clone()));
        for n in &it.needs {
            keep.insert((n.kind.clone(), n.name.clone()));
        }
        picked.push(pick.clone());
    }
    if picked.is_empty() {
        return Err("Nothing to copy: that item never copies between budgets.".into());
    }
    p.items
        .retain(|it| keep.contains(&(it.kind.to_string(), it.name.clone())));
    Ok(LibraryPreview { preview: p, picked })
}

/// The sentence after a copy (`saving`: into the library).
pub fn copied_message(c: &Imported, saving: bool) -> String {
    let (head, nothing, holder) = if saving {
        (
            "Saved to the library",
            "Nothing to save: the library already has what was picked.",
            "the library",
        )
    } else {
        (
            "Added from the library",
            "Nothing to add: this budget already has what was picked.",
            "this budget",
        )
    };
    if c.added == 0 && c.replaced == 0 {
        return nothing.to_string();
    }
    let n = |k: usize| format!("{k} {}", if k == 1 { "item" } else { "items" });
    let mut parts = Vec::new();
    if c.added > 0 {
        parts.push(format!("{} added", n(c.added)));
    }
    if c.replaced > 0 {
        parts.push(format!("{} replaced", n(c.replaced)));
    }
    if c.kept > 0 {
        parts.push(format!(
            "{} kept as {holder} had {}",
            n(c.kept),
            if c.kept == 1 { "it" } else { "them" }
        ));
    }
    format!("{head}: {}.", parts.join(", "))
}

/// Refuses picks of the budget settings (Rust checks what the page sends).
pub fn check_picks(picks: &[ItemRef]) -> Result<(), String> {
    for p in picks {
        if !library_kind(What::parse(&p.kind)?) {
            return Err(format!(
                "A {} isn't kept in the library.",
                p.kind.replace('_', " ")
            ));
        }
    }
    Ok(())
}

/// Copies `picks` (and what they need) from library `lib` into budget `b`
/// as `resolutions` say; what differs keeps the budget's version unless
/// chosen otherwise.
pub fn add_from(
    b: &mut Budget,
    lib: &Budget,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<Imported, String> {
    check_picks(picks)?;
    import_setup::import(b, lib, picks, resolutions)
}

/// Copies `picks` (and what they need) from budget `b` into library `lib`
/// as `resolutions` say; what differs keeps the library's version unless
/// chosen otherwise.
pub fn save_into(
    lib: &mut Budget,
    b: &Budget,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<Imported, String> {
    check_picks(picks)?;
    import_setup::import(lib, b, picks, resolutions)
}

/// One folder change: `create` (`name`), `rename` (`name` to `to`) or
/// `delete` (`name`; its items become unfiled).
pub fn folder_op(
    lib: &mut Budget,
    kind: &str,
    action: &str,
    name: &str,
    to: Option<&str>,
) -> Result<String, String> {
    match action {
        "create" => folders::add_folder(lib, kind, name).map(|n| format!("Made the folder {n}.")),
        "rename" => folders::rename_folder(lib, kind, name, to.unwrap_or(""))
            .map(|n| format!("Renamed the folder {name} to {n}.")),
        "delete" => folders::delete_folder(lib, kind, name).map(|n| match n {
            0 => format!("Deleted the folder {name}."),
            1 => format!("Deleted the folder {name}; its item is unfiled now."),
            n => format!("Deleted the folder {name}; its {n} items are unfiled now."),
        }),
        other => Err(format!("unknown folder action `{other}`")),
    }
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/library.rs reads the library from its file
// and calls these)

/// Adds `picks` from library `lib` (as its review saw it) to the budget as
/// one undo step. The sentence saying what was added.
pub fn library_add(
    s: &mut Session,
    lib: &Budget,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<String, String> {
    let counts = s.edit(crate::messages::word("engine.undo.add_from_library"), |b| {
        add_from(b, lib, picks, resolutions)
    })?;
    Ok(copied_message(&counts, false))
}

/// The library after a change, and a sentence saying what changed.
#[derive(Debug, Serialize)]
pub struct LibraryDone {
    pub library: LibraryView,
    pub message: String,
}

/// What `library_add` did: the budget's view and a sentence.
#[derive(Debug, Serialize)]
pub struct LibraryAdded {
    pub view: View,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    #[test]
    fn the_review_leaves_out_the_settings_and_follows_the_picks() {
        let lib = new_library("USD");
        assert!(view_of(&lib).iter().all(|k| k.items.is_empty()));
        assert!(view_of(&lib).iter().all(|k| k.kind != "settings"));

        let mut b = method_core::starter::budget();
        b.globals.insert("DAYS".into(), "20".into());
        let all = review(&lib, &b, 7, "Pilot", None).unwrap();
        assert!(all.picked.is_empty());
        assert!(all
            .preview
            .items
            .iter()
            .all(|it| What::parse(it.kind).map(library_kind).unwrap_or(false)));
        let one = review(&lib, &b, 7, "Pilot", Some(&[r("global", "DAYS")])).unwrap();
        assert_eq!(one.picked, vec![r("global", "DAYS")]);
        assert_eq!(one.preview.items.len(), 1);

        let mut lib = lib;
        let done = save_into(&mut lib, &b, &[r("global", "DAYS")], &[]).unwrap();
        assert_eq!(
            copied_message(&done, true),
            "Saved to the library: 1 item added."
        );
        assert_eq!(
            folder_op(&mut lib, "global", "create", "Schedule", None).unwrap(),
            "Made the folder Schedule."
        );
        assert!(folder_op(&mut lib, "global", "paint", "x", None).is_err());
    }
}
