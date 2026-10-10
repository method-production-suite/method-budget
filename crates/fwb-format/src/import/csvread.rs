//! CSV (RFC 4180) and tab-delimited text.
//!
//! - **Text encoding.** A UTF-8 byte order mark is dropped; a UTF-16 one
//!   (Excel's "Unicode Text" is UTF-16 tab-delimited) means UTF-16. Without
//!   one the bytes are UTF-8 when they are valid UTF-8, else Windows-1252
//!   (what Excel writes for plain "CSV" on most systems).
//! - **Delimiter.** Comma, tab or semicolon, whichever splits the first lines
//!   most consistently (counted outside quotes). A `.tsv` or `.txt` file
//!   prefers tab when tab works at all. A semicolon file is taken to use
//!   decimal commas (`1.234,56`), as the places that write them do.
//! - **Records.** Fields are split on the delimiter; a field starting with
//!   `"` runs to the next lone `"`, with `""` inside meaning one `"`, and may
//!   hold delimiters and line breaks. CRLF, LF and CR all end a record.
//!   Leniently, a `"` in the middle of an unquoted field is kept as text,
//!   and text after a closing quote is added to the field. A quote that is
//!   never closed is an error (it would swallow the rest of the file).
//! - **Formula guard.** Our own exports put a `'` before text a spreadsheet
//!   would run as a formula (`'=1+2`, `'-SHOOT_DAYS`); reading takes it off
//!   again, so an export reads back as it was written.

use super::{ReadError, Table, Value, MAX_CELLS, MAX_CELL_CHARS, MAX_COLS, MAX_ROWS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Comma,
    Tab,
    Semicolon,
}

impl Delimiter {
    pub fn char(self) -> char {
        match self {
            Delimiter::Comma => ',',
            Delimiter::Tab => '\t',
            Delimiter::Semicolon => ';',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Windows1252,
}

/// A delimited file read into a table, with what was found out about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvRead {
    pub table: Table,
    pub delimiter: Delimiter,
    pub encoding: Encoding,
}

/// Reads delimited text. `name` names the table; `hint` is the delimiter the
/// file's name suggests.
pub fn read_csv(bytes: &[u8], name: &str, hint: Option<Delimiter>) -> Result<CsvRead, ReadError> {
    let (text, encoding) = decode_text(bytes)?;
    let delimiter = sniff(&text, hint);
    let mut table = parse(&text, delimiter)?;
    table.name = name.to_string();
    table.decimal_comma = delimiter == Delimiter::Semicolon;
    Ok(CsvRead {
        table,
        delimiter,
        encoding,
    })
}

/// UTF-16 text with a byte order mark; `None` when it isn't.
pub fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let le = match bytes.get(..2)? {
        [0xFF, 0xFE] => true,
        [0xFE, 0xFF] => false,
        _ => return None,
    };
    let body = &bytes[2..];
    if !body.len().is_multiple_of(2) {
        return None;
    }
    let units = body.chunks_exact(2).map(|p| {
        if le {
            u16::from_le_bytes([p[0], p[1]])
        } else {
            u16::from_be_bytes([p[0], p[1]])
        }
    });
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

/// Windows-1252's characters for bytes 0x80 to 0x9F (the rest are Latin-1).
const CP1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

/// The file's text and how it was encoded.
pub fn decode_text(bytes: &[u8]) -> Result<(String, Encoding), ReadError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let s = std::str::from_utf8(rest)
            .map_err(|_| ReadError::new("the file says it is UTF-8 text but isn't"))?;
        return Ok((s.to_string(), Encoding::Utf8));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let s = decode_utf16(bytes)
            .ok_or_else(|| ReadError::new("the file says it is UTF-16 text but isn't"))?;
        return Ok((s, Encoding::Utf16));
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_string(), Encoding::Utf8));
    }
    let s = bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect();
    Ok((s, Encoding::Windows1252))
}

