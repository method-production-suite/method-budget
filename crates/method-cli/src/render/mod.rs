//! Reports drawn without the app: a budget, its recalculated totals and a
//! report setup ([`ReportSpec`], a saved preset's) become a [`Doc`] of
//! sections and tables, which [`html`], [`text`] (plain text and CSV) and
//! [`pdf`] write out.
//!
//! The figures come from the same places the spreadsheet export's do
//! (`method_format::export::workbook`), so a report and an export never
//! disagree. Like the app's reports (decision 28), filters never change a
//! total: leaving rows or accounts out prints the budget's own totals, and
//! the topsheet always lists every category.

pub mod html;
pub mod pdf;
pub mod text;

use std::collections::BTreeSet;

use method_core::{
    Account, Band, BandField, Baseline, Budget, Cents, Dec, FontSize, InfoValue, Note, Report,
    ReportColumn, ReportSection, ReportSpec, RowKind,
};
use method_format::export::{self, Cell, Sheet};

use crate::cli::money;

/// A report ready to write.
#[derive(Clone, Debug)]
pub struct Doc {
    pub title: String,
    pub landscape: bool,
    pub font_size: FontSize,
    pub shading: bool,
    pub vertical_rules: bool,
    pub header: Option<Bar>,
    pub footer: Option<Bar>,
    pub sections: Vec<Section>,
}

/// A header or footer with its fixed text worked out; the section and the
/// page number are filled in by writers that know them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub left: Slot,
    pub center: Slot,
    pub right: Slot,
    pub first_page_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Text(String),
    /// The section on the page.
    Section,
    /// "Page 3 of 12".
    Page,
}

impl Slot {
    /// The slot's text: `section` for a section slot, `page` for a page slot.
    pub fn resolve(&self, section: &str, page: &str) -> String {
        match self {
            Slot::Text(t) => t.clone(),
            Slot::Section => section.to_string(),
            Slot::Page => page.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub tables: Vec<Table>,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug)]
pub struct Column {
    pub title: String,
    /// Right-aligned figures.
    pub numeric: bool,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    /// One text spanning the row (a category or account heading).
    Heading,
    /// A total row.
    Total,
    /// A drawn rule (a separator row).
    Rule,
    /// One text spanning the row, set in smaller type (a note, a remark).
    Note,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub style: Style,
    pub bold: bool,
    pub italic: bool,
    pub indent: u8,
    /// `#rrggbb`, only when the report prints colors.
    pub color: Option<String>,
}

impl Row {
    fn new(cells: Vec<Cell>, style: Style) -> Row {
        Row {
            cells,
            style,
            bold: false,
            italic: false,
            indent: 0,
            color: None,
        }
    }

    fn spanning(text: impl Into<String>, style: Style) -> Row {
        Row::new(vec![Cell::text(text.into())], style)
    }

    /// The text of a heading, note or rule row.
    pub fn span_text(&self) -> String {
        self.cells.first().map(display).unwrap_or_default()
    }
}

/// A cell as a printed report shows it: money with thousands separators and
/// negatives in parentheses, decimals exactly.
pub fn display(c: &Cell) -> String {
    match c {
        Cell::Empty => String::new(),
        Cell::Text(s) => s.clone(),
        Cell::Money(m) => money(*m),
        Cell::Number(d) => d.to_string(),
        Cell::Int(n) => n.to_string(),
    }
}

/// What a report knows besides the budget: when it was drawn and from which
/// file.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub file_name: String,
    pub date: String,
    pub time: String,
}

impl Context {
    fn printed(&self) -> String {
        format!("{} {}", self.date, self.time).trim().to_string()
    }
}

/// The columns a setup without its own choice prints (the app's standard
/// set, ui/js/report.js).
pub const STANDARD_COLUMNS: [ReportColumn; 9] = [
    ReportColumn::Amount,
    ReportColumn::Unit,
    ReportColumn::X,
    ReportColumn::Rate,
    ReportColumn::Currency,
    ReportColumn::X4,
    ReportColumn::Fringes,
    ReportColumn::Original,
    ReportColumn::Variance,
];

