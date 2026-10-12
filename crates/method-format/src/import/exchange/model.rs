//! What a budget export holds, as every reader hands it over: the topsheet
//! rows in order, each category's accounts, each account's detail rows, and
//! the export's own figures for every one of them. The builder
//! ([`super::build`]) turns it into a budget; the reconciliation
//! ([`super::reconcile`]) compares that budget's totals with these figures.
//!
//! Numbers keep the text the file wrote ([`Num`]), so a formula gets the
//! exact digits; figures the export computed (totals) are plain doubles,
//! read once and rounded to cents where they are compared.

/// Which export layout a file is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExportFormat {
    Json,
    XmlBasic,
    XmlAdvanced,
    Excel,
    Tab,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Json,
        ExportFormat::XmlBasic,
        ExportFormat::XmlAdvanced,
        ExportFormat::Excel,
        ExportFormat::Tab,
    ];

    /// The name commands and tools use: `json`, `xml`, `xml-advanced`,
    /// `xlsx`, `tab`.
    pub fn key(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::XmlBasic => "xml",
            ExportFormat::XmlAdvanced => "xml-advanced",
            ExportFormat::Excel => "xlsx",
            ExportFormat::Tab => "tab",
        }
    }

    /// The format named `key` (as [`ExportFormat::key`] writes it; a few
    /// other spellings are read too).
    pub fn parse(key: &str) -> Option<ExportFormat> {
        match key.trim().to_ascii_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "xml" | "xml-basic" | "basic" => Some(ExportFormat::XmlBasic),
            "xml-advanced" | "advanced" => Some(ExportFormat::XmlAdvanced),
            "xlsx" | "excel" => Some(ExportFormat::Excel),
            "tab" | "tsv" | "txt" | "tab-delimited" => Some(ExportFormat::Tab),
            _ => None,
        }
    }

    /// The file extensions it comes with.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            ExportFormat::Json => &["json"],
            ExportFormat::XmlBasic | ExportFormat::XmlAdvanced => &["xml"],
            ExportFormat::Excel => &["xlsx"],
            ExportFormat::Tab => &["txt", "tab", "tsv"],
        }
    }

    /// How the application names it.
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Json => "Budget exchange JSON",
            ExportFormat::XmlBasic => "Budget exchange XML (basic)",
            ExportFormat::XmlAdvanced => "Budget exchange XML (advanced)",
            ExportFormat::Excel => "Budget export workbook (.xlsx)",
            ExportFormat::Tab => "Budget export, tab-delimited",
        }
    }
}

/// A number as the file wrote it, and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Num {
    /// Plain decimal digits (`-0.3333333333333333`, `132347.66999999998`),
    /// never an exponent: what a formula is given.
    pub text: String,
    pub value: f64,
}

impl Num {
    /// The number `text` spells (a decimal, maybe with an exponent), or
    /// `None` when it isn't one.
    pub fn parse(text: &str) -> Option<Num> {
        let t = text.trim();
        let value: f64 = t.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        let plain = !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-')
            && !t.starts_with('.')
            && !t.ends_with('.');
        let text = if plain {
            t.trim_start_matches('+').to_string()
        } else {
            // Rust writes a double's shortest round-trip digits, with no
            // exponent.
            format!("{value}")
        };
        Some(Num { text, value })
    }

    pub fn from_f64(value: f64) -> Num {
        Num {
            text: format!("{value}"),
            value,
        }
    }
}

/// A whole export.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    pub format: ExportFormat,
    /// The budget's name, when the export gives one.
    pub name: String,
    pub base_currency: String,
    /// The currency the export's figures are in (the base currency unless
    /// the budget shows its totals in another).
    pub totals_currency: String,
    /// Budget information: label and text, as the export gives them.
    pub info: Vec<(String, String)>,
    /// How many of each setup item the export says the budget has
    /// (`FringeCount`, `GlobalCount`, ...), for the report.
    pub counts: Vec<(String, u64)>,
    /// The topsheet, in order.
    pub rows: Vec<TopRow>,
    /// What the reader passed over, in words.
    pub notes: Vec<String>,
    /// Decimal places the export's figures are given to: 2, or 0 for a
    /// layout that shows whole units.
    pub places: u8,
    /// Whether the detail rows name their fringes, groups and globals (a
    /// layout that leaves a field out has `false`).
    pub line_fringes: bool,
    pub line_groups: bool,
    pub line_globals: bool,
    /// Whether fringe rows and accounts carry each fringe's figures (rate,
    /// kind, what it was worked out on).
    pub fringe_figures: bool,
    /// Only the topsheet and the accounts' totals: no detail rows.
    pub totals_only: bool,
}

/// One row of the export's topsheet.
#[derive(Clone, Debug, PartialEq)]
pub enum TopRow {
    Category(XCategory),
    /// A Total Fringes row: one for the budget, or one per production level.
    Fringes(XTotal),
    /// A contractual charge: a row with no number and a total of 0 or more.
    Charge(XTotal),
    /// An applied credit: a row with no number and a negative total.
    Credit(XTotal),
    /// A production total, in the layouts that list them.
    ProductionTotal(String),
    /// An empty row (the entry row at the end of the categories).
    Blank,
}

