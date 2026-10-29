//! Setup packs in the app (decisions.md, entry 50; docs/APP.md, "Setup
//! packs"): exporting a pack from the budget in front or the library,
//! importing one into the budget or the library, and the Packs list.
//!
//! - **Export.** The page lists the source's items by kind and folder
//!   (`pack_items`, the Library tab's own view of any budget) and sends the
//!   picks and the pack's details; Rust walks the dependencies again
//!   (method_format::setup::pack::build), shows the Save dialog (in the user's
//!   Packs folder at first) and writes the pack atomically. A file open in
//!   a tab is refused. The page never names a path.
//! - **Import.** Import Setup's review and rules (src/import_setup.rs): per
//!   item keep, take or add under a new name, dependencies ticked along, one
//!   undo step into a budget, and into the library the library's own
//!   checked write (src/library.rs). The review also carries the pack's
//!   details and each item's source, and what comes in is stamped with the
//!   pack's title, version and effective date. Budget settings never come
//!   from a pack.
//! - **The Packs list.** Packs in the user's `Packs` folder (in the app's
//!   local data folder, made when first listed, so a pack dropped there
//!   shows up) and the packs that ship with the app (`packs/` in the
//!   repository, bundled as a resource; empty but for its README: rate
//!   packs come from a separate data project after review, CLEAN-ROOM.md).
//!   The page names a listed pack by a token from the last listing.
//!
//! The review, importing into a budget and making a pack are
//! method-session's `packs` module (re-exported here); the Packs list, the
//! dialogs, the files and the library's side are here.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use method_core::Budget;
use method_format::setup::pack::{self, PackDetails};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::docs::Docs;
use crate::import_setup::{self, ImportPreview, ImportSources, ItemRef, Resolution};
use crate::library::{self, Library};
use crate::safety::{self, Access};
use crate::session::DocsFiles;

pub use method_session::packs::*;

/// The user's packs folder's name, in the app's local data folder.
pub const PACKS_DIR: &str = "Packs";
/// Most packs listed from one folder.
const MAX_LISTED: usize = 500;

// ---------------------------------------------------------------------------
// The app's state: where packs are, and the last listing

#[derive(Default)]
struct Inner {
    token: u64,
    /// The last listing: token -> file.
    listed: Vec<(u64, PathBuf)>,
}

/// Where packs are kept (one per app).
#[derive(Default)]
pub struct Packs {
    user_dir: Option<PathBuf>,
    bundled_dir: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl Packs {
    pub fn new(user_dir: Option<PathBuf>, bundled_dir: Option<PathBuf>) -> Packs {
        Packs {
            user_dir,
            bundled_dir,
            inner: Mutex::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn listed(&self, token: u64) -> Result<PathBuf, String> {
        self.lock()
            .listed
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| crate::i18n::tr("packs.changed_since"))
    }
}

/// Puts the packs state in the app (from `app.rs`'s setup).
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let user = app
        .path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join(PACKS_DIR));
    let bundled = app.path().resource_dir().ok().map(|d| d.join("packs"));
    app.manage(Packs::new(user, bundled));
}

// ---------------------------------------------------------------------------
// The Packs list

/// One pack in the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListedPack {
    /// Names it to `pack_review`, until the next listing.
    pub token: u64,
    /// Its file name without the extension.
    pub file: String,
    /// `yours` (the Packs folder) or `bundled` (shipped with the app).
    pub place: &'static str,
    /// Its title, version, effective date and region (empty when it isn't
    /// a pack or couldn't be read).
    pub title: String,
    pub version: String,
    pub effective: String,
    pub region: String,
    /// How many setup items it holds.
    pub items: usize,
    /// Why it couldn't be read, or that it isn't a setup pack.
    pub problem: Option<String>,
}

/// The Packs list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PacksView {
    /// The user's Packs folder, to show (never sent back).
    pub folder: String,
    pub packs: Vec<ListedPack>,
}

/// The budget files directly in `dir`, by name (none when it isn't there).
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && method_format::has_budget_extension(p))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    out.sort();
    out.truncate(MAX_LISTED);
    out
}

