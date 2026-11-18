//! Renaming a setup item (a global, fringe, group, unit, set, location or
//! currency) with every reference to it rewritten, as one undo step (roadmap
//! 1.18, decision 7). The engine does the work (`method_core::rename`); this is
//! the command the tools panel calls from an item's editor, its right-click
//! menu, and the Find tab.

use method_core::rename::{self as core_rename, RenameError, SetupKind};
use method_core::Budget;
use serde::Serialize;

use crate::edit::EditResult;
use crate::messages::{msg, word};
use crate::session::Session;
use crate::view::View;

/// Renames item `from` of kind `kind` (`"global"`, `"fringe"`, `"group"`,
/// `"unit"`, `"set"`, `"location"` or `"currency"`) to `to`, trimmed.
/// Returns the new name and how many places now use it. Nothing changes on
/// an error.
pub fn rename_item(
    b: &mut Budget,
    kind: &str,
    from: &str,
    to: &str,
) -> EditResult<(String, usize)> {
    let kind = SetupKind::from_name(kind)
        .ok_or_else(|| msg("engine.setup.unknown_kind", &[("kind", &kind)]))?;
    let places = core_rename::rename(b, kind, from, to).map_err(|e| rename_err(&e))?;
    Ok((to.trim().to_string(), places))
}

/// method-core's rename refusal in the catalog's words (decision 94): the
/// same English it writes itself (tested), with a code the desktop can show
/// in the person's language.
pub(crate) fn rename_err(e: &RenameError) -> String {
    match e {
        RenameError::Missing { kind, name } => msg(
            "engine.edit.item_gone",
            &[("what", &kind_noun(kind.name())), ("name", name)],
        ),
        RenameError::Same => word("engine.rename.same"),
        RenameError::BadName { kind, name } => match kind {
            SetupKind::Global => msg("engine.edit.global_name", &[("name", name)]),
            _ if name.is_empty() => msg(
                "engine.rename.new_name_empty",
                &[("what", &kind_noun(kind.name()))],
            ),
            _ => msg(
                "engine.edit.name_comma",
                &[("what", &kind_noun(kind.name()))],
            ),
        },
        RenameError::Taken { kind, name } => match kind {
            SetupKind::Currency => msg("engine.rename.currency_taken", &[("name", name)]),
            _ => msg(
                "engine.rename.taken",
                &[("what", &kind_noun(kind.name())), ("name", name)],
            ),
        },
        RenameError::InUse { kind, name, places } => msg(
            "engine.rename.in_use",
            &[
                ("name", name),
                ("count", places),
                ("what", &kind_noun(kind.name())),
            ],
        ),
    }
}

/// A kind of setup item as a step label names it ("fringe", "global"); an
/// unknown kind is just "item" (the rename refuses it anyway).
fn kind_noun(kind: &str) -> String {
    crate::messages::word(match kind {
        "global" => "engine.what.global",
        "fringe" => "engine.what.fringe",
        "group" => "engine.what.group",
        "unit" => "engine.what.unit",
        "set" => "engine.what.set",
        "location" => "engine.what.location",
        "currency" => "engine.what.currency",
        _ => "engine.what.item",
    })
}

