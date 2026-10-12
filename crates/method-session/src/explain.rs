//! "Explain This Number…" (roadmap 3.5; decisions.md, entry 43): what the
//! tools panel's Explain tab calls. The engine works the tree out
//! (`method_core::explain`); this sends it to the page.
//!
//! Computed on demand only: nothing here runs on an edit. A total explained
//! is worked out from the budget the totals on screen came from (the
//! sanitized copy when some cells have problems, which is said).
//!
//! What crosses the bridge stays small: a step that can be explained on its
//! own (`expand`) sends its children only near the top of the tree
//! ([`SEND_DEPTH`]); deeper ones say `more` and the page asks for them when
//! they are opened. A grand total of 5,000 lines sends a few dozen steps,
//! not 50,000.

use method_core::explain::{Explainer, Explanation, Link, Role, Step, Target};
use method_core::{Budget, Cents, RecalcOptions, RowId, TopsheetRow};
use serde::{Deserialize, Serialize};

use crate::session::Session;

/// A step that can be explained on its own sends its children only when it
/// is less deep than this (the root is 0).
pub const SEND_DEPTH: usize = 2;

/// A total, as the page names it. Categories and accounts by their place
/// (`ci`: among the categories; `ai`: in the category), as the engine does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetDto {
    Line { row_id: RowId },
    LineFringes { row_id: RowId },
    LineFringe { row_id: RowId, name: String },
    Account { ci: usize, ai: usize },
    AccountLines { ci: usize, ai: usize },
    AccountFringes { ci: usize, ai: usize },
    Category { ci: usize },
    CategoryFringes { ci: usize },
    Level { index: usize },
    LevelFringes { index: usize },
    Atl,
    Btl,
    AtlBtl,
    BudgetFringes,
    Charge { index: usize },
    GrandTotal,
    Credit { index: usize },
    CreditsTotal,
    NetTotal,
    Fringe { name: String },
}

impl From<&TargetDto> for Target {
    fn from(t: &TargetDto) -> Target {
        match t {
            TargetDto::Line { row_id } => Target::Line(*row_id),
            TargetDto::LineFringes { row_id } => Target::LineFringes(*row_id),
            TargetDto::LineFringe { row_id, name } => Target::LineFringe(*row_id, name.clone()),
            TargetDto::Account { ci, ai } => Target::Account { ci: *ci, ai: *ai },
            TargetDto::AccountLines { ci, ai } => Target::AccountLines { ci: *ci, ai: *ai },
            TargetDto::AccountFringes { ci, ai } => Target::AccountFringes { ci: *ci, ai: *ai },
            TargetDto::Category { ci } => Target::Category(*ci),
            TargetDto::CategoryFringes { ci } => Target::CategoryFringes(*ci),
            TargetDto::Level { index } => Target::Level(*index),
            TargetDto::LevelFringes { index } => Target::LevelFringes(*index),
            TargetDto::Atl => Target::Atl,
            TargetDto::Btl => Target::Btl,
            TargetDto::AtlBtl => Target::AtlBtl,
            TargetDto::BudgetFringes => Target::BudgetFringes,
            TargetDto::Charge { index } => Target::Charge(*index),
            TargetDto::GrandTotal => Target::GrandTotal,
            TargetDto::Credit { index } => Target::Credit(*index),
            TargetDto::CreditsTotal => Target::CreditsTotal,
            TargetDto::NetTotal => Target::NetTotal,
            TargetDto::Fringe { name } => Target::Fringe(name.clone()),
        }
    }
}

impl From<&Target> for TargetDto {
    fn from(t: &Target) -> TargetDto {
        match t {
            Target::Line(id) => TargetDto::Line { row_id: *id },
            Target::LineFringes(id) => TargetDto::LineFringes { row_id: *id },
            Target::LineFringe(id, name) => TargetDto::LineFringe {
                row_id: *id,
                name: name.clone(),
            },
            Target::Account { ci, ai } => TargetDto::Account { ci: *ci, ai: *ai },
            Target::AccountLines { ci, ai } => TargetDto::AccountLines { ci: *ci, ai: *ai },
            Target::AccountFringes { ci, ai } => TargetDto::AccountFringes { ci: *ci, ai: *ai },
            Target::Category(ci) => TargetDto::Category { ci: *ci },
            Target::CategoryFringes(ci) => TargetDto::CategoryFringes { ci: *ci },
            Target::Level(i) => TargetDto::Level { index: *i },
            Target::LevelFringes(i) => TargetDto::LevelFringes { index: *i },
            Target::Atl => TargetDto::Atl,
            Target::Btl => TargetDto::Btl,
            Target::AtlBtl => TargetDto::AtlBtl,
            Target::BudgetFringes => TargetDto::BudgetFringes,
            Target::Charge(i) => TargetDto::Charge { index: *i },
            Target::GrandTotal => TargetDto::GrandTotal,
            Target::Credit(i) => TargetDto::Credit { index: *i },
            Target::CreditsTotal => TargetDto::CreditsTotal,
            Target::NetTotal => TargetDto::NetTotal,
            Target::Fringe(name) => TargetDto::Fringe { name: name.clone() },
        }
    }
}

