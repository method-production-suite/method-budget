//! Which column of a table holds which budget field: the fields a sheet can
//! map, finding the header row, guessing the mapping from column titles, and
//! reading cells as numbers and names.
//!
//! A mapping names the header row (or none) and, for each field it maps,
//! one column. Every field is optional; [`super::merge`] says what a row
//! needs to become a detail line.

use std::collections::BTreeMap;

use fwb_core::{Cents, Dec};

use super::{Table, Value};

/// A budget field a column can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    CategoryCode,
    CategoryName,
    AccountCode,
    AccountName,
    /// Line, subtotal, adjustment or separator.
    Kind,
    Description,
    Amount,
    Unit,
    X,
    Rate,
    Currency,
    X4,
    /// Fringe names, separated by commas.
    Fringes,
    /// A number shared by the lines of one fringe range (per account).
    FringeRange,
    /// Group names, separated by commas.
    Groups,
    Set,
    Location,
    Note,
    /// The sheet's own total for the line: not imported, only compared with
    /// what the line comes to (the reconciliation).
    LineTotal,
}

impl Field {
    pub const ALL: [Field; 19] = [
        Field::CategoryCode,
        Field::CategoryName,
        Field::AccountCode,
        Field::AccountName,
        Field::Kind,
        Field::Description,
        Field::Amount,
        Field::Unit,
        Field::X,
        Field::Rate,
        Field::Currency,
        Field::X4,
        Field::Fringes,
        Field::FringeRange,
        Field::Groups,
        Field::Set,
        Field::Location,
        Field::Note,
        Field::LineTotal,
    ];

    /// The field's name between the app and the page.
    pub fn key(self) -> &'static str {
        match self {
            Field::CategoryCode => "category_code",
            Field::CategoryName => "category_name",
            Field::AccountCode => "account_code",
            Field::AccountName => "account_name",
            Field::Kind => "kind",
            Field::Description => "description",
            Field::Amount => "amount",
            Field::Unit => "unit",
            Field::X => "x",
            Field::Rate => "rate",
            Field::Currency => "currency",
            Field::X4 => "x4",
            Field::Fringes => "fringes",
            Field::FringeRange => "fringe_range",
            Field::Groups => "groups",
            Field::Set => "set",
            Field::Location => "location",
            Field::Note => "note",
            Field::LineTotal => "line_total",
        }
    }

    pub fn parse(key: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The field in a sentence ("Rate", "Account number").
    pub fn name(self) -> &'static str {
        match self {
            Field::CategoryCode => "Category number",
            Field::CategoryName => "Category name",
            Field::AccountCode => "Account number",
            Field::AccountName => "Account name",
            Field::Kind => "Row kind",
            Field::Description => "Description",
            Field::Amount => "Amount",
            Field::Unit => "Unit",
            Field::X => "X",
            Field::Rate => "Rate",
            Field::Currency => "Currency",
            Field::X4 => "4X",
            Field::Fringes => "Fringes",
            Field::FringeRange => "Fringe range",
            Field::Groups => "Groups",
            Field::Set => "Set",
            Field::Location => "Location",
            Field::Note => "Note",
            Field::LineTotal => "Line total",
        }
    }

    /// Column titles that mean this field, normalized (see [`norm`]), most
    /// telling first. A title equal to one is a strong guess; a title
    /// containing one (of four letters or more) a weak one.
    fn synonyms(self) -> &'static [&'static str] {
        match self {
            Field::CategoryCode => &[
                "category",
                "categorynumber",
                "categoryno",
                "category#",
                "categorycode",
                "catnumber",
                "catno",
                "cat#",
                "catcode",
                "cat",
            ],
            Field::CategoryName => &[
                "categoryname",
                "categorydescription",
                "categorytitle",
                "catname",
                "catdescription",
                "department",
            ],
            Field::AccountCode => &[
                "account",
                "accountnumber",
                "accountno",
                "account#",
                "accountcode",
                "acct",
                "acctnumber",
                "acctno",
                "acct#",
                "acctcode",
                "glaccount",
                "glcode",
                "gl",
            ],
            Field::AccountName => &[
                "accountname",
                "accountdescription",
                "accounttitle",
                "acctname",
                "acctdescription",
                "acctdesc",
                "accttitle",
            ],
            Field::Kind => &["kind", "rowkind", "rowtype", "linetype"],
            Field::Description => &[
                "description",
                "desc",
                "lineitem",
                "item",
                "itemdescription",
                "details",
                "detail",
                "position",
                "name",
                "title",
            ],
            Field::Amount => &["amount", "amt", "quantity", "qty", "count", "number", "no"],
            Field::Unit => &["unit", "units", "uom", "unitofmeasure", "per"],
            Field::X => &["x", "multiplier", "mult", "times"],
            Field::Rate => &[
                "rate",
                "unitprice",
                "price",
                "unitcost",
                "unitrate",
                "costeach",
                "each",
                "dayrate",
                "weeklyrate",
                "cost",
            ],
            Field::Currency => &["currency", "cur", "curr", "ccy"],
            Field::X4 => &["4x", "x4", "fourthfactor", "factor4"],
            Field::Fringes => &["fringes", "fringe", "fringecodes", "fringenames"],
            Field::FringeRange => &["fringerange", "range"],
            Field::Groups => &["groups", "group"],
            Field::Set => &["set", "sets", "setcode", "setname"],
            Field::Location => &["location", "locations", "loc"],
            Field::Note => &["note", "notes", "comment", "comments", "memo", "remarks"],
            Field::LineTotal => &[
                "total",
                "linetotal",
                "totalcost",
                "amounttotal",
                "extended",
                "extendedtotal",
                "ext",
                "subtotal",
                "budget",
                "estimate",
                "sum",
            ],
        }
    }
}

