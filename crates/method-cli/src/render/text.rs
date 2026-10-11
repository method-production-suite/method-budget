//! Plain text and CSV reports.
//!
//! Plain text lays each table out in fixed-width columns (figures right
//! aligned), the shape the PDF writer prints in a monospaced font. Text cells
//! longer than [`MAX_CELL_CHARS`] are cut with `...` so a long description
//! doesn't push every figure off the page; notes wrap instead.
//!
//! CSV is every section's tables one after another in one file: a row with
//! the section's title, the column titles, the rows, then a blank line.
//! Money is an exact decimal (`1234.50`), as the spreadsheet export writes
//! it, and text a spreadsheet would run as a formula is neutralized the same
//! way (decision 30).

use method_format::export::{csv::neutralize, money_text, Cell};

use super::{display, Doc, Row, Slot, Style, Table};

/// Longest text cell in a fixed-width layout, in characters.
pub const MAX_CELL_CHARS: usize = 60;

/// Narrowest a wrapped note gets, in characters.
const MIN_WRAP: usize = 60;

const GAP: &str = "  ";

/// One printed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub bold: bool,
}

impl Line {
    fn plain(text: impl Into<String>) -> Line {
        Line {
            text: text.into(),
            bold: false,
        }
    }
}

/// A section laid out as lines.
#[derive(Clone, Debug)]
pub struct Block {
    pub title: String,
    pub lines: Vec<Line>,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn clip(s: &str, max: usize) -> String {
    if chars(s) <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(3)).collect();
        out.push_str("...");
        out
    }
}

/// `s` broken into lines of at most `width` characters at spaces (a word
/// longer than that is cut).
fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let mut word = word.to_string();
            while chars(&word) > width {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                let head: String = word.chars().take(width).collect();
                word = word.chars().skip(width).collect();
                out.push(head);
            }
            if !line.is_empty() && chars(&line) + 1 + chars(&word) > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// The text of a cell in a fixed-width row: clipped, with the row's indent
/// on the description column.
fn cell_text(row: &Row, i: usize, c: &Cell, numeric: bool) -> String {
    let text = display(c);
    if numeric {
        return text;
    }
    let indent = if i == 1 {
        "  ".repeat(usize::from(row.indent))
    } else {
        String::new()
    };
    clip(&format!("{indent}{text}"), MAX_CELL_CHARS)
}

/// A table as fixed-width lines.
pub fn table_lines(t: &Table) -> Vec<Line> {
    let n = t.columns.len();
    let mut widths: Vec<usize> = t.columns.iter().map(|c| chars(&c.title)).collect();
    for row in &t.rows {
        if !matches!(row.style, Style::Normal | Style::Total) {
            continue;
        }
        for (i, c) in row.cells.iter().enumerate().take(n) {
            let w = chars(&cell_text(row, i, c, t.columns[i].numeric));
            widths[i] = widths[i].max(w);
        }
    }
    let total: usize = widths.iter().sum::<usize>() + GAP.len() * n.saturating_sub(1);
    let fit = |cells: &[String]| -> String {
        let mut s = String::new();
        for (i, text) in cells.iter().enumerate() {
            if i > 0 {
                s.push_str(GAP);
            }
            let pad = widths[i].saturating_sub(chars(text));
            if t.columns[i].numeric {
                s.push_str(&" ".repeat(pad));
                s.push_str(text);
            } else {
                s.push_str(text);
                s.push_str(&" ".repeat(pad));
            }
        }
        s.trim_end().to_string()
    };
    let mut out = Vec::new();
    let titles: Vec<String> = t.columns.iter().map(|c| c.title.clone()).collect();
    out.push(Line {
        text: fit(&titles),
        bold: true,
    });
    out.push(Line::plain(
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join(GAP),
    ));
    for row in &t.rows {
        match row.style {
            Style::Heading => out.push(Line {
                text: row.span_text(),
                bold: true,
            }),
            Style::Rule => out.push(Line::plain("-".repeat(total))),
            Style::Note => {
                for l in wrap(&row.span_text(), total.max(MIN_WRAP).saturating_sub(4)) {
                    out.push(Line::plain(format!("    {l}")));
                }
            }
            Style::Normal | Style::Total => {
                let cells: Vec<String> = (0..n)
                    .map(|i| {
                        let c = row.cells.get(i).unwrap_or(&Cell::Empty);
                        cell_text(row, i, c, t.columns[i].numeric)
                    })
                    .collect();
                out.push(Line {
                    text: fit(&cells),
                    bold: row.bold || row.style == Style::Total,
                });
            }
        }
    }
    out
}

