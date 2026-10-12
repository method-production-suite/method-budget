//! Budgets as tabs in the one window (decisions.md, entries 19 and 25): the
//! commands the tab strip calls (`ui/js/tabs.js`). The documents themselves
//! are kept in src/docs.rs; the desktop adds the Window menu's list of open
//! budgets, remembering which files are open for the next start, Autosave on
//! closing and the recovery copies.
//!
//! Every command still works on the budget in front unless it names another
//! with `doc`.
//!
//! Split view (roadmap 2.26): two panes side by side, each with its own tabs
//! (src/docs.rs, `Split`). The pane commands here split and merge the
//! window, pick the target pane, move tabs within a pane and to the other
//! one, and give the other pane its own view of a budget (`pane_view`: its
//! own account on screen, nothing changed and nobody told).
//!
//! The commands that reply with a view give back the budget's lock
//! ([`DocGuard`]) for the host to finish the view with its own side.

use serde::Serialize;

use crate::docs::{AppState, DocGuard, DocId, Docs, TabInfo};
use crate::session::Session;
use crate::view::{Focus, View};

type CmdResult<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// Closing a tab

/// What closing a budget's tab did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// Closed (its recovery copy, if any, went with it).
    Closed,
    /// It has unsaved changes: ask, then close it with `discard` (or save).
    Unsaved,
    /// The only tab: close the window instead.
    Last,
}

/// Closes budget `id`'s tab. Unsaved changes stop it unless `discard` (the
/// user chose to throw them away; with Autosave on the desktop autosaves
/// first, so only what can't be autosaved is left). Gives back the budget
/// closed, if one was, for the desktop to remove its recovery copy.
pub fn close_doc(
    docs: &mut Docs,
    id: DocId,
    discard: bool,
) -> CmdResult<(Closed, Option<Session>)> {
    let s = docs.get(id).ok_or("That budget has been closed.")?;
    if docs.len() == 1 {
        return Ok((Closed::Last, None));
    }
    if s.is_dirty() && !discard {
        return Ok((Closed::Unsaved, None));
    }
    let gone = docs.remove(id)?;
    Ok((Closed::Closed, Some(gone)))
}

/// Closes the last tab for the start window (decisions.md, entry 64): its
/// place goes to an untouched Untitled budget, as at launch, which the start
/// window shows over and gives up to whatever is opened or made next.
/// Unsaved changes stop it unless `discard`. With other tabs open, it is
/// [`close_doc`].
pub fn close_last_for_start(
    docs: &mut Docs,
    id: DocId,
    discard: bool,
) -> CmdResult<(Closed, Option<Session>)> {
    let s = docs.get(id).ok_or("That budget has been closed.")?;
    if docs.len() == 1 {
        if s.is_dirty() && !discard {
            return Ok((Closed::Unsaved, None));
        }
        let (_, gone) = docs.replace(id, Session::new())?;
        return Ok((Closed::Closed, Some(gone)));
    }
    close_doc(docs, id, discard)
}

/// What `close_tab` did.
#[derive(Debug, Serialize)]
pub struct Closing {
    /// The tab closed; `view` is the budget now in front.
    pub closed: bool,
    /// Not closed: it has unsaved changes Autosave couldn't save. Ask, then
    /// close it with `discard` (or save it first).
    pub unsaved: bool,
    /// Not closed: it is the only tab. Close the window instead (which asks
    /// about unsaved changes on its way out).
    pub last: bool,
    pub view: Option<View>,
}

impl Closing {
    /// The reply for `closed`, with the view of the budget now in front
    /// (`view`) when the tab closed.
    pub fn of(closed: Closed, view: impl FnOnce() -> View) -> Closing {
        let mut out = Closing {
            closed: false,
            unsaved: false,
            last: false,
            view: None,
        };
        match closed {
            Closed::Last => out.last = true,
            Closed::Unsaved => out.unsaved = true,
            Closed::Closed => {
                out.closed = true;
                out.view = Some(view());
            }
        }
        out
    }
}