/// A title as guesses compare it: lowercase letters, digits and `#` only.
pub fn norm(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '#')
        .flat_map(char::to_lowercase)
        .collect()
}

/// How well `title` (normalized) names `field`: 0 not at all.
fn score(field: Field, title: &str) -> u32 {
    if title.is_empty() {
        return 0;
    }
    let syn = field.synonyms();
    if let Some(i) = syn.iter().position(|s| *s == title) {
        return 200 - i as u32;
    }
    syn.iter()
        .enumerate()
        .filter(|(_, s)| s.len() >= 4 && title.contains(*s))
        .map(|(i, _)| 100 - i as u32)
        .max()
        .unwrap_or(0)
}

/// The header row, mapped columns and nothing else: what a user picks in
/// the import dialog (and what a saved preset keeps).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mapping {
    /// The row holding the column titles; data starts after it. `None`: no
    /// titles, data from the first row.
    pub header_row: Option<usize>,
    pub columns: BTreeMap<Field, usize>,
}

impl Mapping {
    pub fn column(&self, f: Field) -> Option<usize> {
        self.columns.get(&f).copied()
    }

    /// The first row of data.
    pub fn first_data_row(&self) -> usize {
        self.header_row.map_or(0, |h| h + 1)
    }
}

/// How many rows from the top are looked at for the header.
const HEADER_SCAN: usize = 30;

/// Whether a cell reads as a number (a number cell, or text that parses as
/// one).
fn numeric(v: &Value) -> bool {
    match v {
        Value::Number(_) => true,
        Value::Text(s) => parse_number_text(s, false).is_ok_and(|d| d.is_some()),
        _ => false,
    }
}