fn column_title(c: ReportColumn) -> &'static str {
    match c {
        ReportColumn::Amount => "Amount",
        ReportColumn::Unit => "Unit",
        ReportColumn::X => "X",
        ReportColumn::Unit2 => "X unit",
        ReportColumn::Rate => "Rate",
        ReportColumn::Unit3 => "Rate unit",
        ReportColumn::Currency => "Cur",
        ReportColumn::X4 => "4X",
        ReportColumn::Unit4 => "4X unit",
        ReportColumn::Fringes => "Fringes",
        ReportColumn::FringePercent => "Fringe %",
        ReportColumn::FringeTotal => "Fringe $",
        ReportColumn::Groups => "Groups",
        ReportColumn::Set => "Set",
        ReportColumn::Location => "Location",
        ReportColumn::Original => "Original",
        ReportColumn::Variance => "Variance",
    }
}

/// The export Detail sheet's column holding a report column's value.
fn detail_sheet_column(c: ReportColumn) -> Option<&'static str> {
    Some(match c {
        ReportColumn::Amount => "Amount",
        ReportColumn::Unit => "Unit",
        ReportColumn::X => "X",
        ReportColumn::Unit2 => "X Unit",
        ReportColumn::Rate => "Rate",
        ReportColumn::Unit3 => "Rate Unit",
        ReportColumn::Currency => "Currency",
        ReportColumn::X4 => "4X",
        ReportColumn::Unit4 => "4X Unit",
        ReportColumn::Fringes => "Fringes",
        ReportColumn::FringeTotal => "Fringe Total",
        ReportColumn::Groups => "Groups",
        ReportColumn::Set => "Set",
        ReportColumn::Location => "Location",
        ReportColumn::FringePercent | ReportColumn::Original | ReportColumn::Variance => {
            return None
        }
    })
}

fn is_numeric(c: ReportColumn) -> bool {
    matches!(
        c,
        ReportColumn::Amount
            | ReportColumn::X
            | ReportColumn::Rate
            | ReportColumn::X4
            | ReportColumn::FringePercent
            | ReportColumn::FringeTotal
            | ReportColumn::Original
            | ReportColumn::Variance
    )
}

/// `part` as a percent of `whole`, two places (`18.50`), or blank.
fn percent(part: Cents, whole: Cents) -> Cell {
    if part == 0 || whole == 0 {
        return Cell::Empty;
    }
    let pct = Dec::from_cents(part)
        .checked_div(Dec::from_cents(whole))
        .and_then(|d| d.checked_mul(Dec::from_int(100)))
        .and_then(Dec::to_cents);
    match pct {
        Some(c) => Cell::text(crate::cli::money(c).replace(',', "")),
        None => Cell::Empty,
    }
}

fn col(title: &str, numeric: bool) -> Column {
    Column {
        title: title.to_string(),
        numeric,
    }
}

/// Builds the report. `r` must be `method_core::recalc(b, &RecalcOptions::default())`.
pub fn build(b: &Budget, r: &Report, spec: &ReportSpec, ctx: &Context) -> Result<Doc, String> {
    let wb = export::workbook(b, r).map_err(|e| e.to_string())?;
    let b_ctx = Builder {
        b,
        r,
        spec,
        wb: &wb,
        baseline: b.baseline.as_ref(),
        cols: spec
            .columns
            .clone()
            .unwrap_or_else(|| STANDARD_COLUMNS.to_vec())
            .into_iter()
            // Original and variance only print with a baseline.
            .filter(|c| {
                b.baseline.is_some()
                    || !matches!(c, ReportColumn::Original | ReportColumn::Variance)
            })
            .collect(),
    };
    let mut sections = Vec::new();
    for &s in &spec.sections {
        sections.push(b_ctx.section(s));
    }
    let standard_header = Band {
        left: Some(BandField::Title),
        center: Some(BandField::Section),
        right: Some(BandField::Printed),
        first_page_only: false,
    };
    let standard_footer = Band {
        right: Some(BandField::Page),
        ..Band::default()
    };
    let bar = |band: &Band| {
        let slot = |f: &Option<BandField>| match f {
            None => Slot::Text(String::new()),
            Some(BandField::Section) => Slot::Section,
            Some(BandField::Page) => Slot::Page,
            Some(field) => Slot::Text(band_text(b, r, ctx, field)),
        };
        let out = Bar {
            left: slot(&band.left),
            center: slot(&band.center),
            right: slot(&band.right),
            first_page_only: band.first_page_only,
        };
        let empty = Slot::Text(String::new());
        (out.left != empty || out.center != empty || out.right != empty).then_some(out)
    };
    Ok(Doc {
        title: b.title.clone(),
        landscape: spec.landscape,
        font_size: spec.font_size,
        shading: spec.shading,
        vertical_rules: spec.vertical_rules,
        header: bar(spec.header.as_ref().unwrap_or(&standard_header)),
        footer: bar(spec.footer.as_ref().unwrap_or(&standard_footer)),
        sections,
    })
}

