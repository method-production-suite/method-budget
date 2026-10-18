//! The Tauri side of sharing a budget through a folder (src/shared.rs;
//! docs/APP.md, "Sharing a budget"): File > Share…, the commands the
//! Share dialog, the status item, the review and activity tabs and Who
//! Changed This call, and the one-second timer pass that brings in other
//! people's changes and tells the page.
//!
//! Paths still never come from the web view: the shared folder is picked
//! in Rust's own dialog and kept here until the page says to go ahead.
//! This computer's actor id and person (name and color) live beside the
//! command line's, in `~/.method/`, so one computer is one actor whatever
//! program edits the budget.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use method_core::log::{ActorId, Ids};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::session::Session;
use crate::session::{DocsFiles, SessionFiles};
use crate::shared::{self, ActivityDto, Person, ReviewDto, ShareStatus, ValueDto};
use crate::view::View;

/// File > Share…, answered in the web view (the dialog).
pub const SHARE_ITEM: &str = "file.share";

/// Events to the main window.
/// Changes by other people arrived (or an outside edit was taken in):
/// `{ doc, rev, remote: [...] }`; the page fetches the view and flashes
/// the places.
pub const SHARE_CHANGED: &str = "share-changed";
/// The status item's state changed: `{ doc, status }` (a [`ShareStatus`]).
pub const SHARE_STATUS: &str = "share-status";

/// What the timer keeps between passes: the folder picked for sharing,
/// and the last status sent per budget (so it is sent when it changes).
#[derive(Default)]
pub struct ShareState {
    picked: Mutex<Option<PathBuf>>,
    last_status: Mutex<HashMap<DocId, String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Serialize)]
struct Changed {
    doc: DocId,
    rev: u64,
    remote: Vec<shared::RemoteChange>,
}

#[derive(Clone, Serialize)]
struct StatusEvent {
    doc: DocId,
    status: ShareStatus,
}

/// This computer's actor id (made the first time).
pub fn this_actor() -> Result<ActorId, String> {
    let path = method_format::log::actor_id_path()
        .ok_or("No home folder to keep this computer's sharing id in.")?;
    method_format::log::load_or_make_actor(&path, &mut Ids::from_os()).map_err(|e| e.0)
}

/// The person this computer shares as (name and color), as kept beside
/// the actor id.
pub fn this_person(actor: ActorId) -> Person {
    shared::read_person(shared::person_path().as_deref(), actor)
}

