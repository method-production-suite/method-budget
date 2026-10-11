//! Reading an Excel workbook (.xlsx, Office Open XML SpreadsheetML) into
//! tables, one per worksheet.
//!
//! Parts read (ECMA-376 part 1), found through the package's relationships
//! rather than by fixed names, since writers name them differently:
//!
//! ```text
//! _rels/.rels                  where the workbook part is (else xl/workbook.xml)
//! <workbook>                   the sheets: names, order, hidden or not, ids
//! <workbook dir>/_rels/<name>.rels   which part each sheet id is, and the
//!                              shared strings part
//! <sharedStrings>              the text cells point into
//! <worksheet>                  one per sheet: rows of cells
//! ```
//!
//! Cells: `t="s"` is a shared string (by index), `t="inlineStr"` holds its
//! text, `t="str"` is a formula's text result, `t="b"` a boolean, `t="e"` an
//! error (`#DIV/0!`, kept as text), and anything else a number, kept as the
//! digits the file wrote. Formulas themselves are ignored: their cached
//! values are what the sheet shows. Rich text runs are joined; phonetic
//! guides are left out. Excel's `_xHHHH_` escapes are decoded. A cell or row
//! without a reference follows the one before it. Chart sheets and other
//! non-worksheet sheets are skipped. Styles (and so dates shown as dates)
//! are not read: a date comes in as its serial number. The workbook's
//! `<workbookPr date1904="1"/>` (serials counted from 1904) is passed on as
//! [`Table::date1904`] so dates can be read in the right system.

use super::xml::{self, attr, Event, Reader};
use super::zipread::{part_key, ZipArchive};
use super::{ReadError, Table, Value, MAX_CELLS, MAX_CELL_CHARS, MAX_TEXT_BYTES};

/// Most shared strings accepted.
pub const MAX_SHARED_STRINGS: usize = 2_000_000;

const OFFICE_DOCUMENT: &str = "/officeDocument";
const WORKSHEET: &str = "/worksheet";
const SHARED_STRINGS: &str = "/sharedStrings";

fn err(msg: impl Into<String>) -> ReadError {
    ReadError::new(msg)
}

/// Every worksheet in the workbook, in the workbook's order.
pub fn read_xlsx(bytes: &[u8]) -> Result<Vec<Table>, ReadError> {
    let mut zip = ZipArchive::new(bytes)?;
    let workbook_path = office_document(&mut zip)?;
    let workbook = zip
        .read(&workbook_path)?
        .ok_or_else(|| err("the file isn't an Excel workbook (it has no workbook part)"))?;
    let (sheets, date1904) = sheet_list(&xml::decode(&workbook)?)?;
    let rels_path = rels_for(&workbook_path);
    let rels = match zip.read(&rels_path)? {
        Some(b) => relationships(&xml::decode(&b)?, &workbook_path)?,
        None => Vec::new(),
    };
    let strings = match rels.iter().find(|r| r.kind.ends_with(SHARED_STRINGS)) {
        Some(r) => match zip.read(&r.target)? {
            Some(b) => shared_strings(&xml::decode(&b)?)?,
            None => Vec::new(),
        },
        None => match zip.read("xl/sharedStrings.xml")? {
            Some(b) => shared_strings(&xml::decode(&b)?)?,
            None => Vec::new(),
        },
    };
    let mut tables = Vec::new();
    // Text placed in cells, over every sheet.
    let mut text_bytes = 0usize;
    for s in sheets {
        let Some(rel) = rels.iter().find(|r| r.id == s.rel_id) else {
            continue;
        };
        if !rel.kind.ends_with(WORKSHEET) {
            continue; // a chart sheet, dialog sheet or macro sheet
        }
        let part = zip
            .read(&rel.target)?
            .ok_or_else(|| err(format!("the workbook's sheet \"{}\" is missing", s.name)))?;
        let mut t = worksheet(&xml::decode(&part)?, &strings, &mut text_bytes)?;
        t.name = s.name;
        t.hidden = s.hidden;
        t.date1904 = date1904;
        tables.push(t);
    }
    if tables.is_empty() {
        return Err(err("the workbook has no worksheets"));
    }
    Ok(tables)
}

