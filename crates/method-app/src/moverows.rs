//! Moving rows up or down one place: detail lines within their account,
//! accounts within their category, and categories and production totals on
//! the topsheet (roadmap 1.10). Option+Up and Option+Down in the grid, and
//! Move Up and Move Down in the Edit menu and the right-click menu.
//!
//! - **Any selection**, contiguous or not: each selected row steps past the
//!   nearest unselected row (or block, below) in that direction, and the
//!   selected rows keep their order among themselves. Rows already at the
//!   top (or bottom) stay, and the others close up behind them. Nothing
//!   moving is no change and no undo step.
//! - **Fringe ranges stay whole.** A range whose lines are all selected moves
//!   as it is, with the subtotal and separator rows between its lines. A
//!   selected line from a range whose other lines stay behind leaves the
//!   range, and the lines left behind count as one block: a moving row steps
//!   over the whole block in one press, never into the middle of it, so a
//!   range is never split by a row that isn't in it.
//! - **One undo step per run of presses**: presses of the same selection
//!   less than a second apart join one step (`Session::edit_merging`).
//! - Moving subtotal and adjustment rows changes which lines they add up, as
//!   moving them by hand would; nothing else about totals changes.

use std::collections::BTreeSet;

use std::collections::BTreeMap;

use method_core::{Budget, Detail, RowId, TopsheetRow};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::edit::{row_gone, EditResult};
use crate::view::View;

fn direction(by: i32) -> EditResult<isize> {
    match by {
        -1 => Ok(-1),
        1 => Ok(1),
        _ => Err("rows move one place at a time (by -1 or 1)".into()),
    }
}

/// The new order of `n` items, as their old indexes, after the `selected`
/// ones step once in direction `by` (-1 up, 1 down). `blocks` are spans
/// `(first, last)`, in indexes among the unselected items, that a selected
/// item steps over in one go and never lands inside. Selected items keep
/// their order among themselves.
fn reorder(selected: &[bool], blocks: &[(usize, usize)], by: isize) -> Vec<usize> {
    let rest: Vec<usize> = (0..selected.len()).filter(|&i| !selected[i]).collect();
    let n = rest.len();
    // unit_of[k]: the span holding unselected item k (a block, or itself).
    let mut unit_of: Vec<(usize, usize)> = (0..n).map(|k| (k, k)).collect();
    for &(s, e) in blocks {
        for slot in unit_of.iter_mut().take(e + 1).skip(s) {
            *slot = (s, e);
        }
    }
    // Each selected item sits in a gap among the unselected ones (gap g:
    // after the first g of them); find the gap it moves to.
    let mut placed: Vec<(usize, usize)> = Vec::new(); // (new gap, old index)
    let mut before = 0;
    for (i, &sel) in selected.iter().enumerate() {
        if !sel {
            before += 1;
            continue;
        }
        let g = before;
        // Down: past the unit after the gap (to its end, from inside a
        // block). Up: before the unit ahead of it (its start).
        let to = if by > 0 {
            if g < n {
                unit_of[g].1 + 1
            } else {
                g
            }
        } else if g > 0 {
            unit_of[g - 1].0
        } else {
            0
        };
        placed.push((to, i));
    }
    let mut out = Vec::with_capacity(selected.len());
    let mut p = 0;
    let mut take_gap = |gap: usize, out: &mut Vec<usize>| {
        while p < placed.len() && placed[p].0 == gap {
            out.push(placed[p].1);
            p += 1;
        }
    };
    for (gap, &item) in rest.iter().enumerate() {
        take_gap(gap, &mut out);
        out.push(item);
    }
    take_gap(n, &mut out);
    // Gaps only ever move one way, so `placed` is in gap order; anything
    // left (it can't be) goes last rather than being lost.
    out.extend(placed[p..].iter().map(|&(_, i)| i));
    out
}

/// What a move did: the moved items in their new order (rows by id; accounts
/// and topsheet rows by their new place), and whether anything moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved<K> {
    pub items: Vec<K>,
    pub changed: bool,
}

