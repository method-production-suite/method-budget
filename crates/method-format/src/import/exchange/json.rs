//! The budget exchange JSON layout.
//!
//! The file is one object:
//!
//! - `metadata`: `budgetdata` (`BudgetName`, `BaseCurrency`,
//!   `ConvCurrency` for the currency totals are shown in, and counts such
//!   as `FringeCount` and `GlobalCount`), `defaultfields` (budget
//!   information: `Title`, `Production`, `Season`, shoot days, dates...)
//!   and `customfields`.
//! - `categories`: the topsheet in order (`cID`, `cNumber`,
//!   `cDescription`, `cFringe`, `cOriginal`, `cTotal`, `cVariance`). A row
//!   with no number is a Total Fringes row when it carries `fringeDetail`,
//!   a charge when its total is 0 or more, a credit when it is negative,
//!   and the empty entry row when it has no name and `cFringe` 0. A row
//!   with a number but a blank `cFringe` and no accounts is a charge or
//!   credit posted to that account number.
//! - `accounts` (`categoryID`, `aID`, `aNumber`, `aDescription`,
//!   `aFringe`, `aOriginal`, `aTotal`, `aVariance`): a category's fringe
//!   account (fringes posted by category) carries `fringeDetail`.
//! - `details` (`accountID`, `dDescription`, `dAmount`, `dUnit`, `dX`,
//!   `dUnit2`, `dCurrency`, `dRate`, `dUnit3`, `dFourthMlt`, `dUnit4`,
//!   `dSubtotal`, `dFringes`, `dGroups`, `dGlobalsUsed`, `dLocation`,
//!   `dSet`, `dAggPercent`, `dAggSumPercent`): an account's Total Fringes
//!   row (fringes posted by account) carries `fringeDetail` and no
//!   amounts.
//!
//! `fringeDetail` is a list of `{type: pctFringe|flatFringe, name, rate,
//! unit, amount, total}`.
//!
//! The JSON is read by hand (no float ever touches a number before it is
//! parsed once from its digits), with a nesting limit, like every other
//! reader here.

use super::model::{
    parse_percent, Export, ExportFormat, Num, TopRow, XAccount, XCategory, XDetail, XFringe, XTotal,
};
use crate::import::ReadError;

/// Deepest nesting of arrays and objects accepted.
const MAX_DEPTH: usize = 64;