/// Where the workbook part is: the package relationship of type
/// officeDocument, else `xl/workbook.xml`.
fn office_document(zip: &mut ZipArchive<'_>) -> Result<String, ReadError> {
    if let Some(b) = zip.read("_rels/.rels")? {
        let rels = relationships(&xml::decode(&b)?, "")?;
        if let Some(r) = rels.iter().find(|r| r.kind.ends_with(OFFICE_DOCUMENT)) {
            return Ok(r.target.clone());
        }
    }
    Ok("xl/workbook.xml".into())
}

/// The relationships part for `part`: `dir/_rels/name.rels`.
fn rels_for(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

struct SheetRef {
    name: String,
    rel_id: String,
    hidden: bool,
}

/// The sheets, and whether date serials count from 1904 (`workbookPr`'s
/// `date1904`, an XML boolean: `1` or `true`).
fn sheet_list(text: &str) -> Result<(Vec<SheetRef>, bool), ReadError> {
    let mut r = Reader::new(text);
    let mut out = Vec::new();
    let mut date1904 = false;
    while let Some(e) = r.next()? {
        match e {
            Event::Start {
                name: "sheet",
                attrs,
                ..
            } => {
                let state = attr(&attrs, "state").unwrap_or("visible");
                out.push(SheetRef {
                    name: attr(&attrs, "name").unwrap_or("Sheet").to_string(),
                    rel_id: attr(&attrs, "id").unwrap_or("").to_string(),
                    hidden: state != "visible",
                });
            }
            Event::Start {
                name: "workbookPr",
                attrs,
                ..
            } => {
                date1904 = matches!(attr(&attrs, "date1904").map(str::trim), Some("1" | "true"));
            }
            _ => {}
        }
    }
    Ok((out, date1904))
}

struct Rel {
    id: String,
    kind: String,
    /// The target part's name, resolved against the source part's folder.
    target: String,
}

/// The relationships in a `.rels` part belonging to part `source` (`""` for
/// the package). External targets are left out.
fn relationships(text: &str, source: &str) -> Result<Vec<Rel>, ReadError> {
    let base = source.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut r = Reader::new(text);
    let mut out = Vec::new();
    while let Some(e) = r.next()? {
        if let Event::Start {
            name: "Relationship",
            attrs,
            ..
        } = e
        {
            if attr(&attrs, "TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External")) {
                continue;
            }
            let target = attr(&attrs, "Target").unwrap_or("");
            out.push(Rel {
                id: attr(&attrs, "Id").unwrap_or("").to_string(),
                kind: attr(&attrs, "Type").unwrap_or("").to_string(),
                target: resolve(base, target),
            });
        }
    }
    Ok(out)
}