/// Delimiters counted outside quotes in each of the first records.
fn counts(text: &str, d: char, max_records: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut n = 0;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(n);
                n = 0;
                if out.len() >= max_records {
                    return out;
                }
            }
            c if c == d => n += 1,
            _ => {}
        }
    }
    if n > 0 || !text.ends_with(['\r', '\n']) {
        out.push(n);
    }
    out
}

/// The delimiter that splits the first records most consistently: the one
/// whose most common count (above zero) holds for the most records.
pub fn sniff(text: &str, hint: Option<Delimiter>) -> Delimiter {
    let score = |d: Delimiter| -> (usize, usize) {
        let c = counts(text, d.char(), 50);
        let mut best = (0, 0); // (records agreeing, count)
        for &k in &c {
            if k == 0 {
                continue;
            }
            let agree = c.iter().filter(|&&x| x == k).count();
            if (agree, k) > best {
                best = (agree, k);
            }
        }
        best
    };
    if let Some(h) = hint {
        if score(h).0 > 0 {
            return h;
        }
    }
    let mut best = (Delimiter::Comma, (0, 0));
    for d in [Delimiter::Tab, Delimiter::Semicolon, Delimiter::Comma] {
        let s = score(d);
        if s > best.1 {
            best = (d, s);
        }
    }
    if best.1 .0 == 0 {
        hint.unwrap_or(Delimiter::Comma)
    } else {
        best.0
    }
}

/// Takes off the `'` our exports put before formula-like text.
pub fn unguard(s: &str) -> &str {
    match s.strip_prefix('\'') {
        Some(rest)
            if matches!(
                rest.chars().next(),
                Some('=' | '+' | '-' | '@' | '\t' | '\r')
            ) && !is_plain_number(rest) =>
        {
            rest
        }
        _ => s,
    }
}

/// `-5`, `+12.50`, `-.5`: what the export's guard leaves alone.
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

struct Builder {
    table: Table,
    row: Vec<Value>,
    field: String,
    /// Characters in `field`, to cut long text.
    chars: usize,
    /// Cells kept so far, against [`MAX_CELLS`].
    cells: usize,
    full: bool,
}

impl Builder {
    fn push_char(&mut self, c: char) {
        if self.chars < MAX_CELL_CHARS {
            self.field.push(c);
            self.chars += 1;
        } else {
            self.table.truncated = true;
        }
    }

    fn end_field(&mut self) {
        let text = std::mem::take(&mut self.field);
        self.chars = 0;
        if self.row.len() >= MAX_COLS {
            self.table.truncated = true;
            return;
        }
        let v = unguard(&text);
        self.row.push(if v.is_empty() {
            Value::Empty
        } else if v.len() == text.len() {
            Value::Text(text)
        } else {
            Value::Text(v.to_string())
        });
    }

    fn end_record(&mut self) {
        self.end_field();
        let mut row = std::mem::take(&mut self.row);
        while row.last().is_some_and(Value::is_empty) {
            row.pop();
        }
        if self.table.rows.len() >= MAX_ROWS || self.cells + row.len() > MAX_CELLS {
            self.table.truncated = true;
            self.full = true;
            return;
        }
        self.cells += row.len();
        self.table.rows.push(row);
    }
}