fn band_text(b: &Budget, r: &Report, ctx: &Context, f: &BandField) -> String {
    match f {
        BandField::Title => b.title.clone(),
        BandField::Printed => ctx.printed(),
        BandField::Date => ctx.date.clone(),
        BandField::Time => ctx.time.clone(),
        BandField::File => ctx.file_name.clone(),
        BandField::Info(label) => b.info.get(label).map(info_text).unwrap_or_default(),
        BandField::Global(name) => r.globals.get(name).map(Dec::to_string).unwrap_or_default(),
        BandField::Text(t) => t.clone(),
        BandField::Section | BandField::Page => String::new(),
    }
}

fn info_text(v: &InfoValue) -> String {
    match v {
        InfoValue::Text(s) | InfoValue::Date(s) => s.clone(),
        InfoValue::Number(d) => d.to_string(),
    }
}

fn section_title(s: ReportSection) -> &'static str {
    match s {
        ReportSection::Topsheet => "Topsheet",
        ReportSection::Accounts => "Accounts",
        ReportSection::Detail => "Detail",
        ReportSection::Fringes => "Fringes",
        ReportSection::Globals => "Globals",
        ReportSection::Groups => "Groups",
        ReportSection::Notes => "Notes",
        ReportSection::Info => "Budget information",
        ReportSection::Chart => "Chart of accounts",
        ReportSection::Units => "Units",
        ReportSection::Sets => "Sets",
        ReportSection::Locations => "Locations",
        ReportSection::Currencies => "Currencies",
        // Sections this writer doesn't draw yet print their title only.
        #[allow(unreachable_patterns)]
        _ => "",
    }
}

struct Builder<'a> {
    b: &'a Budget,
    r: &'a Report,
    spec: &'a ReportSpec,
    wb: &'a export::Workbook,
    baseline: Option<&'a Baseline>,
    cols: Vec<ReportColumn>,
}