/// The row of column titles: among the first rows, the one whose cells
/// best name budget fields; failing that, the first row of text above a
/// row with numbers. `None` when the sheet seems to have no titles.
pub fn detect_header(t: &Table) -> Option<usize> {
    let rows: Vec<usize> = (0..t.rows.len().min(HEADER_SCAN))
        .filter(|&r| t.rows[r].iter().any(|v| !v.is_empty()))
        .collect();
    let mut best: Option<(u32, usize)> = None;
    for &r in &rows {
        let texts: Vec<String> = t.rows[r]
            .iter()
            .filter(|v| matches!(v, Value::Text(_)) && !numeric(v))
            .map(|v| norm(v.text()))
            .collect();
        if texts.len() < 2 {
            continue;
        }
        // The best field for each title, each field counted once.
        let mut used = std::collections::BTreeSet::new();
        let mut total = 0;
        for title in &texts {
            if let Some((s, f)) = Field::ALL
                .iter()
                .map(|&f| (score(f, title), f))
                .filter(|(s, f)| *s > 0 && !used.contains(f))
                .max_by_key(|(s, _)| *s)
            {
                used.insert(f);
                total += s;
            }
        }
        if total > 0 && best.is_none_or(|(b, _)| total > b) {
            best = Some((total, r));
        }
    }
    if let Some((_, r)) = best {
        return Some(r);
    }
    for (i, &r) in rows.iter().enumerate() {
        let row = &t.rows[r];
        let all_text = row
            .iter()
            .all(|v| v.is_empty() || (matches!(v, Value::Text(_)) && !numeric(v)));
        let next_has_numbers = rows
            .get(i + 1)
            .is_some_and(|&n| t.rows[n].iter().any(numeric));
        if all_text && row.iter().filter(|v| !v.is_empty()).count() >= 2 && next_has_numbers {
            return Some(r);
        }
    }
    None
}

/// Each column's title from the header row (empty without one).
pub fn titles(t: &Table, header_row: Option<usize>) -> Vec<String> {
    (0..t.width())
        .map(|c| match header_row {
            Some(r) => t.cell(r, c).text().trim().to_string(),
            None => String::new(),
        })
        .collect()
}

/// The layout's fingerprint for saved presets: the normalized titles,
/// joined. Two sheets with the same titles in the same order match.
pub fn signature(t: &Table, header_row: Option<usize>) -> String {
    titles(t, header_row)
        .iter()
        .map(|s| norm(s))
        .collect::<Vec<_>>()
        .join("|")
}

/// The mapping the titles suggest: each field to the column whose title
/// names it best, each column used once (strongest matches first, then the
/// leftmost column).
pub fn guess(t: &Table, header_row: Option<usize>) -> BTreeMap<Field, usize> {
    let titles: Vec<String> = titles(t, header_row).iter().map(|s| norm(s)).collect();
    let mut cands: Vec<(u32, usize, Field)> = Vec::new();
    for (c, title) in titles.iter().enumerate() {
        for f in Field::ALL {
            let s = score(f, title);
            if s > 0 {
                cands.push((s, c, f));
            }
        }
    }
    cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut out = BTreeMap::new();
    let mut taken = std::collections::BTreeSet::new();
    for (_, c, f) in cands {
        if out.contains_key(&f) || taken.contains(&c) {
            continue;
        }
        out.insert(f, c);
        taken.insert(c);
    }
    out
}

/// Turns a decimal written with any number of places and an optional
/// exponent (`1234.5`, `1.5E-3`, `-0.30000000000000004`) into a [`Dec`],
/// rounded half away from zero to its nine places. Exact: no floats.
pub fn parse_decimal(s: &str) -> Result<Dec, String> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(at) => {
            let e: i32 = body[at + 1..]
                .parse()
                .map_err(|_| "isn't a number".to_string())?;
            (&body[..at], e)
        }
        None => (body, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if (int.is_empty() && frac.is_empty())
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("isn't a number".into());
    }
    let digits: String = format!("{int}{frac}").trim_start_matches('0').to_string();
    if digits.is_empty() {
        return Ok(Dec::ZERO);
    }
    if exp.abs() > 100 {
        return Err("is too large or too small".into());
    }
    // value = digits x 10^(exp - frac.len()); scaled by 10^9 for Dec.
    let shift = exp - frac.len() as i32 + 9;
    let scaled: i128 = if shift >= 0 {
        let mut v: i128 = digits.parse().map_err(|_| "is too large".to_string())?;
        for _ in 0..shift {
            v = v.checked_mul(10).ok_or("is too large")?;
        }
        v
    } else {
        let cut = (-shift) as usize;
        if cut > digits.len() {
            // Smaller than half a billionth: rounds to zero unless it is
            // exactly at the next digit (which only happens when cut ==
            // len + 1 and the first digit is 5 or more).
            if cut == digits.len() + 1 && digits.as_bytes()[0] >= b'5' {
                1
            } else {
                0
            }
        } else {
            let (keep, drop) = digits.split_at(digits.len() - cut);
            let mut v: i128 = if keep.is_empty() {
                0
            } else {
                keep.parse().map_err(|_| "is too large".to_string())?
            };
            if drop.as_bytes()[0] >= b'5' {
                v = v.checked_add(1).ok_or("is too large")?;
            }
            v
        }
    };
    let text = format!(
        "{}{}.{:09}",
        if neg { "-" } else { "" },
        scaled / 1_000_000_000,
        scaled % 1_000_000_000
    );
    text.parse::<Dec>().map_err(|_| "is too large".to_string())
}