/// What a listed pack says about itself.
fn describe_file(path: &Path, token: u64, place: &'static str) -> ListedPack {
    let file = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = ListedPack {
        token,
        file,
        place,
        title: String::new(),
        version: String::new(),
        effective: String::new(),
        region: String::new(),
        items: 0,
        problem: None,
    };
    match crate::session::read_budget(path) {
        Ok((b, _)) => {
            out.items = import_setup::all_items(&b)
                .into_iter()
                .filter(|(w, _)| pack::pack_kind(*w))
                .count();
            match method_core::pack::info(&b) {
                Some(info) => {
                    out.title = info.title.clone();
                    out.version = info.version.clone();
                    out.effective = info.effective.clone();
                    out.region = info.region.clone();
                }
                None => {
                    out.title = b.title.clone();
                    out.problem = Some(crate::i18n::tr("packs.not_a_pack_items"));
                }
            }
        }
        Err(e) => out.problem = Some(e),
    }
    out
}

/// Lists the packs in the user's folder (made when missing, so packs can
/// be dropped in) and the bundled ones.
pub fn list(packs: &Packs) -> PacksView {
    if let Some(dir) = &packs.user_dir {
        let _ = safety::create_private_dir(dir);
    }
    let mut found: Vec<(PathBuf, &'static str)> = Vec::new();
    for (dir, place) in [(&packs.user_dir, "yours"), (&packs.bundled_dir, "bundled")] {
        if let Some(d) = dir {
            found.extend(files_in(d).into_iter().map(|p| (p, place)));
        }
    }
    let mut g = packs.lock();
    g.listed.clear();
    let mut out = Vec::with_capacity(found.len());
    for (path, place) in found {
        g.token += 1;
        let t = g.token;
        out.push(describe_file(&path, t, place));
        g.listed.push((t, path));
    }
    PacksView {
        folder: packs
            .user_dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        packs: out,
    }
}

// ---------------------------------------------------------------------------
// The review, into a budget or the library

/// The review of importing `src` into the library: the library as the
/// review saw it is kept (src/library.rs) and the pack too.
pub fn review_into_library(
    lib_state: &Library,
    docs: &Docs,
    sources: &ImportSources,
    label: String,
    src: Budget,
) -> Result<PackReview, String> {
    let path = lib_state.required()?;
    if let Some(id) = docs.find_file(&path) {
        return Err(library::in_tab_refusal(&library::tab_name(docs, id)));
    }
    let (lib, _) = crate::session::read_budget(&path)?;
    let preview = without_settings(import_setup::preview(&lib, &src, 0, &label, ""));
    let summary = pack::summary(&src);
    let source_token = sources.keep(label, src);
    let token = lib_state.keep(lib);
    Ok(PackReview {
        preview: ImportPreview { token, ..preview },
        pack: summary,
        into: "library",
        source_token,
    })
}

/// Imports into the library what review `token` showed of pack
/// `source_token`; refused when the library changed since.
pub fn import_into_library(
    lib_state: &Library,
    docs: &Docs,
    src: &Budget,
    token: u64,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<import_setup::Imported, String> {
    check_picks(picks)?;
    let path = lib_state.required()?;
    let seen = lib_state.reviewed(token)?;
    let (done, _) = library::change(docs, &path, |lib| {
        if *lib != *seen {
            return Err(crate::i18n::tr("library.changed_since"));
        }
        pack::import_pack(lib, src, picks, resolutions)
    })?;
    lib_state.changed();
    Ok(done)
}

// ---------------------------------------------------------------------------
// Export

/// The budget a pack is made from: budget `doc`, or the library.
fn export_source(
    docs: &Docs,
    lib_state: &Library,
    doc: DocId,
    source: &str,
) -> Result<(String, Budget), String> {
    match source {
        "budget" => {
            let s = docs
                .get(doc)
                .ok_or_else(|| method_session::messages::word("engine.docs.closed"))?;
            Ok((s.tab_label(), s.budget().clone()))
        }
        "library" => {
            let path = lib_state.required()?;
            let (b, _) = library::read(docs, &path)?;
            Ok(("your library".to_string(), b))
        }
        other => Err(format!("unknown pack source `{other}`")),
    }
}

/// Writes pack `p` to `path` atomically, refusing a file open in a tab.
pub fn write_pack(docs: &Docs, path: &Path, p: &Budget) -> Result<(), String> {
    if docs.find_file(path).is_some() {
        return Err(crate::i18n::tr("packs.open_in_tab"));
    }
    safety::write_atomically(
        path,
        method_format::to_string(p).as_bytes(),
        Access::Document,
    )
    .map_err(|e| format!("Couldn't save {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs and build.rs; allowed in
// capabilities/main.json, tools.json and setup.json, since the Library tab,
// where packs are listed, is in all three windows)

/// The Packs list.
#[tauri::command]
pub fn packs_list(packs: State<'_, Packs>) -> PacksView {
    list(&packs)
}

/// Shows the user's Packs folder in the file manager (made when missing).
#[tauri::command]
pub fn packs_reveal(packs: State<'_, Packs>) -> Result<(), String> {
    let dir = packs
        .user_dir
        .clone()
        .ok_or("There is no packs folder on this computer.")?;
    safety::create_private_dir(&dir).map_err(|e| e.to_string())?;
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't show {}: {e}", dir.display()))
}

/// The items a pack can be made from: budget `doc`'s (`source` `budget`)
/// or the library's (`library`).
#[tauri::command]
pub fn pack_items(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    doc: Option<DocId>,
    source: String,
) -> Result<PackItems, String> {
    let s = state.lock_doc(doc)?;
    let (name, b) = crate::i18n::engine_err(export_source(s.docs(), &library, s.id(), &source))?;
    Ok(items_of(name, &b))
}

/// Makes a setup pack of `picks` (and what they need) from budget `doc` or
/// the library, with `details` and `created` (the page's clock), and writes
/// it where Rust's Save dialog says. `None` when cancelled.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pack_export<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    library: State<'_, Library>,
    packs: State<'_, Packs>,
    doc: Option<DocId>,
    source: String,
    picks: Vec<ItemRef>,
    details: PackDetails,
    created: String,
) -> Result<Option<PackExported>, String> {
    // Built (and checked) before the dialog, so a mistake shows at once.
    let (built, items) = {
        let s = state.lock_doc(doc)?;
        let (_, b) = crate::i18n::engine_err(export_source(s.docs(), &library, s.id(), &source))?;
        crate::i18n::engine_err(pack_build(&b, &picks, &details, &created))?
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.export_setup_pack"))
        .add_filter(
            crate::i18n::tr("dialog.filter_budget"),
            &[method_format::EXTENSION],
        )
        .set_file_name(format!(
            "{}.{}",
            crate::app::safe_file_stem(&details.title),
            method_format::EXTENSION
        ))
        .set_parent(&window);
    if let Some(dir) = &packs.user_dir {
        if safety::create_private_dir(dir).is_ok() {
            dialog = dialog.set_directory(dir);
        }
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = crate::app::with_budget_extension(picked.into_path().map_err(|e| e.to_string())?);
    write_pack(&state.docs(), &path, &built)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let message = crate::i18n::engine(&exported_message(&name, items));
    Ok(Some(PackExported { name, message }))
}

/// Shows an Open dialog for a pack (in the Packs folder at first), reads it
/// and returns the review of importing it into budget `doc` (`into`
/// `budget`) or the library (`library`). `None` when cancelled.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pack_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    library: State<'_, Library>,
    sources: State<'_, ImportSources>,
    packs: State<'_, Packs>,
    doc: Option<DocId>,
    into: String,
) -> Result<Option<PackReview>, String> {
    let target = state.lock_doc(doc)?.id();
    let start = packs.user_dir.clone().filter(|d| d.is_dir());
    let Some(path) =
        crate::app::pick_budget(&window, &crate::i18n::tr("dialog.import_setup_pack"), start)?
    else {
        return Ok(None);
    };
    crate::i18n::engine_err(review_file(
        &state, &library, &sources, target, &path, &into,
    ))
    .map(Some)
}

/// The review of importing pack file `path` into budget `target` or the
/// library.
fn review_file(
    state: &AppState,
    library: &Library,
    sources: &ImportSources,
    target: DocId,
    path: &Path,
    into: &str,
) -> Result<PackReview, String> {
    let docs = state.docs();
    match into {
        "budget" => {
            let (label, src) = import_setup::read_source(&docs, target, path)?;
            review_into_budget(&docs, sources, target, label, src)
        }
        "library" => {
            if library.path().is_some_and(|l| same_file(&l, path)) {
                return Err(crate::i18n::tr("packs.is_your_library"));
            }
            let (label, src) = match docs.find_file(path) {
                Some(id) => {
                    let s = docs
                        .get(id)
                        .ok_or_else(|| method_session::messages::word("engine.docs.closed"))?;
                    (s.tab_label(), s.budget().clone())
                }
                None => {
                    let (b, _) = crate::session::read_budget(path)?;
                    let label = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    (label, b)
                }
            };
            review_into_library(library, &docs, sources, label, src)
        }
        other => Err(format!("unknown place to import into `{other}`")),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// The review of importing listed pack `token` into budget `doc` or the
/// library (`into`).
#[tauri::command]
pub fn pack_review(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    sources: State<'_, ImportSources>,
    packs: State<'_, Packs>,
    doc: Option<DocId>,
    token: u64,
    into: String,
) -> Result<PackReview, String> {
    let path = packs.listed(token)?;
    let target = state.lock_doc(doc)?.id();
    crate::i18n::engine_err(review_file(
        &state, &library, &sources, target, &path, &into,
    ))
}

/// Imports into budget `doc` what review `token` showed, stamped with the
/// pack, as one undo step.
#[tauri::command]
pub fn pack_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<PackImported, String> {
    let mut s = state.lock_doc(doc)?;
    let message = crate::i18n::engine_err(method_session::packs::pack_import(
        &mut s,
        &sources,
        token,
        &picks,
        &resolutions,
    ))?;
    Ok(PackImported {
        view: finish(&window, &mut s, None),
        message: crate::i18n::engine(&message),
    })
}

/// Imports into the library what review `token` showed of pack
/// `source_token`.
#[tauri::command]
pub fn pack_import_library(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    sources: State<'_, ImportSources>,
    token: u64,
    source_token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<PackIntoLibrary, String> {
    let (_, src) = sources
        .get(source_token)
        .ok_or_else(|| method_session::messages::word("engine.import.picked_since"))?;
    let title = method_core::pack::info(&src)
        .map(|i| i.title.clone())
        .unwrap_or_default();
    let docs = state.docs();
    let counts = crate::i18n::engine_err(import_into_library(
        &library,
        &docs,
        &src,
        token,
        &picks,
        &resolutions,
    ))?;
    let path = library.path();
    let mut done = PackIntoLibrary {
        library: library::library_view_at(&docs, path.as_deref()),
        message: imported_message(&counts, &title, true),
    };
    crate::i18n::Localize::localize(&mut done.library);
    done.message = crate::i18n::engine(&done.message);
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::tests::temp_dir;
    use crate::session::Session;
    use crate::session::SessionFiles;
    use method_core::{starter, Fringe, FringeKind};

    fn r(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn show() -> Budget {
        let mut b = starter::budget();
        b.title = "Show".into();
        b.globals.insert("PENSION_RATE".into(), "8.5".into());
        b.fringes.insert(
            "PENSION".into(),
            Fringe {
                description: "Pension".into(),
                short_code: None,
                rate: "PENSION_RATE".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b
    }

    fn details() -> PackDetails {
        PackDetails {
            title: "Union Rates".into(),
            version: "2026.1".into(),
            effective: "2026-01-01".into(),
            ..PackDetails::default()
        }
    }

    #[test]
    fn packs_are_listed_from_both_folders_and_reviewed_by_token() {
        let dir = temp_dir("packs-list");
        let user = dir.join("data").join(PACKS_DIR);
        let bundled = dir.join("res").join("packs");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::write(bundled.join("README.md"), "no rate data").unwrap();
        let (p, _) = pack::build(
            &show(),
            &[r("fringe", "PENSION")],
            &details(),
            "2026-10-10",
            "0.1.0",
        )
        .unwrap();
        let packs = Packs::new(Some(user.clone()), Some(bundled.clone()));
        // The user's folder is made on the first listing.
        let v = list(&packs);
        assert!(user.is_dir());
        assert!(v.packs.is_empty(), "a README is not a pack");
        write_pack(&Docs::default(), &user.join("Union.mthd"), &p).unwrap();
        std::fs::write(user.join("broken.mthd"), "{").unwrap();
        crate::library::write(&bundled.join("Plain.mthd"), &starter::budget()).unwrap();
        let v = list(&packs);
        assert_eq!(v.packs.len(), 3, "{v:?}");
        let union = v.packs.iter().find(|x| x.file == "Union").unwrap();
        assert_eq!(union.place, "yours");
        assert_eq!(union.title, "Union Rates");
        assert_eq!(union.effective, "2026-01-01");
        assert_eq!(union.items, 2);
        assert!(union.problem.is_none());
        assert!(v
            .packs
            .iter()
            .find(|x| x.file == "broken")
            .unwrap()
            .problem
            .is_some());
        let plain = v.packs.iter().find(|x| x.file == "Plain").unwrap();
        assert_eq!(plain.place, "bundled");
        assert!(plain
            .problem
            .as_deref()
            .unwrap()
            .contains("Not a setup pack"));

        // Reviewed by token, into a budget: settings left out, details along.
        let state = AppState::with(Docs::with(Session::new()));
        let sources = ImportSources::default();
        let lib = Library::new(None);
        let target = state.docs().active();
        let path = packs.listed(union.token).unwrap();
        let rv = review_file(&state, &lib, &sources, target, &path, "budget").unwrap();
        assert_eq!(rv.into, "budget");
        assert_eq!(rv.pack.details.as_ref().unwrap().title, "Union Rates");
        assert!(rv.preview.items.iter().all(|i| i.kind != "fringe_posting"));
        assert!(packs.listed(9999).is_err());
        // A new listing forgets the old tokens.
        list(&packs);
        assert!(packs.listed(union.token).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn importing_into_a_budget_is_one_undo_step_and_stamps_items() {
        let (p, _) = pack::build(
            &show(),
            &[r("fringe", "PENSION")],
            &details(),
            "2026-10-10",
            "0.1.0",
        )
        .unwrap();
        let state = AppState::with(Docs::with(Session::new()));
        let sources = ImportSources::default();
        let target = state.docs().active();
        let rv = review_into_budget(&state.docs(), &sources, target, "Union".into(), p).unwrap();
        let (_, src) = sources.get(rv.preview.token).unwrap();
        let mut s = state.lock();
        let before = s.budget().clone();
        let c = s
            .edit("Import setup pack", |b| {
                pack::import_pack(b, &src, &[r("fringe", "PENSION")], &[])
            })
            .unwrap();
        assert_eq!(c.added, 2);
        assert_eq!(
            imported_message(&c, "Union Rates", false),
            "Imported from Union Rates: 2 items added."
        );
        assert_eq!(
            s.budget()
                .setup_sources
                .get("fringe", "PENSION")
                .unwrap()
                .pack,
            "Union Rates"
        );
        assert!(s.undo());
        assert_eq!(s.budget(), &before, "one undo step");
        assert!(check_picks(&[r("fringe_posting", "")]).is_err());
    }

    #[test]
    fn importing_into_the_library_is_checked_and_written() {
        let dir = temp_dir("packs-library");
        let lib = Library::new(Some(&dir.join("data")));
        let (path, _) = library::create_in(&dir, "USD").unwrap();
        lib.set_path(path.clone()).unwrap();
        let state = AppState::with(Docs::with(Session::new()));
        let sources = ImportSources::default();
        let (p, _) = pack::build(
            &show(),
            &[r("fringe", "PENSION")],
            &details(),
            "2026-10-10",
            "0.1.0",
        )
        .unwrap();
        let rv =
            review_into_library(&lib, &state.docs(), &sources, "Union".into(), p.clone()).unwrap();
        assert_eq!(rv.into, "library");
        let (_, src) = sources.get(rv.source_token).unwrap();
        let c = import_into_library(
            &lib,
            &state.docs(),
            &src,
            rv.preview.token,
            &[r("fringe", "PENSION")],
            &[],
        )
        .unwrap();
        assert_eq!(c.added, 2);
        let on_disk = crate::session::read_budget(&path).unwrap().0;
        assert_eq!(
            on_disk.setup_sources.get("fringe", "PENSION").unwrap().pack,
            "Union Rates"
        );
        // The token is spent: the library changed.
        assert!(import_into_library(
            &lib,
            &state.docs(),
            &src,
            rv.preview.token,
            &[r("fringe", "PENSION")],
            &[]
        )
        .is_err());
        // Exporting from the library makes a pack of what it holds.
        let active = state.docs().active();
        let (name, b) = export_source(&state.docs(), &lib, active, "library").unwrap();
        assert_eq!(name, "your library");
        let items = items_of(name, &b);
        assert!(items
            .kinds
            .iter()
            .any(|k| k.kind == "fringe" && k.items.len() == 1));
        assert!(items.kinds.iter().all(|k| k.kind != "fringe_posting"));
        // A file open in a tab is never written over.
        let mut docs = Docs::default();
        let mut tab = Session::new();
        let tab_path = dir.join("Open.mthd");
        tab.save_to(&tab_path).unwrap();
        docs.add(tab);
        assert!(write_pack(&docs, &tab_path, &p).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pack_commands_are_wired_for_every_window_with_the_library() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        for c in [
            "packs_list",
            "packs_reveal",
            "pack_items",
            "pack_export",
            "pack_import_pick",
            "pack_review",
            "pack_import",
            "pack_import_library",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::packs::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            for (window, caps) in [
                ("main", include_str!("../capabilities/main.json")),
                ("tools", include_str!("../capabilities/tools.json")),
                ("setup", include_str!("../capabilities/setup.json")),
            ] {
                assert!(
                    caps.contains(&allow),
                    "{c} not allowed for the {window} window"
                );
            }
        }
    }
}