/// The view after a rename, the new name, and how many places were rewritten.
#[derive(Clone, Debug, Serialize)]
pub struct Renamed {
    #[serde(flatten)]
    pub view: View,
    pub name: String,
    pub places: usize,
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/rename.rs and the host's src/host/rename.rs
// call these)

/// Renames a setup item everywhere, as one undo step. See [`rename_item`].
/// The new name and how many places now use it.
pub fn rename_setup_item(
    s: &mut Session,
    kind: &str,
    from: &str,
    to: &str,
) -> EditResult<(String, usize)> {
    let mut out = (String::new(), 0);
    let label = crate::messages::msg(
        "engine.undo.rename_item",
        &[
            ("what", &kind_noun(kind)),
            ("from", &from.trim()),
            ("to", &to.trim()),
        ],
    );
    // A shared budget's change log records the rename as one write to the
    // item's name (src/shared.rs), not a delete and an add.
    let hints: Vec<method_core::log::Hint> = SetupKind::from_name(kind)
        .map(|k| vec![method_core::log::Hint::rename(k, from, to)])
        .unwrap_or_default();
    s.edit_with_hints(label, &hints, |b| {
        out = rename_item(b, kind, from, to)?;
        Ok(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::{Detail, RowId};

    /// Every refusal method-core's rename can give comes out in the same
    /// English, recognized as a code, for every kind of item.
    #[test]
    fn every_rename_refusal_keeps_its_english_and_has_a_code() {
        for &kind in SetupKind::ALL {
            let name = "Crew".to_string();
            let errors = [
                RenameError::Missing {
                    kind,
                    name: name.clone(),
                },
                RenameError::Same,
                RenameError::BadName {
                    kind,
                    name: "a,b".into(),
                },
                RenameError::BadName {
                    kind,
                    name: String::new(),
                },
                RenameError::Taken {
                    kind,
                    name: name.clone(),
                },
                RenameError::InUse {
                    kind,
                    name: name.clone(),
                    places: 1,
                },
                RenameError::InUse {
                    kind,
                    name: name.clone(),
                    places: 3,
                },
            ];
            for e in errors {
                let text = rename_err(&e);
                assert_eq!(text, e.to_string(), "{kind:?}");
                assert!(
                    crate::messages::recognize(&text).is_some(),
                    "{kind:?}: {text} has no code"
                );
            }
        }
    }

    fn first_line(b: &Budget) -> RowId {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.kind == method_core::RowKind::Line)
            .unwrap()
            .id
    }

    fn row(b: &Budget, id: RowId) -> &Detail {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.id == id)
            .unwrap()
    }

    #[test]
    fn a_rename_is_one_undo_step_and_undoes_cleanly() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        s.edit("Test edit", |b| {
            crate::edit::set_global(b, "DAYS", "5")?;
            crate::edit::set_detail_field(b, id, crate::edit::Field::Amount, "DAYS*2")?;
            crate::edit::set_group(
                b,
                "Crew",
                &serde_json::from_value(serde_json::json!({"in_budget_total": true})).unwrap(),
            )?;
            crate::apply::apply_tag(b, &[id], "groups", "Crew", true)
        })
        .unwrap();
        let before = s.budget().clone();
        let total = s.totals().1.report.as_ref().map(|r| r.net_total);
        let (name, n) = s
            .edit("Test edit", |b| {
                rename_item(b, "global", "DAYS", " BUILD_DAYS ")
            })
            .unwrap();
        assert_eq!((name.as_str(), n), ("BUILD_DAYS", 1));
        assert_eq!(row(s.budget(), id).amount.0, "BUILD_DAYS*2");
        assert_eq!(s.totals().1.report.as_ref().map(|r| r.net_total), total);
        s.edit("Test edit", |b| {
            rename_item(b, "group", "Crew", "Shoot crew")
        })
        .unwrap();
        assert!(row(s.budget(), id).groups.contains("Shoot crew"));
        assert!(s.undo() && s.undo());
        assert_eq!(*s.budget(), before);
    }

    #[test]
    fn refusals_say_why_and_change_nothing() {
        let mut s = Session::new();
        s.edit("Test edit", |b| crate::edit::set_global(b, "DAYS", "5"))
            .unwrap();
        let before = s.budget().clone();
        let e = s
            .edit("Test edit", |b| rename_item(b, "global", "DAYS", "1.5x"))
            .unwrap_err();
        assert!(e.contains("can't be a global name"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "unit", "Day", "Week"))
            .unwrap_err();
        assert!(e.contains("already a unit named Week"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "thing", "a", "b"))
            .unwrap_err();
        assert!(e.contains("unknown kind"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "fringe", "NOPE", "b"))
            .unwrap_err();
        assert!(e.contains("no longer exists"), "{e}");
        assert_eq!(*s.budget(), before);
        assert!(!s.can_undo() || s.undo());
    }
}
