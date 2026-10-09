//! Timings for the edit -> recalc -> view path on a large sample budget.
//!
//! Build and run in release (no window opens; this only uses the command layer):
//!
//! ```bash
//! cargo build --release -p fwb-app --example perf
//! target/release/examples/perf            # 5,000 rows
//! target/release/examples/perf 10000      # another size
//! target/release/examples/perf loop       # recalc for 8 s, for a sampling profiler
//! target/release/examples/perf dump DIR   # canned replies for a UI harness
//! ```
//!
//! Each figure is the median of several runs. "Command" is what one command
//! costs in Rust: the edit with its undo step, the recalc, building the view
//! and turning it into JSON (what Tauri sends to the web view). The biggest
//! account is on screen, as when editing its lines.

use std::time::{Duration, Instant};

use fwb_app::batch::{self, CellEdit};
use fwb_app::diagnose::lenient_recalc;
use fwb_app::view::Focus;
use fwb_app::Session;
use fwb_core::sample::{self, SampleSpec};
use fwb_core::{recalc, Budget, RecalcOptions, RowId, RowKind, TopsheetRow};

const RUNS: usize = 15;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn ms(d: Duration) -> String {
    format!("{:8.2} ms", d.as_secs_f64() * 1000.0)
}

fn kb(n: usize) -> String {
    format!("{:8.1} KB", n as f64 / 1024.0)
}

fn time<T>(mut f: impl FnMut() -> T) -> Duration {
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let t = Instant::now();
        std::hint::black_box(f());
        times.push(t.elapsed());
    }
    median(times)
}

/// The account with the most lines: where it is, and its line ids.
fn biggest_account(b: &Budget) -> (Focus, Vec<RowId>) {
    let mut best: Option<(Focus, Vec<RowId>)> = None;
    for (ti, r) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = r else {
            continue;
        };
        for (ai, a) in c.accounts.iter().enumerate() {
            let ids: Vec<RowId> = a
                .details
                .iter()
                .filter(|d| d.kind == RowKind::Line)
                .map(|d| d.id)
                .collect();
            if best.as_ref().is_none_or(|(_, b)| ids.len() > b.len()) {
                let focus = Focus {
                    category: c.code.clone(),
                    ti,
                    account: a.code.clone(),
                    ai,
                };
                best = Some((focus, ids));
            }
        }
    }
    best.expect("accounts")
}

/// One command's parts, as medians.
#[derive(Default)]
struct Parts {
    edit: Vec<Duration>,
    view: Vec<Duration>,
    json: Vec<Duration>,
    size: usize,
}

impl Parts {
    fn print(self, title: &str) {
        let (e, v, j) = (median(self.edit), median(self.view), median(self.json));
        println!("{title}:");
        println!("  edit and undo step            {}", ms(e));
        println!("  recalc and view               {}", ms(v));
        println!("  JSON                          {}", ms(j));
        println!("  command in all                {}", ms(e + v + j));
        println!("  JSON size                     {}", kb(self.size));
    }
}

fn run(s: &mut Session, parts: &mut Parts, edit: impl FnOnce(&mut Session)) {
    let t = Instant::now();
    edit(s);
    parts.edit.push(t.elapsed());
    let t = Instant::now();
    let v = s.view();
    parts.view.push(t.elapsed());
    let t = Instant::now();
    let text = serde_json::to_string(&v).unwrap();
    parts.json.push(t.elapsed());
    parts.size = text.len();
}

/// `perf dump <dir>`: canned replies for the UI harness, a large budget's
/// views (lean and full) before and after editing a cell of its biggest
/// account, and the apply panel's and footer's numbers for one row.
fn dump(dir: &std::path::Path) {
    use fwb_app::apply::{rows_under, summary};
    use fwb_app::session::SelectionIndex;
    std::fs::create_dir_all(dir).unwrap();
    let write = |name: &str, v: &dyn erased::Json| {
        std::fs::write(dir.join(name), v.json()).unwrap();
    };
    let budget = sample::budget(&SampleSpec::default());
    let (focus, ids) = biggest_account(&budget);
    let mut s = Session::with_budget(budget, None);
    write("topsheet.json", &s.view());
    write("full.json", &s.full_view());
    s.set_focus(Some(focus.clone()));
    write("account.json", &s.view());
    for (n, rate) in [(1, "1234.5"), (2, "2345.75")] {
        s.set_detail_field(ids[0], "rate", rate).unwrap();
        write(&format!("edit{n}.json"), &s.view());
        write(&format!("full-edit{n}.json"), &s.full_view());
    }
    let (b, o) = s.totals();
    let rows = rows_under(b, &[ids[0]], &[], &[]);
    let index = SelectionIndex::new(b, o);
    write("summary.json", &summary(b, rows, |ids| index.totals(ids)));
    write("selection.json", &s.selection(&[ids[0]]));
    let meta = serde_json::json!({
        "category": focus.category, "ti": focus.ti, "account": focus.account,
        "ai": focus.ai, "row": ids[0], "lines": ids.len(),
    });
    write("meta.json", &meta);
    println!("wrote {}", dir.display());
}