/// Where a step goes when clicked, in the shape the page's `jumpTo` takes:
/// a row (`ti`, `ai`, `account`, `row_id`), an account (no `row_id`) or a
/// category (`kind: "category"`, `ti`, `category`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JumpDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    pub ti: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub row_id: Option<RowId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StepDto {
    pub label: String,
    pub text: String,
    pub amount: Option<Cents>,
    /// "sum", "value" or "note".
    pub role: &'static str,
    pub minus: bool,
    pub jump: Option<JumpDto>,
    /// Ask for this to get the step's children when `more` is true.
    pub expand: Option<TargetDto>,
    /// The step has children that weren't sent.
    pub more: bool,
    pub children: Vec<StepDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExplanationDto {
    pub target: TargetDto,
    pub title: String,
    pub reported: Cents,
    pub root: StepDto,
    /// Totals treat cells with problems as blank, and so does this.
    pub partial: bool,
}

/// Topsheet index of each category, by its place among the categories.
fn category_rows(b: &Budget) -> Vec<(usize, &method_core::Category)> {
    b.topsheet
        .iter()
        .enumerate()
        .filter_map(|(ti, r)| match r {
            TopsheetRow::Category(c) => Some((ti, c)),
            TopsheetRow::ProductionTotal(_) => None,
        })
        .collect()
}

fn jump(cats: &[(usize, &method_core::Category)], link: &Link) -> Option<JumpDto> {
    let (ci, ai, row) = match *link {
        Link::Row { ci, ai, row } => (ci, Some(ai), Some(row)),
        Link::Account { ci, ai } => (ci, Some(ai), None),
        Link::Category { ci } => (ci, None, None),
    };
    let &(ti, cat) = cats.get(ci)?;
    Some(match ai {
        Some(ai) => JumpDto {
            kind: None,
            ti,
            ai: Some(ai),
            account: Some(cat.accounts.get(ai)?.code.clone()),
            category: None,
            row_id: row,
        },
        None => JumpDto {
            kind: Some("category"),
            ti,
            ai: None,
            account: None,
            category: Some(cat.code.clone()),
            row_id: None,
        },
    })
}

/// `s` for the page, children cut below [`SEND_DEPTH`] where they can be
/// asked for again (`more`). A note pointing at another total is `more` too.
pub fn step_dto(s: &Step, depth: usize, cats: &[(usize, &method_core::Category)]) -> StepDto {
    let cut = s.expand.is_some() && depth >= SEND_DEPTH && !s.children.is_empty();
    // A note that points at a total explained elsewhere (a line's fringes,
    // a charge's base) opens onto that total's steps.
    let pointer = s.role == Role::Note && s.expand.is_some() && s.children.is_empty();
    StepDto {
        label: s.label.clone(),
        text: s.text.clone(),
        amount: s.amount,
        role: match s.role {
            Role::Sum => "sum",
            Role::Value => "value",
            Role::Note => "note",
        },
        minus: s.minus,
        jump: s.link.as_ref().and_then(|l| jump(cats, l)),
        expand: s.expand.as_ref().map(TargetDto::from),
        more: cut || pointer,
        children: if cut {
            Vec::new()
        } else {
            s.children
                .iter()
                .map(|c| step_dto(c, depth + 1, cats))
                .collect()
        },
    }
}

pub fn explanation_dto(b: &Budget, e: &Explanation, partial: bool) -> ExplanationDto {
    let cats = category_rows(b);
    ExplanationDto {
        target: TargetDto::from(&e.target),
        title: e.title.clone(),
        reported: e.reported,
        root: step_dto(&e.root, 0, &cats),
        partial,
    }
}

/// Explains `target` in `b` (the budget the totals on screen came from).
pub fn explain_in(b: &Budget, target: &TargetDto, partial: bool) -> Result<ExplanationDto, String> {
    let ex = Explainer::new(b, &RecalcOptions::default()).map_err(|e| e.to_string())?;
    let e = ex
        .explain(&Target::from(target))
        .map_err(|e| e.to_string())?;
    Ok(explanation_dto(b, &e, partial))
}