/// A topsheet row that is only a figure: a fringe row, charge or credit.
#[derive(Clone, Debug, PartialEq)]
pub struct XTotal {
    /// The account number a charge or credit posts to, when it has one
    /// (`600-00`, `900-01`); empty otherwise.
    pub number: String,
    pub name: String,
    pub total: f64,
    pub original: f64,
    /// A fringe row's figures per fringe.
    pub fringes: Vec<XFringe>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XCategory {
    pub number: String,
    pub name: String,
    /// Its fringes, wherever they post.
    pub fringe: f64,
    pub original: f64,
    pub total: f64,
    pub accounts: Vec<XAccount>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XAccount {
    pub number: String,
    pub name: String,
    pub fringe: f64,
    pub original: f64,
    pub total: f64,
    /// `Some` for a category's fringe account (fringes posted by
    /// category): its figures per fringe.
    pub fringe_account: Option<Vec<XFringe>>,
    pub details: Vec<XDetail>,
}

/// One fringe's figures in a fringe row or account.
#[derive(Clone, Debug, PartialEq)]
pub struct XFringe {
    pub percent: bool,
    pub name: String,
    /// The percentage, or the amount per unit of a flat fringe.
    pub rate: String,
    /// A flat fringe's unit, as written (`Days`); empty for none.
    pub unit: String,
    /// What it was worked out on: the capped total of a percent fringe,
    /// the units of a flat one.
    pub amount: f64,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XDetail {
    /// `Some` for an account's Total Fringes row (fringes posted by
    /// account): its figures per fringe. Its total is `subtotal`.
    pub fringe_row: Option<Vec<XFringe>>,
    pub description: String,
    pub amount: Option<Num>,
    pub unit: String,
    pub x: Option<Num>,
    pub unit2: String,
    pub currency: String,
    pub rate: Option<Num>,
    pub unit3: String,
    pub x4: Option<Num>,
    pub unit4: String,
    /// The row's total as the export shows it, in the export's currency.
    pub subtotal: f64,
    pub fringes: Vec<String>,
    pub groups: Vec<String>,
    pub globals: Vec<String>,
    pub location: String,
    pub set: String,
    /// The fringe dollars the row earned, as a percent of its total
    /// (`None` when the export shows none).
    pub fringe_percent: Option<f64>,
    /// Its fringes' nominal percentages added up.
    pub fringe_percent_sum: Option<f64>,
    /// Notes the import puts on the line (not read from the file).
    pub notes: Vec<String>,
}

impl XDetail {
    /// An empty detail row (all fields blank or default).
    pub fn empty() -> XDetail {
        XDetail {
            fringe_row: None,
            description: String::new(),
            amount: None,
            unit: String::new(),
            x: None,
            unit2: String::new(),
            currency: String::new(),
            rate: None,
            unit3: String::new(),
            x4: None,
            unit4: String::new(),
            subtotal: 0.0,
            fringes: Vec::new(),
            groups: Vec::new(),
            globals: Vec::new(),
            location: String::new(),
            set: String::new(),
            fringe_percent: None,
            fringe_percent_sum: None,
            notes: Vec::new(),
        }
    }

    /// Whether this is the empty entry row the exporting program keeps at
    /// the end of each account: no description, tags or units, Amount 1,
    /// X 1, Rate 0, 4X 1 (or blank), total 0.
    pub fn is_blank(&self) -> bool {
        let one = |n: &Option<Num>| n.as_ref().is_none_or(|n| n.value == 1.0);
        let zero = |n: &Option<Num>| n.as_ref().is_none_or(|n| n.value == 0.0);
        self.fringe_row.is_none()
            && self.description.trim().is_empty()
            && one(&self.amount)
            && one(&self.x)
            && one(&self.x4)
            && zero(&self.rate)
            && self.currency.trim().is_empty()
            && [&self.unit, &self.unit2, &self.unit3, &self.unit4]
                .iter()
                .all(|u| u.trim().is_empty())
            && self.fringes.is_empty()
            && self.groups.is_empty()
            && self.globals.is_empty()
            && self.location.trim().is_empty()
            && self.set.trim().is_empty()
            && self.subtotal == 0.0
    }
}

/// A percentage as the export writes it (`12.5%`, `2,000%`), as a number
/// of percent.
pub fn parse_percent(text: &str) -> Option<f64> {
    let t: String = text
        .trim()
        .trim_end_matches('%')
        .chars()
        .filter(|c| *c != ',')
        .collect();
    if t.trim().is_empty() {
        return None;
    }
    t.trim().parse().ok().filter(|v: &f64| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_keep_their_digits() {
        let n = Num::parse("132347.66999999998").unwrap();
        assert_eq!(n.text, "132347.66999999998");
        assert_eq!(Num::parse("1e-7").unwrap().text, "0.0000001");
        assert_eq!(Num::parse("-3").unwrap().text, "-3");
        assert_eq!(Num::parse(" 17 ").unwrap().value, 17.0);
        assert!(Num::parse("abc").is_none());
        assert!(Num::parse("").is_none());
        assert!(Num::parse("NaN").is_none());
    }

    #[test]
    fn percentages_read_with_thousands() {
        assert_eq!(parse_percent("2,000%"), Some(2000.0));
        assert_eq!(parse_percent("12.5%"), Some(12.5));
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_percent("%"), None);
    }

    #[test]
    fn formats_round_trip_their_keys() {
        for f in ExportFormat::ALL {
            assert_eq!(ExportFormat::parse(f.key()), Some(f));
        }
    }
}
