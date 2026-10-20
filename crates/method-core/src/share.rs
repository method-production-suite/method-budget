//! Copies made for sharing (roadmap 2.21): a snapshot of the budget, or of
//! one of its saved versions, optionally without what shouldn't leave the
//! office, stamped with where it came from ([`Provenance`]).
//!
//! The application opens a copy whose provenance says
//! [`ProvenanceKind::SharedCopy`] read-only; [`editable_copy`] is what its
//! "Make editable copy" opens. Neither the engine nor the file format ever
//! enforces read-only: the stamp is a request, so a reviewer can't change
//! totals by accident, not a lock.

use std::collections::BTreeSet;

use crate::model::{Budget, Provenance, ProvenanceKind, VersionId};
use crate::rename::SetupKind;
use crate::versions::VersionError;

/// What to leave out of a shared copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShareOptions {
    /// Every note on lines, accounts and categories (and in line blocks).
    pub drop_notes: bool,
    /// The budget info fields (director, start date...).
    pub drop_info: bool,
    /// The saved versions.
    pub drop_versions: bool,
    /// The locked original totals.
    pub drop_baseline: bool,
    /// Setup items nothing uses (see [`prune_unused_setup`]).
    pub drop_unused_setup: bool,
    /// The actuals: transactions, imports and estimates to complete.
    pub drop_actuals: bool,
}

/// A copy of `b` (or of its version `version`) for sharing, without what
/// `opts` leaves out, stamped as a shared copy of `b`: its title, the
/// version's name, `saved` (the caller's clock), `app_version` and `note`.
/// A version's copy has the current presets, display settings and setup
/// order (tools, which versions don't hold) and the version list unless it
/// is left out.
pub fn share_copy(
    b: &Budget,
    version: Option<VersionId>,
    opts: &ShareOptions,
    saved: &str,
    app_version: &str,
    note: Option<String>,
) -> Result<Budget, VersionError> {
    let (mut out, version_name) = match version {
        None => (b.clone(), None),
        Some(id) => {
            let v = b.version(id).ok_or(VersionError::NotFound(id))?;
            let mut out = v.budget.to_budget();
            out.versions = b.versions.clone();
            out.report_presets = b.report_presets.clone();
            out.tag_presets = b.tag_presets.clone();
            out.line_presets = b.line_presets.clone();
            out.ot_patterns = b.ot_patterns.clone();
            out.rate_tables = b.rate_tables.clone();
            out.display = b.display.clone();
            out.setup_order = b.setup_order.clone();
            out.setup_folders = b.setup_folders.clone();
            out.setup_sources = b.setup_sources.clone();
            // Actuals aren't version content: a version's copy has today's.
            out.actuals = b.actuals.clone();
            (out, Some(v.name.clone()))
        }
    };
    if opts.drop_notes {
        drop_notes(&mut out);
    }
    if opts.drop_info {
        out.info.clear();
    }
    if opts.drop_versions {
        out.versions.clear();
    }
    if opts.drop_baseline {
        out.baseline = None;
    }
    if opts.drop_actuals {
        out.actuals = Default::default();
    }
    if opts.drop_unused_setup {
        prune_unused_setup(&mut out);
    }
    out.provenance = Some(Provenance {
        kind: ProvenanceKind::SharedCopy,
        source_title: b.title.trim().to_string(),
        source_version: version_name,
        saved: saved.to_string(),
        app_version: app_version.to_string(),
        note: note.filter(|n| !n.trim().is_empty()),
        pack: None,
    });
    Ok(out)
}

/// Clears every note: categories', accounts', lines' and line blocks' rows'.
pub fn drop_notes(b: &mut Budget) {
    for c in b.categories_mut() {
        c.notes.clear();
        for a in &mut c.accounts {
            a.notes.clear();
            for d in &mut a.details {
                d.notes.clear();
            }
        }
    }
    for p in &mut b.line_presets {
        for d in &mut p.rows {
            d.notes.clear();
        }
    }
}

const KINDS: [SetupKind; 7] = [
    SetupKind::Global,
    SetupKind::Fringe,
    SetupKind::Group,
    SetupKind::Unit,
    SetupKind::Set,
    SetupKind::Location,
    SetupKind::Currency,
];