impl Builder<'_> {
    fn section(&self, s: ReportSection) -> Section {
        let tables = match s {
            ReportSection::Topsheet => vec![self.topsheet()],
            ReportSection::Accounts => vec![self.accounts()],
            ReportSection::Detail if self.spec.collapse_details => vec![self.accounts()],
            ReportSection::Detail => vec![self.detail()],
            ReportSection::Notes => vec![self.notes()],
            ReportSection::Info => vec![self.info()],
            ReportSection::Chart => vec![self.chart()],
            ReportSection::Fringes => vec![self.sheet("Fringes")],
            ReportSection::Globals => vec![self.sheet("Globals")],
            ReportSection::Groups => vec![self.sheet("Groups")],
            ReportSection::Units => vec![self.sheet("Units")],
            ReportSection::Sets => vec![self.sheet("Sets")],
            ReportSection::Locations => vec![self.sheet("Locations")],
            ReportSection::Currencies => vec![self.sheet("Currencies")],
            #[allow(unreachable_patterns)]
            _ => Vec::new(),
        };
        let title = match section_title(s) {
            "" => s.name().to_string(),
            t => t.to_string(),
        };
        Section { title, tables }
    }

    fn has(&self, c: ReportColumn) -> bool {
        self.cols.contains(&c)
    }

    /// The original and variance columns' titles, as far as they print.
    fn baseline_columns(&self) -> Vec<Column> {
        let mut out = Vec::new();
        for c in [ReportColumn::Original, ReportColumn::Variance] {
            if self.has(c) {
                out.push(col(column_title(c), true));
            }
        }
        out
    }

    /// Original and variance cells for a total with this original.
    fn baseline_cells(&self, total: Cents, original: Option<Cents>) -> Vec<Cell> {
        let mut out = Vec::new();
        if self.has(ReportColumn::Original) {
            out.push(original.map_or(Cell::Empty, Cell::Money));
        }
        if self.has(ReportColumn::Variance) {
            out.push(original.map_or(Cell::Empty, |o| Cell::Money(total.saturating_sub(o))));
        }
        out
    }

    fn topsheet(&self) -> Table {
        let (b, r) = (self.b, self.r);
        let fringes = self.spec.topsheet_fringes;
        let mut columns = vec![col("Code", false), col("Description", false)];
        if fringes {
            columns.push(col("Fringes", true));
        }
        columns.push(col("Total", true));
        columns.extend(self.baseline_columns());
        let mut rows = Vec::new();
        let mut push = |code: &str, label: &str, fr: Option<Cents>, total: Cents, orig, style| {
            let mut cells = vec![Cell::text(code), Cell::text(label)];
            if fringes {
                cells.push(fr.map_or(Cell::Empty, Cell::Money));
            }
            cells.push(Cell::Money(total));
            cells.extend(self.baseline_cells(total, orig));
            rows.push(Row::new(cells, style));
        };
        let base = self.baseline;
        for level in &r.levels {
            let cats = r.categories.get(level.start..level.end).unwrap_or_default();
            for cat in cats {
                let orig = base.map(|bl| bl.categories.get(&cat.code).copied().unwrap_or(0));
                push(
                    &cat.code,
                    &cat.name,
                    Some(cat.fringe_total),
                    cat.total,
                    orig,
                    Style::Normal,
                );
            }
            if level.posted_fringes != 0 {
                push(
                    "",
                    "Fringes",
                    None,
                    level.posted_fringes,
                    None,
                    Style::Normal,
                );
            }
            if !level.name.is_empty() {
                let fr = cats.iter().map(|c| c.fringe_total).sum();
                push("", &level.name, Some(fr), level.total, None, Style::Total);
            }
        }
        if b.display.summary_rows {
            push(
                "",
                "Total above-the-line",
                None,
                r.total_atl,
                None,
                Style::Total,
            );
            push(
                "",
                "Total below-the-line",
                None,
                r.total_btl,
                None,
                Style::Total,
            );
            push(
                "",
                "Total above and below-the-line",
                None,
                r.atl_btl,
                None,
                Style::Total,
            );
        }
        if r.budget_fringes != 0 {
            push(
                "",
                "Total fringes",
                None,
                r.budget_fringes,
                None,
                Style::Normal,
            );
        }
        let charges = b.charges.iter().filter(|c| c.applied);
        for (c, ct) in charges.zip(&r.charges) {
            let code = c.account.as_deref().unwrap_or("");
            push(code, &ct.name, None, ct.amount, None, Style::Normal);
        }
        let orig = base.map(|bl| bl.grand_total);
        push("", "Grand total", None, r.grand_total, orig, Style::Total);
        let credits = b.credits.iter().filter(|c| c.applied);
        for (c, ct) in credits.zip(&r.credits) {
            let code = c.account.as_deref().unwrap_or("");
            let label = format!("Less {}", ct.name);
            push(code, &label, None, -ct.amount, None, Style::Normal);
        }
        if b.display.net_total {
            let orig = base.map(|bl| bl.net_total);
            push("", "Net total", None, r.net_total, orig, Style::Total);
        }
        if r.totals_currency != r.base_currency {
            if let Some(n) = r.in_totals_currency(r.net_total) {
                let label = format!("Net total in {}", r.totals_currency);
                push("", &label, None, n, None, Style::Total);
            }
        }
        Table { columns, rows }
    }

    fn category_included(&self, code: &str) -> bool {
        !self.spec.excluded_categories.contains(code)
    }

    fn account_included(&self, a: &Account, total: Cents) -> bool {
        let s = self.spec;
        !(s.excluded_accounts.contains(&a.code)
            || (s.skip_empty_accounts && a.details.is_empty())
            || (s.skip_zero_accounts && total == 0))
    }

    /// Each category's accounts with their totals (also the detail section
    /// when accounts are collapsed to one line).
    fn accounts(&self) -> Table {
        let mut columns = vec![col("Account", false), col("Description", false)];
        columns.push(col("Total", true));
        columns.extend(self.baseline_columns());
        let base = self.baseline;
        let mut rows = Vec::new();
        for (cat, rc) in self.b.categories().zip(&self.r.categories) {
            if !self.category_included(&cat.code) {
                continue;
            }
            rows.push(Row::spanning(
                format!("{}  {}", cat.code, cat.name),
                Style::Heading,
            ));
            let mut left_out = false;
            for (a, ra) in cat.accounts.iter().zip(&rc.accounts) {
                if !self.account_included(a, ra.total) {
                    left_out = true;
                    continue;
                }
                let orig = base.map(|bl| bl.accounts.get(&a.code).copied().unwrap_or(0));
                let mut cells = vec![Cell::text(&a.code), Cell::text(&a.name)];
                cells.push(Cell::Money(ra.total));
                cells.extend(self.baseline_cells(ra.total, orig));
                rows.push(Row::new(cells, Style::Normal));
            }
            if let Some((code, amount)) = &rc.fringe_account {
                let mut cells = vec![Cell::text(code), Cell::text("Fringes")];
                cells.push(Cell::Money(*amount));
                cells.extend(self.baseline_cells(*amount, None));
                rows.push(Row::new(cells, Style::Normal));
            }
            if left_out {
                rows.push(Row::spanning(
                    "Some accounts are left out; the total is the category's own.",
                    Style::Note,
                ));
            }
            let orig = base.map(|bl| bl.categories.get(&cat.code).copied().unwrap_or(0));
            let mut cells = vec![
                Cell::text(""),
                Cell::text(format!("Total {} {}", cat.code, cat.name)),
            ];
            cells.push(Cell::Money(rc.total));
            cells.extend(self.baseline_cells(rc.total, orig));
            rows.push(Row::new(cells, Style::Total));
        }
        Table { columns, rows }
    }

    fn note_rows(&self, notes: &[Note], rows: &mut Vec<Row>) {
        for n in notes {
            if (self.spec.skip_done_notes && n.done) || n.text.trim().is_empty() {
                continue;
            }
            let mut by = Vec::new();
            if let Some(a) = n.author.as_deref().filter(|a| !a.is_empty()) {
                by.push(a.to_string());
            }
            if !n.created.is_empty() {
                by.push(n.created.clone());
            }
            let by = if by.is_empty() {
                String::new()
            } else {
                format!(" ({})", by.join(", "))
            };
            rows.push(Row::spanning(
                format!("Note: {}{by}", n.text.trim()),
                Style::Note,
            ));
        }
    }

    fn detail(&self) -> Table {
        let (b, r, spec) = (self.b, self.r, self.spec);
        let sheet = self.wb.sheet(export::DETAIL);
        let index_of = |title: &str| sheet.and_then(|s| s.columns.iter().position(|c| c == title));
        let mut columns = vec![col("#", true), col("Description", false)];
        for &c in &self.cols {
            columns.push(col(column_title(c), is_numeric(c)));
        }
        columns.push(col("Total", true));
        let base = self.baseline;
        let mut rows = Vec::new();
        // The Detail sheet has one row per detail row, in budget order.
        let mut sheet_row = 0usize;
        for (cat, rc) in b.categories().zip(&r.categories) {
            let included = self.category_included(&cat.code);
            for (a, ra) in cat.accounts.iter().zip(&rc.accounts) {
                let first = sheet_row;
                sheet_row += a.details.len();
                if !included || !self.account_included(a, ra.total) {
                    continue;
                }
                rows.push(Row::spanning(
                    format!("{}  {}", a.code, a.name),
                    Style::Heading,
                ));
                let mut left_out = false;
                for (i, (d, rr)) in a.details.iter().zip(&ra.rows).enumerate() {
                    let costed = d.kind.is_costed();
                    if costed
                        && ((spec.skip_suppressed_lines && !rr.included)
                            || (!spec.only_groups.is_empty()
                                && d.groups.is_disjoint(&spec.only_groups)))
                    {
                        left_out = true;
                        continue;
                    }
                    if d.kind == RowKind::Separator {
                        rows.push(Row::spanning("", Style::Rule));
                        continue;
                    }
                    let src = sheet.and_then(|s| s.rows.get(first + i));
                    let mut cells = vec![Cell::Int(i as u64 + 1), Cell::text(&d.description)];
                    for &c in &self.cols {
                        let cell = match c {
                            ReportColumn::FringePercent if costed => {
                                percent(rr.fringe_total, rr.total)
                            }
                            ReportColumn::Original | ReportColumn::Variance if costed => {
                                let orig = base.map(|bl| bl.rows.get(&d.id).copied().unwrap_or(0));
                                let pair = self.baseline_cells(rr.total, orig);
                                // Exactly the one asked for.
                                let pick = usize::from(
                                    c == ReportColumn::Variance && self.has(ReportColumn::Original),
                                );
                                pair.get(pick).cloned().unwrap_or(Cell::Empty)
                            }
                            _ => detail_sheet_column(c)
                                .and_then(index_of)
                                .and_then(|k| src.and_then(|row| row.get(k)))
                                .cloned()
                                .unwrap_or(Cell::Empty),
                        };
                        cells.push(cell);
                    }
                    cells.push(match (d.kind, rr.subtotal) {
                        (RowKind::Subtotal, Some(s)) => Cell::Money(s),
                        _ if costed => Cell::Money(rr.total),
                        _ => Cell::Empty,
                    });
                    let style = if d.kind == RowKind::Subtotal {
                        Style::Total
                    } else {
                        Style::Normal
                    };
                    let mut row = Row::new(cells, style);
                    row.bold = d.bold;
                    row.italic = d.italic;
                    row.indent = d.indent;
                    if spec.group_colors {
                        row.color = d.color.clone().or_else(|| {
                            d.groups
                                .iter()
                                .find_map(|g| b.groups.get(g).and_then(|g| g.color.clone()))
                        });
                    }
                    rows.push(row);
                    if !spec.skip_notes {
                        self.note_rows(&d.notes, &mut rows);
                    }
                }
                if left_out {
                    rows.push(Row::spanning(
                        "Some rows are left out; the total is the account's own.",
                        Style::Note,
                    ));
                }
                let mut cells = vec![Cell::Empty, Cell::text(format!("Total {}", a.code))];
                cells.extend(self.cols.iter().map(|&c| match c {
                    ReportColumn::FringeTotal => Cell::Money(ra.fringe_total),
                    ReportColumn::Original | ReportColumn::Variance => {
                        let orig = base.map(|bl| bl.accounts.get(&a.code).copied().unwrap_or(0));
                        let pair = self.baseline_cells(ra.total, orig);
                        let pick = usize::from(
                            c == ReportColumn::Variance && self.has(ReportColumn::Original),
                        );
                        pair.get(pick).cloned().unwrap_or(Cell::Empty)
                    }
                    _ => Cell::Empty,
                }));
                cells.push(Cell::Money(ra.total));
                rows.push(Row::new(cells, Style::Total));
            }
            if included {
                if let Some((code, amount)) = &rc.fringe_account {
                    let mut cells = vec![Cell::Empty, Cell::text(format!("{code}  Fringes"))];
                    cells.extend(self.cols.iter().map(|_| Cell::Empty));
                    cells.push(Cell::Money(*amount));
                    rows.push(Row::new(cells, Style::Total));
                }
            }
        }
        Table { columns, rows }
    }

    fn notes(&self) -> Table {
        let columns = vec![
            col("Where", false),
            col("Written", false),
            col("Author", false),
            col("Done", false),
            col("Note", false),
        ];
        let mut rows = Vec::new();
        let skip_done = self.spec.skip_done_notes;
        let mut push = |place: String, notes: &[Note]| {
            for n in notes {
                if skip_done && n.done {
                    continue;
                }
                rows.push(Row::new(
                    vec![
                        Cell::text(place.clone()),
                        Cell::text(&n.created),
                        Cell::text(n.author.clone().unwrap_or_default()),
                        Cell::text(if n.done { "Yes" } else { "" }),
                        Cell::text(n.text.trim()),
                    ],
                    Style::Normal,
                ));
            }
        };
        for cat in self.b.categories() {
            if !self.category_included(&cat.code) {
                continue;
            }
            push(format!("Category {}", cat.code), &cat.notes);
            for a in &cat.accounts {
                if self.spec.excluded_accounts.contains(&a.code) {
                    continue;
                }
                push(format!("Account {}", a.code), &a.notes);
                for (i, d) in a.details.iter().enumerate() {
                    let what = if d.description.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", d.description)
                    };
                    push(format!("{} line {}{what}", a.code, i + 1), &d.notes);
                }
            }
        }
        Table { columns, rows }
    }

    fn info(&self) -> Table {
        let b = self.b;
        let mut rows = vec![
            Row::new(
                vec![Cell::text("Title"), Cell::text(&b.title)],
                Style::Normal,
            ),
            Row::new(
                vec![Cell::text("Base currency"), Cell::text(&b.base_currency)],
                Style::Normal,
            ),
        ];
        if let Some(t) = &b.totals_currency {
            rows.push(Row::new(
                vec![Cell::text("Totals currency"), Cell::text(t)],
                Style::Normal,
            ));
        }
        for (label, v) in &b.info {
            rows.push(Row::new(
                vec![Cell::text(label), Cell::text(info_text(v))],
                Style::Normal,
            ));
        }
        Table {
            columns: vec![col("Field", false), col("Value", false)],
            rows,
        }
    }

    fn chart(&self) -> Table {
        let mut rows = Vec::new();
        for cat in self.b.categories() {
            let mut row = Row::new(
                vec![Cell::text(&cat.code), Cell::text(&cat.name)],
                Style::Normal,
            );
            row.bold = true;
            rows.push(row);
            for a in &cat.accounts {
                let mut row = Row::new(
                    vec![Cell::text(&a.code), Cell::text(&a.name)],
                    Style::Normal,
                );
                row.indent = 1;
                rows.push(row);
            }
        }
        Table {
            columns: vec![col("Code", false), col("Name", false)],
            rows,
        }
    }

    /// A setup table straight from the export.
    fn sheet(&self, name: &str) -> Table {
        let empty = Sheet {
            name: "",
            columns: Vec::new(),
            rows: Vec::new(),
        };
        let s = self.wb.sheet(name).unwrap_or(&empty);
        let numeric: BTreeSet<usize> = s
            .rows
            .iter()
            .flat_map(|row| row.iter().enumerate())
            .filter(|(_, c)| matches!(c, Cell::Money(_) | Cell::Number(_) | Cell::Int(_)))
            .map(|(i, _)| i)
            .collect();
        Table {
            columns: s
                .columns
                .iter()
                .enumerate()
                .map(|(i, t)| col(t, numeric.contains(&i)))
                .collect(),
            rows: s
                .rows
                .iter()
                .map(|cells| Row::new(cells.clone(), Style::Normal))
                .collect(),
        }
    }
}