/// Opens `path` as a session and, when a change log is beside it, joins
/// it (docs/research/collaboration.md, 4.7). A log that can't be read
/// leaves the budget open on its own, with the reason as a notice.
pub fn open_joining(path: &std::path::Path) -> Result<Session, String> {
    let mut s = Session::open(path)?;
    if method_format::log::LogDir::for_budget(path).exists() {
        let actor = this_actor()?;
        let me = this_person(actor);
        if let Err(e) = s.join_shared(actor, me) {
            s.set_notice(format!(
                "This budget has a change log beside it that couldn't be read, so it opened on its own: {e}"
            ));
        }
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Commands

/// The status item: `None` when the budget isn't shared.
#[tauri::command]
pub fn share_status(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Option<ShareStatus>, String> {
    let s = state.lock_doc(doc)?;
    Ok(s.shared().map(|sh| sh.status(Instant::now())))
}

/// What the Share dialog needs before sharing: this person's name and
/// color, the budget's folder and file, and whether it can be shared.
#[derive(Clone, Debug, Serialize)]
pub struct ShareSetup {
    pub name: String,
    pub color: String,
    pub colors: Vec<&'static str>,
    pub file: Option<String>,
    pub folder: Option<String>,
    /// Why it can't be shared yet (unsaved, read-only), if so.
    pub blocked: Option<String>,
    /// The folder picked in Rust's dialog, if one was.
    pub picked: Option<String>,
    /// The topsheet's categories, for "Edits some categories" (only
    /// when the budget is shared and this person sets roles).
    pub categories: Vec<CategoryChoice>,
}

/// One category a department editor can be given.
#[derive(Clone, Debug, Serialize)]
pub struct CategoryChoice {
    pub code: String,
    pub name: String,
}

#[tauri::command]
pub fn share_setup(
    state: State<'_, AppState>,
    share: State<'_, ShareState>,
    doc: Option<DocId>,
) -> Result<ShareSetup, String> {
    let s = state.lock_doc(doc)?;
    let actor = this_actor()?;
    let me = s
        .shared()
        .map(|sh| sh.me().clone())
        .unwrap_or_else(|| this_person(actor));
    let blocked = if s.is_shared() {
        None
    } else if s.is_read_only() {
        Some(
            "A copy saved for sharing can't be shared this way. Make an editable copy first."
                .into(),
        )
    } else if s.path().is_none() {
        Some("Save the budget first, then share it.".into())
    } else {
        None
    };
    Ok(ShareSetup {
        name: me.name,
        color: me.color,
        colors: shared::COLORS.to_vec(),
        file: s
            .path()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        folder: s
            .path()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string()),
        blocked,
        picked: lock(&share.picked)
            .as_ref()
            .map(|p| p.display().to_string()),
        categories: match s.shared() {
            Some(sh) if sh.role().sets_roles() => s
                .budget()
                .categories()
                .map(|c| CategoryChoice {
                    code: c.code.clone(),
                    name: c.name.clone(),
                })
                .collect(),
            _ => Vec::new(),
        },
    })
}

/// Rust's folder dialog for the shared folder; the choice is kept here
/// until `share_start`. `None` when cancelled.
#[tauri::command]
pub async fn share_pick_folder<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    share: State<'_, ShareState>,
    doc: Option<DocId>,
) -> Result<Option<String>, String> {
    let start = state
        .lock_doc(doc)?
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Choose the shared folder")
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let text = dir.display().to_string();
    *lock(&share.picked) = Some(dir);
    Ok(Some(text))
}

/// Starts sharing budget `doc` through a folder: in its own folder, or
/// in the folder picked (`use_picked`), where the file is copied or moved
/// (`move_file`). The person's name and color are kept for every budget.
#[tauri::command(rename_all = "snake_case")]
pub fn share_start<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    share: State<'_, ShareState>,
    doc: Option<DocId>,
    use_picked: bool,
    move_file: bool,
    me: Person,
) -> Result<View, String> {
    let actor = this_actor()?;
    let me = Person::tidy(&me.name, &me.color, actor);
    if me.name.is_empty() {
        return Err("Type the name other people will see.".into());
    }
    let mut s = state.lock_doc(doc)?;
    if s.is_shared() {
        return Err("This budget is already shared.".into());
    }
    let from = s
        .path()
        .map(PathBuf::from)
        .ok_or("Save the budget first, then share it.")?;
    if use_picked {
        let dir = lock(&share.picked)
            .clone()
            .ok_or("Choose the shared folder first.")?;
        if !dir.is_dir() {
            return Err(format!(
                "{} isn't a folder that can be reached.",
                dir.display()
            ));
        }
        let name = from.file_name().ok_or("The budget's file has no name.")?;
        let target = dir.join(name);
        if !crate::safety::same_file(&target, &from) {
            if target.exists() {
                return Err(format!(
                    "There is already a file named {} in {}. Rename one of them first.",
                    name.to_string_lossy(),
                    dir.display()
                ));
            }
            if s.docs().find_file(&target).is_some() {
                return Err("That file is open in another tab.".into());
            }
            if method_format::log::LogDir::for_budget(&target).exists() {
                return Err(format!(
                    "{} already has a change log beside it in that folder.",
                    name.to_string_lossy()
                ));
            }
            s.save_to(&target)?;
            if move_file {
                let _ = std::fs::remove_file(&from);
            }
        }
    }
    shared::write_person(shared::person_path().as_deref(), &me);
    s.start_sharing(actor, me)?;
    *lock(&share.picked) = None;
    Ok(finish(&window, &mut s, None))
}

/// Sets this person's name and color, for every shared budget.
#[tauri::command]
pub fn share_set_me(
    state: State<'_, AppState>,
    name: String,
    color: String,
) -> Result<Person, String> {
    let actor = this_actor()?;
    let me = Person::tidy(&name, &color, actor);
    if me.name.is_empty() {
        return Err("Type the name other people will see.".into());
    }
    shared::write_person(shared::person_path().as_deref(), &me);
    let mut docs = state.docs();
    for id in docs.ids() {
        if let Some(sh) = docs.get_mut(id).and_then(Session::shared_mut) {
            sh.set_me(me.clone());
        }
    }
    Ok(me)
}

/// Sets what person `actor` may change in budget `doc` (decisions.md,
/// entry 78): `role` is "co_owner", "admin", "editor", "department", "actuals",
/// "comment" or "read_only"; `scope` a department editor's category
/// numbers. Returns the status with the new role.
#[tauri::command]
pub fn share_set_role(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    actor: String,
    role: String,
    scope: Option<Vec<String>>,
) -> Result<ShareStatus, String> {
    let mut s = state.lock_doc(doc)?;
    let sh = s.shared_mut().ok_or("This budget isn't shared.")?;
    sh.set_role(&actor, &role, &scope.unwrap_or_default())?;
    Ok(sh.status(Instant::now()))
}

