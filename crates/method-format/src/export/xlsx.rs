//! A minimal Office Open XML workbook (.xlsx): a ZIP of SpreadsheetML parts.
//!
//! Parts written (ECMA-376 part 1):
//!
//! ```text
//! [Content_Types].xml          what each part is
//! _rels/.rels                  the package's parts: workbook, core and app properties
//! docProps/core.xml            the title
//! docProps/app.xml             the application's name
//! xl/workbook.xml              the sheets, in order
//! xl/_rels/workbook.xml.rels   where each sheet, the styles and the strings are
//! xl/styles.xml                fonts and number formats (bold headers, money)
//! xl/sharedStrings.xml         every text, once
//! xl/worksheets/sheetN.xml     one per sheet
//! ```
//!
//! Each sheet has a bold header row that stays put when scrolling (a frozen
//! pane), column widths from what the columns hold, and numbers as numbers:
//! money is written as its exact decimal (`1234.56`, from integer cents,
//! never through a float) with a `#,##0.00;(#,##0.00)` format, decimals as
//! the engine holds them. Text always goes in as a string (never a formula).
//! The output is deterministic: the same workbook gives the same bytes.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::zip::{read_entries, ZipWriter};
use super::{money_display_len, money_text, Cell, ExportError, Sheet, Workbook};

/// The longest text an Excel cell holds.
pub const MAX_CELL_CHARS: usize = 32_767;

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_SHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";

/// Style indexes into `cellXfs` (see [`styles`]).
const STYLE_HEADER: u32 = 1;
const STYLE_MONEY: u32 = 2;

/// Column widths, in characters of the default font.
const MIN_WIDTH: usize = 8;
const MAX_WIDTH: usize = 50;

/// The workbook as .xlsx bytes.
pub fn write(wb: &Workbook) -> Result<Vec<u8>, ExportError> {
    let mut strings = Strings::default();
    let sheets: Vec<String> = wb
        .sheets
        .iter()
        .enumerate()
        .map(|(i, s)| worksheet(s, i == 0, &mut strings))
        .collect();

    let mut zip = ZipWriter::new();
    zip.add(
        "[Content_Types].xml",
        content_types(wb.sheets.len()).as_bytes(),
    )?;
    zip.add("_rels/.rels", package_rels().as_bytes())?;
    zip.add("docProps/core.xml", core_props(&wb.title).as_bytes())?;
    zip.add("docProps/app.xml", app_props().as_bytes())?;
    zip.add("xl/workbook.xml", workbook_xml(wb).as_bytes())?;
    zip.add(
        "xl/_rels/workbook.xml.rels",
        workbook_rels(wb.sheets.len()).as_bytes(),
    )?;
    zip.add("xl/styles.xml", styles().as_bytes())?;
    zip.add("xl/sharedStrings.xml", strings.xml().as_bytes())?;
    for (i, xml) in sheets.iter().enumerate() {
        zip.add(&format!("xl/worksheets/sheet{}.xml", i + 1), xml.as_bytes())?;
    }
    Ok(zip.finish()?)
}