/// A JSON value with numbers kept as written.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    /// A string value.
    pub fn s(text: impl Into<String>) -> J {
        J::Str(text.into())
    }

    /// An object from its fields.
    pub fn obj(fields: Vec<(&str, J)>) -> J {
        J::Obj(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn arr(&self) -> &[J] {
        match self {
            J::Arr(a) => a,
            _ => &[],
        }
    }

    /// Text: a string as it is, a number as written, true/false; else empty.
    pub fn text(&self) -> String {
        match self {
            J::Str(s) | J::Num(s) => s.clone(),
            J::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    fn num(&self) -> Option<Num> {
        match self {
            J::Num(s) | J::Str(s) => Num::parse(s),
            _ => None,
        }
    }

    fn f64(&self) -> f64 {
        self.num().map_or(0.0, |n| n.value)
    }

    fn opt_f64(&self) -> Option<f64> {
        self.num().map(|n| n.value)
    }

    fn u64(&self) -> Option<u64> {
        match self {
            J::Num(s) | J::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn strings(&self) -> Vec<String> {
        self.arr()
            .iter()
            .map(|v| v.text().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Parses JSON text.
pub fn parse(text: &str) -> Result<J, ReadError> {
    let mut p = Parser {
        s: text.as_bytes(),
        pos: 0,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.pos != p.s.len() {
        return Err(p.err("unexpected text after the end"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> ReadError {
        ReadError::new(format!(
            "the file isn't valid JSON ({what} at byte {})",
            self.pos
        ))
    }

    fn ws(&mut self) {
        while self
            .s
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.pos += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<J, ReadError> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        match self.s.get(self.pos) {
            None => Err(self.err("unexpected end")),
            Some(b'{') => {
                self.pos += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.eat(b"}") {
                    return Ok(J::Obj(fields));
                }
                loop {
                    self.ws();
                    if self.s.get(self.pos) != Some(&b'"') {
                        return Err(self.err("expected a key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b":") {
                        return Err(self.err("expected ':'"));
                    }
                    self.ws();
                    let v = self.value(depth + 1)?;
                    fields.push((k, v));
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"}") {
                        return Ok(J::Obj(fields));
                    }
                    return Err(self.err("expected ',' or '}'"));
                }
            }
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b"]") {
                    return Ok(J::Arr(items));
                }
                loop {
                    self.ws();
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"]") {
                        return Ok(J::Arr(items));
                    }
                    return Err(self.err("expected ',' or ']'"));
                }
            }
            Some(b'"') => self.string().map(J::Str),
            Some(b't') if self.eat(b"true") => Ok(J::Bool(true)),
            Some(b'f') if self.eat(b"false") => Ok(J::Bool(false)),
            Some(b'n') if self.eat(b"null") => Ok(J::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.pos;
                if self.s[self.pos] == b'-' {
                    self.pos += 1;
                }
                while self.s.get(self.pos).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')
                }) {
                    self.pos += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.pos]).unwrap_or("");
                if t.parse::<f64>().is_err() {
                    return Err(self.err("a bad number"));
                }
                Ok(J::Num(t.to_string()))
            }
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn hex4(&mut self) -> Result<u32, ReadError> {
        let h = self
            .s
            .get(self.pos..self.pos + 4)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| self.err("a bad \\u escape"))?;
        self.pos += 4;
        Ok(h)
    }

    fn string(&mut self) -> Result<String, ReadError> {
        self.pos += 1; // the opening quote
        let mut out = String::new();
        loop {
            let start = self.pos;
            while self
                .s
                .get(self.pos)
                .is_some_and(|b| *b != b'"' && *b != b'\\' && *b >= 0x20)
            {
                self.pos += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.s[start..self.pos])
                    .map_err(|_| self.err("text that isn't UTF-8"))?,
            );
            match self.s.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let c = *self
                        .s
                        .get(self.pos)
                        .ok_or_else(|| self.err("unexpected end"))?;
                    self.pos += 1;
                    match c {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) && self.eat(b"\\u") {
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("a bad surrogate pair"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.err("a bad escape")),
                    }
                }
                Some(_) => return Err(self.err("a control character in a string")),
                None => return Err(self.err("an unfinished string")),
            }
        }
    }
}

/// Whether `bytes` look like the budget exchange JSON layout: an object
/// with `categories`, `accounts` and `details`.
pub fn looks_like(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let s = String::from_utf8_lossy(head);
    let t = s.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('{') && (s.contains("\"metadata\"") || s.contains("\"categories\""))
}

fn fringes(v: Option<&J>) -> Vec<XFringe> {
    v.map(J::arr)
        .unwrap_or_default()
        .iter()
        .map(|f| XFringe {
            percent: !f
                .get("type")
                .map(J::text)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .starts_with("flat"),
            name: f
                .get("name")
                .map(J::text)
                .unwrap_or_default()
                .trim()
                .to_string(),
            rate: f
                .get("rate")
                .map(J::text)
                .unwrap_or_default()
                .trim()
                .to_string(),
            unit: f
                .get("unit")
                .map(J::text)
                .unwrap_or_default()
                .trim()
                .to_string(),
            amount: f.get("amount").map_or(0.0, J::f64),
            total: f.get("total").map_or(0.0, J::f64),
        })
        .filter(|f| !f.name.is_empty())
        .collect()
}

fn text(v: &J, key: &str) -> String {
    v.get(key)
        .map(J::text)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn num(v: &J, key: &str) -> f64 {
    v.get(key).map_or(0.0, J::f64)
}

/// Budget information fields, with the labels the import gives them.
const INFO_FIELDS: [(&str, &str); 13] = [
    ("Production", "Production"),
    ("Season", "Season"),
    ("EpisodeNumber", "Episode number"),
    ("TotalEpisodes", "Total episodes"),
    ("Status", "Status"),
    ("ProductionBaseCountry", "Production base country"),
    ("ProductionBaseRegion", "Production base region"),
    ("ProductionBaseCity", "Production base city"),
    ("LocationShootDays", "Location shoot days"),
    ("StageShootDays", "Stage shoot days"),
    ("StartProductionDate", "Start of production"),
    ("EndProductionDate", "End of production"),
    ("ProductionYear", "Production year"),
];

/// Reads the budget exchange JSON layout.
pub fn read(bytes: &[u8]) -> Result<Export, ReadError> {
    let src = std::str::from_utf8(bytes)
        .map_err(|_| ReadError::new("the file isn't UTF-8 text"))?
        .trim_start_matches('\u{feff}');
    let root = parse(src)?;
    if !matches!(root, J::Obj(_)) || !root.has("categories") {
        return Err(ReadError::new(
            "this JSON file isn't a budget export (it has no categories)",
        ));
    }
    from_tree(&root, ExportFormat::Json, 2)
}

/// The export a tree of values holds, laid out as the JSON export is: the
/// JSON itself, or an XML export or a workbook's sheets made into the same
/// tree. `places` is how many decimal places its figures are given to.
pub fn from_tree(root: &J, format: ExportFormat, places: u8) -> Result<Export, ReadError> {
    if !root.has("categories") {
        return Err(ReadError::new(
            "this file isn't a budget export (it has no categories)",
        ));
    }
    let details_j = root.get("details").map(J::arr).unwrap_or_default();
    let named = |key: &str| details_j.is_empty() || details_j.iter().any(|d| d.has(key));
    let mut x = Export {
        format,
        name: String::new(),
        base_currency: String::new(),
        totals_currency: String::new(),
        info: Vec::new(),
        counts: Vec::new(),
        rows: Vec::new(),
        notes: Vec::new(),
        places,
        line_fringes: named("dFringes"),
        line_groups: named("dGroups"),
        line_globals: named("dGlobalsUsed"),
        fringe_figures: true,
        totals_only: false,
    };
    let meta = root.get("metadata");
    let data = meta.and_then(|m| m.get("budgetdata"));
    if let Some(d) = data {
        x.name = text(d, "BudgetName");
        x.base_currency = text(d, "BaseCurrency");
        x.totals_currency = text(d, "ConvCurrency");
        if let J::Obj(fields) = d {
            for (k, v) in fields {
                if let Some(n) = k.strip_suffix("Count").and(v.u64()) {
                    x.counts.push((k.clone(), n));
                }
            }
        }
    }
    if x.base_currency.is_empty() {
        x.base_currency = "USD".into();
        x.notes
            .push("The export names no base currency: USD was used.".into());
    }
    if x.totals_currency.is_empty() {
        x.totals_currency = x.base_currency.clone();
    }
    if let Some(df) = meta.and_then(|m| m.get("defaultfields")) {
        let title = text(df, "Title");
        if !title.is_empty() {
            x.name = title;
        }
        for (key, label) in INFO_FIELDS {
            let v = text(df, key);
            if !v.is_empty() {
                x.info.push((label.to_string(), v));
            }
        }
    }
    if let Some(cf) = meta.and_then(|m| m.get("customfields")) {
        for f in cf.arr() {
            let label = ["name", "label", "Name", "Label", "field", "Field"]
                .iter()
                .map(|k| text(f, k))
                .find(|s| !s.is_empty());
            let value = ["value", "Value", "text", "Text"]
                .iter()
                .map(|k| text(f, k))
                .find(|s| !s.is_empty());
            match (label, value) {
                (Some(l), Some(v)) => x.info.push((l, v)),
                (Some(_), None) => {}
                _ => x.notes.push(
                    "A custom budget field in a shape this import doesn't know was left out."
                        .into(),
                ),
            }
        }
    }

    // Details by account id, accounts by category id, in file order.
    let mut details: Vec<(String, XDetail)> = Vec::new();
    for d in root.get("details").map(J::arr).unwrap_or_default() {
        let fringe_row = d
            .has("fringeDetail")
            .then(|| fringes(d.get("fringeDetail")));
        let n = |k: &str| d.get(k).and_then(J::num);
        details.push((
            text(d, "accountID"),
            XDetail {
                fringe_row,
                description: d.get("dDescription").map(J::text).unwrap_or_default(),
                amount: n("dAmount"),
                unit: text(d, "dUnit"),
                x: n("dX"),
                unit2: text(d, "dUnit2"),
                currency: text(d, "dCurrency"),
                rate: n("dRate"),
                unit3: text(d, "dUnit3"),
                x4: n("dFourthMlt"),
                unit4: text(d, "dUnit4"),
                subtotal: num(d, "dSubtotal"),
                fringes: d.get("dFringes").map(J::strings).unwrap_or_default(),
                groups: d.get("dGroups").map(J::strings).unwrap_or_default(),
                globals: d.get("dGlobalsUsed").map(J::strings).unwrap_or_default(),
                location: text(d, "dLocation"),
                set: text(d, "dSet"),
                fringe_percent: parse_percent(&text(d, "dAggPercent")),
                fringe_percent_sum: parse_percent(&text(d, "dAggSumPercent")),
                notes: Vec::new(),
            },
        ));
    }
    let mut accounts: Vec<(String, XAccount)> = Vec::new();
    for a in root.get("accounts").map(J::arr).unwrap_or_default() {
        let id = text(a, "aID");
        accounts.push((
            text(a, "categoryID"),
            XAccount {
                number: text(a, "aNumber"),
                name: a.get("aDescription").map(J::text).unwrap_or_default(),
                fringe: num(a, "aFringe"),
                original: num(a, "aOriginal"),
                total: num(a, "aTotal"),
                fringe_account: a
                    .has("fringeDetail")
                    .then(|| fringes(a.get("fringeDetail"))),
                details: details
                    .iter()
                    .filter(|(acct, _)| *acct == id)
                    .map(|(_, d)| d.clone())
                    .collect(),
            },
        ));
    }
    let known: std::collections::BTreeSet<String> = root
        .get("accounts")
        .map(J::arr)
        .unwrap_or_default()
        .iter()
        .map(|a| text(a, "aID"))
        .collect();
    let orphans = details.iter().filter(|(a, _)| !known.contains(a)).count();
    if orphans > 0 {
        x.notes.push(format!(
            "Detail rows naming an account the export doesn't list, left out: {orphans}."
        ));
    }
    let cat_ids: std::collections::BTreeSet<String> = root
        .get("categories")
        .map(J::arr)
        .unwrap_or_default()
        .iter()
        .map(|c| text(c, "cID"))
        .collect();
    let orphan_accounts = accounts
        .iter()
        .filter(|(c, _)| !cat_ids.contains(c))
        .count();
    if orphan_accounts > 0 {
        x.notes.push(format!(
            "Accounts naming a category the export doesn't list, left out: {orphan_accounts}."
        ));
    }

    for c in root.get("categories").map(J::arr).unwrap_or_default() {
        let id = text(c, "cID");
        let number = text(c, "cNumber");
        let name = c.get("cDescription").map(J::text).unwrap_or_default();
        let total = num(c, "cTotal");
        let original = num(c, "cOriginal");
        let fringe = c.get("cFringe").and_then(J::opt_f64);
        let accts: Vec<XAccount> = accounts
            .iter()
            .filter(|(cat, _)| *cat == id)
            .map(|(_, a)| a.clone())
            .collect();
        // A numbered row with a blank fringe figure and no accounts is a
        // charge or credit posted to that account number (a category, even
        // an empty one, shows its fringes, 0 when none).
        let figure_only =
            c.has("cFringe") && fringe.is_none() && accts.is_empty() && !c.has("fringeDetail");
        let row = if !number.is_empty() && !figure_only {
            TopRow::Category(XCategory {
                number,
                name,
                fringe: fringe.unwrap_or(0.0),
                original,
                total,
                accounts: accts,
            })
        } else if c.has("fringeDetail") {
            TopRow::Fringes(XTotal {
                number: String::new(),
                name: name.trim().to_string(),
                total,
                original,
                fringes: fringes(c.get("fringeDetail")),
            })
        } else if fringe.is_some()
            || accts
                .iter()
                .any(|a| !a.details.iter().all(XDetail::is_blank))
        {
            // A category with no number: blank (the entry row) unless it
            // has a name or lines.
            if name.trim().is_empty()
                && total == 0.0
                && accts
                    .iter()
                    .all(|a| a.number.is_empty() && a.details.iter().all(XDetail::is_blank))
            {
                TopRow::Blank
            } else {
                TopRow::Category(XCategory {
                    number,
                    name,
                    fringe: fringe.unwrap_or(0.0),
                    original,
                    total,
                    accounts: accts,
                })
            }
        } else {
            let t = XTotal {
                number,
                name: name.trim().to_string(),
                total,
                original,
                fringes: Vec::new(),
            };
            // `-0` is a credit that rounds to nothing.
            if total < 0.0 || (total == 0.0 && total.is_sign_negative()) {
                TopRow::Credit(t)
            } else {
                TopRow::Charge(t)
            }
        };
        x.rows.push(row);
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parses_with_numbers_as_written() {
        let v = parse(r#"{"a": [1, -2.5e3, "xé\n"], "b": null, "c": true}"#).unwrap();
        assert_eq!(
            v.get("a").unwrap().arr(),
            &[
                J::Num("1".into()),
                J::Num("-2.5e3".into()),
                J::Str("x\u{e9}\n".into())
            ]
        );
        assert_eq!(v.get("b"), Some(&J::Null));
        assert_eq!(v.get("c"), Some(&J::Bool(true)));
        assert!(parse("{\"a\":1,}").is_err());
        assert!(parse("[1 2]").is_err());
        assert!(parse("{\"a\":1} x").is_err());
        assert!(parse("\"\\ud83d\\ude00\"").is_ok());
        let deep = "[".repeat(100) + &"]".repeat(100);
        assert!(parse(&deep).unwrap_err().0.contains("deep"));
    }

    #[test]
    fn a_json_file_that_isnt_an_export_is_refused() {
        assert!(read(b"{\"x\": 1}").unwrap_err().0.contains("categories"));
        assert!(read(b"[1,2]").is_err());
        assert!(read(&[0xff, 0xfe]).is_err());
    }
}