/// A number typed into a text cell: currency signs, spaces and thousands
/// separators are ignored, `(5)` is -5, `10%` is 0.1. With `decimal_comma`
/// a lone comma is the decimal point (`7,5`) and points group thousands
/// (`1.234,56`); otherwise commas group thousands. `Ok(None)` for blank
/// text, `Err` for text that isn't a number.
pub fn parse_number_text(s: &str, decimal_comma: bool) -> Result<Option<Dec>, String> {
    let mut t: String = s
        .trim()
        .chars()
        .filter(|c| {
            !c.is_whitespace() && !matches!(c, '$' | '\u{20AC}' | '\u{A3}' | '\u{A5}' | '\u{2032}')
        })
        .collect();
    if t.is_empty() {
        return Ok(None);
    }
    let mut neg = false;
    if t.starts_with('(') && t.ends_with(')') && t.len() > 2 {
        neg = true;
        t = t[1..t.len() - 1].to_string();
    }
    let percent = t.ends_with('%');
    if percent {
        t.pop();
    }
    let (thousands, point) = if decimal_comma {
        ('.', ',')
    } else {
        (',', '.')
    };
    let t = if decimal_comma || t.contains(',') && t.contains('.') {
        // Both kinds (or a decimal comma): whichever comes last is the point.
        let (th, pt) = match (t.rfind(','), t.rfind('.')) {
            (Some(c), Some(p)) if c > p => ('.', ','),
            (Some(_), Some(_)) => (',', '.'),
            _ => (thousands, point),
        };
        t.replace(th, "").replace(pt, ".")
    } else if t.contains(',') {
        // Commas only: thousands when every group after the first has three
        // digits (1,234,567), else not a number.
        let groups: Vec<&str> = t.trim_start_matches(['-', '+']).split(',').collect();
        let ok = groups.len() > 1
            && !groups[0].is_empty()
            && groups[0].len() <= 3
            && groups[1..].iter().all(|g| {
                g.len() == 3
                    || (g.len() > 3
                        && g.as_bytes()[3] == b'.'
                        && g[..3].bytes().all(|b| b.is_ascii_digit()))
            });
        if !ok {
            return Err("isn't a number".into());
        }
        t.replace(',', "")
    } else {
        t
    };
    let mut d = parse_decimal(&t)?;
    if percent {
        d = d.checked_div(Dec::from_int(100)).ok_or("is too large")?;
    }
    if neg {
        d = d.checked_neg().ok_or("is too large")?;
    }
    Ok(Some(d))
}

/// A cell as a number: `Ok(None)` when empty.
pub fn cell_number(v: &Value, decimal_comma: bool) -> Result<Option<Dec>, String> {
    match v {
        Value::Empty => Ok(None),
        Value::Number(s) => parse_decimal(s).map(Some),
        Value::Text(s) => parse_number_text(s, decimal_comma),
        Value::Bool(_) => Err("isn't a number".into()),
    }
}

/// A cell as money (rounded to cents once): `Ok(None)` when empty.
pub fn cell_money(v: &Value, decimal_comma: bool) -> Result<Option<Cents>, String> {
    match cell_number(v, decimal_comma)? {
        None => Ok(None),
        Some(d) => d.to_cents().map(Some).ok_or_else(|| "is too large".into()),
    }
}