/// Anything serializable, as JSON text (so `dump` can take any of them).
mod erased {
    pub trait Json {
        fn json(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("dump") {
        let dir = std::env::args().nth(2).expect("perf dump <dir>");
        dump(std::path::Path::new(&dir));
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("loop") {
        let budget = sample::budget(&SampleSpec::default());
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(8) {
            std::hint::black_box(recalc(&budget, &RecalcOptions::default()).unwrap());
        }
        return;
    }
    let rows: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5_000);
    let budget = sample::budget(&SampleSpec {
        rows,
        ..SampleSpec::default()
    });
    let (focus, ids) = biggest_account(&budget);
    println!(
        "sample: {rows} rows, biggest account {} with {} lines",
        focus.account,
        ids.len()
    );

    // The engine alone.
    println!(
        "recalc                          {}",
        ms(time(|| recalc(&budget, &RecalcOptions::default()).unwrap()))
    );
    println!(
        "lenient recalc (clean budget)   {}",
        ms(time(|| lenient_recalc(&budget)))
    );
    let mut broken = budget.clone();
    if let Some(d) = broken
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .find(|d| d.id == ids[0])
    {
        d.rate = "NOPE * 2".into();
    }
    println!(
        "lenient recalc (one bad cell)   {}",
        ms(time(|| lenient_recalc(&broken)))
    );
    println!(
        "copy the budget                 {}",
        ms(time(|| budget.clone()))
    );

    let mut s = Session::with_budget(budget.clone(), None);
    s.set_focus(Some(focus.clone()));

    // (a) One cell.
    let mut parts = Parts::default();
    for n in 0..RUNS {
        let id = ids[n % ids.len()];
        run(&mut s, &mut parts, |s| {
            s.set_detail_field(id, "rate", &format!("{}", 100 + n))
                .unwrap();
        });
    }
    parts.print("single-cell edit");

    // (b) 500 cells in one batch (a paste), across the budget.
    let all_lines: Vec<RowId> = fwb_app::edit::details(s.budget())
        .filter(|d| d.kind == RowKind::Line)
        .map(|d| d.id)
        .collect();
    let mut parts = Parts::default();
    for n in 0..RUNS {
        let edits: Vec<CellEdit> = all_lines
            .iter()
            .skip(n * 7)
            .step_by((all_lines.len() / 500).max(1))
            .take(500)
            .map(|&id| CellEdit {
                row_id: id,
                field: "amount".into(),
                value: format!("{}", 2 + n),
            })
            .collect();
        run(&mut s, &mut parts, |s| {
            s.edit(|b| batch::set_cells(b, &edits)).unwrap();
        });
    }
    parts.print("set_cells, 500 cells");

    // Undo and redo after those edits.
    let mut parts = Parts::default();
    for _ in 0..RUNS {
        run(&mut s, &mut parts, |s| {
            s.undo();
        });
    }
    parts.print("undo");

    // Showing another account (no recalc: the totals are cached).
    let mut parts = Parts::default();
    for _ in 0..RUNS {
        run(&mut s, &mut parts, |s| s.set_focus(Some(focus.clone())));
    }
    parts.print("show an account");

    // Printing asks for everything.
    let t = time(|| s.full_view());
    let size = serde_json::to_string(&s.full_view()).unwrap().len();
    let j = time(|| serde_json::to_string(&s.full_view()).unwrap());
    println!("full view (printing):");
    println!("  build                         {}", ms(t));
    println!("  build and JSON                {}", ms(j));
    println!("  JSON size                     {}", kb(size));
}