/// Splits `text` into records and fields.
fn parse(text: &str, d: Delimiter) -> Result<Table, ReadError> {
    let delim = d.char();
    let mut b = Builder {
        table: Table::default(),
        row: Vec::new(),
        field: String::new(),
        chars: 0,
        cells: 0,
        full: false,
    };
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut at_start = true; // nothing yet in this field
    let mut quote_line = 0;
    while let Some(c) = chars.next() {
        if b.full {
            break;
        }
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    b.push_char('"');
                } else {
                    quoted = false;
                }
            } else {
                b.push_char(c);
            }
            continue;
        }
        match c {
            c if c == delim => {
                b.end_field();
                at_start = true;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                b.end_record();
                at_start = true;
            }
            '"' if at_start => {
                quoted = true;
                at_start = false;
                quote_line = b.table.rows.len() + 1;
            }
            c => {
                b.push_char(c);
                at_start = false;
            }
        }
    }
    if quoted {
        return Err(ReadError::new(format!(
            "a quoted field starting on row {quote_line} never ends"
        )));
    }
    if !b.full && (!b.row.is_empty() || !b.field.is_empty() || !at_start) {
        b.end_record();
    }
    let mut t = b.table;
    t.trim();
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(t: &Table) -> Vec<Vec<&str>> {
        t.rows
            .iter()
            .map(|r| r.iter().map(Value::text).collect())
            .collect()
    }

    fn read(s: &str) -> CsvRead {
        read_csv(s.as_bytes(), "t", None).unwrap()
    }

    #[test]
    fn rfc_4180_records() {
        let r = read("a,b,c\r\n1,\"two, too\",\"say \"\"hi\"\"\"\r\n\"multi\nline\",,x\r\n");
        assert_eq!(r.delimiter, Delimiter::Comma);
        assert_eq!(
            texts(&r.table),
            vec![
                vec!["a", "b", "c"],
                vec!["1", "two, too", "say \"hi\""],
                vec!["multi\nline", "", "x"],
            ]
        );
        // No final line break; LF and CR endings; blank lines kept as rows.
        let r = read("a,b\n\n1,2\r3,4");
        assert_eq!(
            texts(&r.table),
            vec![vec!["a", "b"], vec![], vec!["1", "2"], vec!["3", "4"]]
        );
        // Lenient quotes in the middle and after a closing quote.
        let r = read("a,b\n5\" tall,\"x\"y\n");
        assert_eq!(texts(&r.table)[1], vec!["5\" tall", "xy"]);
        // An unfinished quote is an error naming its row.
        let e = read_csv(b"a,b\n1,\"open\n2,3\n", "t", None).unwrap_err();
        assert!(e.0.contains("row 2"), "{e}");
        // Empty file.
        assert!(read("").table.rows.is_empty());
    }

    #[test]
    fn delimiters_are_sniffed() {
        assert_eq!(read("a\tb\tc\n1\t2\t3\n").delimiter, Delimiter::Tab);
        let r = read("Account;Rate;Note\n1100;1.234,56;a, b\n1200;7,5;c\n");
        assert_eq!(r.delimiter, Delimiter::Semicolon);
        assert!(r.table.decimal_comma);
        assert_eq!(texts(&r.table)[1], vec!["1100", "1.234,56", "a, b"]);
        // Commas inside quotes don't count.
        assert_eq!(
            read("\"a,b,c\";d\n\"e,f,g\";h\n").delimiter,
            Delimiter::Semicolon
        );
        // One column: the hint, else comma.
        assert_eq!(sniff("just\none\ncolumn\n", None), Delimiter::Comma);
        assert_eq!(sniff("just\none\n", Some(Delimiter::Tab)), Delimiter::Tab);
        // A tab-delimited file with commas in its text, named .txt.
        let r = read_csv(
            b"Desc\tRate\nWriter, story\t50\nGrip, key\t40\n",
            "t",
            Some(Delimiter::Tab),
        )
        .unwrap();
        assert_eq!(r.delimiter, Delimiter::Tab);
        assert_eq!(texts(&r.table)[1], vec!["Writer, story", "50"]);
    }

    #[test]
    fn encodings() {
        let r = read_csv(b"\xEF\xBB\xBFCaf\xC3\xA9,1\r\n", "t", None).unwrap();
        assert_eq!(r.encoding, Encoding::Utf8);
        assert_eq!(texts(&r.table), vec![vec!["Caf\u{e9}", "1"]]);
        // Windows-1252: e acute, the euro sign, curly quotes.
        let r = read_csv(b"Caf\xE9,\x80 5,\x93hi\x94\r\n", "t", None).unwrap();
        assert_eq!(r.encoding, Encoding::Windows1252);
        assert_eq!(
            texts(&r.table),
            vec![vec!["Caf\u{e9}", "\u{20ac} 5", "\u{201c}hi\u{201d}"]]
        );
        // UTF-16 little-endian with a BOM, tab-delimited (Excel's Unicode Text).
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "A\tB\r\n\u{e9}\t2\r\n"
                    .encode_utf16()
                    .flat_map(|u| u.to_le_bytes()),
            )
            .collect();
        let r = read_csv(&le, "t", None).unwrap();
        assert_eq!((r.encoding, r.delimiter), (Encoding::Utf16, Delimiter::Tab));
        assert_eq!(texts(&r.table), vec![vec!["A", "B"], vec!["\u{e9}", "2"]]);
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("x,y\n".encode_utf16().flat_map(|u| u.to_be_bytes()))
            .collect();
        assert_eq!(
            texts(&read_csv(&be, "t", None).unwrap().table),
            vec![vec!["x", "y"]]
        );
        // Broken UTF-16 (a lone surrogate, an odd length) is an error.
        assert!(read_csv(&[0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00], "t", None).is_err());
        assert!(read_csv(&[0xFF, 0xFE, 0x41], "t", None).is_err());
        assert!(read_csv(b"\xEF\xBB\xBF\xFF", "t", None).is_err());
    }

    #[test]
    fn the_formula_guard_comes_off() {
        assert_eq!(unguard("'=1+2"), "=1+2");
        assert_eq!(unguard("'-SHOOT_DAYS"), "-SHOOT_DAYS");
        assert_eq!(unguard("'- overtime"), "- overtime");
        assert_eq!(unguard("'@cmd"), "@cmd");
        // Left alone: what the guard never adds.
        assert_eq!(unguard("'-5"), "'-5");
        assert_eq!(unguard("'quoted'"), "'quoted'");
        assert_eq!(unguard("plain"), "plain");
        let r = read("a\n'=SUM(A1)\n\"'+x, y\"\n");
        assert_eq!(
            texts(&r.table),
            vec![vec!["a"], vec!["=SUM(A1)"], vec!["+x, y"]]
        );
    }

    #[test]
    fn limits_hold() {
        let wide = format!("{}\n", vec!["x"; MAX_COLS + 5].join(","));
        let r = read(&wide);
        assert!(r.table.truncated);
        assert_eq!(r.table.rows[0].len(), MAX_COLS);
        let long = format!("{}\n", "y".repeat(MAX_CELL_CHARS + 10));
        let r = read(&long);
        assert!(r.table.truncated);
        assert_eq!(r.table.rows[0][0].text().len(), MAX_CELL_CHARS);
        // Many full rows: the cell budget stops them before the row limit.
        let row = format!("{}\n", vec!["x"; MAX_COLS].join(","));
        let r = read(&row.repeat(MAX_CELLS / MAX_COLS + 10));
        assert!(r.table.truncated);
        assert!(r.table.rows.iter().map(Vec::len).sum::<usize>() <= MAX_CELLS);
        let tall = "1\n".repeat(MAX_ROWS + 3);
        let r = read(&tall);
        assert!(r.table.truncated);
        assert_eq!(r.table.rows.len(), MAX_ROWS);
    }

    #[test]
    fn noise_never_panics() {
        let mut seed = 12345u64;
        let alphabet = [
            b'"', b',', b';', b'\t', b'\r', b'\n', b'a', b'\'', b'=', 0xC3, 0xA9, 0xFF,
        ];
        for _ in 0..5000 {
            let mut s = Vec::new();
            for _ in 0..(seed % 60) {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1442695040888963407);
                s.push(alphabet[(seed >> 33) as usize % alphabet.len()]);
            }
            let _ = read_csv(&s, "t", None);
            let _ = read_csv(&s, "t", Some(Delimiter::Tab));
        }
    }
}