/// The account named as `focus_account` names it, or `None`.
pub fn focus_of(
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> Option<Focus> {
    category.zip(account).map(|(category, account)| Focus {
        category,
        ti: ti.unwrap_or(usize::MAX),
        account,
        ai: ai.unwrap_or(usize::MAX),
    })
}

// ---------------------------------------------------------------------------
// Commands

/// A new tab with a fresh starter budget, in front. Its id.
pub fn new_tab(docs: &mut Docs) -> DocId {
    docs.add(Session::new())
}

/// Brings budget `doc`'s tab to the front; its lock, for the view. Split,
/// `pane` says which pane's tab was picked (that pane becomes the target);
/// without it the tab comes to the front where it is (the target pane
/// first).
pub fn switch_tab(state: &AppState, doc: DocId, pane: Option<usize>) -> CmdResult<DocGuard<'_>> {
    if let Some(p) = pane {
        state.docs().switch_in(p, doc)?;
    }
    let mut s = state.lock_doc(Some(doc))?;
    if pane.is_none() {
        s.bring_to_front();
    }
    Ok(s)
}

/// Moves budget `doc`'s tab to place `to` in tab order (the end when past
/// it); the tabs as they are now.
pub fn move_tab(docs: &mut Docs, doc: DocId, to: usize) -> CmdResult<Vec<TabInfo>> {
    docs.move_tab(doc, to)?;
    Ok(docs.tabs())
}

/// The open budgets' tabs, in order.
pub fn list_tabs(docs: &Docs) -> Vec<TabInfo> {
    docs.tabs()
}

/// Closes budget `doc`'s tab (`None`: the one in front), with no Autosave
/// and no recovery copies (the desktop's command adds both). What is
/// unsaved stops it unless `discard`. Split, `pane` names the pane whose
/// tab it is: when the other pane shows the budget too, only this tab goes.
/// With `to_start`, the last tab closes too and an untouched Untitled
/// budget takes its place; without it, the last tab answers `last`.
pub fn close_tab(
    state: &AppState,
    doc: Option<DocId>,
    discard: bool,
    pane: Option<usize>,
    to_start: Option<bool>,
) -> CmdResult<Closing> {
    let id = state.lock_doc(doc)?.id();
    if pane.is_some_and(|p| state.docs().close_in_pane(p, id)) {
        return Ok(Closing::of(Closed::Closed, || state.lock().finish(None)));
    }
    let (closed, _) = if to_start == Some(true) {
        close_last_for_start(&mut state.docs(), id, discard)?
    } else {
        close_doc(&mut state.docs(), id, discard)?
    };
    Ok(Closing::of(closed, || state.lock().finish(None)))
}

/// Splits the window: a second pane at the right showing the budget in front
/// too, now the target. The lock of the budget in front.
pub fn split_view(state: &AppState) -> CmdResult<DocGuard<'_>> {
    state.docs().split_right()?;
    Ok(state.lock())
}

/// Closes the split: every tab back in one strip, the target's budget in
/// front. The lock of the budget in front.
pub fn close_split(state: &AppState) -> CmdResult<DocGuard<'_>> {
    state.docs().close_split()?;
    Ok(state.lock())
}

/// Makes `pane` the target (a click or focus in it): its budget in front
/// becomes the one commands act on, showing the account that pane is on
/// (named as `focus_account` names it; none for the topsheet or a list of
/// accounts). Its lock.
pub fn set_target_pane(
    state: &AppState,
    pane: usize,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> CmdResult<DocGuard<'_>> {
    state.docs().set_target(pane)?;
    let mut s = state.lock();
    s.set_focus(focus_of(category, ti, account, ai));
    Ok(s)
}

/// Moves budget `doc`'s tab to place `to` among `pane`'s tabs. The lock of
/// the budget in front.
pub fn move_tab_in_pane(
    state: &AppState,
    pane: usize,
    doc: DocId,
    to: usize,
) -> CmdResult<DocGuard<'_>> {
    state.docs().move_in_pane(pane, doc, to)?;
    Ok(state.lock())
}

/// Moves budget `doc`'s tab from pane `from` to the other pane (at place
/// `to` there, or the end), in front there; that pane becomes the target.
/// The lock of the budget in front.
pub fn move_to_pane(
    state: &AppState,
    doc: DocId,
    from: usize,
    to: Option<usize>,
) -> CmdResult<DocGuard<'_>> {
    state.docs().move_to_pane(doc, from, to)?;
    Ok(state.lock())
}

