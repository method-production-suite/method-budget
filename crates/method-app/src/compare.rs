//! The compare screen's commands (decisions.md, entries 17, 19 and 27): the
//! budget in front (the base) against 1 to 15 others, each one an open tab,
//! a saved version of the base, or a budget file picked in a native dialog.
//!
//! - Paths never come from the page. `compare_pick_files` shows the Open
//!   dialog here, reads the chosen files and keeps them in [`CompareFiles`];
//!   the page names them by token afterwards. A file that is open in a tab
//!   comes back as that tab, so its unsaved changes count.
//! - Each budget is totalled the way its own grid totals it: cells with
//!   problems count as blank (src/diagnose.rs), and the source says so.
//! - The budgets are copied out under the lock and compared after it is
//!   released, so a big comparison never holds up other commands. Versions
//!   share their content (`Arc`), so they cost nothing to hold meanwhile.
//! - Every figure is integer cents; percents are text with two decimals,
//!   rounded once, half away from zero (`method_core::Dec`).
//!
//! Detail rows match by id when every budget is the base or one of its
//! versions (row ids are stable there), else by description.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::{Budget, CompareLine, Comparison, Dec, RowId, RowMatch, VersionId, MAX_COMPARED};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::diagnose::{error_message, lenient_recalc};
use crate::docs::Docs;
use crate::session::DocsFiles;

/// Most budgets compared with the base at once.
pub const MAX_OTHERS: usize = MAX_COMPARED - 1;
/// Files kept for comparing; the oldest picked goes first.
const KEEP_FILES: usize = 32;

/// One budget to compare with the base, as the page names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// An open budget (its tab).
    Tab { doc: DocId },
    /// A version saved in the base budget.
    Version { id: VersionId },
    /// A file picked with `compare_pick_files`.
    File { token: u64 },
}

/// A file read for comparing.
struct LoadedFile {
    token: u64,
    path: PathBuf,
    label: String,
    budget: Arc<Budget>,
}

/// Files picked for comparing, read once and kept by token (the app's
/// state; one list for every budget).
#[derive(Default)]
pub struct CompareFiles {
    inner: Mutex<FilesInner>,
}

#[derive(Default)]
struct FilesInner {
    next: u64,
    files: Vec<LoadedFile>,
}

/// A source the page can add after picking files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Picked {
    pub source: Source,
    pub label: String,
    /// Where the file is (for its tooltip).
    pub path: String,
}