/// `target` against folder `base`: an absolute target (`/xl/...`) from the
/// package root, `..` going up a folder (never above the root).
fn resolve(base: &str, target: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let (start, rest) = match target.strip_prefix('/') {
        Some(rest) => ("", rest),
        None => (base, target),
    };
    for seg in start.split('/').chain(rest.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    part_key(&parts.join("/"))
}

/// Decodes Excel's `_xHHHH_` escapes (`_x000D_` is a carriage return,
/// `_x005F_` an underscore that protects a literal `_xHHHH_`).
pub fn unescape_ooxml(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if b[i] == b'_'
            && b.get(i + 1) == Some(&b'x')
            && b.get(i + 6) == Some(&b'_')
            && b[i + 2..i + 6].iter().all(u8::is_ascii_hexdigit)
        {
            let code = u32::from_str_radix(&s[i + 2..i + 6], 16).unwrap_or(0xFFFD);
            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            i += 7;
            continue;
        }
        let c = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Text cut to [`MAX_CELL_CHARS`] characters; whether it was cut.
fn cap(s: String) -> (String, bool) {
    match s.char_indices().nth(MAX_CELL_CHARS) {
        Some((at, _)) => (s[..at].to_string(), true),
        None => (s, false),
    }
}

/// Collects the text of `<t>` elements inside a string item or an inline
/// string, leaving out phonetic runs (`<rPh>`).
#[derive(Default)]
struct RunText {
    text: String,
    in_t: bool,
    in_phonetic: usize,
}

impl RunText {
    fn start(&mut self, name: &str) {
        match name {
            "t" if self.in_phonetic == 0 => self.in_t = true,
            "rPh" => self.in_phonetic += 1,
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        match name {
            "t" => self.in_t = false,
            "rPh" => self.in_phonetic = self.in_phonetic.saturating_sub(1),
            _ => {}
        }
    }

    fn text(&mut self, t: &str) {
        if self.in_t && self.text.len() <= MAX_CELL_CHARS * 4 {
            self.text.push_str(t);
        }
    }

    fn take(&mut self) -> String {
        let s = unescape_ooxml(&std::mem::take(&mut self.text));
        self.in_t = false;
        self.in_phonetic = 0;
        s
    }
}

fn shared_strings(text: &str) -> Result<Vec<String>, ReadError> {
    let mut r = Reader::new(text);
    let mut out = Vec::new();
    let mut item: Option<RunText> = None;
    while let Some(e) = r.next()? {
        match e {
            Event::Start { name: "si", .. } => item = Some(RunText::default()),
            Event::End { name: "si" } => {
                if let Some(mut it) = item.take() {
                    if out.len() >= MAX_SHARED_STRINGS {
                        return Err(err("the workbook has too many different texts to import"));
                    }
                    out.push(cap(it.take()).0);
                }
            }
            Event::Start { name, .. } => {
                if let Some(it) = item.as_mut() {
                    it.start(name);
                }
            }
            Event::End { name } => {
                if let Some(it) = item.as_mut() {
                    it.end(name);
                }
            }
            Event::Text(t) => {
                if let Some(it) = item.as_mut() {
                    it.text(&t);
                }
            }
        }
    }
    Ok(out)
}

/// `B12` as (row 11, column 1); `None` for anything else.
pub fn cell_ref(r: &str) -> Option<(usize, usize)> {
    let split = r.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = r.split_at(split);
    if letters.is_empty() || letters.len() > 3 || digits.len() > 7 {
        return None;
    }
    let mut col = 0usize;
    for c in letters.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    let row: usize = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    Some((row - 1, col - 1))
}

/// Whether `s` is a number as SpreadsheetML writes them (`-12`, `1.5E-3`).
fn is_number(s: &str) -> bool {
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(at) => (&s[..at], Some(&s[at + 1..])),
        None => (s, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = !(int.is_empty() && frac.is_empty()) && digits(int) && digits(frac);
    let exp_ok = exp.is_none_or(|e| {
        let e = e.strip_prefix(['-', '+']).unwrap_or(e);
        !e.is_empty() && e.len() <= 4 && digits(e)
    });
    mantissa_ok && exp_ok
}

/// One worksheet's cells.
/// One worksheet's cells. `text_bytes` counts the text placed so far in the
/// workbook, against [`MAX_TEXT_BYTES`].
fn worksheet(text: &str, strings: &[String], text_bytes: &mut usize) -> Result<Table, ReadError> {
    let mut cells = 0usize;
    let mut r = Reader::new(text);
    let mut t = Table::default();
    let mut row: Option<usize> = None; // the row being read
    let mut last_row: Option<usize> = None;
    let mut col: usize = 0; // the next cell's column when it has no reference
                            // The cell being read: (column, type, value text, inline text).
    let mut cell: Option<(usize, String, String, RunText)> = None;
    let mut in_v = false;
    let mut in_is = false;
    let mut in_sheet_data = false;
    while let Some(e) = r.next()? {
        match e {
            Event::Start {
                name: "sheetData", ..
            } => in_sheet_data = true,
            Event::End { name: "sheetData" } => in_sheet_data = false,
            Event::Start {
                name: "row", attrs, ..
            } if in_sheet_data => {
                let n = match attr(&attrs, "r").and_then(|v| v.trim().parse::<usize>().ok()) {
                    Some(n) if n >= 1 => n - 1,
                    _ => last_row.map_or(0, |r| r + 1),
                };
                row = Some(n);
                last_row = Some(n);
                col = 0;
            }
            Event::End { name: "row" } => row = None,
            Event::Start {
                name: "c", attrs, ..
            } if row.is_some() => {
                let at = attr(&attrs, "r").and_then(cell_ref).map_or(col, |(_, c)| c);
                col = at + 1;
                let kind = attr(&attrs, "t").unwrap_or("n").to_string();
                cell = Some((at, kind, String::new(), RunText::default()));
            }
            Event::End { name: "c" } => {
                let Some((c, kind, v, mut inline)) = cell.take() else {
                    continue;
                };
                let rr = row.unwrap_or(0);
                let v = v.trim();
                let value = match kind.as_str() {
                    "s" => {
                        if v.is_empty() {
                            Value::Empty
                        } else {
                            let i: usize = v.parse().map_err(|_| {
                                err("the workbook has a damaged text cell (a bad shared string number)")
                            })?;
                            let s = strings.get(i).ok_or_else(|| {
                                err("the workbook has a damaged text cell (a shared string that isn't there)")
                            })?;
                            Value::Text(s.clone())
                        }
                    }
                    "inlineStr" => Value::Text(inline.take()),
                    "str" | "e" | "d" => Value::Text(unescape_ooxml(v)),
                    "b" => match v {
                        "1" | "true" => Value::Bool(true),
                        "0" | "false" => Value::Bool(false),
                        "" => Value::Empty,
                        _ => Value::Text(v.to_string()),
                    },
                    _ if v.is_empty() => Value::Empty,
                    _ if is_number(v) => Value::Number(v.to_string()),
                    _ => Value::Text(v.to_string()),
                };
                let value = match value {
                    Value::Text(s) if s.is_empty() => Value::Empty,
                    Value::Text(s) => {
                        let (s, cut) = cap(s);
                        if cut {
                            t.truncated = true;
                        }
                        Value::Text(s)
                    }
                    v => v,
                };
                if value != Value::Empty {
                    let len = match &value {
                        Value::Text(s) | Value::Number(s) => s.len(),
                        _ => 0,
                    };
                    let grow = (c + 1).saturating_sub(t.rows.get(rr).map_or(0, Vec::len));
                    if cells + grow > MAX_CELLS || *text_bytes + len > MAX_TEXT_BYTES {
                        t.truncated = true;
                    } else {
                        *text_bytes += len;
                        cells += t.put(rr, c, value);
                    }
                }
            }
            Event::Start { name: "v", .. } if cell.is_some() => in_v = true,
            Event::End { name: "v" } => in_v = false,
            Event::Start { name: "is", .. } if cell.is_some() => in_is = true,
            Event::End { name: "is" } => in_is = false,
            Event::Start { name, .. } => {
                if in_is {
                    if let Some(c) = cell.as_mut() {
                        c.3.start(name);
                    }
                }
            }
            Event::End { name } => {
                if in_is {
                    if let Some(c) = cell.as_mut() {
                        c.3.end(name);
                    }
                }
            }
            Event::Text(s) => {
                if let Some(c) = cell.as_mut() {
                    if in_v {
                        if c.2.len() <= MAX_CELL_CHARS * 4 {
                            c.2.push_str(&s);
                        }
                    } else if in_is {
                        c.3.text(&s);
                    }
                }
            }
        }
    }
    t.trim();
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::zip::ZipWriter;

    #[test]
    fn cell_references() {
        assert_eq!(cell_ref("A1"), Some((0, 0)));
        assert_eq!(cell_ref("B12"), Some((11, 1)));
        assert_eq!(cell_ref("Z3"), Some((2, 25)));
        assert_eq!(cell_ref("AA1"), Some((0, 26)));
        assert_eq!(cell_ref("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(cell_ref("a2"), Some((1, 0)));
        for bad in ["", "A", "1", "A0", "AAAA1", "A1B", "$A$1", "A12345678"] {
            assert_eq!(cell_ref(bad), None, "{bad}");
        }
    }

    #[test]
    fn numbers_and_escapes() {
        for ok in ["0", "-12", "1234.5", "1.5E-3", "2e10", ".5", "5.", "+3"] {
            assert!(is_number(ok), "{ok}");
        }
        for bad in [
            "", "-", "1.2.3", "1e", "abc", "1,000", "1e99999", "NaN", "INF",
        ] {
            assert!(!is_number(bad), "{bad}");
        }
        assert_eq!(unescape_ooxml("a_x000D_b"), "a\rb");
        assert_eq!(unescape_ooxml("_x005F_x0041_"), "_x0041_");
        assert_eq!(unescape_ooxml("x_y_x00"), "x_y_x00");
        assert_eq!(unescape_ooxml("caf\u{e9}_x0009_"), "caf\u{e9}\t");
        assert_eq!(
            resolve("xl", "worksheets/sheet1.xml"),
            "xl/worksheets/sheet1.xml"
        );
        assert_eq!(resolve("xl", "/xl/worksheets/s.xml"), "xl/worksheets/s.xml");
        assert_eq!(resolve("xl/a", "../b/../c.xml"), "xl/c.xml");
        assert_eq!(resolve("", "../../x.xml"), "x.xml");
    }

    /// A workbook with these parts, stored.
    fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut z = ZipWriter::new();
        for (name, text) in parts {
            z.add(name, text.as_bytes()).unwrap();
        }
        z.finish().unwrap()
    }

    const WB: &str = "<workbook xmlns:r=\"r\"><sheets><sheet name=\"S\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    const RELS: &str = "<Relationships><Relationship Id=\"rId1\" Type=\"x/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>";

    #[test]
    fn a_minimal_workbook_without_package_rels() {
        let sheet = "<worksheet><sheetData><row><c><v>1</v></c><c t=\"inlineStr\"><is><t>x</t></is></c></row><row><c r=\"C2\" t=\"b\"><v>0</v></c></row></sheetData></worksheet>";
        let bytes = package(&[
            ("xl/workbook.xml", WB),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let t = read_xlsx(&bytes).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "S");
        assert_eq!(
            t[0].rows,
            vec![
                vec![Value::Number("1".into()), Value::Text("x".into())],
                vec![Value::Empty, Value::Empty, Value::Bool(false)],
            ]
        );
    }

    /// A workbook in the 1904 date system says so on every table, and its
    /// date serials import as the dates they show, not 1,462 days early.
    #[test]
    fn the_1904_date_system_is_passed_on_and_read() {
        let sheet = "<worksheet><sheetData>\
            <row><c t=\"inlineStr\"><is><t>Date</t></is></c><c t=\"inlineStr\"><is><t>Account</t></is></c><c t=\"inlineStr\"><is><t>Amount</t></is></c></row>\
            <row><c><v>44833</v></c><c t=\"inlineStr\"><is><t>1101</t></is></c><c><v>12.5</v></c></row>\
            </sheetData></worksheet>";
        let book = |pr: &str| {
            let wb = format!(
                "<workbook xmlns:r=\"r\">{pr}<sheets><sheet name=\"S\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"
            );
            read_xlsx(&package(&[
                ("xl/workbook.xml", &wb),
                ("xl/_rels/workbook.xml.rels", RELS),
                ("xl/worksheets/sheet1.xml", sheet),
            ]))
            .unwrap()
        };
        assert!(book("<workbookPr date1904=\"1\"/>")[0].date1904);
        assert!(book("<x:workbookPr date1904=\"true\" defaultThemeVersion=\"1\"/>")[0].date1904);
        assert!(!book("<workbookPr date1904=\"0\"/>")[0].date1904);
        assert!(!book("<workbookPr/>")[0].date1904);
        assert!(!book("")[0].date1904);

        use crate::import::actuals::{self as ia, Mapping, Options};
        let budget = method_core::starter::budget();
        let date_of = |t: &Table| {
            let header_row = ia::detect_header(t);
            let m = Mapping {
                header_row,
                columns: ia::guess(t, header_row),
            };
            let p = ia::plan(&budget, t, &m, &Options::default());
            p.transactions[0].transaction.date.clone()
        };
        assert_eq!(
            date_of(&book("<workbookPr date1904=\"1\"/>")[0]),
            "2026-09-30"
        );
        assert_eq!(date_of(&book("")[0]), "2022-09-29");
    }

    #[test]
    fn damaged_workbooks_are_errors() {
        let sheet =
            |c: &str| format!("<worksheet><sheetData><row>{c}</row></sheetData></worksheet>");
        let with = |s: &str| {
            package(&[
                ("xl/workbook.xml", WB),
                ("xl/_rels/workbook.xml.rels", RELS),
                ("xl/worksheets/sheet1.xml", s),
            ])
        };
        // A shared string that isn't there.
        assert!(read_xlsx(&with(&sheet("<c t=\"s\"><v>3</v></c>"))).is_err());
        assert!(read_xlsx(&with(&sheet("<c t=\"s\"><v>x</v></c>"))).is_err());
        // A sheet part that is missing.
        let bytes = package(&[
            ("xl/workbook.xml", WB),
            ("xl/_rels/workbook.xml.rels", RELS),
        ]);
        assert!(read_xlsx(&bytes).unwrap_err().0.contains("missing"));
        // No workbook at all.
        let bytes = package(&[("a.xml", "<a/>")]);
        assert!(read_xlsx(&bytes).unwrap_err().0.contains("workbook part"));
        // No worksheets.
        let bytes = package(&[("xl/workbook.xml", "<workbook><sheets/></workbook>")]);
        assert!(read_xlsx(&bytes).unwrap_err().0.contains("no worksheets"));
    }

    #[test]
    fn cell_and_text_budgets_hold() {
        // Every row padded out to column IV: the cell budget stops it.
        let mut s = String::from("<worksheet><sheetData>");
        for r in 1..=10_000 {
            s.push_str(&format!("<row r=\"{r}\"><c r=\"IV{r}\"><v>1</v></c></row>"));
        }
        s.push_str("</sheetData></worksheet>");
        let mut text = 0;
        let t = worksheet(&s, &[], &mut text).unwrap();
        assert!(t.truncated);
        let cells: usize = t.rows.iter().map(Vec::len).sum();
        assert!(cells <= crate::import::MAX_CELLS, "{cells}");
        // One long shared string used by many cells: the text budget stops it.
        let long = "x".repeat(30_000);
        let mut s = String::from("<worksheet><sheetData>");
        for r in 1..=5_000 {
            s.push_str(&format!("<row r=\"{r}\"><c t=\"s\"><v>0</v></c></row>"));
        }
        s.push_str("</sheetData></worksheet>");
        let mut text = 0;
        let t = worksheet(&s, &[long], &mut text).unwrap();
        assert!(t.truncated);
        assert!(text <= crate::import::MAX_TEXT_BYTES);
        assert!(t.rows.len() < 5_000);
    }

    #[test]
    fn far_cells_are_cut_not_allocated() {
        let s = "<worksheet><sheetData><row r=\"1048576\"><c r=\"XFD1048576\"><v>1</v></c></row><row r=\"1\"><c r=\"A1\"><v>2</v></c></row></sheetData></worksheet>";
        let bytes = package(&[
            ("xl/workbook.xml", WB),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/worksheets/sheet1.xml", s),
        ]);
        let t = &read_xlsx(&bytes).unwrap()[0];
        assert!(t.truncated);
        assert_eq!(t.rows, vec![vec![Value::Number("2".into())]]);
    }
}