/// Budget `doc` as the pane that isn't the target draws it: with its own
/// account on screen (named as `focus_account` names it), without changing
/// the budget's account, the budget in front or anything else, and without
/// telling the other windows (nothing changed).
pub fn pane_view(
    state: &AppState,
    doc: DocId,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> CmdResult<View> {
    let mut s = state.lock_doc(Some(doc))?;
    let focus = focus_of(category, ti, account, ai);
    let mut view = s.view_at(focus.as_ref());
    view.doc = s.id();
    view.tabs = s.docs().tabs();
    view.panes = Some(s.docs().layout());
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty() -> Session {
        let mut s = Session::new();
        s.edit("Test edit", |b| crate::edit::set_title(b, "Edited"))
            .unwrap();
        s
    }

    #[test]
    fn closing_keeps_the_last_tab_and_unsaved_changes() {
        let mut docs = Docs::default();
        let only = docs.active();
        assert_eq!(close_doc(&mut docs, only, false).unwrap().0, Closed::Last);
        let edited = docs.add(dirty());
        assert_eq!(
            close_doc(&mut docs, edited, false).unwrap().0,
            Closed::Unsaved
        );
        let (closed, gone) = close_doc(&mut docs, edited, true).unwrap();
        assert_eq!(closed, Closed::Closed);
        assert_eq!(gone.unwrap().budget().title, "Edited");
        assert!(close_doc(&mut docs, edited, true).is_err());

        // For the start window, the last tab gives way to a fresh budget.
        let only = docs.active();
        let (closed, _) = close_last_for_start(&mut docs, only, false).unwrap();
        assert_eq!(closed, Closed::Closed);
        assert_eq!(docs.len(), 1);
        assert_ne!(docs.active(), only);
    }

    #[test]
    fn tabs_open_switch_and_close() {
        let state = AppState::default();
        let first = state.docs().active();
        let second = new_tab(&mut state.docs());
        assert_eq!(state.docs().active(), second);
        assert_eq!(switch_tab(&state, first, None).unwrap().id(), first);
        assert_eq!(state.docs().active(), first);
        let tabs = move_tab(&mut state.docs(), second, 0).unwrap();
        assert_eq!(tabs[0].id, second);
        let out = close_tab(&state, Some(first), false, None, None).unwrap();
        assert!(out.closed);
        assert_eq!(out.view.unwrap().doc, second);
        let out = close_tab(&state, None, false, None, None).unwrap();
        assert!(out.last && out.view.is_none());
    }

    #[test]
    fn tabs_and_panes_through_the_host() {
        use serde_json::json;
        let h = crate::host::Host::new();
        let call =
            |c: &str, a: serde_json::Value| h.dispatch(c, a).unwrap_or_else(|e| panic!("{c}: {e}"));
        let first = call("get_view", json!({}))["doc"].clone();
        call("set_title", json!({"title": "Second"}));
        let v = call("new_tab", json!({}));
        let second = v["doc"].clone();
        assert_eq!(v["tabs"].as_array().unwrap().len(), 2);
        let v = call("switch_tab", json!({"doc": first}));
        assert_eq!(v["doc"], first);
        let tabs = call("move_tab", json!({"doc": second, "to": 0}));
        assert_eq!(tabs[0]["id"], second);
        assert_eq!(call("list_tabs", json!({})), tabs);

        let v = call("split_view", json!({}));
        assert!(!v["panes"]["split"].is_null(), "{v}");
        call("set_target_pane", json!({"pane": 0, "category": null}));
        call(
            "move_to_pane",
            json!({"doc": second, "from": 0, "to": null}),
        );
        let v = call("pane_view", json!({"doc": second}));
        assert_eq!(v["doc"], second);
        call("close_split", json!({}));

        // The first budget has unsaved changes; the page asks, then discards.
        let r = call("close_tab", json!({"doc": first, "discard": false}));
        assert_eq!(r["unsaved"], true);
        let r = call("close_tab", json!({"doc": first, "discard": true}));
        assert_eq!(r["closed"], true);
        assert_eq!(r["view"]["doc"], second);
        let r = call("close_tab", json!({"discard": false}));
        assert_eq!(r["last"], true);
        let r = call("close_tab", json!({"discard": false, "toStart": true}));
        assert_eq!(r["closed"], true);
        assert_ne!(r["view"]["doc"], second);
    }
}
