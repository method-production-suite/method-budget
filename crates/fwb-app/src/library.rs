//! The personal setup library (roadmap 2.15; decisions.md, entries 15, 29
//! and 37).
//!
//! - **A library is a budget file.** `My Method Budget Library.mthd` (or any
//!   budget file the user picks, `.fwb` ones included; a folder that already
//!   has a library under its pre-rename name keeps using it): our
//!   ordinary format with setup and no lines, plus
//!   folders per kind (`setup_folders`, fwb_core::folders). It starts empty:
//!   no units, fringes or anything else; we ship no library data. It opens
//!   in the app like any other budget.
//! - **Remembered per user**, in `library.json` in the app's local data
//!   folder (next to `settings.json`), written atomically. The path is picked
//!   in Rust's dialogs and never comes from the page.
//! - **Copying in and out reuses Import Setup** (src/import_setup.rs): the
//!   same dependency walk, conflict review and renaming. Adding to a budget
//!   is one undo step there, keeping the budget's version of what differs
//!   unless the user picks otherwise. Saving to the library runs the same
//!   import the other way: the library keeps its version unless the user
//!   picks "replace" or "add under a new name". The three budget settings are
//!   not library items. No live link: each copy is a one-time copy.
//! - **Library writes** read the file, change it and write it back through
//!   a temporary file (`safety::write_atomically`), with every budget locked
//!   meanwhile.
//! - **Open as a tab**: when the library file is also open as a budget tab,
//!   the library is read from that tab (its unsaved changes count) and every
//!   change to it from the Library tab is refused with a message: edit it in
//!   its tab, or close the tab. That keeps one writer per file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use fwb_core::{folders, Budget};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::docs::Docs;
use crate::import_setup::{
    self, all_items, closure, describe, item_ref, ImportPreview, Imported, ItemRef, Resolution,
    What,
};
use crate::safety::{self, Access};
use crate::view::View;

/// The file a new library gets, in the folder the user picks.
pub const LIBRARY_FILE_NAME: &str = "My Method Budget Library.mthd";
/// What a new library was called before the rename to Method Budget: a
/// folder that has one (and no new one) keeps using it.
pub const LEGACY_LIBRARY_FILE_NAME: &str = "My Film Wizard Library.fwb";
/// A new library's title.
pub const LIBRARY_TITLE: &str = "My Method Budget Library";

const NO_LIBRARY: &str = "No library yet. Create one or choose a library file first.";

/// A new, empty library: no lines, no units, no setup.
pub fn new_library(base_currency: &str) -> Budget {
    let mut b = Budget::new(LIBRARY_TITLE, base_currency);
    b.units.clear();
    b
}

/// Whether kind `w` can be a library item (everything but the settings).
fn library_kind(w: What) -> bool {
    !w.is_setting()
}

// ---------------------------------------------------------------------------
// Reading and writing the library file

/// What a tab is called, for messages.
pub(crate) fn tab_name(docs: &Docs, id: DocId) -> String {
    docs.get(id)
        .map(|s| s.tab_label())
        .unwrap_or_else(|| "it".into())
}

/// The message for a change refused because the library is open as a tab.
pub(crate) fn in_tab_refusal(name: &str) -> String {
    format!(
        "Your library is open as the budget tab \u{201c}{name}\u{201d}. Make the change there, or close that tab and try again."
    )
}

/// The library in `path`: from its tab when it is open (unsaved changes
/// count), else read from the file. Also says which tab, if any.
pub fn read(docs: &Docs, path: &Path) -> Result<(Budget, Option<DocId>), String> {
    if let Some(id) = docs.find_file(path) {
        let s = docs.get(id).ok_or("That budget has been closed.")?;
        return Ok((s.budget().clone(), Some(id)));
    }
    let (b, _) = crate::session::read_budget(path)?;
    Ok((b, None))
}