/// Names that tag presets and report presets refer to, by kind: kept even
/// when no line uses them, so a preset never names a missing item.
fn preset_names(b: &Budget, kind: SetupKind) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in &b.tag_presets {
        match kind {
            SetupKind::Fringe => out.extend(p.fringes.iter().cloned()),
            SetupKind::Group => out.extend(p.groups.iter().cloned()),
            SetupKind::Set => out.extend(p.set.iter().cloned()),
            SetupKind::Location => out.extend(p.location.iter().cloned()),
            SetupKind::Currency => out.extend(p.currency.iter().cloned()),
            SetupKind::Global | SetupKind::Unit => {}
        }
    }
    if kind == SetupKind::Group {
        for p in &b.report_presets {
            out.extend(p.spec.only_groups.iter().cloned());
        }
    }
    out
}

fn remove(b: &mut Budget, kind: SetupKind, name: &str) {
    match kind {
        SetupKind::Global => {
            b.globals.remove(name);
            b.global_info.remove(name);
        }
        SetupKind::Fringe => {
            b.fringes.remove(name);
        }
        SetupKind::Group => {
            b.groups.remove(name);
        }
        SetupKind::Unit => {
            b.units.remove(name);
        }
        SetupKind::Set => {
            b.sets.remove(name);
        }
        SetupKind::Location => {
            b.locations.remove(name);
        }
        SetupKind::Currency => {
            b.currencies.remove(name);
        }
    }
}

/// Removes every setup item (global, fringe, group, unit, set, location,
/// foreign currency) that no line, no other kept setup item, no charge or
/// credit, the totals currency, and no tag or report preset names, until
/// none is left (a global only an unused global used goes too). Returns how
/// many went. Totals never change: nothing that counts names them.
pub fn prune_unused_setup(b: &mut Budget) -> usize {
    let mut removed = 0;
    loop {
        let mut gone = Vec::new();
        for kind in KINDS {
            let keep = preset_names(b, kind);
            for (name, u) in crate::usage::usage(b, kind) {
                if !u.is_used() && !keep.contains(&name) {
                    gone.push((kind, name));
                }
            }
        }
        if gone.is_empty() {
            return removed;
        }
        removed += gone.len();
        for (kind, name) in gone {
            remove(b, kind, &name);
        }
    }
}

/// An editable copy of a shared copy: the same budget, its provenance kept
/// but no longer asking to be read-only (so it can still be compared with,
/// or stored in, the original).
pub fn editable_copy(b: &Budget) -> Budget {
    let mut out = b.clone();
    if let Some(p) = &mut out.provenance {
        if p.kind == ProvenanceKind::SharedCopy {
            p.kind = ProvenanceKind::EditableCopy;
        }
    }
    out
}