/// `0` is `A`, `25` is `Z`, `26` is `AA`.
pub fn column_name(mut i: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Escapes `& < > " '` for element text and attribute values.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Text as a cell can hold it: at most [`MAX_CELL_CHARS`] characters,
/// control characters XML can't carry (and CR, which XML readers turn into
/// LF) as Excel's `_xHHHH_` escapes, and a literal `_xHHHH_` protected by
/// escaping its underscore (`_x005F_`), so Excel shows exactly the text.
fn cell_text(s: &str) -> String {
    let s = match s.char_indices().nth(MAX_CELL_CHARS) {
        Some((at, _)) => &s[..at],
        None => s,
    };
    let bytes = s.as_bytes();
    let looks_escaped = |at: usize| {
        bytes.get(at + 1) == Some(&b'x')
            && bytes.get(at + 6) == Some(&b'_')
            && bytes
                .get(at + 2..at + 6)
                .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit))
    };
    let mut out = String::with_capacity(s.len());
    for (at, c) in s.char_indices() {
        match c {
            '_' if looks_escaped(at) => out.push_str("_x005F_"),
            '\t' | '\n' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {
                let _ = write!(out, "_x{:04X}_", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// The shared string table: every distinct text once, in first-use order.
#[derive(Default)]
struct Strings {
    index: HashMap<String, usize>,
    list: Vec<String>,
    /// String cells in all, counting repeats.
    uses: usize,
}

impl Strings {
    fn id(&mut self, s: &str) -> usize {
        self.uses += 1;
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.list.len();
        self.index.insert(s.to_string(), i);
        self.list.push(s.to_string());
        i
    }

    fn xml(&self) -> String {
        let mut x = format!(
            "{XML_HEAD}<sst xmlns=\"{NS_MAIN}\" count=\"{}\" uniqueCount=\"{}\">",
            self.uses,
            self.list.len()
        );
        for s in &self.list {
            let _ = write!(
                x,
                "<si><t xml:space=\"preserve\">{}</t></si>",
                esc(&cell_text(s))
            );
        }
        x.push_str("</sst>");
        x
    }
}

/// How wide a cell's value shows, in characters.
fn display_len(cell: &Cell) -> usize {
    match cell {
        Cell::Empty => 0,
        Cell::Text(s) => s.lines().map(|l| l.chars().count()).max().unwrap_or(0),
        Cell::Money(c) => money_display_len(*c),
        Cell::Number(d) => d.to_string().len(),
        Cell::Int(n) => n.to_string().len(),
    }
}

fn widths(sheet: &Sheet) -> Vec<usize> {
    sheet
        .columns
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let data = sheet
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(display_len)
                .max()
                .unwrap_or(0);
            // Bold header text runs a little wider.
            let head = title.chars().count() + title.chars().count() / 8 + 1;
            (data.max(head) + 2).clamp(MIN_WIDTH, MAX_WIDTH)
        })
        .collect()
}

fn worksheet(sheet: &Sheet, first: bool, strings: &mut Strings) -> String {
    let ncols = sheet.columns.len().max(1);
    let last = format!("{}{}", column_name(ncols - 1), sheet.rows.len() + 1);
    let mut x = format!(
        "{XML_HEAD}<worksheet xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\">\
         <dimension ref=\"A1:{last}\"/>\
         <sheetViews><sheetView{} workbookViewId=\"0\">\
         <pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/>\
         <selection pane=\"bottomLeft\" activeCell=\"A2\" sqref=\"A2\"/>\
         </sheetView></sheetViews>\
         <sheetFormatPr defaultRowHeight=\"15\"/><cols>",
        if first { " tabSelected=\"1\"" } else { "" }
    );
    for (i, w) in widths(sheet).iter().enumerate() {
        let _ = write!(
            x,
            "<col min=\"{n}\" max=\"{n}\" width=\"{w}\" customWidth=\"1\"/>",
            n = i + 1
        );
    }
    x.push_str("</cols><sheetData>");

    x.push_str("<row r=\"1\">");
    for (i, title) in sheet.columns.iter().enumerate() {
        let _ = write!(
            x,
            "<c r=\"{}1\" s=\"{STYLE_HEADER}\" t=\"s\"><v>{}</v></c>",
            column_name(i),
            strings.id(title)
        );
    }
    x.push_str("</row>");
    for (r, row) in sheet.rows.iter().enumerate() {
        let n = r + 2;
        let _ = write!(x, "<row r=\"{n}\">");
        for (i, cell) in row.iter().enumerate() {
            let at = format!("{}{n}", column_name(i));
            match cell {
                Cell::Empty => {}
                Cell::Text(s) => {
                    let _ = write!(x, "<c r=\"{at}\" t=\"s\"><v>{}</v></c>", strings.id(s));
                }
                Cell::Money(c) => {
                    let _ = write!(
                        x,
                        "<c r=\"{at}\" s=\"{STYLE_MONEY}\"><v>{}</v></c>",
                        money_text(*c)
                    );
                }
                Cell::Number(d) => {
                    let _ = write!(x, "<c r=\"{at}\"><v>{d}</v></c>");
                }
                Cell::Int(v) => {
                    let _ = write!(x, "<c r=\"{at}\"><v>{v}</v></c>");
                }
            }
        }
        x.push_str("</row>");
    }
    x.push_str(
        "</sheetData>\
         <pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>\
         </worksheet>",
    );
    x
}

fn content_types(sheets: usize) -> String {
    let mut x = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>"
    );
    for i in 1..=sheets {
        let _ = write!(
            x,
            "<Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"{CT_SHEET}\"/>"
        );
    }
    x.push_str(
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\
         <Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>\
         <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
         <Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>\
         </Types>",
    );
    x
}

fn package_rels() -> String {
    format!(
        "{XML_HEAD}<Relationships xmlns=\"{NS_PKG_REL}\">\
         <Relationship Id=\"rId1\" Type=\"{NS_REL}/officeDocument\" Target=\"xl/workbook.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
         <Relationship Id=\"rId3\" Type=\"{NS_REL}/extended-properties\" Target=\"docProps/app.xml\"/>\
         </Relationships>"
    )
}

fn core_props(title: &str) -> String {
    format!(
        "{XML_HEAD}<cp:coreProperties \
         xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:dcterms=\"http://purl.org/dc/terms/\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
         <dc:title>{}</dc:title></cp:coreProperties>",
        esc(&cell_text(title))
    )
}

fn app_props() -> String {
    format!(
        "{XML_HEAD}<Properties \
         xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" \
         xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
         <Application>Method Budget</Application></Properties>"
    )
}

fn workbook_xml(wb: &Workbook) -> String {
    let mut x = format!(
        "{XML_HEAD}<workbook xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_REL}\">\
         <bookViews><workbookView activeTab=\"0\"/></bookViews><sheets>"
    );
    for (i, s) in wb.sheets.iter().enumerate() {
        let _ = write!(
            x,
            "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>",
            esc(s.name),
            n = i + 1
        );
    }
    x.push_str("</sheets></workbook>");
    x
}

fn workbook_rels(sheets: usize) -> String {
    let mut x = format!("{XML_HEAD}<Relationships xmlns=\"{NS_PKG_REL}\">");
    for i in 1..=sheets {
        let _ = write!(
            x,
            "<Relationship Id=\"rId{i}\" Type=\"{NS_REL}/worksheet\" Target=\"worksheets/sheet{i}.xml\"/>"
        );
    }
    let _ = write!(
        x,
        "<Relationship Id=\"rId{}\" Type=\"{NS_REL}/styles\" Target=\"styles.xml\"/>\
         <Relationship Id=\"rId{}\" Type=\"{NS_REL}/sharedStrings\" Target=\"sharedStrings.xml\"/>\
         </Relationships>",
        sheets + 1,
        sheets + 2
    );
    x
}

/// Fonts: regular and bold. Cell formats: 0 plain, 1 bold (headers), 2 money
/// (`#,##0.00`, negatives in parentheses, as the app shows money).
fn styles() -> String {
    format!(
        "{XML_HEAD}<styleSheet xmlns=\"{NS_MAIN}\">\
         <numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"#,##0.00;(#,##0.00)\"/></numFmts>\
         <fonts count=\"2\">\
         <font><sz val=\"11\"/><name val=\"Calibri\"/><family val=\"2\"/></font>\
         <font><b/><sz val=\"11\"/><name val=\"Calibri\"/><family val=\"2\"/></font>\
         </fonts>\
         <fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill>\
         <fill><patternFill patternType=\"gray125\"/></fill></fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"3\">\
         <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
         <xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>\
         <xf numFmtId=\"164\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>\
         </cellXfs>\
         <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
         </styleSheet>"
    )
}

/// The parts every workbook this module writes has, besides its sheets.
pub const FIXED_PARTS: [&str; 8] = [
    "[Content_Types].xml",
    "_rels/.rels",
    "docProps/core.xml",
    "docProps/app.xml",
    "xl/workbook.xml",
    "xl/_rels/workbook.xml.rels",
    "xl/styles.xml",
    "xl/sharedStrings.xml",
];

/// Reads an .xlsx this module wrote back and checks its structure: the ZIP
/// (signatures, sizes, CRCs), every expected part present, one sheet part
/// per sheet, and every part well-formed XML. Returns the part names in
/// order. A self-check for tests; Excel is the real judge.
pub fn verify(bytes: &[u8], sheets: usize) -> Result<Vec<String>, String> {
    let entries = read_entries(bytes)?;
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    let mut expected: Vec<String> = FIXED_PARTS.iter().map(|s| s.to_string()).collect();
    expected.extend((1..=sheets).map(|i| format!("xl/worksheets/sheet{i}.xml")));
    if names != expected {
        return Err(format!("parts {names:?}, expected {expected:?}"));
    }
    for e in &entries {
        let text = std::str::from_utf8(&e.data).map_err(|err| format!("{}: {err}", e.name))?;
        well_formed(text).map_err(|err| format!("{}: {err}", e.name))?;
    }
    Ok(names)
}

/// A small well-formedness check: one root element, every tag closed in
/// order, quoted attributes with no duplicates, only the five predefined
/// entities and character references, and only characters XML 1.0 allows.
/// Enough to catch an escaping or nesting mistake; not a full XML parser.
pub fn well_formed(xml: &str) -> Result<(), String> {
    fn name_at(s: &str, at: usize) -> Result<(&str, usize), String> {
        let end = s[at..]
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.')))
            .map_or(s.len(), |n| at + n);
        let name = &s[at..end];
        match name.chars().next() {
            Some(c) if c.is_alphabetic() || c == '_' || c == ':' => Ok((name, end)),
            _ => Err(format!("bad name at byte {at}")),
        }
    }
    fn check_text(t: &str, at: usize) -> Result<(), String> {
        let mut rest = t;
        while let Some(i) = rest.find('&') {
            let after = &rest[i + 1..];
            let semi = after
                .find(';')
                .ok_or(format!("unterminated entity near byte {at}"))?;
            let ent = &after[..semi];
            let ok = matches!(ent, "amp" | "lt" | "gt" | "quot" | "apos")
                || ent
                    .strip_prefix("#x")
                    .is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
                || ent
                    .strip_prefix('#')
                    .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
            if !ok {
                return Err(format!("unknown entity &{ent}; near byte {at}"));
            }
            rest = &after[semi + 1..];
        }
        Ok(())
    }

    if let Some(c) = xml.chars().find(|&c| {
        ((c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r'))
            || c == '\u{FFFE}'
            || c == '\u{FFFF}'
    }) {
        return Err(format!(
            "character U+{:04X} is not allowed in XML",
            c as u32
        ));
    }
    let mut i = 0;
    if xml.starts_with("<?xml") {
        i = xml.find("?>").ok_or("unterminated declaration")? + 2;
    }
    let mut stack: Vec<&str> = Vec::new();
    let mut roots = 0;
    while i < xml.len() {
        let rest = &xml[i..];
        if rest.starts_with("</") {
            let (name, end) = name_at(xml, i + 2)?;
            let gt = xml[end..].find('>').ok_or("unterminated end tag")? + end;
            if !xml[end..gt].trim().is_empty() {
                return Err(format!("junk in end tag </{name}"));
            }
            match stack.pop() {
                Some(open) if open == name => {}
                other => return Err(format!("</{name}> closes {other:?}")),
            }
            i = gt + 1;
        } else if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or("unterminated comment")? + 3;
        } else if rest.starts_with("<?") {
            i += rest
                .find("?>")
                .ok_or("unterminated processing instruction")?
                + 2;
        } else if rest.starts_with("<!") {
            return Err(format!("unexpected <! at byte {i}"));
        } else if rest.starts_with('<') {
            let (name, mut j) = name_at(xml, i + 1)?;
            if stack.is_empty() {
                roots += 1;
                if roots > 1 {
                    return Err(format!("second root element <{name}>"));
                }
            }
            let mut attrs: Vec<&str> = Vec::new();
            loop {
                let after = &xml[j..];
                let trimmed = after.trim_start();
                j += after.len() - trimmed.len();
                if trimmed.starts_with("/>") {
                    j += 2;
                    break;
                }
                if trimmed.starts_with('>') {
                    stack.push(name);
                    j += 1;
                    break;
                }
                if j == i + 1 + name.len() {
                    return Err(format!("no space before attribute in <{name}"));
                }
                let (attr, end) = name_at(xml, j)?;
                if attrs.contains(&attr) {
                    return Err(format!("duplicate attribute {attr} in <{name}>"));
                }
                attrs.push(attr);
                let eq = xml[end..].trim_start();
                let eq_at = xml.len() - eq.len();
                let value = eq
                    .strip_prefix('=')
                    .ok_or(format!("attribute {attr} without a value in <{name}>"))?
                    .trim_start();
                let q_at = xml.len() - value.len();
                let quote = value
                    .chars()
                    .next()
                    .filter(|c| matches!(c, '"' | '\''))
                    .ok_or(format!("unquoted attribute {attr} in <{name}>"))?;
                let close = value[1..]
                    .find(quote)
                    .ok_or(format!("unterminated attribute {attr}"))?;
                let v = &value[1..1 + close];
                if v.contains('<') {
                    return Err(format!("< in attribute {attr}"));
                }
                check_text(v, eq_at)?;
                j = q_at + 1 + close + 1;
            }
            i = j;
        } else {
            let end = rest.find('<').map_or(xml.len(), |n| i + n);
            let text = &xml[i..end];
            if stack.is_empty() && !text.trim().is_empty() {
                return Err(format!("text outside the root element at byte {i}"));
            }
            check_text(text, i)?;
            i = end;
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("<{open}> is never closed"));
    }
    if roots != 1 {
        return Err("no root element".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::Dec;

    fn sample() -> Workbook {
        let d = |s: &str| Cell::Number(s.parse::<Dec>().unwrap());
        Workbook {
            title: "Night & Day <\"pilot\">".into(),
            sheets: vec![
                Sheet {
                    name: "Topsheet",
                    columns: vec!["Code".into(), "Description".into(), "Total".into()],
                    rows: vec![
                        vec![
                            Cell::text("10"),
                            Cell::text("Story & rights"),
                            Cell::Money(5_000_000),
                        ],
                        vec![
                            Cell::Empty,
                            Cell::text("Less <credit>"),
                            Cell::Money(-1_234),
                        ],
                    ],
                },
                Sheet {
                    name: "Globals",
                    columns: vec!["Name".into(), "Value".into(), "Count".into()],
                    rows: vec![
                        vec![Cell::text("SHOOT_DAYS"), d("25"), Cell::Int(3)],
                        vec![Cell::text("THIRD"), d("0.333333333"), Cell::Empty],
                    ],
                },
                Sheet {
                    name: "Empty",
                    columns: vec!["Only a header".into()],
                    rows: vec![],
                },
            ],
        }
    }

    fn part(bytes: &[u8], name: &str) -> String {
        let e = read_entries(bytes).unwrap();
        let e = e.iter().find(|e| e.name == name).unwrap();
        String::from_utf8(e.data.clone()).unwrap()
    }

    #[test]
    fn column_names() {
        let cases = [
            (0, "A"),
            (25, "Z"),
            (26, "AA"),
            (27, "AB"),
            (51, "AZ"),
            (52, "BA"),
            (701, "ZZ"),
            (702, "AAA"),
            (16_383, "XFD"),
        ];
        for (i, want) in cases {
            assert_eq!(column_name(i), want, "{i}");
        }
    }

    #[test]
    fn package_parts_exist_and_are_well_formed() {
        let bytes = write(&sample()).unwrap();
        let names = verify(&bytes, 3).unwrap();
        assert_eq!(names.len(), FIXED_PARTS.len() + 3);
        assert!(names.contains(&"xl/worksheets/sheet3.xml".to_string()));
        // Same workbook, same bytes.
        assert_eq!(write(&sample()).unwrap(), bytes);
        // Content types and relationships name every sheet.
        let ct = part(&bytes, "[Content_Types].xml");
        let rels = part(&bytes, "xl/_rels/workbook.xml.rels");
        for i in 1..=3 {
            assert!(ct.contains(&format!("/xl/worksheets/sheet{i}.xml")));
            assert!(rels.contains(&format!("Target=\"worksheets/sheet{i}.xml\"")));
        }
        let wbx = part(&bytes, "xl/workbook.xml");
        assert!(wbx.contains("<sheet name=\"Globals\" sheetId=\"2\" r:id=\"rId2\"/>"));
        assert!(rels.contains("Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\""));
        assert!(part(&bytes, "docProps/core.xml")
            .contains("<dc:title>Night &amp; Day &lt;&quot;pilot&quot;&gt;</dc:title>"));
    }

    #[test]
    fn cells_are_numbers_strings_and_styles() {
        let bytes = write(&sample()).unwrap();
        let s1 = part(&bytes, "xl/worksheets/sheet1.xml");
        // Money: the exact decimal with the money style; never a float.
        assert!(
            s1.contains("<c r=\"C2\" s=\"2\"><v>50000.00</v></c>"),
            "{s1}"
        );
        assert!(s1.contains("<c r=\"C3\" s=\"2\"><v>-12.34</v></c>"));
        // Header: bold shared strings; the pane below it is frozen.
        assert!(s1.contains("<c r=\"A1\" s=\"1\" t=\"s\"><v>0</v></c>"));
        assert!(s1.contains("state=\"frozen\""));
        assert!(s1.contains("tabSelected=\"1\""));
        assert!(s1.contains("<dimension ref=\"A1:C3\"/>"));
        // An empty cell is left out.
        assert!(!s1.contains("r=\"A3\""));
        let s2 = part(&bytes, "xl/worksheets/sheet2.xml");
        assert!(s2.contains("<c r=\"B3\"><v>0.333333333</v></c>"));
        assert!(s2.contains("<c r=\"C2\"><v>3</v></c>"));
        assert!(!s2.contains("tabSelected"));
        let s3 = part(&bytes, "xl/worksheets/sheet3.xml");
        assert!(s3.contains("<dimension ref=\"A1:A1\"/>"));
        // Strings are escaped and listed once each.
        let sst = part(&bytes, "xl/sharedStrings.xml");
        assert!(sst.contains("<t xml:space=\"preserve\">Story &amp; rights</t>"));
        assert!(sst.contains("Less &lt;credit&gt;"));
        assert_eq!(sst.matches("<si>").count(), 12);
        assert!(sst.contains("count=\"12\" uniqueCount=\"12\""));
    }

    #[test]
    fn widths_follow_the_content() {
        let wb = sample();
        let w = widths(&wb.sheets[0]);
        // "Code" is short: the minimum. "Story & rights" is 14 characters.
        assert_eq!(w[0], MIN_WIDTH);
        assert_eq!(w[1], 16);
        // 50,000.00 is 9 characters, with room either side.
        assert_eq!(w[2], 11);
        let long = Sheet {
            name: "Long",
            columns: vec!["Note".into()],
            rows: vec![vec![Cell::text("x".repeat(500))]],
        };
        assert_eq!(widths(&long), vec![MAX_WIDTH]);
    }

    #[test]
    fn text_survives_xml() {
        assert_eq!(cell_text("plain"), "plain");
        assert_eq!(cell_text("a\tb\nc"), "a\tb\nc");
        assert_eq!(cell_text("cr\r\nlf"), "cr_x000D_\nlf");
        assert_eq!(cell_text("bell\u{7}"), "bell_x0007_");
        assert_eq!(cell_text("nul\u{0}"), "nul_x0000_");
        assert_eq!(cell_text("odd\u{FFFE}"), "odd_xFFFE_");
        // A literal escape-looking text keeps its underscore.
        assert_eq!(cell_text("_x0041_"), "_x005F_x0041_");
        assert_eq!(cell_text("_x00_"), "_x00_");
        assert_eq!(cell_text("my_x_value"), "my_x_value");
        // Long text is cut to what a cell holds, on a character boundary.
        let long = "\u{e9}".repeat(MAX_CELL_CHARS + 10);
        assert_eq!(cell_text(&long).chars().count(), MAX_CELL_CHARS);
        // Every hostile string still makes well-formed XML.
        let hostile = "]]><!-- & \"'<x/>\u{1}\u{1f3ac}";
        let wb = Workbook {
            title: hostile.into(),
            sheets: vec![Sheet {
                name: "Detail",
                columns: vec![hostile.into()],
                rows: vec![vec![Cell::text(hostile)]],
            }],
        };
        verify(&write(&wb).unwrap(), 1).unwrap();
    }

    #[test]
    fn the_checker_catches_mistakes() {
        assert!(well_formed("<a><b/></a>").is_ok());
        assert!(well_formed(
            "<?xml version=\"1.0\"?>\n<a x='1' y=\"&amp;\">t &lt; &#65;&#x41;</a>"
        )
        .is_ok());
        for bad in [
            "<a><b></a>",
            "<a>",
            "<a></a><b/>",
            "<a x=1/>",
            "<a x=\"1\" x=\"2\"/>",
            "<a>&nbsp;</a>",
            "<a>& </a>",
            "<a x=\"<\"/>",
            "text<a/>",
            "<a>\u{1}</a>",
            "",
        ] {
            assert!(well_formed(bad).is_err(), "{bad:?} passed");
        }
    }
}
