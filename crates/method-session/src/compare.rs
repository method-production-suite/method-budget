//! The compare screen's commands (decisions.md, entries 17, 19 and 27): the
//! budget in front (the base) against 1 to 15 others, each one an open tab,
//! a saved version of the base, or a budget file picked in a native dialog.
//!
//! - Paths never come from the page. The host shows the Open dialog and
//!   reads the chosen files; they are kept in [`CompareFiles`] (the desktop
//!   reads them from disk, a browser host hands their bytes to
//!   [`pick_files`]) and the page names them by token afterwards.
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

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::{Budget, CompareLine, Comparison, Dec, RowId, RowMatch, VersionId, MAX_COMPARED};
use serde::{Deserialize, Serialize};

use crate::diagnose::{error_message, lenient_recalc};
use crate::docs::{AppState, DocId, Docs};
use crate::host::FileIn;

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
    /// Where it is, or its name where the host has no path.
    path: String,
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
    /// Where the file is (for its tooltip), or its name where the host has
    /// no path.
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
            path: path.display().to_string(),
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

    /// File `token`'s label, where it is and its budget.
    pub fn get(&self, token: u64) -> Option<(String, String, Arc<Budget>)> {
        self.lock()
            .files
            .iter()
            .find(|f| f.token == token)
            .map(|f| (f.label.clone(), f.path.clone(), Arc::clone(&f.budget)))
    }

    /// How many files are kept.
    pub fn kept(&self) -> usize {
        self.lock().files.len()
    }
}

/// Reads `picked` (`.mthd` files a host handed in, by name) for comparing.
/// One that can't be read is an error naming it (nothing is kept).
pub fn pick_files(files: &CompareFiles, picked: &[FileIn]) -> Result<Vec<Picked>, String> {
    let read = picked
        .iter()
        .map(|f| Ok((f.name.as_str(), read_file(f)?)))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(read
        .into_iter()
        .map(|(name, budget)| files.add(Path::new(name), budget))
        .collect())
}

/// A picked file's budget, refused as the desktop refuses a file it can't
/// read.
pub fn read_file(f: &FileIn) -> Result<Budget, String> {
    let text = std::str::from_utf8(&f.bytes).map_err(|_| {
        format!(
            "Couldn't read {}: stream did not contain valid UTF-8",
            f.name
        )
    })?;
    method_format::from_str(text).map_err(|e| format!("{} isn't a readable budget: {e}", f.name))
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
// Commands

/// Budget `doc` (the one in front when `None`) against `sources`. The
/// budgets are compared after the lock is let go.
pub fn compare_budgets(
    state: &AppState,
    files: &CompareFiles,
    doc: Option<DocId>,
    sources: &[Source],
) -> Result<CompareDto, String> {
    let gathered = {
        let base = state.lock_doc(doc)?.id();
        gather(&mut state.docs(), files, base, sources)?
    };
    gathered.compare()
}

/// One account's lines in budget `doc` against `sources`.
pub fn compare_detail(
    state: &AppState,
    files: &CompareFiles,
    doc: Option<DocId>,
    sources: &[Source],
    code: &str,
) -> Result<DetailDto, String> {
    let gathered = {
        let base = state.lock_doc(doc)?.id();
        gather(&mut state.docs(), files, base, sources)?
    };
    gathered.compare_account(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

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
        let p = Path::new("x.mthd");
        let first = files.add(p, Budget::new("x", "USD"));
        for _ in 0..KEEP_FILES {
            files.add(p, Budget::new("x", "USD"));
        }
        assert_eq!(files.kept(), KEEP_FILES);
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

    /// A picked file as the page's `invoke` sends it.
    fn file_arg(name: &str, bytes: &[u8]) -> serde_json::Value {
        serde_json::json!({"name": name, "bytes": bytes})
    }

    #[test]
    fn picked_files_compare_through_the_host() {
        use serde_json::json;
        let h = crate::host::Host::new();
        let call =
            |c: &str, a: serde_json::Value| h.dispatch(c, a).unwrap_or_else(|e| panic!("{c}: {e}"));
        let mut b = method_core::starter::budget();
        let (row, code) = first_line(&b);
        crate::edit::set_detail_field(&mut b, row, crate::edit::Field::Rate, "7").unwrap();
        let text = method_format::to_string(&b);
        let picked = call(
            "compare_pick_files",
            json!({"files": [file_arg("Pilot.mthd", text.as_bytes())]}),
        );
        assert_eq!(picked[0]["label"], "Pilot");
        assert_eq!(picked[0]["path"], "Pilot.mthd");
        assert_eq!(picked[0]["source"]["kind"], "file");
        let sources = json!([picked[0]["source"]]);
        let c = call("compare_budgets", json!({"sources": sources}));
        assert_eq!(c["sources"][1]["kind"], "file");
        assert_eq!(c["sources"][1]["detail"], "Pilot.mthd");
        let d = call("compare_detail", json!({"sources": sources, "code": code}));
        assert_eq!(d["code"], code.as_str());

        // A file that isn't a budget is refused by name; nothing is kept.
        let e = h
            .dispatch(
                "compare_pick_files",
                json!({"files": [file_arg("Notes.mthd", b"{ not a budget")]}),
            )
            .unwrap_err();
        assert!(e.message().contains("Notes.mthd"), "{e}");
        let e = h
            .dispatch(
                "compare_budgets",
                json!({"sources": [{"kind": "file", "token": 99}]}),
            )
            .unwrap_err();
        assert!(e.message().contains("add it again"), "{e}");
    }
}