/// Where this person is in budget `doc` (the category and account on
/// screen), for the heartbeat other people see.
#[tauri::command]
pub fn share_place(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category: Option<String>,
    account: Option<String>,
) -> Result<(), String> {
    let mut s = state.lock_doc(doc)?;
    if let Some(sh) = s.shared_mut() {
        sh.set_place(
            category
                .filter(|c| !c.is_empty())
                .map(|c| c.chars().take(40).collect()),
            account
                .filter(|a| !a.is_empty())
                .map(|a| a.chars().take(40).collect()),
        );
    }
    Ok(())
}

/// The review list.
#[tauri::command]
pub fn share_review(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Vec<ReviewDto>, String> {
    let s = state.lock_doc(doc)?;
    Ok(s.shared().map(|sh| sh.review()).unwrap_or_default())
}

/// Recent changes by everyone, newest first.
#[tauri::command]
pub fn share_activity(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    limit: Option<usize>,
) -> Result<Vec<ActivityDto>, String> {
    let s = state.lock_doc(doc)?;
    Ok(s.shared()
        .map(|sh| sh.activity(limit.unwrap_or(200).clamp(1, 2000)))
        .unwrap_or_default())
}

/// Who changed a field: every value it has had, oldest first.
#[tauri::command]
pub fn share_blame(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    key: String,
    field: String,
) -> Result<Vec<ValueDto>, String> {
    let s = state.lock_doc(doc)?;
    let sh = s.shared().ok_or("This budget isn't shared.")?;
    sh.blame(&kind, &key, &field)
}

/// Puts value `index` of a field's history back, as this person's change.
#[tauri::command]
pub fn share_use_value<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    key: String,
    field: String,
    index: usize,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    s.share_use_value(&kind, &key, &field, index)?;
    Ok(finish(&window, &mut s, None))
}

// ---------------------------------------------------------------------------
// The timer

/// One pass over every shared budget (from the autosave timer, every
/// second): changes in, changes out, the keeper's file, heartbeats; then
/// the page is told what changed.
pub fn tick<R: Runtime>(app: &AppHandle<R>) {
    let Some(share) = app.try_state::<ShareState>() else {
        return;
    };
    let state = app.state::<AppState>();
    let now = Instant::now();
    let mut changed: Vec<Changed> = Vec::new();
    let mut statuses: Vec<(DocId, ShareStatus, bool)> = Vec::new();
    {
        let mut docs = state.docs();
        for id in docs.ids() {
            let Some(s) = docs.get_mut(id) else { continue };
            if !s.is_shared() {
                continue;
            }
            let was_dirty = s.is_dirty();
            let Some(out) = s.share_tick(now) else {
                continue;
            };
            if out.budget_changed {
                changed.push(Changed {
                    doc: id,
                    rev: s.view_rev(),
                    remote: out.remote,
                });
            }
            if let Some(sh) = s.shared() {
                statuses.push((id, sh.status(now), was_dirty != s.is_dirty()));
            }
        }
    }
    for c in changed {
        if let Some(main) = app.get_webview_window(crate::tools::MAIN) {
            crate::tools::changed(&main, c.doc, c.rev);
        }
        let _ = app.emit_to(crate::tools::MAIN, SHARE_CHANGED, c);
    }
    for (doc, status, dirty_flipped) in statuses {
        if dirty_flipped {
            crate::safety_app::tell_saved(app, doc);
        }
        let text = serde_json::to_string(&status).unwrap_or_default();
        let fresh = {
            let mut last = lock(&share.last_status);
            last.get(&doc) != Some(&text) && {
                last.insert(doc, text);
                true
            }
        };
        if fresh {
            let _ = app.emit_to(
                crate::tools::MAIN,
                SHARE_STATUS,
                StatusEvent { doc, status },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn commands_are_registered() {
        const APP_RS: &str = include_str!("app.rs");
        for c in [
            "share_status",
            "share_setup",
            "share_pick_folder",
            "share_start",
            "share_set_me",
            "share_place",
            "share_review",
            "share_activity",
            "share_blame",
            "share_use_value",
            "share_set_role",
        ] {
            assert!(APP_RS.contains(&format!("crate::shared_app::{c},")), "{c}");
        }
    }
}
