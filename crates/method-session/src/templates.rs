//! Templates (roadmap 1.20; decisions.md, entry 29): File > New From
//! Template… and File > Save as Template….
//!
//! A template is an ordinary budget file, no new format. What makes a
//! budget a good start for another is its chart (categories, accounts and
//! production totals), its setup (globals, fringes, groups, units, sets,
//! locations, currencies, charges, credits, presets) and its settings
//! (account numbering, fringe posting, new-line defaults, currencies). What
//! belongs to one production is left out: the detail lines (unless the user
//! keeps them), the budget info, the baseline, the saved versions and every
//! line note. [`strip`] does that, and both commands use it:
//!
//! - **New From Template** reads a picked budget file (from its tab when it is
//!   open), strips it and opens it in a new tab with no file, titled
//!   "Untitled", so the first Save asks where.
//! - **Save as Template** writes the budget in front, stripped, to a new
//!   file picked in a Save dialog, titled after that file. The budget itself
//!   is not changed and keeps its own file.
//!
//! The dialogs, reading the picked file and writing the template are the
//! host's (the desktop's src/templates.rs); this is what they do with the
//! budget.

use method_core::Budget;

use crate::docs::{AppState, DocId};
use crate::session::Session;

/// What a budget opened from a template is called until it is saved.
pub const UNTITLED: &str = "Untitled";

/// `b` as a template titled `title`: chart, setup and settings kept; budget
/// info, baseline, versions and the notes on categories and accounts
/// cleared; the detail rows dropped, or with `keep_lines` kept without their
/// notes.
pub fn strip(b: &Budget, keep_lines: bool, title: &str) -> Budget {
    let mut t = b.clone();
    t.title = title.trim().to_string();
    t.info.clear();
    t.baseline = None;
    t.versions.clear();
    // Actuals are one production's spending.
    t.actuals = Default::default();
    // A template is nobody's copy: a shared copy's stamp would open it read-only.
    t.provenance = None;
    for c in t.categories_mut() {
        c.notes.clear();
        for a in &mut c.accounts {
            a.notes.clear();
            if keep_lines {
                for d in &mut a.details {
                    d.notes.clear();
                }
            } else {
                a.details.clear();
            }
        }
    }
    t
}

/// Opens `template` stripped (as [`strip`] says) in a new tab, in front,
/// with no file; its id.
pub fn open_new(state: &AppState, template: &Budget, keep_lines: bool) -> DocId {
    let b = strip(template, keep_lines, UNTITLED);
    state.docs().add(Session::with_budget(b, None))
}

/// The budget in a picked file's bytes, `name` being the file's name (for
/// the message when it isn't one).
pub fn budget_from_bytes(name: &str, bytes: &[u8]) -> Result<Budget, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("Couldn't read {name}: stream did not contain valid UTF-8"))?;
    method_format::from_str(text).map_err(|e| format!("{name} isn't a readable budget: {e}"))
}

/// A file name stem from a budget title: path separators and other characters
/// file systems reject become spaces.
pub fn safe_file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.to_string()
    }
}