/// Writes library `b` to `path` atomically (a failed write leaves the old
/// file as it was).
pub fn write(path: &Path, b: &Budget) -> Result<(), String> {
    let text = fwb_format::to_string(b);
    safety::write_atomically(path, text.as_bytes(), Access::Document)
        .map_err(|e| format!("Couldn't save the library {}: {e}", path.display()))
}

/// Changes the library in `path` with `f` and writes it back. Refused
/// (nothing written) when it is open as a tab or `f` refuses.
pub fn change<T>(
    docs: &Docs,
    path: &Path,
    f: impl FnOnce(&mut Budget) -> Result<T, String>,
) -> Result<(T, Budget), String> {
    if let Some(id) = docs.find_file(path) {
        return Err(in_tab_refusal(&tab_name(docs, id)));
    }
    let (mut b, _) = crate::session::read_budget(path)?;
    let t = f(&mut b)?;
    write(path, &b)?;
    Ok((t, b))
}

/// Makes a new library in folder `dir` (based on `base_currency`), or uses
/// the library already there. Returns its path and whether it was there.
pub fn create_in(dir: &Path, base_currency: &str) -> Result<(PathBuf, bool), String> {
    let path = dir.join(LIBRARY_FILE_NAME);
    let legacy = dir.join(LEGACY_LIBRARY_FILE_NAME);
    for existing in [&path, &legacy] {
        if existing.exists() {
            crate::session::read_budget(existing)?;
            return Ok((existing.clone(), true));
        }
    }
    write(&path, &new_library(base_currency))?;
    Ok((path, false))
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

/// The Library tab's view of the library in `path` (`None`: none picked).
pub fn library_view_at(docs: &Docs, path: Option<&Path>) -> LibraryView {
    let Some(path) = path else {
        return LibraryView::default();
    };
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let location = path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let base = LibraryView {
        configured: true,
        name,
        location,
        ..LibraryView::default()
    };
    match read(docs, path) {
        Ok((lib, tab)) => LibraryView {
            in_tab: tab.is_some(),
            kinds: view_of(&lib),
            ..base
        },
        Err(e) => LibraryView {
            problem: Some(e),
            ..base
        },
    }
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
fn copied_message(c: &Imported, saving: bool) -> String {
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
pub(crate) fn check_picks(picks: &[ItemRef]) -> Result<(), String> {
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
// The app's state: which file is the library, and the reviews in flight

#[derive(Default, Serialize, Deserialize)]
struct LibraryJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[derive(Default)]
struct Inner {
    path: Option<PathBuf>,
    /// Counts reviews, so each gets its own token.
    token: u64,
    /// The library as the last review saw it.
    reviewed: Option<(u64, Arc<Budget>)>,
}

/// The library file and the open review (one per app).
#[derive(Default)]
pub struct Library {
    config: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl Library {
    /// Reads which library was in use from `library.json` in `dir`.
    pub fn new(dir: Option<&Path>) -> Library {
        let config = dir.map(|d| d.join("library.json"));
        let path = config
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<LibraryJson>(&t).ok())
            .and_then(|j| j.path)
            .map(PathBuf::from);
        Library {
            config,
            inner: Mutex::new(Inner {
                path,
                ..Inner::default()
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The library file, if one was picked.
    pub fn path(&self) -> Option<PathBuf> {
        self.lock().path.clone()
    }

    /// Uses the library in `path` from now on, and remembers it.
    pub fn set_path(&self, path: PathBuf) -> Result<(), String> {
        if let Some(f) = &self.config {
            if let Some(dir) = f.parent() {
                safety::create_private_dir(dir).map_err(|e| e.to_string())?;
            }
            let json = LibraryJson {
                path: Some(path.display().to_string()),
            };
            let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
            safety::write_atomically(f, &bytes, Access::Private)
                .map_err(|e| format!("Couldn't remember the library: {e}"))?;
        }
        let mut g = self.lock();
        g.path = Some(path);
        g.reviewed = None;
        Ok(())
    }

    /// Keeps the library as a review saw it; returns the review's token.
    pub(crate) fn keep(&self, lib: Budget) -> u64 {
        let mut g = self.lock();
        g.token += 1;
        let t = g.token;
        g.reviewed = Some((t, Arc::new(lib)));
        t
    }

    /// The library as review `token` saw it.
    pub(crate) fn reviewed(&self, token: u64) -> Result<Arc<Budget>, String> {
        let g = self.lock();
        g.reviewed
            .as_ref()
            .filter(|(t, _)| *t == token)
            .map(|(_, b)| Arc::clone(b))
            .ok_or_else(|| "The library changed since. Try again.".to_string())
    }

    /// Forgets the review (the library was just written).
    pub(crate) fn changed(&self) {
        self.lock().reviewed = None;
    }

    pub(crate) fn required(&self) -> Result<PathBuf, String> {
        self.path().ok_or_else(|| NO_LIBRARY.to_string())
    }
}

/// Puts the library state in the app (from `app.rs`'s setup).
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let dir = app.path().app_local_data_dir().ok();
    app.manage(Library::new(dir.as_deref()));
}

/// Review for adding from the library into budget `id` (`picks`: the items
/// picked in the Library tab).
pub fn add_review(
    lib_state: &Library,
    docs: &Docs,
    target: DocId,
    picks: &[ItemRef],
) -> Result<LibraryPreview, String> {
    let path = lib_state.required()?;
    let (lib, tab) = read(docs, &path)?;
    if tab == Some(target) {
        return Err("This budget is your library. Switch to another budget to add to it.".into());
    }
    let here = docs
        .get(target)
        .ok_or("That budget has been closed.")?
        .budget();
    let mut p = review(here, &lib, 0, "your library", Some(picks))?;
    p.preview.token = lib_state.keep(lib);
    Ok(p)
}

/// Review for saving from budget `id` into the library (`picks`: an item
/// picked in a setup list, or none for the whole list).
pub fn save_review(
    lib_state: &Library,
    docs: &Docs,
    source: DocId,
    picks: Option<&[ItemRef]>,
) -> Result<LibraryPreview, String> {
    let path = lib_state.required()?;
    if let Some(id) = docs.find_file(&path) {
        return Err(in_tab_refusal(&tab_name(docs, id)));
    }
    let (lib, _) = crate::session::read_budget(&path)?;
    let s = docs.get(source).ok_or("That budget has been closed.")?;
    let mut p = review(&lib, s.budget(), 0, &s.tab_label(), picks)?;
    p.preview.token = lib_state.keep(lib);
    Ok(p)
}

/// Saves into the library what review `token` showed; refused when the
/// library changed since. Returns the counts and the library as written.
pub fn save_reviewed(
    lib_state: &Library,
    docs: &Docs,
    source: &Budget,
    token: u64,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<(Imported, Budget), String> {
    let path = lib_state.required()?;
    let seen = lib_state.reviewed(token)?;
    let out = change(docs, &path, |lib| {
        if *lib != *seen {
            return Err("The library changed since. Try again.".into());
        }
        save_into(lib, source, picks, resolutions)
    })?;
    lib_state.changed();
    Ok(out)
}

/// A folder change or a move, written to the library.
fn change_library(
    lib_state: &Library,
    docs: &Docs,
    f: impl FnOnce(&mut Budget) -> Result<String, String>,
) -> Result<LibraryDone, String> {
    let path = lib_state.required()?;
    let (message, _) = change(docs, &path, f)?;
    lib_state.changed();
    Ok(LibraryDone {
        library: library_view_at(docs, Some(&path)),
        message,
    })
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs, capabilities/main.json and
// capabilities/tools.json: the Library tab works in the tools window too)

/// The library after a change, and a sentence saying what changed.
#[derive(Debug, Serialize)]
pub struct LibraryDone {
    pub library: LibraryView,
    pub message: String,
}

/// The Library tab's view of the library.
#[tauri::command]
pub fn library_view(state: State<'_, AppState>, library: State<'_, Library>) -> LibraryView {
    let path = library.path();
    library_view_at(&state.docs(), path.as_deref())
}

/// Asks for a folder (Rust's dialog) and makes `My Method Budget Library.mthd`
/// in it, or uses the one already there (or the pre-rename one); it becomes the library. `None`
/// when cancelled.
#[tauri::command]
pub async fn library_create<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
) -> Result<Option<LibraryDone>, String> {
    let base = state.lock_doc(doc)?.budget().base_currency.clone();
    let Some(picked) = window
        .dialog()
        .file()
        .set_title("Choose a folder for your library")
        .set_parent(&window)
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let (path, existed) = create_in(&dir, &base)?;
    library.set_path(path.clone())?;
    let message = if existed {
        format!("Using the library already in {}.", dir.display())
    } else {
        format!("Made your library in {}.", dir.display())
    };
    Ok(Some(LibraryDone {
        library: library_view_at(&state.docs(), Some(&path)),
        message,
    }))
}

/// Asks for a library file (Rust's Open dialog) and uses it from now on.
/// `None` when cancelled.
#[tauri::command]
pub async fn library_choose<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    library: State<'_, Library>,
) -> Result<Option<LibraryDone>, String> {
    let start = library
        .path()
        .as_deref()
        .and_then(Path::parent)
        .map(PathBuf::from);
    let Some(path) = crate::app::pick_budget(&window, "Choose your library", start)? else {
        return Ok(None);
    };
    read(&state.docs(), &path)?;
    library.set_path(path.clone())?;
    let view = library_view_at(&state.docs(), Some(&path));
    let message = format!("Using {} as your library.", view.name);
    Ok(Some(LibraryDone {
        library: view,
        message,
    }))
}

/// The review for adding the picked library items to budget `doc`.
#[tauri::command]
pub fn library_add_preview(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
    picks: Vec<ItemRef>,
) -> Result<LibraryPreview, String> {
    let s = state.lock_doc(doc)?;
    add_review(&library, s.docs(), s.id(), &picks)
}

/// What `library_add` did: the budget's view and a sentence.
#[derive(Debug, Serialize)]
pub struct LibraryAdded {
    pub view: View,
    pub message: String,
}

/// Adds what review `token` showed to budget `doc`, as one undo step.
#[tauri::command]
pub fn library_add<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<LibraryAdded, String> {
    let lib = library.reviewed(token)?;
    let mut s = state.lock_doc(doc)?;
    let counts = s.edit("Add from library", |b| {
        add_from(b, &lib, &picks, &resolutions)
    })?;
    Ok(LibraryAdded {
        view: finish(&window, &mut s, None),
        message: copied_message(&counts, false),
    })
}

/// The review for saving budget `doc`'s items to the library: `picks` (an
/// item and what it needs), or every item when none.
#[tauri::command]
pub fn library_save_preview(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
    picks: Option<Vec<ItemRef>>,
) -> Result<LibraryPreview, String> {
    let s = state.lock_doc(doc)?;
    save_review(&library, s.docs(), s.id(), picks.as_deref())
}

/// Saves what review `token` showed from budget `doc` into the library file.
#[tauri::command]
pub fn library_save(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<LibraryDone, String> {
    let s = state.lock_doc(doc)?;
    let source = s.budget().clone();
    let (counts, _) = save_reviewed(&library, s.docs(), &source, token, &picks, &resolutions)?;
    let path = library.path();
    Ok(LibraryDone {
        library: library_view_at(s.docs(), path.as_deref()),
        message: copied_message(&counts, true),
    })
}

/// Makes, renames or deletes one of the library's folders of `kind`.
#[tauri::command]
pub fn library_folder(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    kind: String,
    action: String,
    name: String,
    to: Option<String>,
) -> Result<LibraryDone, String> {
    change_library(&library, &state.docs(), |lib| {
        folder_op(lib, &kind, &action, &name, to.as_deref())
    })
}

/// Moves library items of `kind` into `folder` (`None`: unfiled).
#[tauri::command]
pub fn library_move(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    kind: String,
    names: Vec<String>,
    folder: Option<String>,
) -> Result<LibraryDone, String> {
    change_library(&library, &state.docs(), |lib| {
        folders::move_items(lib, &kind, &names, folder.as_deref())?;
        let n = names.len();
        let what = if n == 1 {
            names[0].clone()
        } else {
            format!("{n} items")
        };
        Ok(match &folder {
            Some(f) => format!("Moved {what} to {f}."),
            None => format!("Moved {what} out of its folder."),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::tests::temp_dir;
    use crate::session::Session;
    use fwb_core::{Dec, Fringe, FringeKind, Group, UnitDef};

    fn r(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn res(kind: &str, name: &str, choice: &str) -> Resolution {
        Resolution {
            kind: kind.into(),
            name: name.into(),
            choice: choice.into(),
        }
    }

    /// A budget with a fringe that needs a unit and two globals.
    fn budget() -> Budget {
        let mut b = Budget::new("Show", "USD");
        b.globals.insert("DAYS".into(), "20".into());
        b.globals.insert("WEEKS".into(), "DAYS / 5".into());
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "WEEKS * 10".into(),
                kind: FringeKind::Flat {
                    unit: Some("Shift".into()),
                },
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Overtime".into(),
                in_budget_total: true,
                color: None,
            },
        );
        b
    }

    fn setup() -> (PathBuf, Library, AppState) {
        let dir = temp_dir("library");
        let lib = Library::new(Some(&dir.join("data")));
        let (path, existed) = create_in(&dir, "USD").unwrap();
        assert!(!existed);
        lib.set_path(path).unwrap();
        let state = AppState::with(Docs::with(Session::with_budget(budget(), None)));
        (dir, lib, state)
    }

    fn on_disk(lib: &Library) -> Budget {
        crate::session::read_budget(&lib.path().unwrap()).unwrap().0
    }

    #[test]
    fn a_folder_with_a_pre_rename_library_keeps_using_it() {
        let dir = temp_dir("library-legacy");
        let legacy = dir.join(LEGACY_LIBRARY_FILE_NAME);
        write(&legacy, &new_library("USD")).unwrap();
        let (path, existed) = create_in(&dir, "EUR").unwrap();
        assert!(existed);
        assert_eq!(path, legacy);
        assert!(!dir.join(LIBRARY_FILE_NAME).exists(), "no second library");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_library_is_empty_and_remembered() {
        let (dir, lib, state) = setup();
        let path = lib.path().unwrap();
        assert_eq!(path.file_name().unwrap(), LIBRARY_FILE_NAME);
        let b = on_disk(&lib);
        assert_eq!(b.title, LIBRARY_TITLE);
        assert!(b.units.is_empty() && b.fringes.is_empty() && b.globals.is_empty());
        assert!(b.topsheet.is_empty(), "no lines");
        // It opens like any budget.
        let mut s = Session::open(&path).unwrap();
        let _ = s.view();
        // Remembered for the next start.
        let again = Library::new(Some(&dir.join("data")));
        assert_eq!(again.path(), Some(path.clone()));
        // Making one in the same folder uses it.
        let (same, existed) = create_in(&dir, "EUR").unwrap();
        assert!(existed);
        assert_eq!(same, path);
        assert_eq!(on_disk(&lib).base_currency, "USD", "left as it was");
        let v = library_view_at(&state.docs(), Some(&path));
        assert!(v.configured && !v.in_tab && v.problem.is_none());
        assert_eq!(v.name, "My Method Budget Library");
        assert!(v.kinds.iter().all(|k| k.items.is_empty()));
        assert!(!v.kinds.iter().any(|k| k.kind == "account_coding"));
        // A library that isn't there says so.
        let gone = library_view_at(&state.docs(), Some(&dir.join("gone.fwb")));
        assert!(gone.problem.is_some());
        assert!(!library_view_at(&state.docs(), None).configured);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_brings_what_an_item_needs_and_keeps_the_librarys_version() {
        let (dir, lib, state) = setup();
        let docs = state.docs();
        let id = docs.active();
        let p = save_review(&lib, &docs, id, Some(&[r("fringe", "KIT")])).unwrap();
        let mut names: Vec<_> = p
            .preview
            .items
            .iter()
            .map(|i| format!("{} {}", i.kind, i.name))
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["fringe KIT", "global DAYS", "global WEEKS", "unit Shift"],
            "the pick and what it needs, nothing else"
        );
        assert_eq!(p.picked, [r("fringe", "KIT")]);
        let source = docs.get(id).unwrap().budget().clone();
        let (c, _) = save_reviewed(&lib, &docs, &source, p.preview.token, &p.picked, &[]).unwrap();
        assert_eq!(c.added, 4);
        assert_eq!(
            copied_message(&c, true),
            "Saved to the library: 4 items added."
        );
        let l = on_disk(&lib);
        assert!(l.fringes.contains_key("KIT") && l.units.contains_key("Shift"));
        assert_eq!(l.globals["WEEKS"].0, "DAYS / 5");
        // The same token can't be used twice: the library changed.
        assert!(save_reviewed(&lib, &docs, &source, p.preview.token, &p.picked, &[]).is_err());

        // Now the budget's DAYS differs: the library keeps its own by default.
        let mut changed = source.clone();
        changed.globals.insert("DAYS".into(), "25".into());
        drop(docs);
        state
            .lock()
            .edit("Test edit", |b| {
                b.globals.insert("DAYS".into(), "25".into());
                Ok(())
            })
            .unwrap();
        let docs = state.docs();
        let p = save_review(&lib, &docs, id, Some(&[r("global", "WEEKS")])).unwrap();
        let days = p.preview.items.iter().find(|i| i.name == "DAYS").unwrap();
        assert_eq!(days.status, "differs");
        assert_eq!(days.rename_to.as_deref(), Some("DAYS_2"));
        let (c, _) = save_reviewed(&lib, &docs, &changed, p.preview.token, &p.picked, &[]).unwrap();
        assert_eq!((c.added, c.replaced, c.kept), (0, 0, 1));
        assert_eq!(on_disk(&lib).globals["DAYS"].0, "20");
        // Overwrite.
        let p = save_review(&lib, &docs, id, Some(&[r("global", "DAYS")])).unwrap();
        save_reviewed(
            &lib,
            &docs,
            &changed,
            p.preview.token,
            &p.picked,
            &[res("global", "DAYS", "imported")],
        )
        .unwrap();
        assert_eq!(on_disk(&lib).globals["DAYS"].0, "25");
        // Add as a new name: the library's WEEKS keeps its DAYS.
        let mut third = changed.clone();
        third.globals.insert("DAYS".into(), "30".into());
        let p = review(
            &on_disk(&lib),
            &third,
            0,
            "x",
            Some(&[r("global", "WEEKS")]),
        )
        .unwrap();
        assert_eq!(p.preview.items.len(), 2);
        let p2 = save_review(&lib, &docs, id, None).unwrap();
        assert!(p2.picked.is_empty());
        assert!(
            !p2.preview.items.iter().any(|i| i.kind == "fringe_posting"),
            "settings never go to the library"
        );
        let (c, l) = save_reviewed(
            &lib,
            &docs,
            &third,
            p2.preview.token,
            &[r("global", "DAYS")],
            &[res("global", "DAYS", "rename")],
        )
        .unwrap();
        assert_eq!(c.added, 1);
        assert_eq!(l.globals["DAYS"].0, "25");
        assert_eq!(l.globals["DAYS_2"].0, "30");
        // Settings are refused.
        let p = save_review(&lib, &docs, id, None).unwrap();
        assert!(save_reviewed(
            &lib,
            &docs,
            &third,
            p.preview.token,
            &[r("fringe_posting", "")],
            &[]
        )
        .is_err());
        drop(docs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn adding_brings_dependencies_keeps_the_budgets_version_and_is_one_undo_step() {
        let (dir, lib, state) = setup();
        // A library with KIT and what it needs, DAYS set differently.
        let mut l = budget();
        l.globals.insert("DAYS".into(), "10".into());
        l.units.clear();
        l.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        l.groups.clear();
        write(&lib.path().unwrap(), &l).unwrap();
        // The budget lacks KIT, WEEKS and Shift.
        {
            let mut s = state.lock();
            s.edit("Test edit", |b| {
                b.fringes.remove("KIT");
                b.globals.remove("WEEKS");
                b.units.remove("Shift");
                Ok(())
            })
            .unwrap();
        }
        let docs = state.docs();
        let id = docs.active();
        let p = add_review(&lib, &docs, id, &[r("fringe", "KIT")]).unwrap();
        let st = |n: &str| {
            p.preview
                .items
                .iter()
                .find(|i| i.name == n)
                .map(|i| i.status)
                .unwrap()
        };
        assert_eq!(st("KIT"), "new");
        assert_eq!(st("WEEKS"), "new");
        assert_eq!(st("Shift"), "new");
        assert_eq!(st("DAYS"), "differs");
        drop(docs);
        let lib_b = lib.reviewed(p.preview.token).unwrap();
        let mut s = state.lock();
        let undo_before = s.can_undo();
        let c = s
            .edit("Add from library", |b| add_from(b, &lib_b, &p.picked, &[]))
            .unwrap();
        assert_eq!((c.added, c.kept), (3, 1));
        assert_eq!(
            copied_message(&c, false),
            "Added from the library: 3 items added, 1 item kept as this budget had it."
        );
        assert!(s.budget().fringes.contains_key("KIT"));
        assert_eq!(s.budget().globals["DAYS"].0, "20", "the budget's version");
        assert!(s.can_undo());
        s.undo();
        assert!(!s.budget().fringes.contains_key("KIT"), "one undo step");
        assert!(!s.budget().units.contains_key("Shift"));
        assert_eq!(s.can_undo(), undo_before);
        // Use the library's version.
        s.edit("Add from library", |b| {
            add_from(b, &lib_b, &p.picked, &[res("global", "DAYS", "imported")])
        })
        .unwrap();
        assert_eq!(s.budget().globals["DAYS"].0, "10");
        drop(s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_are_written_to_the_library_and_kept() {
        let (dir, lib, state) = setup();
        let mut l = on_disk(&lib);
        l.globals.insert("DAYS".into(), "20".into());
        l.globals.insert("RATE".into(), "7".into());
        write(&lib.path().unwrap(), &l).unwrap();
        let docs = state.docs();
        let d = change_library(&lib, &docs, |b| {
            folder_op(b, "global", "create", "Schedule", None)
        })
        .unwrap();
        assert_eq!(d.message, "Made the folder Schedule.");
        change_library(&lib, &docs, |b| {
            folders::move_items(b, "global", &["DAYS".into()], Some("Schedule"))?;
            Ok(String::new())
        })
        .unwrap();
        let v = library_view_at(&docs, lib.path().as_deref());
        let g = v.kinds.iter().find(|k| k.kind == "global").unwrap();
        assert_eq!(g.folders, ["Schedule"]);
        let days = g.items.iter().find(|i| i.name == "DAYS").unwrap();
        assert_eq!(days.folder.as_deref(), Some("Schedule"));
        // On disk, as `setup_folders`.
        let text = std::fs::read_to_string(lib.path().unwrap()).unwrap();
        assert!(text.contains("\"setup_folders\""), "{text}");
        assert!(text.contains("\"Schedule\""), "{text}");
        // Renamed, then deleted: the item stays, unfiled.
        change_library(&lib, &docs, |b| {
            folder_op(b, "global", "rename", "Schedule", Some("Sched"))
        })
        .unwrap();
        assert_eq!(
            folders::folder_of(&on_disk(&lib), "global", "DAYS"),
            Some("Sched")
        );
        let d = change_library(&lib, &docs, |b| {
            folder_op(b, "global", "delete", "Sched", None)
        })
        .unwrap();
        assert!(d.message.contains("its item is unfiled"), "{}", d.message);
        let after = on_disk(&lib);
        assert!(after.globals.contains_key("DAYS"));
        assert!(after.setup_folders.is_empty());
        // A refused change writes nothing.
        let before = std::fs::read(lib.path().unwrap()).unwrap();
        assert!(change_library(&lib, &docs, |b| folder_op(
            b, "global", "delete", "Nope", None
        ))
        .is_err());
        assert_eq!(std::fs::read(lib.path().unwrap()).unwrap(), before);
        drop(docs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn library_writes_are_atomic() {
        let (dir, lib, state) = setup();
        let path = lib.path().unwrap();
        let docs = state.docs();
        change_library(&lib, &docs, |b| {
            folder_op(b, "fringe", "create", "Union", None)
        })
        .unwrap();
        // No temporary file is left behind, and the file reads whole.
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
        assert!(crate::session::read_budget(&path).is_ok());
        // A write that can't happen (the folder is gone) leaves no file
        // half-written.
        let gone = dir.join("no such folder").join("lib.fwb");
        assert!(write(&gone, &new_library("USD")).is_err());
        assert!(!gone.exists());
        drop(docs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_library_open_as_a_tab_is_read_from_it_and_changed_only_there() {
        let (dir, lib, state) = setup();
        let path = lib.path().unwrap();
        // Open the library as a tab and give it an unsaved global.
        let mut tab = Session::open(&path).unwrap();
        tab.edit("Test edit", |b| {
            b.globals.insert("UNSAVED".into(), "1".into());
            Ok(())
        })
        .unwrap();
        let lib_id = state.docs().add(tab);
        let budget_id = state.docs().ids()[0];
        let docs = state.docs();
        // Read from the tab.
        let v = library_view_at(&docs, Some(&path));
        assert!(v.in_tab);
        let g = v.kinds.iter().find(|k| k.kind == "global").unwrap();
        assert_eq!(g.items[0].name, "UNSAVED");
        // Adding from it works (into another budget).
        let p = add_review(&lib, &docs, budget_id, &[r("global", "UNSAVED")]).unwrap();
        assert_eq!(p.preview.items.len(), 1);
        // But not into the library itself.
        assert!(add_review(&lib, &docs, lib_id, &[r("global", "UNSAVED")]).is_err());
        // Changes are refused, and the file is untouched.
        let before = std::fs::read(&path).unwrap();
        let e = save_review(&lib, &docs, budget_id, None).unwrap_err();
        assert!(e.contains("open as the budget tab"), "{e}");
        let e = change_library(&lib, &docs, |b| folder_op(b, "global", "create", "X", None))
            .unwrap_err();
        assert!(e.contains("open as the budget tab"), "{e}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(docs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_library_command_is_registered_for_both_windows() {
        const BUILD: &str = include_str!("../build.rs");
        const APP: &str = include_str!("app.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in [
            "library_view",
            "library_create",
            "library_choose",
            "library_add_preview",
            "library_add",
            "library_save_preview",
            "library_save",
            "library_folder",
            "library_move",
        ] {
            assert!(BUILD.contains(&format!("\"{c}\"")), "{c} not in build.rs");
            assert!(
                APP.contains(&format!("crate::library::{c},")),
                "{c} not in app.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN_CAPS.contains(&allow), "{c} not allowed in main.json");
            assert!(TOOLS_CAPS.contains(&allow), "{c} not allowed in tools.json");
        }
    }
}