/// Moves the detail rows `ids` (all in one account) one place up (`by` -1)
/// or down (1), keeping fringe ranges whole (see the module notes). Returns
/// the moved rows' ids in their new order: the selection, plus any subtotal
/// or separator rows inside a fully selected range, which move with it.
pub fn shift_rows(b: &mut Budget, ids: &[RowId], by: i32) -> EditResult<Moved<RowId>> {
    let by = direction(by)?;
    let want: BTreeSet<RowId> = ids.iter().copied().collect();
    let Some(&first) = want.iter().next() else {
        return Err("select one or more rows first".into());
    };
    // Which account: the one holding the first row, and it must hold all.
    let mut home = None;
    for (k, a) in b.categories().flat_map(|c| &c.accounts).enumerate() {
        let here = a.details.iter().filter(|d| want.contains(&d.id)).count();
        if here == 0 {
            continue;
        }
        if here != want.len() {
            return Err("rows move within their own account: select rows in one account".into());
        }
        home = Some(k);
        break;
    }
    let k = home.ok_or_else(|| row_gone(first))?;
    let Some(acct) = b.categories_mut().flat_map(|c| &mut c.accounts).nth(k) else {
        return Err(row_gone(first));
    };
    let rows = &mut acct.details;
    let mut selected: Vec<bool> = rows.iter().map(|d| want.contains(&d.id)).collect();

    // Fringe ranges, by number: their costed rows' places.
    let mut ranges: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, d) in rows.iter().enumerate() {
        if let (true, Some(n)) = (d.kind.is_costed(), d.fringe_range) {
            ranges.entry(n).or_default().push(i);
        }
    }
    let mut leaving = Vec::new(); // selected lines that leave their range
    let mut behind = Vec::new(); // ranges with lines staying behind
    for (n, at) in &ranges {
        if at.iter().all(|&i| selected[i]) {
            // Whole: the subtotal and separator rows between its lines go too.
            let (lo, hi) = (at[0], at[at.len() - 1]);
            for (i, s) in selected.iter_mut().enumerate().take(hi + 1).skip(lo) {
                if !rows[i].kind.is_costed() {
                    *s = true;
                }
            }
        } else {
            leaving.extend(at.iter().copied().filter(|&i| selected[i]));
            behind.push(*n);
        }
    }
    // The lines staying behind in each range are one block, from the first
    // to the last, counted in places among the rows that don't move.
    let mut rank = vec![0; rows.len()];
    let mut k = 0;
    for (i, &s) in selected.iter().enumerate() {
        rank[i] = k;
        if !s {
            k += 1;
        }
    }
    let mut spans: Vec<(usize, usize)> = behind
        .iter()
        .filter_map(|n| {
            let mut stay = ranges[n]
                .iter()
                .filter(|&&i| !selected[i])
                .map(|&i| rank[i]);
            let lo = stay.next()?;
            let hi = stay.next_back().unwrap_or(lo);
            (hi > lo).then_some((lo, hi))
        })
        .collect();
    // Overlapping spans (ranges already interleaved) join into one block.
    spans.sort_unstable();
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match blocks.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => blocks.push((s, e)),
        }
    }

    let order = reorder(&selected, &blocks, by);
    let items: Vec<RowId> = order
        .iter()
        .filter(|&&i| selected[i])
        .map(|&i| rows[i].id)
        .collect();
    let changed = order.iter().enumerate().any(|(to, &from)| to != from);
    if changed {
        for i in leaving {
            rows[i].fringe_range = None;
        }
        let mut old: Vec<Option<Detail>> = std::mem::take(rows).into_iter().map(Some).collect();
        *rows = order.iter().filter_map(|&i| old[i].take()).collect();
    }
    Ok(Moved { items, changed })
}

/// Moves the accounts `codes` (all in one category) one place up or down
/// among the category's accounts. Returns their new places in the category.
pub fn shift_accounts(b: &mut Budget, codes: &[String], by: i32) -> EditResult<Moved<usize>> {
    let by = direction(by)?;
    let want: BTreeSet<&str> = codes.iter().map(String::as_str).collect();
    let Some(&first) = want.iter().next() else {
        return Err("select one or more accounts first".into());
    };
    let cat = b
        .categories_mut()
        .find(|c| c.accounts.iter().any(|a| a.code == first))
        .ok_or_else(|| format!("account {first} no longer exists"))?;
    let selected: Vec<bool> = cat
        .accounts
        .iter()
        .map(|a| want.contains(a.code.as_str()))
        .collect();
    if selected.iter().filter(|&&s| s).count() != want.len() {
        return Err(
            "accounts move within their own category: select accounts in one category".into(),
        );
    }
    let order = reorder(&selected, &[], by);
    let places = (0..order.len()).filter(|&to| selected[order[to]]).collect();
    let changed = order.iter().enumerate().any(|(to, &from)| to != from);
    if changed {
        let mut old: Vec<_> = std::mem::take(&mut cat.accounts)
            .into_iter()
            .map(Some)
            .collect();
        cat.accounts = order.iter().filter_map(|&i| old[i].take()).collect();
    }
    Ok(Moved {
        items: places,
        changed,
    })
}