// ---------------------------------------------------------------------------
// Command (the desktop's src/explain.rs and the host's src/host/explain.rs
// call it)

/// How total `target` of the budget is worked out. Changes nothing.
pub fn explain_number(s: &mut Session, target: &TargetDto) -> Result<ExplanationDto, String> {
    let (budget, outcome) = s.totals();
    if outcome.report.is_none() {
        let why = outcome
            .error
            .as_ref()
            .map(|e| format!(": {e}"))
            .unwrap_or_default();
        return Err(format!("the totals can't be worked out{why}"));
    }
    let partial = outcome.sanitized.is_some();
    let b = outcome.sanitized.as_ref().unwrap_or(budget);
    explain_in(b, target, partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    fn first_line(b: &Budget) -> RowId {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.kind == method_core::RowKind::Line)
            .unwrap()
            .id
    }

    fn count(s: &StepDto) -> usize {
        1 + s.children.iter().map(count).sum::<usize>()
    }

    #[test]
    fn targets_read_what_the_page_sends() {
        let t: TargetDto = serde_json::from_str(r#"{"kind": "line", "row_id": 7}"#).unwrap();
        assert_eq!(Target::from(&t), Target::Line(7));
        let t: TargetDto =
            serde_json::from_str(r#"{"kind": "account_fringes", "ci": 1, "ai": 2}"#).unwrap();
        assert_eq!(Target::from(&t), Target::AccountFringes { ci: 1, ai: 2 });
        let t: TargetDto = serde_json::from_str(r#"{"kind": "grand_total"}"#).unwrap();
        assert_eq!(Target::from(&t), Target::GrandTotal);
        assert!(serde_json::from_str::<TargetDto>(r#"{"kind": "line"}"#).is_err());
        assert!(serde_json::from_str::<TargetDto>(r#"{"kind": "nope"}"#).is_err());
        // Every engine target goes there and back.
        let b = method_core::starter::budget();
        let ex = Explainer::new(&b, &RecalcOptions::default()).unwrap();
        for t in ex.targets() {
            assert_eq!(Target::from(&TargetDto::from(&t)), t);
        }
    }

    #[test]
    fn explains_a_line_with_jumps_to_it() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        let (b, _) = s.totals();
        let e = explain_in(b, &TargetDto::Line { row_id: id }, false).unwrap();
        assert_eq!(e.root.amount, Some(e.reported));
        assert_eq!(e.root.role, "value");
        let j = e.root.jump.as_ref().unwrap();
        assert_eq!(j.row_id, Some(id));
        let (code, _) = crate::history::row_place(b, id).unwrap();
        assert_eq!(j.account.as_deref(), Some(code.as_str()));
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(
            json["target"],
            serde_json::json!({"kind": "line", "row_id": id})
        );
    }

    #[test]
    fn big_totals_send_the_top_of_the_tree_only() {
        let b = method_core::sample::budget(&method_core::sample::SampleSpec::default());
        let e = explain_in(&b, &TargetDto::GrandTotal, false).unwrap();
        assert!(count(&e.root) < 400, "{} steps", count(&e.root));
        let text = serde_json::to_string(&e).unwrap();
        assert!(text.len() < 200_000, "{} bytes", text.len());
        // The cut steps can be asked for, and add up the same.
        fn cut(s: &StepDto, out: &mut Vec<TargetDto>) {
            if s.more {
                out.push(s.expand.clone().unwrap());
            }
            for c in &s.children {
                cut(c, out);
            }
        }
        let mut more = Vec::new();
        cut(&e.root, &mut more);
        assert!(!more.is_empty());
        for t in more.iter().take(5) {
            let sub = explain_in(&b, t, false).unwrap();
            assert_eq!(sub.root.amount, Some(sub.reported));
        }
        // Category headers jump to the topsheet row.
        fn find_cat(s: &StepDto) -> Option<&JumpDto> {
            s.jump
                .as_ref()
                .filter(|j| j.kind == Some("category"))
                .or_else(|| s.children.iter().find_map(find_cat))
        }
        let sub = explain_in(&b, &TargetDto::Atl, false).unwrap();
        let j = find_cat(&sub.root).unwrap();
        assert!(matches!(b.topsheet[j.ti], TopsheetRow::Category(_)));
    }

    #[test]
    fn a_bad_target_is_a_message() {
        let b = method_core::starter::budget();
        let e = explain_in(&b, &TargetDto::Line { row_id: 999_999 }, false).unwrap_err();
        assert!(e.contains("not in the budget"), "{e}");
        let e = explain_in(&b, &TargetDto::Charge { index: 99 }, false).unwrap_err();
        assert!(e.contains("not in the budget"), "{e}");
    }
}
