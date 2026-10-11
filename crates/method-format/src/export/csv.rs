//! CSV (RFC 4180) and tab-delimited text.
//!
//! - Records end in CRLF, the last one too. The header row comes first.
//! - CSV quotes a field only when it has to (a comma, a double quote, CR or
//!   LF in it) and doubles any double quote inside.
//! - Tab-delimited text never quotes (accounting importers read it field by
//!   field), so tabs and line breaks inside a field become spaces.
//! - Money is an exact decimal with two places (`-1234.50`), decimals as the
//!   engine holds them (`12.5`), no thousands separators or currency signs.
//! - Text that a spreadsheet would run as a formula (starting with `=`, `+`,
//!   `-` or `@`, or a tab or CR) gets a leading `'`, unless it is a plain
//!   number such as `-5`. Budget files can come from anyone, and opening an
//!   export must never run anything.
//! - With `bom`, the file starts with a UTF-8 byte order mark, so Excel
//!   reads accented letters correctly.

use super::{money_text, Cell, Sheet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Comma,
    Tab,
}

const BOM: &str = "\u{feff}";

/// `sheet` as CSV or tab-delimited text, header row first.
pub fn write(sheet: &Sheet, delimiter: Delimiter, bom: bool) -> Vec<u8> {
    let mut out = String::new();
    if bom {
        out.push_str(BOM);
    }
    let header: Vec<Cell> = sheet
        .columns
        .iter()
        .map(|c| Cell::text(c.as_str()))
        .collect();
    for row in std::iter::once(&header).chain(&sheet.rows) {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push(match delimiter {
                    Delimiter::Comma => ',',
                    Delimiter::Tab => '\t',
                });
            }
            let text = field_text(cell);
            match delimiter {
                Delimiter::Comma => push_csv_field(&mut out, &text),
                Delimiter::Tab => push_tsv_field(&mut out, &text),
            }
        }
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// A cell's text before quoting.
fn field_text(cell: &Cell) -> String {
    match cell {
        Cell::Empty => String::new(),
        Cell::Text(s) => neutralize(s),
        Cell::Money(c) => money_text(*c),
        Cell::Number(d) => d.to_string(),
        Cell::Int(n) => n.to_string(),
    }
}

/// Text that would start a spreadsheet formula gets a leading `'`.
pub fn neutralize(s: &str) -> String {
    let risky = matches!(s.chars().next(), Some('=' | '+' | '-' | '@' | '\t' | '\r'));
    if risky && !is_plain_number(s) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

/// `-5`, `+12.50`, `-.5`: a sign, digits and at most one point.
fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    let mut digits = 0;
    let mut points = 0;
    for c in body.chars() {
        match c {
            '0'..='9' => digits += 1,
            '.' => points += 1,
            _ => return false,
        }
    }
    digits > 0 && points <= 1
}

fn push_csv_field(out: &mut String, s: &str) {
    if s.contains([',', '"', '\r', '\n']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

fn push_tsv_field(out: &mut String, s: &str) {
    // CRLF is one break, so it becomes one space.
    let s = s.replace("\r\n", " ");
    out.extend(s.chars().map(|c| match c {
        '\t' | '\r' | '\n' => ' ',
        c => c,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::Dec;

    fn sheet(rows: Vec<Vec<Cell>>) -> Sheet {
        Sheet {
            name: "Test",
            columns: vec!["Name".into(), "Amount".into()],
            rows,
        }
    }

    fn csv(rows: Vec<Vec<Cell>>) -> String {
        String::from_utf8(write(&sheet(rows), Delimiter::Comma, false)).unwrap()
    }

    fn tsv(rows: Vec<Vec<Cell>>) -> String {
        String::from_utf8(write(&sheet(rows), Delimiter::Tab, false)).unwrap()
    }

    fn t(s: &str) -> Cell {
        Cell::text(s)
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(
            csv(vec![vec![t("plain"), Cell::Money(150)]]),
            "Name,Amount\r\nplain,1.50\r\n"
        );
        assert_eq!(
            csv(vec![
                vec![t("a, b"), Cell::Empty],
                vec![t("say \"hi\""), Cell::Empty],
                vec![t("two\nlines"), Cell::Empty],
                vec![t("cr\rhere"), Cell::Empty],
                vec![t("\"quoted\""), Cell::Empty],
                vec![t(" spaced "), Cell::Empty],
                vec![t("caf\u{e9} \u{1f3ac}"), Cell::Empty],
            ]),
            "Name,Amount\r\n\
             \"a, b\",\r\n\
             \"say \"\"hi\"\"\",\r\n\
             \"two\nlines\",\r\n\
             \"cr\rhere\",\r\n\
             \"\"\"quoted\"\"\",\r\n\
             \x20spaced ,\r\n\
             caf\u{e9} \u{1f3ac},\r\n"
        );
    }

    #[test]
    fn empty_cells_and_rows_keep_their_commas() {
        assert_eq!(
            csv(vec![vec![Cell::Empty, Cell::Empty]]),
            "Name,Amount\r\n,\r\n"
        );
        assert_eq!(csv(vec![]), "Name,Amount\r\n");
    }

    #[test]
    fn numbers_are_exact() {
        let d = |s: &str| Cell::Number(s.parse::<Dec>().unwrap());
        assert_eq!(
            csv(vec![
                vec![Cell::Money(-123_456), d("0.333333333")],
                vec![Cell::Money(7), d("-12.5")],
                vec![Cell::Int(42), d("1000000")],
            ]),
            "Name,Amount\r\n-1234.56,0.333333333\r\n0.07,-12.5\r\n42,1000000\r\n"
        );
    }

    #[test]
    fn formula_text_is_neutralized() {
        assert_eq!(neutralize("=1+2"), "'=1+2");
        assert_eq!(neutralize("+SUM(A1)"), "'+SUM(A1)");
        assert_eq!(neutralize("@cmd"), "'@cmd");
        assert_eq!(neutralize("-SHOOT_DAYS"), "'-SHOOT_DAYS");
        assert_eq!(neutralize("- overtime"), "'- overtime");
        assert_eq!(neutralize("\tx"), "'\tx");
        // Plain numbers and ordinary text stay as they are.
        assert_eq!(neutralize("-5"), "-5");
        assert_eq!(neutralize("+12.50"), "+12.50");
        assert_eq!(neutralize("-.5"), "-.5");
        assert_eq!(neutralize("SHOOT_DAYS / 5"), "SHOOT_DAYS / 5");
        assert_eq!(neutralize("1st AD"), "1st AD");
        assert_eq!(neutralize(""), "");
        // A neutralized field is still quoted when it needs to be.
        assert_eq!(
            csv(vec![vec![t("=HYPERLINK(\"x\",\"y\")"), Cell::Empty]]),
            "Name,Amount\r\n\"'=HYPERLINK(\"\"x\"\",\"\"y\"\")\",\r\n"
        );
    }

    #[test]
    fn bom_comes_first() {
        let bytes = write(&sheet(vec![]), Delimiter::Comma, true);
        assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], b"Name,Amount\r\n");
    }

    #[test]
    fn tab_delimited_never_quotes() {
        assert_eq!(
            tsv(vec![
                vec![t("a, \"b\""), Cell::Money(-5)],
                vec![t("tab\there"), Cell::Empty],
                vec![t("line\r\nbreak\nand\rmore"), Cell::Empty],
            ]),
            "Name\tAmount\r\n\
             a, \"b\"\t-0.05\r\n\
             tab here\t\r\n\
             line break and more\t\r\n"
        );
    }
}