/// The name Save as Template suggests for budget `s`, without the
/// extension: its file's name (or its title) and " template".
pub fn template_name(s: &Session) -> String {
    let stem = match s.path().and_then(|p| p.file_stem()) {
        Some(stem) => stem.to_string_lossy().into_owned(),
        None => s.budget().title.trim().to_string(),
    };
    format!("{} template", safe_file_stem(&stem))
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/templates.rs and the host's
// src/host/templates.rs call these)

/// Opens a new budget from `template` (a picked budget) in a new tab, in
/// front: its chart, setup and settings, no lines unless `keep_lines`.
pub fn template_new(state: &AppState, template: &Budget, keep_lines: bool) -> DocId {
    open_new(state, template, keep_lines)
}

/// Budget `s` as a template titled `title` (the file's name), as `.mthd`
/// text: no lines unless `keep_lines`, no info, baseline, versions or
/// notes. The budget is not changed.
pub fn template_save(s: &Session, keep_lines: bool, title: &str) -> String {
    method_format::to_string(&strip(s.budget(), keep_lines, title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::Docs;
    use method_core::{starter, Baseline, InfoValue};

    fn full() -> Budget {
        let mut b = starter::budget();
        b.title = "Big Show".into();
        b.info
            .insert("Director".into(), InfoValue::Text("Someone".into()));
        b.baseline = Some(Baseline::default());
        method_core::save_version(&mut b, "Draft", "2026-10-09T10:00:00Z", None);
        let c = b.categories_mut().next().unwrap();
        c.notes = vec![method_core::Note::plain(1, "category note")];
        c.accounts[0].notes = vec![method_core::Note::plain(1, "account note")];
        let d = &mut c.accounts[0].details[0];
        d.notes = vec![method_core::Note::plain(1, "private")];
        b
    }

    fn lines(b: &Budget) -> usize {
        b.categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum()
    }

    #[test]
    fn a_template_keeps_chart_setup_and_settings_only() {
        let b = full();
        let t = strip(&b, false, "Feature template");
        assert_eq!(t.title, "Feature template");
        assert_eq!(lines(&t), 0, "no detail lines");
        assert!(t.info.is_empty() && t.baseline.is_none() && t.versions.is_empty());
        // The chart, setup and settings are as they were.
        let codes = |b: &Budget| -> Vec<String> {
            b.categories()
                .flat_map(|c| &c.accounts)
                .map(|a| a.code.clone())
                .collect()
        };
        assert_eq!(codes(&t), codes(&b));
        assert_eq!(t.topsheet.len(), b.topsheet.len());
        assert_eq!(t.globals, b.globals);
        assert_eq!(t.fringes, b.fringes);
        assert_eq!(t.charges, b.charges);
        assert_eq!(t.units, b.units);
        assert_eq!(t.account_coding, b.account_coding);
        assert_eq!(t.fringe_posting, b.fringe_posting);
        assert_eq!(t.base_currency, b.base_currency);
        // It recalculates (to its charges' share of nothing).
        let r = method_core::recalc(&t, &method_core::RecalcOptions::default()).unwrap();
        assert_eq!(r.grand_total, 0);

        let kept = strip(&b, true, "With lines");
        assert_eq!(lines(&kept), lines(&b), "keep lines");
        assert!(kept
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .all(|d| d.notes.is_empty()));
        assert!(kept
            .categories()
            .all(|c| c.notes.is_empty() && c.accounts.iter().all(|a| a.notes.is_empty())));
        // The format round-trips it.
        let text = method_format::to_string(&t);
        assert_eq!(method_format::from_str(&text).unwrap(), t);
    }

    #[test]
    fn a_template_opens_untitled_in_a_new_tab() {
        let state = AppState::with(Docs::default());
        let first = state.docs().active();
        let id = open_new(&state, &full(), false);
        let docs = state.docs();
        assert_ne!(id, first);
        assert_eq!(docs.len(), 2, "a new tab, the first one kept");
        assert_eq!(docs.active(), id, "in front");
        let s = docs.get(id).unwrap();
        assert!(s.path().is_none(), "no file: the first Save asks where");
        assert_eq!(s.budget().title, UNTITLED);
        assert_eq!(s.tab_label(), UNTITLED);
        assert_eq!(lines(s.budget()), 0);
        assert!(s.budget().versions.is_empty());
    }

    #[test]
    fn a_template_is_saved_under_a_suggested_name() {
        let mut s = Session::with_budget(full(), None);
        assert_eq!(template_name(&s), "Big Show template");
        s.edit("Test edit", |b| {
            b.title = "A/B: test".into();
            Ok(())
        })
        .unwrap();
        assert_eq!(template_name(&s), "A B  test template");
        let text = template_save(&s, false, "A B  test template");
        let t = budget_from_bytes("t.mthd", text.as_bytes()).unwrap();
        assert_eq!(t.title, "A B  test template");
        assert_eq!(lines(&t), 0);
        assert!(budget_from_bytes("x.mthd", b"\xff").is_err());
        assert!(budget_from_bytes("x.mthd", b"{}")
            .unwrap_err()
            .starts_with("x.mthd isn't a readable budget"));
        assert_eq!(safe_file_stem(" .. "), "Untitled");
    }
}
