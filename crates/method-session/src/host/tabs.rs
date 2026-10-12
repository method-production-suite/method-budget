//! The tab strip's and the split view's commands (src/tabs.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::docs::DocId;
use crate::tabs as t;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    reply(d).transpose()
}

/// The reply to `d`, or `None` when it names none of these commands.
fn reply(d: &Call<'_>) -> Result<Option<Value>, Error> {
    let state = d.state();
    let v = match d.command() {
        "new_tab" => {
            let id = t::new_tab(&mut state.docs());
            json(state.lock_doc(Some(id))?.finish(None))
        }
        "switch_tab" => {
            let a: Switch = d.args()?;
            json(t::switch_tab(state, a.doc, a.pane)?.finish(None))
        }
        "move_tab" => {
            let a: Move = d.args()?;
            json(t::move_tab(&mut state.docs(), a.doc, a.to)?)
        }
        "list_tabs" => json(t::list_tabs(&state.docs())),
        // Without Autosave or recovery copies: the host keeps the budget's
        // text (`save_mthd`) before it closes the tab.
        "close_tab" => {
            let a: Close = d.args()?;
            json(t::close_tab(state, d.doc(), a.discard, a.pane, a.to_start)?)
        }
        "split_view" => json(t::split_view(state)?.finish(None)),
        "close_split" => json(t::close_split(state)?.finish(None)),
        "set_target_pane" => {
            let a: Target = d.args()?;
            let mut s = t::set_target_pane(state, a.pane, a.category, a.ti, a.account, a.ai)?;
            json(s.finish(None))
        }
        "move_tab_in_pane" => {
            let a: InPane = d.args()?;
            json(t::move_tab_in_pane(state, a.pane, a.doc, a.to)?.finish(None))
        }
        "move_to_pane" => {
            let a: ToPane = d.args()?;
            json(t::move_to_pane(state, a.doc, a.from, a.to)?.finish(None))
        }
        "pane_view" => {
            let a: PaneView = d.args()?;
            json(t::pane_view(
                state, a.doc, a.category, a.ti, a.account, a.ai,
            )?)
        }
        _ => return Ok(None),
    };
    v.map(Some)
}

#[derive(Deserialize)]
struct Switch {
    doc: DocId,
    pane: Option<usize>,
}

#[derive(Deserialize)]
struct Move {
    doc: DocId,
    to: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Close {
    discard: bool,
    pane: Option<usize>,
    to_start: Option<bool>,
}

#[derive(Deserialize)]
struct Target {
    pane: usize,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
}

#[derive(Deserialize)]
struct InPane {
    pane: usize,
    doc: DocId,
    to: usize,
}

#[derive(Deserialize)]
struct ToPane {
    doc: DocId,
    from: usize,
    to: Option<usize>,
}

#[derive(Deserialize)]
struct PaneView {
    doc: DocId,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
}