/// The date and time now, in UTC: (`2026-10-09`, `14:30 UTC`). Reports are
/// the only place the command line reads the clock, and `--date` overrides
/// it for output that must not change from run to run.
pub fn now_utc() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{:02}:{:02} UTC", rem / 3600, (rem % 3600) / 60),
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// public-domain `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{recalc, RecalcOptions};

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_735), (2026, 10, 9));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn percents() {
        assert_eq!(percent(1850, 10_000), Cell::text("18.50"));
        assert_eq!(percent(0, 10_000), Cell::Empty);
        assert_eq!(percent(5, 0), Cell::Empty);
    }

    #[test]
    fn every_section_builds() {
        let b = method_core::sample::budget(&method_core::sample::SampleSpec::default());
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let spec = ReportSpec {
            sections: ReportSection::ALL.to_vec(),
            columns: Some(ReportColumn::ALL.to_vec()),
            ..ReportSpec::default()
        };
        let doc = build(&b, &r, &spec, &Context::default()).unwrap();
        assert_eq!(doc.sections.len(), ReportSection::ALL.len());
        for s in &doc.sections {
            for t in &s.tables {
                for row in &t.rows {
                    if matches!(row.style, Style::Normal | Style::Total) {
                        assert_eq!(row.cells.len(), t.columns.len(), "{}", s.title);
                    }
                }
            }
        }
    }
}
