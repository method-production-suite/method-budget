//! Several detail cells changed as one edit: pasting a block of cells, filling
//! down, cutting or clearing a range. Run through `Session::edit`, the whole
//! batch is one undo step and the budget is recalculated once, when the next
//! view is built.

use method_core::{Budget, RowId};
use serde::Deserialize;

use crate::edit::{self, EditResult, Field};

/// One cell: a detail row, a field named as in the file format, and the text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellEdit {
    pub row_id: RowId,
    pub field: String,
    pub value: String,
}

/// Most cells one batch may change. A paste of a whole budget's worth of
/// lines is far below this; it only stops a runaway request.
pub const MAX_CELLS: usize = 100_000;

/// Applies every edit in order, or none of them. Fails if any row is gone, a
/// field is unknown, or a row doesn't use a field (a subtotal's rate, say);
/// the UI only sends cells it shows as editable. Returns how many cells were
/// set.
pub fn set_cells(b: &mut Budget, edits: &[CellEdit]) -> EditResult<usize> {
    if edits.len() > MAX_CELLS {
        return Err(crate::messages::msg(
            "engine.edit.too_many_cells",
            &[("count", &edits.len()), ("max", &MAX_CELLS)],
        ));
    }
    // Check every field name before touching anything.
    let fields = edits
        .iter()
        .map(|e| Field::parse(&e.field))
        .collect::<EditResult<Vec<Field>>>()?;
    // Find every row in one pass, and check every cell before changing any,
    // so a bad cell late in the batch leaves no partial change, even outside
    // a Session. (Setting a field can only fail on these checks.)
    let mut rows = edit::rows_by_id(b, &edits.iter().map(|e| e.row_id).collect());
    for (e, &field) in edits.iter().zip(&fields) {
        let d = rows
            .get(&e.row_id)
            .ok_or_else(|| edit::row_gone(e.row_id))?;
        edit::check_field(d, field)?;
    }
    for (e, field) in edits.iter().zip(fields) {
        let d = rows
            .get_mut(&e.row_id)
            .ok_or_else(|| edit::row_gone(e.row_id))?;
        edit::apply_field(d, field, &e.value)?;
    }
    Ok(edits.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::{selection_totals, RowKind};

    fn cell(row_id: RowId, field: &str, value: &str) -> CellEdit {
        CellEdit {
            row_id,
            field: field.into(),
            value: value.into(),
        }
    }

    /// The first account with at least `n` plain lines, and those lines' ids.
    fn lines(s: &Session, n: usize) -> Vec<RowId> {
        s.budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| {
                a.details
                    .iter()
                    .filter(|d| d.kind == RowKind::Line)
                    .map(|d| d.id)
                    .collect::<Vec<_>>()
            })
            .find(|ids| ids.len() >= n)
            .expect("the starter budget has an account with enough lines")
    }

    fn detail(s: &Session, id: RowId) -> method_core::Detail {
        edit::details(s.budget())
            .find(|d| d.id == id)
            .cloned()
            .unwrap()
    }

    #[test]
    fn a_batch_is_one_undo_step() {
        let mut s = Session::new();
        let original = s.budget().clone();
        let ids = lines(&s, 2);
        let edits = vec![
            cell(ids[0], "description", "Camera operator"),
            cell(ids[0], "rate", "1250"),
            cell(ids[1], "amount", "3"),
            cell(ids[1], "x", "2"),
        ];
        assert_eq!(s.edit("Test edit", |b| set_cells(b, &edits)), Ok(4));
        assert!(s.can_undo());
        assert_eq!(detail(&s, ids[0]).description, "Camera operator");
        assert_eq!(detail(&s, ids[0]).rate.0, "1250");
        assert_eq!(detail(&s, ids[1]).amount.0, "3");

        // One undo takes back every cell, and there is nothing left to undo.
        assert!(s.undo());
        assert_eq!(s.budget(), &original);
        assert!(!s.can_undo());
        // And one redo puts them all back.
        assert!(s.redo());
        assert_eq!(detail(&s, ids[1]).x.0, "2");
    }

    #[test]
    fn a_batch_recalcs_once_with_every_cell_in_place() {
        // The batch itself never recalculates; the totals of the one view built
        // after it already include every cell. Compare against the same cells
        // set one at a time.
        let mut batched = Session::new();
        let mut single = Session::new();
        let ids = lines(&batched, 2);
        let edits = vec![
            cell(ids[0], "amount", "2"),
            cell(ids[0], "rate", "1000"),
            cell(ids[1], "amount", "5"),
            cell(ids[1], "rate", "40"),
        ];
        batched.edit("Test edit", |b| set_cells(b, &edits)).unwrap();
        for e in &edits {
            single
                .set_detail_field(e.row_id, &e.field, &e.value)
                .unwrap();
        }
        let v = serde_json::to_value(batched.view()).unwrap();
        let w = serde_json::to_value(single.view()).unwrap();
        assert_eq!(v["report"], w["report"]);

        // The two lines' own totals are the new numbers: 2 x 1000 and 5 x 40.
        let report = batched.outcome().report.clone().unwrap();
        let sel = selection_totals(
            batched.budget(),
            &report,
            &ids[..2].iter().copied().collect(),
        )
        .unwrap();
        // In cents: 2,000.00 + 200.00.
        assert_eq!(sel.lines.details_total, 200_000 + 20_000);
    }

    #[test]
    fn an_unknown_row_or_field_changes_nothing() {
        let mut s = Session::new();
        let original = s.budget().clone();
        let ids = lines(&s, 1);

        // The bad cell comes last, after good ones.
        let unknown_row = vec![cell(ids[0], "rate", "99"), cell(999_999, "rate", "1")];
        assert!(s.edit("Test edit", |b| set_cells(b, &unknown_row)).is_err());
        assert_eq!(s.budget(), &original);
        assert!(!s.can_undo());

        let unknown_field = vec![cell(ids[0], "rate", "99"), cell(ids[0], "salary", "1")];
        let err = s
            .edit("Test edit", |b| set_cells(b, &unknown_field))
            .unwrap_err();
        assert!(err.contains("salary"), "{err}");
        assert_eq!(s.budget(), &original);
        assert!(!s.is_dirty());

        // Also atomic without a Session around it.
        let mut b = original.clone();
        assert!(set_cells(&mut b, &unknown_row).is_err());
        assert_eq!(b, original);
    }

    #[test]
    fn a_row_that_does_not_use_the_field_is_refused() {
        let mut s = Session::new();
        let account = s
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .next()
            .unwrap()
            .code
            .clone();
        let sub = s.add_detail(&account, None, "subtotal").unwrap();
        let before = s.budget().clone();
        let ids = lines(&s, 1);
        let edits = vec![cell(ids[0], "rate", "5"), cell(sub, "rate", "5")];
        assert!(s.edit("Test edit", |b| set_cells(b, &edits)).is_err());
        assert_eq!(s.budget(), &before);
        // A subtotal's description is fine.
        let ok = vec![cell(sub, "description", "Prep")];
        assert_eq!(s.edit("Test edit", |b| set_cells(b, &ok)), Ok(1));
    }

    #[test]
    fn an_empty_batch_is_not_an_edit() {
        let mut s = Session::new();
        assert_eq!(s.edit("Test edit", |b| set_cells(b, &[])), Ok(0));
        assert!(!s.can_undo());
        assert!(!s.is_dirty());
    }

    #[test]
    fn a_huge_batch_is_refused() {
        let mut b = method_core::starter::budget();
        let ids = edit::details(&b).map(|d| d.id).next().unwrap();
        let edits = vec![cell(ids, "description", "x"); MAX_CELLS + 1];
        assert!(set_cells(&mut b, &edits).is_err());
    }

    #[test]
    fn cell_edits_read_camel_case_json() {
        let e: CellEdit =
            serde_json::from_str(r#"{"rowId": 7, "field": "amount", "value": "2"}"#).unwrap();
        assert_eq!(e, cell(7, "amount", "2"));
    }
}