/// Every section as lines, in order.
pub fn blocks(doc: &Doc) -> Vec<Block> {
    doc.sections
        .iter()
        .map(|s| {
            let mut lines = Vec::new();
            for (k, t) in s.tables.iter().enumerate() {
                if k > 0 {
                    lines.push(Line::plain(""));
                }
                lines.extend(table_lines(t));
            }
            Block {
                title: s.title.clone(),
                lines,
            }
        })
        .collect()
}

/// A header or footer as one line `width` wide: left, centered, right.
pub fn bar_line(left: &str, center: &str, right: &str, width: usize) -> String {
    let (l, c, r) = (chars(left), chars(center), chars(right));
    let width = width.max(l + c + r + 4);
    let mut s = left.to_string();
    if c > 0 {
        let start = (width - c) / 2;
        let start = start.max(l + 2);
        s.push_str(&" ".repeat(start - l));
        s.push_str(center);
    }
    if r > 0 {
        let used = chars(&s);
        s.push_str(&" ".repeat(width.saturating_sub(used + r).max(2)));
        s.push_str(right);
    }
    s.trim_end().to_string()
}

/// The whole report as plain text: the header once at the top, each
/// section under its title, the footer once at the end (no page numbers:
/// plain text has no pages).
pub fn to_txt(doc: &Doc) -> String {
    let blocks = blocks(doc);
    let width = blocks
        .iter()
        .flat_map(|b| b.lines.iter().map(|l| chars(&l.text)))
        .max()
        .unwrap_or(0)
        .max(66);
    let mut out = String::new();
    let bar = |slots: [&Slot; 3]| {
        let [l, c, r] = slots.map(|s| s.resolve("", ""));
        bar_line(&l, &c, &r, width)
    };
    if let Some(h) = &doc.header {
        let line = bar([&h.left, &h.center, &h.right]);
        if !line.is_empty() {
            out.push_str(&line);
            out.push_str("\n\n");
        }
    }
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&b.title);
        out.push('\n');
        out.push_str(&"=".repeat(chars(&b.title)));
        out.push_str("\n\n");
        for l in &b.lines {
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    if let Some(f) = &doc.footer {
        let line = bar([&f.left, &f.center, &f.right]);
        if !line.is_empty() {
            out.push('\n');
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn push_csv_field(out: &mut String, s: &str) {
    let s = neutralize(s);
    if s.contains([',', '"', '\r', '\n']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(&s);
    }
}

fn csv_record(out: &mut String, fields: &[String]) {
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_csv_field(out, f);
    }
    out.push_str("\r\n");
}

fn csv_text(c: &Cell) -> String {
    match c {
        Cell::Money(m) => money_text(*m),
        other => display(other),
    }
}

/// The whole report as CSV (RFC 4180, CRLF), sections one after another.
pub fn to_csv(doc: &Doc, bom: bool) -> String {
    let mut out = String::new();
    if bom {
        out.push('\u{feff}');
    }
    for (k, s) in doc.sections.iter().enumerate() {
        if k > 0 {
            out.push_str("\r\n");
        }
        for t in &s.tables {
            csv_record(&mut out, std::slice::from_ref(&s.title));
            let titles: Vec<String> = t.columns.iter().map(|c| c.title.clone()).collect();
            csv_record(&mut out, &titles);
            for row in &t.rows {
                match row.style {
                    Style::Rule => {}
                    Style::Heading | Style::Note => {
                        csv_record(&mut out, &[row.span_text()]);
                    }
                    Style::Normal | Style::Total => {
                        let fields: Vec<String> = (0..t.columns.len())
                            .map(|i| csv_text(row.cells.get(i).unwrap_or(&Cell::Empty)))
                            .collect();
                        csv_record(&mut out, &fields);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_and_clipping() {
        assert_eq!(wrap("aa bb cc", 5), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("", 3), [""]);
        assert_eq!(clip("abcdef", 5), "ab...");
        assert_eq!(clip("abc", 5), "abc");
    }

    #[test]
    fn bars() {
        assert_eq!(bar_line("L", "C", "R", 11), "L    C    R");
        assert_eq!(bar_line("", "", "Page 1", 10), "    Page 1");
        assert_eq!(bar_line("Title", "", "", 10), "Title");
    }

    #[test]
    fn csv_quotes_and_neutralizes() {
        let mut s = String::new();
        csv_record(
            &mut s,
            &["a,b".into(), "=1+1".into(), "-5".into(), "q\"".into()],
        );
        assert_eq!(s, "\"a,b\",'=1+1,-5,\"q\"\"\"\r\n");
    }
}