/// The file's name without its extension.
fn file_label(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl CompareFiles {
    fn lock(&self) -> MutexGuard<'_, FilesInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `budget`, read from `path`, and returns how to name it.
    pub fn add(&self, path: &Path, budget: Budget) -> Picked {
        let mut f = self.lock();
        f.next += 1;
        let token = f.next;
        let label = file_label(path);
        f.files.push(LoadedFile {
            token,
            path: path.to_path_buf(),
            label: label.clone(),
            budget: Arc::new(budget),
        });
        let excess = f.files.len().saturating_sub(KEEP_FILES);
        f.files.drain(..excess);
        Picked {
            source: Source::File { token },
            label,
            path: path.display().to_string(),
        }
    }

    pub(crate) fn get(&self, token: u64) -> Option<(String, String, Arc<Budget>)> {
        self.lock()
            .files
            .iter()
            .find(|f| f.token == token)
            .map(|f| {
                (
                    f.label.clone(),
                    f.path.display().to_string(),
                    Arc::clone(&f.budget),
                )
            })
    }
}

/// Reads the files at `paths` for comparing. A file open in a tab comes back
/// as that tab; one that can't be read is an error naming it (nothing is kept).
pub fn pick(docs: &Docs, files: &CompareFiles, paths: &[PathBuf]) -> Result<Vec<Picked>, String> {
    let mut read = Vec::with_capacity(paths.len());
    for p in paths {
        if let Some(doc) = docs.find_file(p) {
            let label = docs.get(doc).map(|s| s.tab_label()).unwrap_or_default();
            read.push(Err(Picked {
                source: Source::Tab { doc },
                label,
                path: p.display().to_string(),
            }));
            continue;
        }
        let (budget, _) = crate::session::read_budget(p)?;
        read.push(Ok((p, budget)));
    }
    Ok(read
        .into_iter()
        .map(|r| match r {
            Ok((p, budget)) => files.add(p, budget),
            Err(tab) => tab,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// What the page draws

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceDto {
    /// "base", "tab", "version" or "file".
    pub kind: &'static str,
    pub label: String,
    /// The open budget (base and tabs), to go to it.
    pub doc: Option<DocId>,
    pub version: Option<VersionId>,
    pub token: Option<u64>,
    /// A tab's or file's path, or a version's `created` time.
    pub detail: String,
    /// Some cells have problems; they count as blank, as in its own grid.
    pub partial: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiffDto {
    pub amount: i64,
    /// Percent of the base, two decimals ("12.50"); `None` against nothing.
    pub percent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AggregateDto {
    pub sum: i64,
    pub average: i64,
    /// The sum minus the base.
    pub variance: i64,
    /// The variance as a percent of the average, two decimals.
    pub percent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LineDto {
    /// Per source, base first; `None` where that budget doesn't have it.
    pub values: Vec<Option<i64>>,
    /// Per source, base first (always zero).
    pub diffs: Vec<DiffDto>,
    /// The others taken together (decision 19).
    pub agg: Option<AggregateDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountDto {
    pub code: String,
    pub name: String,
    pub fringe_account: bool,
    pub line: LineDto,
    /// Per source: (report category index, account index).
    pub at: Vec<Option<(usize, usize)>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CategoryDto {
    pub code: String,
    pub name: String,
    pub line: LineDto,
    /// Per source: report category index.
    pub at: Vec<Option<usize>>,
    pub accounts: Vec<AccountDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LevelDto {
    pub name: String,
    pub line: LineDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TotalsDto {
    pub total_atl: LineDto,
    pub total_btl: LineDto,
    pub fringes: LineDto,
    pub charges: LineDto,
    pub grand_total: LineDto,
    pub credits: LineDto,
    pub net_total: LineDto,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompareDto {
    pub sources: Vec<SourceDto>,
    pub categories: Vec<CategoryDto>,
    pub levels: Vec<LevelDto>,
    /// Each charge, matched by name; a value is `None` where that budget
    /// has no charge by that name.
    pub charges: Vec<LevelDto>,
    /// Each credit, the same way.
    pub credits: Vec<LevelDto>,
    pub totals: TotalsDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RowDto {
    pub kind: &'static str,
    pub description: String,
    /// Per source: the row's id there.
    pub ids: Vec<Option<RowId>>,
    pub line: LineDto,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DetailDto {
    pub sources: Vec<SourceDto>,
    pub code: String,
    pub name: String,
    /// "id" or "description".
    pub matched_by: &'static str,
    pub at: Vec<Option<(usize, usize)>>,
    pub rows: Vec<RowDto>,
    pub details_total: LineDto,
    pub fringe_total: LineDto,
    pub total: LineDto,
}

/// A percent with two decimals, rounded once ("-12.35"); `Dec::to_cents`
/// rounds to hundredths.
pub fn percent_text(p: Dec) -> Option<String> {
    let h = p.to_cents()?;
    let sign = if h < 0 { "-" } else { "" };
    let a = h.unsigned_abs();
    Some(format!("{sign}{}.{:02}", a / 100, a % 100))
}

fn line_dto(l: &CompareLine) -> LineDto {
    LineDto {
        values: l.values.clone(),
        diffs: l
            .diffs
            .iter()
            .map(|d| DiffDto {
                amount: d.amount,
                percent: d.percent.and_then(percent_text),
            })
            .collect(),
        agg: l.aggregate().map(|a| AggregateDto {
            sum: a.sum,
            average: a.average,
            variance: a.variance,
            percent: a.percent.and_then(percent_text),
        }),
    }
}

fn compare_dto(sources: Vec<SourceDto>, c: &Comparison) -> CompareDto {
    let t = &c.totals;
    CompareDto {
        sources,
        categories: c
            .categories
            .iter()
            .map(|cat| CategoryDto {
                code: cat.code.clone(),
                name: cat.name.clone(),
                line: line_dto(&cat.line),
                at: cat.at.clone(),
                accounts: cat
                    .accounts
                    .iter()
                    .map(|a| AccountDto {
                        code: a.code.clone(),
                        name: a.name.clone(),
                        fringe_account: a.fringe_account,
                        line: line_dto(&a.line),
                        at: a.at.clone(),
                    })
                    .collect(),
            })
            .collect(),
        levels: c
            .levels
            .iter()
            .map(|l| LevelDto {
                name: l.name.clone(),
                line: line_dto(&l.line),
            })
            .collect(),
        charges: c
            .charges
            .iter()
            .map(|x| LevelDto {
                name: x.name.clone(),
                line: line_dto(&x.line),
            })
            .collect(),
        credits: c
            .credits
            .iter()
            .map(|x| LevelDto {
                name: x.name.clone(),
                line: line_dto(&x.line),
            })
            .collect(),
        totals: TotalsDto {
            total_atl: line_dto(&t.total_atl),
            total_btl: line_dto(&t.total_btl),
            fringes: line_dto(&t.fringes),
            charges: line_dto(&t.charges),
            grand_total: line_dto(&t.grand_total),
            credits: line_dto(&t.credits),
            net_total: line_dto(&t.net_total),
        },
    }
}

// ---------------------------------------------------------------------------
// Gathering the budgets

/// A budget copied out for comparing, before or after totalling.
enum Content {
    /// Ready: an open budget as its grid totals it.
    Totalled(Arc<Budget>),
    /// A file's budget, to total.
    Raw(Arc<Budget>),
    /// A version's content, to turn into a budget and total.
    Snapshot(Arc<method_core::BudgetSnapshot>),
}

/// The budgets to compare, base first, copied out of the open documents
/// ([`gather`], under the lock); [`Gathered::compare`] does the rest.
pub struct Gathered {
    pub sources: Vec<SourceDto>,
    contents: Vec<Content>,
    /// Every budget is the base or one of its versions.
    same_lineage: bool,
}

/// `b` as its grid totals it: cells with problems blanked. Err when it can't
/// be totalled at all. The flag says whether anything was blanked.
fn totalled(label: &str, b: &Budget) -> Result<(Budget, bool), String> {
    let o = lenient_recalc(b);
    if o.report.is_none() {
        let why = o.error.as_ref().map(error_message).unwrap_or_default();
        return Err(format!("{label} can't be totalled: {why}"));
    }
    let partial = o.partial();
    Ok((o.sanitized.unwrap_or_else(|| b.clone()), partial))
}

/// An open budget, totalled through its session (which keeps the result):
/// what its grid shows is what is compared.
fn open_doc(
    docs: &mut Docs,
    doc: DocId,
    kind: &'static str,
) -> Result<(SourceDto, Content), String> {
    let s = docs
        .get_mut(doc)
        .ok_or("A budget in the comparison has been closed.")?;
    let label = s.tab_label();
    let detail = s
        .path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let (b, o) = s.totals();
    if o.report.is_none() {
        let why = o.error.as_ref().map(error_message).unwrap_or_default();
        return Err(format!("{label} can't be totalled yet: {why}"));
    }
    let budget = o.sanitized.clone().unwrap_or_else(|| b.clone());
    let src = SourceDto {
        kind,
        label,
        doc: Some(doc),
        version: None,
        token: None,
        detail,
        partial: o.partial(),
    };
    Ok((src, Content::Totalled(Arc::new(budget))))
}

/// Checks the selection (1 to [`MAX_OTHERS`] sources, none twice, never the
/// base itself) and copies out budget `base` and each source. Call it under
/// the app's lock, and [`Gathered::compare`] after letting go.
pub fn gather(
    docs: &mut Docs,
    files: &CompareFiles,
    base: DocId,
    sources: &[Source],
) -> Result<Gathered, String> {
    if sources.is_empty() {
        return Err("Choose at least one budget to compare with.".into());
    }
    if sources.len() > MAX_OTHERS {
        return Err(format!(
            "Up to {MAX_OTHERS} budgets can be compared with this one at a time."
        ));
    }
    for (i, s) in sources.iter().enumerate() {
        if sources[..i].contains(s) {
            return Err("That budget is in the comparison already.".into());
        }
        if *s == (Source::Tab { doc: base }) {
            return Err("That is the budget being compared.".into());
        }
    }

    let mut out = Gathered {
        sources: Vec::with_capacity(sources.len() + 1),
        contents: Vec::with_capacity(sources.len() + 1),
        same_lineage: true,
    };
    let (src, c) = open_doc(docs, base, "base")?;
    out.sources.push(src);
    out.contents.push(c);
    for s in sources {
        let (src, c) = match *s {
            Source::Tab { doc } => {
                out.same_lineage = false;
                open_doc(docs, doc, "tab")?
            }
            Source::Version { id } => {
                let base_doc = docs.get(base).ok_or("The budget has been closed.")?;
                let v = base_doc
                    .budget()
                    .version(id)
                    .ok_or("That version has been deleted.")?;
                let src = SourceDto {
                    kind: "version",
                    label: v.name.clone(),
                    doc: None,
                    version: Some(id),
                    token: None,
                    detail: v.created.clone(),
                    partial: false,
                };
                (src, Content::Snapshot(Arc::clone(&v.budget)))
            }
            Source::File { token } => {
                out.same_lineage = false;
                let (label, path, b) = files
                    .get(token)
                    .ok_or("That file is no longer at hand; add it again.")?;
                let src = SourceDto {
                    kind: "file",
                    label,
                    doc: None,
                    version: None,
                    token: Some(token),
                    detail: path,
                    partial: false,
                };
                (src, Content::Raw(b))
            }
        };
        out.sources.push(src);
        out.contents.push(c);
    }
    Ok(out)
}

impl Gathered {
    /// Totals the versions and files (cells with problems count as blank,
    /// as in their own grids) and labels the budgets for the engine.
    fn budgets(&mut self) -> Result<Vec<Arc<Budget>>, String> {
        let mut out = Vec::with_capacity(self.contents.len());
        for (src, c) in self.sources.iter_mut().zip(&self.contents) {
            let b = match c {
                Content::Totalled(b) => Arc::clone(b),
                Content::Raw(b) => {
                    let (b, partial) = totalled(&src.label, b)?;
                    src.partial = partial;
                    Arc::new(b)
                }
                Content::Snapshot(s) => {
                    let (b, partial) = totalled(&src.label, &s.to_budget())?;
                    src.partial = partial;
                    Arc::new(b)
                }
            };
            out.push(b);
        }
        Ok(out)
    }

    /// Category, account, production-level and budget totals side by side.
    pub fn compare(mut self) -> Result<CompareDto, String> {
        let budgets = self.budgets()?;
        let labelled: Vec<(String, &Budget)> = self
            .sources
            .iter()
            .zip(&budgets)
            .map(|(s, b)| (s.label.clone(), b.as_ref()))
            .collect();
        let c = method_core::compare(&labelled).map_err(|e| e.to_string())?;
        Ok(compare_dto(self.sources, &c))
    }

    /// One account's rows side by side; `code` is matched through each
    /// budget's own coding.
    pub fn compare_account(mut self, code: &str) -> Result<DetailDto, String> {
        let budgets = self.budgets()?;
        let labelled: Vec<(String, &Budget)> = self
            .sources
            .iter()
            .zip(&budgets)
            .map(|(s, b)| (s.label.clone(), b.as_ref()))
            .collect();
        let by = if self.same_lineage {
            RowMatch::Id
        } else {
            RowMatch::Description
        };
        let a = method_core::compare_account(&labelled, code, by).map_err(|e| e.to_string())?;
        Ok(DetailDto {
            sources: self.sources,
            code: a.code,
            name: a.name,
            matched_by: if by == RowMatch::Id {
                "id"
            } else {
                "description"
            },
            at: a.at,
            rows: a
                .rows
                .iter()
                .map(|r| RowDto {
                    kind: crate::report_dto::kind_name(r.kind),
                    description: r.description.clone(),
                    ids: r.ids.clone(),
                    line: line_dto(&r.line),
                })
                .collect(),
            details_total: line_dto(&a.details_total),
            fringe_total: line_dto(&a.fringe_total),
            total: line_dto(&a.total),
        })
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// Budget `doc` (the one in front when `None`) against `sources`.
#[tauri::command]
pub fn compare_budgets(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    sources: Vec<Source>,
) -> Result<CompareDto, String> {
    let gathered = {
        let base = state.lock_doc(doc)?.id();
        gather(&mut state.docs(), &files, base, &sources)?
    };
    gathered.compare()
}

/// One account's lines in budget `doc` against `sources`.
#[tauri::command]
pub fn compare_detail(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    sources: Vec<Source>,
    code: String,
) -> Result<DetailDto, String> {
    let gathered = {
        let base = state.lock_doc(doc)?.id();
        gather(&mut state.docs(), &files, base, &sources)?
    };
    gathered.compare_account(&code)
}

/// Shows an Open dialog for budgets to compare with and reads the chosen
/// files. Empty when cancelled.
#[tauri::command]
pub async fn compare_pick_files<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
) -> Result<Vec<Picked>, String> {
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Compare with budgets")
        .add_filter(crate::app::FILTER_NAME, method_format::EXTENSIONS)
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_files() else {
        return Ok(Vec::new());
    };
    let paths = picked
        .into_iter()
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let docs = state.docs();
    pick(&docs, &files, &paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use crate::session::SessionFiles;

    const AT: &str = "2026-10-09T14:30:00-07:00";

    /// The first costed line with a rate, and its account's code.
    fn first_line(b: &Budget) -> (RowId, String) {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find_map(|a| {
                a.details
                    .iter()
                    .find(|d| !d.rate.is_blank())
                    .map(|d| (d.id, a.code.clone()))
            })
            .unwrap()
    }

    fn account<'a>(c: &'a CompareDto, code: &str) -> &'a AccountDto {
        c.categories
            .iter()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == code && !a.fringe_account)
            .unwrap()
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "method-compare-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Two open budgets: the base (1) and a copy with one rate changed.
    fn two_tabs() -> (Docs, DocId, DocId, String) {
        let mut base = Session::new();
        base.edit("Test edit", |b| crate::edit::set_title(b, "Feature"))
            .unwrap();
        let (row, code) = first_line(base.budget());
        let mut other = Session::with_budget(base.budget().clone(), None);
        other.set_detail_field(row, "rate", "99999").unwrap();
        let mut docs = Docs::with(base);
        let first = docs.active();
        let second = docs.add(other);
        (docs, first, second, code)
    }

    #[test]
    fn compares_with_an_open_budget() {
        let (mut docs, base, other, code) = two_tabs();
        let files = CompareFiles::default();
        let c = gather(&mut docs, &files, base, &[Source::Tab { doc: other }])
            .unwrap()
            .compare()
            .unwrap();
        let kinds: Vec<&str> = c.sources.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["base", "tab"]);
        assert_eq!(c.sources[0].doc, Some(base));
        assert_eq!(c.sources[1].doc, Some(other));
        let a = account(&c, &code);
        assert_ne!(a.line.values[0], a.line.values[1]);
        assert_eq!(
            a.line.diffs[1].amount,
            a.line.values[1].unwrap() - a.line.values[0].unwrap()
        );
        assert!(a.line.diffs[1].percent.is_some());
        // With one other budget, the variance is its difference.
        let agg = a.line.agg.as_ref().unwrap();
        assert_eq!(agg.variance, a.line.diffs[1].amount);
        assert_eq!(agg.sum, a.line.values[1].unwrap());
        assert_eq!(a.at[0], a.at[1]);
        assert!(a.at[0].is_some());

        // Lines of different budgets match by description.
        let d = gather(&mut docs, &files, base, &[Source::Tab { doc: other }])
            .unwrap()
            .compare_account(&code)
            .unwrap();
        assert_eq!(d.matched_by, "description");
        assert!(d.rows.iter().all(|r| r.ids.iter().all(Option::is_some)));
    }

    #[test]
    fn charges_list_one_by_one_beside_their_total() {
        let fee = |name: &str, amount: i64| method_core::Charge {
            name: name.into(),
            account: None,
            applied: true,
            kind: method_core::ChargeKind::Flat {
                amount,
                currency: None,
            },
            excluded_amounts: Vec::new(),
            excluded_groups: Default::default(),
            excluded_categories: Default::default(),
            excluded_accounts: Default::default(),
            label_percent: false,
            label_excluded: false,
        };
        let mut a = method_core::starter::budget();
        a.charges.clear();
        a.credits.clear();
        a.charges.push(fee("Insurance", 50_000));
        let mut b = a.clone();
        b.charges.push(fee("Bond", 20_000));
        let c = method_core::compare(&[("A".into(), &a), ("B".into(), &b)]).unwrap();
        let dto = compare_dto(Vec::new(), &c);
        let names: Vec<&str> = dto.charges.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Insurance", "Bond"]);
        assert_eq!(dto.charges[0].line.values, [Some(50_000), Some(50_000)]);
        assert_eq!(dto.charges[1].line.values, [None, Some(20_000)]);
        assert_eq!(dto.charges[1].line.diffs[1].amount, 20_000);
        assert_eq!(dto.totals.charges.values, [Some(50_000), Some(70_000)]);
        assert!(dto.credits.is_empty());
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["charges"][1]["name"], "Bond");
        assert!(json["charges"][1]["line"]["values"][0].is_null());
    }

    #[test]
    fn compares_with_versions_of_the_base() {
        let mut s = Session::new();
        let (row, code) = first_line(s.budget());
        let v = s
            .edit("Test edit", |b| {
                crate::versions::save(b, "Approved", Some("studio"), AT)
            })
            .unwrap();
        s.set_detail_field(row, "rate", "1").unwrap();
        let mut docs = Docs::with(s);
        let base = docs.active();
        let files = CompareFiles::default();
        let sel = [Source::Version { id: v }];
        let c = gather(&mut docs, &files, base, &sel)
            .unwrap()
            .compare()
            .unwrap();
        assert_eq!(c.sources[1].kind, "version");
        assert_eq!(c.sources[1].label, "Approved");
        assert_eq!(c.sources[1].detail, AT);
        assert_eq!((c.sources[1].doc, c.sources[1].version), (None, Some(v)));
        let a = account(&c, &code);
        assert!(
            a.line.diffs[1].amount > 0,
            "the version had the higher rate"
        );
        // Versions of one budget match lines by id.
        let d = gather(&mut docs, &files, base, &sel)
            .unwrap()
            .compare_account(&code)
            .unwrap();
        assert_eq!(d.matched_by, "id");
        let r = d.rows.iter().find(|r| r.ids[0] == Some(row)).unwrap();
        assert_eq!(r.ids, vec![Some(row), Some(row)]);
        assert_eq!(
            r.line.diffs[1].amount,
            r.line.values[1].unwrap() - r.line.values[0].unwrap()
        );
    }

    #[test]
    fn compares_with_files_and_open_files_come_back_as_tabs() {
        let dir = temp_dir();
        let mut b = method_core::starter::budget();
        b.title = "Pilot".into();
        let (row, code) = first_line(&b);
        crate::edit::set_detail_field(&mut b, row, crate::edit::Field::Rate, "7").unwrap();
        let path = dir.join("Pilot.mthd");
        std::fs::write(&path, method_format::to_string(&b)).unwrap();
        let opened = dir.join("Open.mthd");
        std::fs::write(
            &opened,
            method_format::to_string(&method_core::starter::budget()),
        )
        .unwrap();

        let mut docs = Docs::with(Session::new());
        let base = docs.active();
        let open_doc = docs.add(Session::open(&opened).unwrap());
        docs.switch(base).unwrap();
        let files = CompareFiles::default();
        let picked = pick(&docs, &files, &[path.clone(), opened.clone()]).unwrap();
        assert_eq!(picked[0].label, "Pilot");
        assert!(matches!(picked[0].source, Source::File { .. }));
        assert_eq!(picked[1].source, Source::Tab { doc: open_doc });

        let sel: Vec<Source> = picked.iter().map(|p| p.source).collect();
        let c = gather(&mut docs, &files, base, &sel)
            .unwrap()
            .compare()
            .unwrap();
        let kinds: Vec<&str> = c.sources.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["base", "file", "tab"]);
        assert_eq!(c.sources[1].detail, path.display().to_string());
        let a = account(&c, &code);
        assert_ne!(a.line.values[0], a.line.values[1]);
        // Sum, average and variance cover both others.
        let agg = a.line.agg.as_ref().unwrap();
        assert_eq!(
            agg.sum,
            a.line.values[1].unwrap() + a.line.values[2].unwrap()
        );

        // An unreadable file is an error naming it; nothing is kept.
        let bad = dir.join("Broken.mthd");
        std::fs::write(&bad, "{ not a budget").unwrap();
        let err = pick(&docs, &files, &[path.clone(), bad]).unwrap_err();
        assert!(err.contains("Broken.mthd"), "{err}");
        assert_eq!(files.lock().files.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_selection_is_checked() {
        let (mut docs, base, other, _) = two_tabs();
        let files = CompareFiles::default();
        let mut try_ = |sel: &[Source]| gather(&mut docs, &files, base, sel).map(|_| ());
        assert!(try_(&[]).unwrap_err().contains("at least one"));
        assert!(try_(&[Source::Tab { doc: base }])
            .unwrap_err()
            .contains("being compared"));
        let twice = [Source::Tab { doc: other }, Source::Tab { doc: other }];
        assert!(try_(&twice).unwrap_err().contains("already"));
        assert!(try_(&[Source::Version { id: 3 }])
            .unwrap_err()
            .contains("deleted"));
        assert!(try_(&[Source::File { token: 9 }])
            .unwrap_err()
            .contains("add it again"));
        assert!(try_(&[Source::Tab { doc: 77 }])
            .unwrap_err()
            .contains("closed"));

        // Up to 15 others.
        let mut s = Session::new();
        let ids: Vec<VersionId> = (0..=MAX_OTHERS)
            .map(|i| {
                s.edit("Test edit", |b| {
                    crate::versions::save(b, &format!("v{i}"), None, AT)
                })
                .unwrap()
            })
            .collect();
        let mut docs = Docs::with(s);
        let base = docs.active();
        let sel: Vec<Source> = ids.iter().map(|&id| Source::Version { id }).collect();
        let err = gather(&mut docs, &files, base, &sel).err().unwrap();
        assert!(err.contains("Up to 15"), "{err}");
        let c = gather(&mut docs, &files, base, &sel[..MAX_OTHERS])
            .unwrap()
            .compare()
            .unwrap();
        assert_eq!(c.sources.len(), MAX_COMPARED);
        assert_eq!(c.totals.grand_total.values.len(), MAX_COMPARED);
    }

    #[test]
    fn budgets_with_problems_compare_as_their_grids_show_them() {
        let (mut docs, base, other, code) = two_tabs();
        let (row, _) = first_line(docs.get(other).unwrap().budget());
        docs.get_mut(other)
            .unwrap()
            .set_detail_field(row, "rate", "NO_SUCH_GLOBAL * 2")
            .unwrap();
        let files = CompareFiles::default();
        let c = gather(&mut docs, &files, base, &[Source::Tab { doc: other }])
            .unwrap()
            .compare()
            .unwrap();
        assert!(c.sources[1].partial);
        assert!(!c.sources[0].partial);
        // The bad cell counts as blank, as in its grid.
        let grid_total = {
            let s = docs.get_mut(other).unwrap();
            let (_, o) = s.totals();
            o.report.as_ref().unwrap().grand_total
        };
        assert_eq!(c.totals.grand_total.values[1], Some(grid_total));
        assert!(account(&c, &code).line.values[1].is_some());
    }

    #[test]
    fn files_kept_are_bounded() {
        let files = CompareFiles::default();
        let p = PathBuf::from("/tmp/x.mthd");
        let first = files.add(&p, Budget::new("x", "USD"));
        for _ in 0..KEEP_FILES {
            files.add(&p, Budget::new("x", "USD"));
        }
        assert_eq!(files.lock().files.len(), KEEP_FILES);
        let Source::File { token } = first.source else {
            panic!("a file")
        };
        assert!(files.get(token).is_none(), "the oldest went first");
        assert_eq!(first.label, "x");
    }

    #[test]
    fn percents_are_two_decimals_rounded_once() {
        let d = |s: &str| s.parse::<Dec>().unwrap();
        assert_eq!(percent_text(d("12.5")).as_deref(), Some("12.50"));
        assert_eq!(percent_text(d("6.666666667")).as_deref(), Some("6.67"));
        assert_eq!(percent_text(d("-0.005")).as_deref(), Some("-0.01"));
        assert_eq!(percent_text(d("-0.004")).as_deref(), Some("0.00"));
        assert_eq!(percent_text(d("-100")).as_deref(), Some("-100.00"));
    }

    #[test]
    fn compare_commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in ["compare_budgets", "compare_detail", "compare_pick_files"] {
            assert!(
                APP_RS.contains(&format!("crate::compare::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
        assert!(APP_RS.contains(".manage(crate::compare::CompareFiles::default())"));
    }
}
