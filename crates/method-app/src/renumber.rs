//! Renumbering accounts when the account-number pattern changes (roadmap
//! 2.20; decisions.md, entry 39). The engine works it out
//! (`method_core::renumber`); these are the commands the Settings tab's
//! "Account numbers" form calls: a preview of every category, account and
//! category fringe account number before and after, with any conflicts, and
//! the renumbering itself, which also sets the pattern, as one undo step.
//! Changing the pattern without renumbering stays `set_account_coding`.

use method_core::coding::AccountCoding;
use method_core::renumber::{self, CodeChange, Conflict, RenumberError, RenumberPlan};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// A pattern from the form's widths and separator (one character, or blank
/// or `None` for none), checked against the pattern rules.
pub fn coding_from(widths: [u8; 4], separator: Option<&str>) -> EditResult<AccountCoding> {
    let separator = match separator {
        None | Some("") => None,
        Some(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => return Err("the separator is one character, or none".into()),
            }
        }
    };
    let coding = AccountCoding {
        category_width: widths[0],
        account_width: widths[1],
        sub_width: widths[2],
        set_width: widths[3],
        separator,
    };
    coding.validate().map_err(|e| e.to_string())?;
    Ok(coding)
}

/// One number before and after, for the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChangeDto {
    pub old: String,
    /// Empty when it doesn't fit the new pattern.
    pub new: String,
}

impl From<&CodeChange> for ChangeDto {
    fn from(c: &CodeChange) -> ChangeDto {
        ChangeDto {
            old: c.old.clone(),
            new: c.new.clone(),
        }
    }
}

/// What renumbering would do, for the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanDto {
    pub categories: Vec<ChangeDto>,
    pub accounts: Vec<ChangeDto>,
    pub fringe_accounts: Vec<ChangeDto>,
    /// Why it can't go ahead, one sentence each; empty when it can.
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
    /// Whether any category or account number would change.
    pub changes: bool,
}

impl From<&RenumberPlan> for PlanDto {
    fn from(p: &RenumberPlan) -> PlanDto {
        let list = |v: &[CodeChange]| v.iter().map(ChangeDto::from).collect();
        PlanDto {
            categories: list(&p.categories),
            accounts: list(&p.accounts),
            fringe_accounts: list(&p.fringe_accounts),
            conflicts: p.conflicts.iter().map(Conflict::to_string).collect(),
            warnings: p.warnings.clone(),
            changes: p.changes_anything(),
        }
    }
}

/// The preview of renumbering `b` to the pattern the form describes.
pub fn preview(
    b: &method_core::Budget,
    widths: [u8; 4],
    separator: Option<&str>,
) -> EditResult<PlanDto> {
    let to = coding_from(widths, separator)?;
    let plan = renumber::plan(b, &to).map_err(|e| e.to_string())?;
    Ok(PlanDto::from(&plan))
}

/// Renumbers `b` to the pattern the form describes and sets it. Refused,
/// changing nothing, on any conflict.
pub fn apply(b: &mut method_core::Budget, widths: [u8; 4], separator: Option<&str>) -> EditResult {
    let to = coding_from(widths, separator)?;
    renumber::renumber(b, to)
        .map(|_| ())
        .map_err(|e: RenumberError| e.to_string())
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs, and the main window's and the
// Budget Setup window's capabilities: the Settings tab is in Budget Setup)

/// What renumbering budget `doc` to the pattern would do. Changes nothing.
/// Each width is its own argument, as for `set_account_coding`.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn renumber_preview(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> Result<PlanDto, String> {
    let s = state.lock_doc(doc)?;
    preview(
        s.budget(),
        [category_width, account_width, sub_width, set_width],
        separator.as_deref(),
    )
}

/// Sets the pattern and renumbers every category and account to it, with
/// everything that names them, as one undo step.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn renumber_accounts<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> Result<View, String> {
    let widths = [category_width, account_width, sub_width, set_width];
    edit_budget(
        &window,
        &state,
        doc,
        "Renumber accounts to the new pattern",
        |b| apply(b, widths, separator.as_deref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    fn codes(s: &Session) -> Vec<String> {
        s.budget()
            .categories()
            .flat_map(|c| c.accounts.iter().map(|a| a.code.clone()))
            .collect()
    }

    #[test]
    fn patterns_from_the_form() {
        assert_eq!(
            coding_from([2, 2, 0, 0], Some("-")).unwrap(),
            AccountCoding::default()
        );
        assert_eq!(coding_from([2, 2, 0, 0], Some("")).unwrap().separator, None);
        assert!(coding_from([2, 2, 0, 0], Some("--")).is_err());
        assert!(coding_from([2, 0, 2, 0], None).is_err());
        assert!(coding_from([7, 2, 0, 0], None).is_err());
    }

    #[test]
    fn preview_then_renumber_as_one_undo_step() {
        let mut b = method_core::starter::budget();
        b.fringe_posting = method_core::FringePosting::Category {
            suffix: "99".into(),
        };
        let mut s = Session::with_budget(b, None);
        let before = codes(&s);
        let p = preview(s.budget(), [3, 3, 0, 0], Some(".")).unwrap();
        assert!(p.changes && p.conflicts.is_empty());
        assert_eq!(p.accounts.len(), before.len());
        assert_eq!(p.accounts[0].old, before[0]);
        assert!(
            !p.fringe_accounts.is_empty(),
            "this budget posts fringes by category"
        );
        // The preview changes nothing.
        assert_eq!(codes(&s), before);

        s.edit("Renumber", |b| apply(b, [3, 3, 0, 0], Some(".")))
            .unwrap();
        let after = codes(&s);
        assert_eq!(
            after,
            p.accounts.iter().map(|c| c.new.clone()).collect::<Vec<_>>()
        );
        assert_eq!(s.budget().account_coding.separator, Some('.'));
        assert!(s.can_undo());
        s.undo();
        assert_eq!(codes(&s), before);
        assert_eq!(s.budget().account_coding, AccountCoding::new_budget());
    }

    #[test]
    fn conflicts_are_listed_and_refused() {
        let mut s = Session::new();
        s.edit("Add", |b| {
            crate::edit::add_account(b, "50", "50-12", "Twelve", None)
        })
        .unwrap();
        // A one-character account part can't hold 12.
        let p = preview(s.budget(), [2, 1, 0, 0], Some("-")).unwrap();
        assert!(
            p.conflicts.iter().any(|c| c.contains("50-12")),
            "{:?}",
            p.conflicts
        );
        assert!(p
            .accounts
            .iter()
            .any(|c| c.old == "50-12" && c.new.is_empty()));
        let before = s.budget().clone();
        let e = s
            .edit("Renumber", |b| apply(b, [2, 1, 0, 0], Some("-")))
            .unwrap_err();
        assert!(e.starts_with("can't renumber"), "{e}");
        assert_eq!(s.budget(), &before, "refused: nothing changed");
    }
}