/// Whether `copy` (a shared or editable copy) names `original` as where it
/// came from: its source title is the original's title, trimmed and ignoring
/// case. Titles are the only link: a copy carries no path or id of the
/// budget it came from, so it says nothing about the sender's disk.
pub fn came_from(copy: &Budget, original: &Budget) -> bool {
    let Some(p) = &copy.provenance else {
        return false;
    };
    // A sub-budget or a setup pack is not a copy to compare or store back.
    if matches!(
        p.kind,
        ProvenanceKind::SubBudget | ProvenanceKind::SetupPack
    ) {
        return false;
    }
    if original
        .provenance
        .as_ref()
        .is_some_and(Provenance::is_shared_copy)
    {
        return false;
    }
    let t = original.title.trim();
    !t.is_empty() && p.source_title.trim().to_lowercase() == t.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, InfoValue, Note};
    use crate::{recalc, starter, RecalcOptions};

    const AT: &str = "2026-10-09T14:30:00-07:00";

    fn full() -> Budget {
        let mut b = starter::budget();
        b.title = "Big Show".into();
        b.info
            .insert("Director".into(), InfoValue::Text("Someone".into()));
        let c = b.categories_mut().next().unwrap();
        c.notes = vec![Note::plain(1, "category note")];
        c.accounts[0].details[0].notes = vec![Note::plain(1, "private")];
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        b.baseline = crate::Baseline::capture(&r);
        crate::save_version(&mut b, "Studio draft", AT, None);
        b.groups.insert(
            "Unused".into(),
            Group {
                description: String::new(),
                short_code: None,
                in_budget_total: true,
                color: None,
            },
        );
        b.globals.insert("LONELY".into(), "2".into());
        b.globals.insert("LONELIER".into(), "LONELY * 2".into());
        b
    }

    fn grand(b: &Budget) -> i64 {
        recalc(b, &RecalcOptions::default()).unwrap().grand_total
    }

    #[test]
    fn a_plain_copy_is_the_budget_stamped() {
        let b = full();
        let c = share_copy(
            &b,
            None,
            &ShareOptions::default(),
            AT,
            "0.0.1",
            Some("For the studio".into()),
        )
        .unwrap();
        let p = c.provenance.clone().unwrap();
        assert_eq!(p.kind, ProvenanceKind::SharedCopy);
        assert!(p.is_shared_copy());
        assert_eq!(p.source_title, "Big Show");
        assert_eq!(p.source_version, None);
        assert_eq!(p.saved, AT);
        assert_eq!(p.app_version, "0.0.1");
        assert_eq!(p.note.as_deref(), Some("For the studio"));
        let mut back = c.clone();
        back.provenance = None;
        assert_eq!(back, b);
        // A blank note is none.
        let c = share_copy(
            &b,
            None,
            &ShareOptions::default(),
            AT,
            "0.0.1",
            Some("  ".into()),
        )
        .unwrap();
        assert_eq!(c.provenance.unwrap().note, None);
    }

    #[test]
    fn leaving_things_out() {
        let b = full();
        let opts = ShareOptions {
            drop_notes: true,
            drop_info: true,
            drop_versions: true,
            drop_baseline: true,
            drop_unused_setup: true,
            drop_actuals: true,
        };
        let c = share_copy(&b, None, &opts, AT, "0.0.1", None).unwrap();
        assert!(c.info.is_empty() && c.versions.is_empty() && c.baseline.is_none());
        assert!(c.actuals.is_empty());
        assert!(c.categories().all(|c| c.notes.is_empty()
            && c.accounts
                .iter()
                .all(|a| a.notes.is_empty() && a.details.iter().all(|d| d.notes.is_empty()))));
        assert!(!c.groups.contains_key("Unused"));
        assert!(
            !c.globals.contains_key("LONELY") && !c.globals.contains_key("LONELIER"),
            "a chain of unused globals goes"
        );
        assert_eq!(grand(&c), grand(&b), "totals don't move");
        assert!(c.fringes.len() <= b.fringes.len());
    }

    #[test]
    fn presets_keep_what_they_name() {
        let mut b = full();
        b.tag_presets.push(crate::TagPreset {
            name: "Unused group".into(),
            groups: ["Unused".to_string()].into(),
            ..crate::TagPreset::default()
        });
        let mut b2 = b.clone();
        prune_unused_setup(&mut b2);
        assert!(b2.groups.contains_key("Unused"));
    }

    #[test]
    fn a_version_can_be_shared() {
        let mut b = full();
        let id = b.versions[0].id;
        // Change the budget after the version was saved.
        b.categories_mut().next().unwrap().name = "Renamed".into();
        let c = share_copy(&b, Some(id), &ShareOptions::default(), AT, "0.0.1", None).unwrap();
        assert_eq!(
            c.provenance.as_ref().unwrap().source_version.as_deref(),
            Some("Studio draft")
        );
        assert_ne!(c.categories().next().unwrap().name, "Renamed");
        assert_eq!(c.versions.len(), 1);
        assert_eq!(c.report_presets, b.report_presets);
        assert!(share_copy(&b, Some(999), &ShareOptions::default(), AT, "0.0.1", None).is_err());
    }

    #[test]
    fn editable_copies_and_where_they_came_from() {
        let b = full();
        let c = share_copy(&b, None, &ShareOptions::default(), AT, "0.0.1", None).unwrap();
        assert!(came_from(&c, &b));
        let mut other = b.clone();
        other.title = "  big show ".into();
        assert!(came_from(&c, &other), "titles match trimmed, ignoring case");
        other.title = "Another".into();
        assert!(!came_from(&c, &other));
        assert!(!came_from(&b, &c), "the budget didn't come from its copy");
        assert!(!came_from(&c, &c), "a shared copy is never the original");
        let e = editable_copy(&c);
        assert_eq!(
            e.provenance.as_ref().unwrap().kind,
            ProvenanceKind::EditableCopy
        );
        assert!(came_from(&e, &b));
        let mut sub = c.clone();
        sub.provenance.as_mut().unwrap().kind = ProvenanceKind::SubBudget;
        assert!(!came_from(&sub, &b), "a sub-budget isn't a returned copy");
        assert_eq!(
            editable_copy(&b),
            b,
            "a budget without provenance is copied as it is"
        );
    }
}