/// Moves the topsheet rows at `indexes` (categories and production totals)
/// one place up or down. Returns their new places.
pub fn shift_topsheet_rows(b: &mut Budget, indexes: &[usize], by: i32) -> EditResult<Moved<usize>> {
    let by = direction(by)?;
    let want: BTreeSet<usize> = indexes.iter().copied().collect();
    if want.is_empty() {
        return Err("select one or more topsheet rows first".into());
    }
    if want.iter().any(|&i| i >= b.topsheet.len()) {
        return Err("that topsheet row no longer exists".into());
    }
    let selected: Vec<bool> = (0..b.topsheet.len()).map(|i| want.contains(&i)).collect();
    let order = reorder(&selected, &[], by);
    let places = (0..order.len()).filter(|&to| selected[order[to]]).collect();
    let changed = order.iter().enumerate().any(|(to, &from)| to != from);
    if changed {
        let mut old: Vec<Option<TopsheetRow>> = std::mem::take(&mut b.topsheet)
            .into_iter()
            .map(Some)
            .collect();
        b.topsheet = order.iter().filter_map(|&i| old[i].take()).collect();
    }
    Ok(Moved {
        items: places,
        changed,
    })
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// The view after a move, and the moved items in their new order: row ids
/// for lines, new places for accounts and topsheet rows. The grid selects
/// them again, so the next press moves the same rows.
#[derive(Clone, Debug, Serialize)]
pub struct MovedView {
    #[serde(flatten)]
    pub view: View,
    pub moved: Vec<u64>,
    pub changed: bool,
}

/// The key that joins quick repeats into one undo step: what moved, and
/// which of them (order aside).
fn key_of(what: &str, items: impl IntoIterator<Item = u64>) -> String {
    let set: BTreeSet<u64> = items.into_iter().collect();
    let parts: Vec<String> = set.iter().map(u64::to_string).collect();
    format!("{what}:{}", parts.join(","))
}

/// Runs a move as a merging edit (`Session::edit_merging`) and builds the reply.
fn run<R: Runtime, K: Copy>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    key: String,
    what: &'static str,
    label: String,
    f: impl FnOnce(&mut Budget) -> EditResult<Moved<K>>,
) -> Result<MovedView, String>
where
    u64: TryFrom<K>,
{
    let wide = |k: K| u64::try_from(k).unwrap_or(u64::MAX);
    let mut s = state.lock_doc(doc)?;
    let moved = s.edit_merging(&key, label, |b| {
        let m = f(b)?;
        let next = key_of(what, m.items.iter().map(|&k| wide(k)));
        Ok((m, next))
    })?;
    let view = finish(window, &mut s, None);
    Ok(MovedView {
        view,
        moved: moved.items.into_iter().map(wide).collect(),
        changed: moved.changed,
    })
}

/// Moves detail rows one place up (`by` -1) or down (1). See [`shift_rows`].
#[tauri::command]
pub fn move_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    by: i32,
) -> Result<MovedView, String> {
    let key = key_of("rows", row_ids.iter().copied());
    let label = move_label(row_ids.len(), "line", "lines", by);
    run(&window, &state, doc, key, "rows", label, |b| {
        shift_rows(b, &row_ids, by)
    })
}

/// Moves accounts (by code, all in one category) one place up or down
/// among the category's accounts. See [`shift_accounts`].
#[tauri::command]
pub fn move_accounts<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    codes: Vec<String>,
    by: i32,
) -> Result<MovedView, String> {
    // Places, not codes, in the key: the reply gives places.
    let key = {
        let s = state.lock_doc(doc)?;
        let places = s
            .budget()
            .categories()
            .find_map(|c| {
                let at: Vec<u64> = c
                    .accounts
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| codes.contains(&a.code))
                    .map(|(i, _)| i as u64)
                    .collect();
                (!at.is_empty()).then_some(at)
            })
            .unwrap_or_default();
        key_of("accounts", places)
    };
    let label = move_label(codes.len(), "account", "accounts", by);
    run(&window, &state, doc, key, "accounts", label, |b| {
        shift_accounts(b, &codes, by)
    })
}