/// A list cell (`FICA, MEDI; WC`) as names, in order, without blanks or
/// repeats.
pub fn cell_list(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in v.text().split([',', ';', '\n']) {
        let p = part.trim();
        if !p.is_empty() && !out.iter().any(|o| o == p) {
            out.push(p.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn table(rows: &[&[&str]]) -> Table {
        let mut t = Table::default();
        for (r, row) in rows.iter().enumerate() {
            for (c, s) in row.iter().enumerate() {
                if !s.is_empty() {
                    let v = if s.parse::<f64>().is_ok() {
                        Value::Number(s.to_string())
                    } else {
                        Value::Text(s.to_string())
                    };
                    t.put(r, c, v);
                }
            }
        }
        t
    }

    #[test]
    fn decimals_are_exact_and_rounded_once() {
        assert_eq!(parse_decimal("1234.5").unwrap(), d("1234.5"));
        assert_eq!(parse_decimal("1.5E-3").unwrap(), d("0.0015"));
        assert_eq!(parse_decimal("2e3").unwrap(), d("2000"));
        assert_eq!(parse_decimal("-0.30000000000000004").unwrap(), d("-0.3"));
        assert_eq!(parse_decimal("0.1234567895").unwrap(), d("0.12345679"));
        assert_eq!(parse_decimal("-0.1234567885").unwrap(), d("-0.123456789"));
        assert_eq!(parse_decimal("0.0000000005").unwrap(), d("0.000000001"));
        assert_eq!(parse_decimal("0.0000000004").unwrap(), Dec::ZERO);
        assert_eq!(parse_decimal("5E-10").unwrap(), d("0.000000001"));
        assert_eq!(parse_decimal("4.9E-10").unwrap(), Dec::ZERO);
        assert_eq!(parse_decimal("000.000").unwrap(), Dec::ZERO);
        assert_eq!(parse_decimal(".5").unwrap(), d("0.5"));
        assert_eq!(
            parse_decimal("12345678901234567890").unwrap(),
            d("12345678901234567890")
        );
        for bad in [
            "", ".", "-", "1.2.3", "1e", "e5", "1,5", "abc", "1e500", "1e-500",
        ] {
            assert!(parse_decimal(bad).is_err(), "{bad}");
        }
        assert!(parse_decimal("1e40").is_err(), "too large for a Dec");
    }

    #[test]
    fn typed_numbers() {
        let n = |s: &str| parse_number_text(s, false).unwrap().unwrap();
        assert_eq!(n("$1,234.50"), d("1234.5"));
        assert_eq!(n(" (1,000) "), d("-1000"));
        assert_eq!(n("10%"), d("0.1"));
        assert_eq!(n("-7.25"), d("-7.25"));
        assert_eq!(n("1,234,567"), d("1234567"));
        assert_eq!(n("1,234.5"), d("1234.5"));
        assert_eq!(n("\u{20ac}5"), d("5"));
        assert_eq!(parse_number_text("  ", false).unwrap(), None);
        for bad in ["1,5", "12,34", "TBD", "1.2.3", "5 days", "=A1"] {
            assert!(parse_number_text(bad, false).is_err(), "{bad}");
        }
        // Decimal commas.
        let e = |s: &str| parse_number_text(s, true).unwrap().unwrap();
        assert_eq!(e("1.234,56"), d("1234.56"));
        assert_eq!(e("7,5"), d("7.5"));
        assert_eq!(e("1234"), d("1234"));
        // Both separators: the last one is the point, either way.
        assert_eq!(n("1.234,56"), d("1234.56"));
        assert_eq!(
            cell_money(&Value::Number("1234.565".into()), false).unwrap(),
            Some(123_457)
        );
        assert_eq!(
            cell_money(&Value::Number("-0.005".into()), false).unwrap(),
            Some(-1)
        );
        assert!(cell_number(&Value::Bool(true), false).is_err());
    }

    #[test]
    fn lists() {
        assert_eq!(
            cell_list(&Value::Text("FICA, MEDI;WC, FICA,,".into())),
            ["FICA", "MEDI", "WC"]
        );
        assert!(cell_list(&Value::Empty).is_empty());
    }

    #[test]
    fn header_rows_are_found() {
        // A title block above the header.
        let t = table(&[
            &["Camera department", "", ""],
            &["Prepared by someone"],
            &[],
            &["Acct #", "Item", "Qty", "Rate", "Total"],
            &["2101", "Camera body", "2", "450", "900"],
        ]);
        assert_eq!(detect_header(&t), Some(3));
        // No titles that name fields, but text above numbers.
        let t = table(&[&["Foo", "Bar"], &["1", "2"]]);
        assert_eq!(detect_header(&t), Some(0));
        // No titles at all.
        let t = table(&[&["1", "2"], &["3", "4"]]);
        assert_eq!(detect_header(&t), None);
    }

    #[test]
    fn guesses_from_titles() {
        // Our own export's Detail sheet maps itself.
        let export = [
            "Category",
            "Account",
            "Account Name",
            "Line",
            "Kind",
            "Description",
            "Amount Formula",
            "Amount",
            "Unit",
            "X",
            "X Unit",
            "Rate Formula",
            "Rate",
            "Rate Unit",
            "Currency",
            "4X",
            "4X Unit",
            "Subtotal",
            "Total",
            "Fringes",
            "Fringe Total",
            "Groups",
            "Set",
            "Location",
            "Note",
            "Included",
            "Row ID",
            "Fringe Range",
        ];
        let t = table(&[&export]);
        let g = guess(&t, Some(0));
        let col = |f: Field| g.get(&f).map(|&c| export[c]);
        assert_eq!(col(Field::CategoryCode), Some("Category"));
        assert_eq!(col(Field::CategoryName), None);
        assert_eq!(col(Field::AccountCode), Some("Account"));
        assert_eq!(col(Field::AccountName), Some("Account Name"));
        assert_eq!(col(Field::Kind), Some("Kind"));
        assert_eq!(col(Field::Description), Some("Description"));
        assert_eq!(col(Field::Amount), Some("Amount"));
        assert_eq!(col(Field::Unit), Some("Unit"));
        assert_eq!(col(Field::X), Some("X"));
        assert_eq!(col(Field::Rate), Some("Rate"));
        assert_eq!(col(Field::Currency), Some("Currency"));
        assert_eq!(col(Field::X4), Some("4X"));
        assert_eq!(col(Field::Fringes), Some("Fringes"));
        assert_eq!(col(Field::FringeRange), Some("Fringe Range"));
        assert_eq!(col(Field::Groups), Some("Groups"));
        assert_eq!(col(Field::Set), Some("Set"));
        assert_eq!(col(Field::Location), Some("Location"));
        assert_eq!(col(Field::Note), Some("Note"));
        assert_eq!(col(Field::LineTotal), Some("Total"));

        // A department's sheet.
        let dept = [
            "Acct #",
            "Item Description",
            "Qty",
            "Unit Price",
            "Ext. Total",
            "Comments",
        ];
        let g = guess(&table(&[&dept]), Some(0));
        let col = |f: Field| g.get(&f).map(|&c| dept[c]);
        assert_eq!(col(Field::AccountCode), Some("Acct #"));
        assert_eq!(col(Field::Description), Some("Item Description"));
        assert_eq!(col(Field::Amount), Some("Qty"));
        assert_eq!(col(Field::Rate), Some("Unit Price"));
        assert_eq!(col(Field::LineTotal), Some("Ext. Total"));
        assert_eq!(col(Field::Note), Some("Comments"));

        // "Total Cost" is a total, not a rate; each column is used once.
        let g = guess(&table(&[&["Cost", "Total Cost"]]), Some(0));
        assert_eq!(g.get(&Field::Rate), Some(&0));
        assert_eq!(g.get(&Field::LineTotal), Some(&1));
        // No header, no guesses.
        assert!(guess(&table(&[&["1", "2"]]), None).is_empty());
    }

    #[test]
    fn signatures_ignore_case_and_punctuation() {
        let a = table(&[&["Acct #", "Rate ($)"]]);
        let b = table(&[&["ACCT#", "rate $"]]);
        assert_eq!(signature(&a, Some(0)), signature(&b, Some(0)));
        assert_eq!(signature(&a, Some(0)), "acct#|rate");
    }
}