/// Moves topsheet rows (categories and production totals, by place) one
/// place up or down. See [`shift_topsheet_rows`].
#[tauri::command]
pub fn move_topsheet_rows<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    indexes: Vec<usize>,
    by: i32,
) -> Result<MovedView, String> {
    let key = key_of("topsheet", indexes.iter().map(|&i| i as u64));
    let label = move_label(indexes.len(), "topsheet row", "topsheet rows", by);
    run(&window, &state, doc, key, "topsheet", label, |b| {
        shift_topsheet_rows(b, &indexes, by)
    })
}

/// "Move 3 lines down".
fn move_label(n: usize, one: &str, many: &str, by: i32) -> String {
    let way = if by < 0 { "up" } else { "down" };
    format!("Move {} {way}", crate::history::count(n, one, many))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use method_core::{Account, Category, ProductionTotal, RowKind};

    fn line(id: RowId, range: Option<u32>) -> Detail {
        let mut d = Detail::line(id, format!("L{id}"), "1", "", "100");
        d.fringe_range = range;
        d
    }

    fn budget(rows: Vec<Detail>) -> Budget {
        let mut b = Budget::new("t", "USD");
        let acct = |code: &str, details| Account {
            notes: Vec::new(),
            code: code.into(),
            name: code.into(),
            details,
        };
        b.topsheet = vec![
            TopsheetRow::Category(Category {
                notes: Vec::new(),
                code: "10".into(),
                name: "Above".into(),
                accounts: vec![
                    acct("10-01", rows),
                    acct("10-02", vec![line(90, None)]),
                    acct("10-03", Vec::new()),
                ],
            }),
            TopsheetRow::ProductionTotal(ProductionTotal { name: "ATL".into() }),
            TopsheetRow::Category(Category {
                notes: Vec::new(),
                code: "20".into(),
                name: "Below".into(),
                accounts: vec![acct("20-01", vec![line(91, None)])],
            }),
        ];
        b
    }

    fn ids(b: &Budget) -> Vec<RowId> {
        b.categories().next().unwrap().accounts[0]
            .details
            .iter()
            .map(|d| d.id)
            .collect()
    }

    fn range_of(b: &Budget, id: RowId) -> Option<u32> {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.id == id)
            .unwrap()
            .fringe_range
    }

    #[test]
    fn any_selection_steps_once_and_keeps_its_order() {
        let sel = |s: &[usize]| (0..5).map(|i| s.contains(&i)).collect::<Vec<_>>();
        assert_eq!(reorder(&sel(&[1, 3]), &[], -1), [1, 0, 3, 2, 4]);
        assert_eq!(reorder(&sel(&[0, 2]), &[], 1), [1, 0, 3, 2, 4]);
        // At the top already: stays; the rest close up behind it.
        assert_eq!(reorder(&sel(&[0, 1]), &[], -1), [0, 1, 2, 3, 4]);
        assert_eq!(reorder(&sel(&[0, 2]), &[], -1), [0, 2, 1, 3, 4]);
        assert_eq!(reorder(&sel(&[4]), &[], 1), [0, 1, 2, 3, 4]);
        // A block of unselected items is stepped over whole.
        assert_eq!(reorder(&sel(&[0]), &[(0, 2)], 1), [1, 2, 3, 0, 4]);
    }

    #[test]
    fn plain_lines_move_and_errors_change_nothing() {
        let mut b = budget(vec![line(1, None), line(2, None), line(3, None)]);
        let m = shift_rows(&mut b, &[3], -1).unwrap();
        assert_eq!(
            m,
            Moved {
                items: vec![3],
                changed: true
            }
        );
        assert_eq!(ids(&b), [1, 3, 2]);
        let m = shift_rows(&mut b, &[1, 3], -1).unwrap();
        assert!(!m.changed);
        let before = b.clone();
        assert!(shift_rows(&mut b, &[1, 90], 1)
            .unwrap_err()
            .contains("one account"));
        assert!(shift_rows(&mut b, &[1], 2).is_err());
        assert!(shift_rows(&mut b, &[], 1).is_err());
        assert!(shift_rows(&mut b, &[777], 1)
            .unwrap_err()
            .contains("no longer exists"));
        assert_eq!(b, before);
    }

    #[test]
    fn a_line_leaving_a_range_never_splits_it() {
        let rows = || {
            vec![
                line(1, None),
                line(2, Some(1)),
                line(3, Some(1)),
                line(4, Some(1)),
                line(5, None),
            ]
        };
        // The middle line, down: out past the rest of the range.
        let mut b = budget(rows());
        shift_rows(&mut b, &[3], 1).unwrap();
        assert_eq!(ids(&b), [1, 2, 4, 3, 5]);
        assert_eq!(range_of(&b, 3), None);
        assert_eq!((range_of(&b, 2), range_of(&b, 4)), (Some(1), Some(1)));
        // The middle line, up: out before it.
        let mut b = budget(rows());
        shift_rows(&mut b, &[3], -1).unwrap();
        assert_eq!(ids(&b), [1, 3, 2, 4, 5]);
        // The first line, down: past both others in one press.
        let mut b = budget(rows());
        shift_rows(&mut b, &[2], 1).unwrap();
        assert_eq!(ids(&b), [1, 3, 4, 2, 5]);
        // The first line, up: just out of the range.
        let mut b = budget(rows());
        shift_rows(&mut b, &[2], -1).unwrap();
        assert_eq!(ids(&b), [2, 1, 3, 4, 5]);
        assert_eq!(range_of(&b, 2), None);
        // A line from outside steps over the whole range, both ways.
        let mut b = budget(rows());
        shift_rows(&mut b, &[1], 1).unwrap();
        assert_eq!(ids(&b), [2, 3, 4, 1, 5]);
        shift_rows(&mut b, &[5], -1).unwrap();
        assert_eq!(ids(&b), [2, 3, 4, 5, 1]);
        shift_rows(&mut b, &[5], -1).unwrap();
        assert_eq!(ids(&b), [5, 2, 3, 4, 1]);
        // Nothing moved: no range is touched either.
        let mut b = budget(vec![line(2, Some(1)), line(3, Some(1))]);
        assert!(!shift_rows(&mut b, &[2], -1).unwrap().changed);
        assert_eq!(range_of(&b, 2), Some(1));
    }

    #[test]
    fn a_whole_range_moves_whole_with_what_is_inside_it() {
        let mut b = budget(vec![
            line(1, None),
            line(2, Some(4)),
            Detail::subtotal(3, "sub"),
            line(4, Some(4)),
            line(5, None),
        ]);
        let m = shift_rows(&mut b, &[2, 4], 1).unwrap();
        assert_eq!(m.items, [2, 3, 4]);
        assert_eq!(ids(&b), [1, 5, 2, 3, 4]);
        assert_eq!((range_of(&b, 2), range_of(&b, 4)), (Some(4), Some(4)));
        shift_rows(&mut b, &m.items, -1).unwrap();
        shift_rows(&mut b, &m.items, -1).unwrap();
        assert_eq!(ids(&b), [2, 3, 4, 1, 5]);
    }

    /// Every range number's costed rows are next to each other (among the
    /// costed rows): nothing else costed sits between its first and last.
    fn ranges_whole(rows: &[Detail]) -> bool {
        let costed: Vec<&Detail> = rows.iter().filter(|d| d.kind.is_costed()).collect();
        let mut seen = BTreeSet::new();
        let mut last = None;
        for d in costed {
            if d.fringe_range != last {
                if let Some(n) = d.fringe_range {
                    if !seen.insert(n) {
                        return false;
                    }
                }
                last = d.fringe_range;
            }
        }
        true
    }

    #[test]
    fn random_moves_keep_ranges_whole_and_rows_in_order() {
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        for case in 0..400 {
            let mut rows = Vec::new();
            let mut range = None;
            for id in 1..=(3 + next(10)) {
                if next(4) == 0 {
                    range = if next(2) == 0 { Some(id as u32) } else { None };
                }
                let d = match next(8) {
                    0 => Detail::subtotal(id, "s"),
                    1 => Detail::separator(id),
                    2 => {
                        let mut a = Detail::adjustment(id, "a", "0.1");
                        a.fringe_range = range;
                        a
                    }
                    _ => line(id, range),
                };
                rows.push(d);
            }
            let mut b = budget(rows);
            assert!(ranges_whole(
                &b.categories().next().unwrap().accounts[0].details
            ));
            for _ in 0..12 {
                let all = ids(&b);
                let pick: Vec<RowId> = all.iter().copied().filter(|_| next(3) == 0).collect();
                if pick.is_empty() {
                    continue;
                }
                let by = if next(2) == 0 { -1 } else { 1 };
                let m = shift_rows(&mut b, &pick, by).unwrap();
                let now = ids(&b);
                let mut sorted = now.clone();
                sorted.sort_unstable();
                let mut want = all.clone();
                want.sort_unstable();
                assert_eq!(sorted, want, "case {case}: rows lost or added");
                // The moved rows (the picked ones among them) keep their order.
                let picked_now: Vec<RowId> =
                    now.iter().copied().filter(|i| pick.contains(i)).collect();
                let picked_then: Vec<RowId> =
                    all.iter().copied().filter(|i| pick.contains(i)).collect();
                assert_eq!(picked_now, picked_then, "case {case}");
                assert!(m.items.iter().all(|i| now.contains(i)));
                assert!(
                    ranges_whole(&b.categories().next().unwrap().accounts[0].details),
                    "case {case}: {now:?} after moving {pick:?} by {by}"
                );
            }
        }
    }

    #[test]
    fn accounts_and_topsheet_rows_move_too() {
        let mut b = budget(vec![line(1, None)]);
        let m = shift_accounts(&mut b, &["10-03".into()], -1).unwrap();
        assert_eq!(
            m,
            Moved {
                items: vec![1],
                changed: true
            }
        );
        let codes: Vec<&str> = b
            .categories()
            .next()
            .unwrap()
            .accounts
            .iter()
            .map(|a| a.code.as_str())
            .collect();
        assert_eq!(codes, ["10-01", "10-03", "10-02"]);
        assert!(shift_accounts(&mut b, &["10-01".into(), "20-01".into()], 1)
            .unwrap_err()
            .contains("one category"));
        let m = shift_topsheet_rows(&mut b, &[2], -1).unwrap();
        assert_eq!(m.items, [1]);
        assert!(matches!(&b.topsheet[1], TopsheetRow::Category(c) if c.code == "20"));
        assert!(matches!(&b.topsheet[2], TopsheetRow::ProductionTotal(_)));
        assert!(shift_topsheet_rows(&mut b, &[3], 1).is_err());
        assert!(!shift_topsheet_rows(&mut b, &[0, 1, 2], 1).unwrap().changed);
    }

    #[test]
    fn quick_presses_are_one_undo_step_that_undoes_cleanly() {
        let mut s = Session::with_budget(
            budget(vec![
                line(1, None),
                line(2, Some(1)),
                line(3, Some(1)),
                line(4, None),
            ]),
            None,
        );
        let start = s.budget().clone();
        let press = |s: &mut Session, sel: &[RowId], by| {
            let key = key_of("rows", sel.iter().copied());
            s.edit_merging(&key, "Test edit", |b| {
                let m = shift_rows(b, sel, by)?;
                let next = key_of("rows", m.items.iter().copied());
                Ok((m, next))
            })
            .unwrap()
        };
        press(&mut s, &[1], 1);
        press(&mut s, &[1], 1);
        assert_eq!(ids(s.budget()), [2, 3, 4, 1]);
        assert!(s.undo());
        assert_eq!(*s.budget(), start);
        assert!(!s.can_undo());
        // Another selection is a step of its own; so is a press after an undo.
        assert!(s.redo());
        press(&mut s, &[4], -1);
        assert!(s.undo());
        assert_eq!(ids(s.budget()), [2, 3, 4, 1]);
        // There and back again within the window leaves no step at all.
        assert!(s.undo());
        press(&mut s, &[4], -1);
        press(&mut s, &[4], 1);
        assert_eq!(*s.budget(), start);
        assert!(!s.can_undo());
        assert!(!s.is_dirty());
        // A press that moves nothing makes no step and can't join another.
        press(&mut s, &[1], -1);
        assert!(!s.can_undo());
    }

    #[test]
    fn subtotals_follow_their_new_place() {
        let mut b = budget(vec![
            line(1, None),
            Detail::subtotal(2, "sub"),
            line(3, None),
        ]);
        shift_rows(&mut b, &[3], -1).unwrap();
        let r = method_core::recalc(&b, &method_core::RecalcOptions::default()).unwrap();
        let sub = r.categories[0].accounts[0]
            .rows
            .iter()
            .find(|x| x.kind == RowKind::Subtotal)
            .unwrap();
        assert_eq!(sub.subtotal, Some(20_000));
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn move_commands_are_wired_for_the_main_window_only() {
        for c in ["move_rows", "move_accounts", "move_topsheet_rows"] {
            assert!(APP_RS.contains(&format!("crate::moverows::{c},")), "{c}");
            assert!(BUILD_RS.contains(&format!("\"{c}\",")), "{c}");
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN_CAPS.contains(&allow), "{c}");
            assert!(!TOOLS_CAPS.contains(&allow), "{c}");
        }
    }
}
