//! A small PDF writer for headless reports: the plain-text layout
//! ([`super::text`]) set in Courier on Letter pages, with the report's
//! header and footer on every page and "Page X of Y" where the setup asks.
//!
//! Plain std, no crates: PDF 1.4, uncompressed content streams, and the
//! standard Courier and Courier-Bold fonts (every PDF reader has them, so
//! nothing is embedded). A monospaced font keeps the figures lined up with
//! no font metrics. Text is WinAnsi encoded: Latin-1 letters and the usual
//! typographic punctuation print, anything else prints as `?`. The type
//! shrinks (down to 4 points) until the widest line fits the page.
//!
//! This is not the app's PDF (decision 28), which lays out proportional
//! pages in the system web view; it is a plain printout for pipelines.

use std::fmt::Write as _;

use fwb_core::FontSize;

use super::text::{bar_line, blocks, Line};
use super::{Bar, Doc};

const MARGIN: f64 = 36.0;
/// Courier's advance width, as a fraction of the type size.
const ADVANCE: f64 = 0.6;
const MIN_SIZE: f64 = 4.0;

/// The WinAnsi byte for `c`, or `?`.
fn win_ansi(c: char) -> u8 {
    let n = c as u32;
    if (0x20..=0x7e).contains(&n) || (0xa0..=0xff).contains(&n) {
        return n as u8;
    }
    match c {
        '\u{20ac}' => 0x80,
        '\u{201a}' => 0x82,
        '\u{0192}' => 0x83,
        '\u{201e}' => 0x84,
        '\u{2026}' => 0x85,
        '\u{2020}' => 0x86,
        '\u{2021}' => 0x87,
        '\u{02c6}' => 0x88,
        '\u{2030}' => 0x89,
        '\u{0160}' => 0x8a,
        '\u{2039}' => 0x8b,
        '\u{0152}' => 0x8c,
        '\u{017d}' => 0x8e,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{02dc}' => 0x98,
        '\u{2122}' => 0x99,
        '\u{0161}' => 0x9a,
        '\u{203a}' => 0x9b,
        '\u{0153}' => 0x9c,
        '\u{017e}' => 0x9e,
        '\u{0178}' => 0x9f,
        '\t' => b' ',
        _ => b'?',
    }
}

/// `s` as a PDF literal string, parentheses included.
fn literal(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        match win_ansi(c) {
            b @ (b'(' | b')' | b'\\') => {
                out.push(b'\\');
                out.push(b);
            }
            b if b >= 0x80 => out.extend(format!("\\{b:03o}").bytes()),
            b => out.push(b),
        }
    }
    out.push(b')');
    out
}

struct Page {
    section: String,
    lines: Vec<Line>,
}

/// The report as a PDF file.
pub fn to_pdf(doc: &Doc) -> Vec<u8> {
    let (pw, ph) = if doc.landscape {
        (792.0, 612.0)
    } else {
        (612.0, 792.0)
    };
    let blocks = blocks(doc);
    let widest = blocks
        .iter()
        .flat_map(|b| b.lines.iter().map(|l| l.text.chars().count()))
        .chain(blocks.iter().map(|b| b.title.chars().count()))
        .max()
        .unwrap_or(1)
        .max(1);
    let preferred = match doc.font_size {
        FontSize::Small => 7.0,
        FontSize::Medium => 8.5,
        FontSize::Large => 10.0,
    };
    let usable = pw - 2.0 * MARGIN;
    let size = (usable / (widest as f64 * ADVANCE))
        .min(preferred)
        .max(MIN_SIZE);
    let lead = size * 1.25;
    let width_chars = (usable / (size * ADVANCE)).floor() as usize;
    let header_h = if doc.header.is_some() {
        2.0 * lead
    } else {
        0.0
    };
    let footer_h = if doc.footer.is_some() {
        2.0 * lead
    } else {
        0.0
    };
    let per_page = (((ph - 2.0 * MARGIN - header_h - footer_h) / lead).floor() as usize).max(4);

    // Each section starts a page, under its title.
    let mut pages: Vec<Page> = Vec::new();
    for b in &blocks {
        let mut page = Page {
            section: b.title.clone(),
            lines: vec![
                Line {
                    text: b.title.clone(),
                    bold: true,
                },
                Line {
                    text: String::new(),
                    bold: false,
                },
            ],
        };
        for l in &b.lines {
            if page.lines.len() >= per_page {
                pages.push(std::mem::replace(
                    &mut page,
                    Page {
                        section: b.title.clone(),
                        lines: Vec::new(),
                    },
                ));
            }
            page.lines.push(l.clone());
        }
        pages.push(page);
    }
    if pages.is_empty() {
        pages.push(Page {
            section: String::new(),
            lines: Vec::new(),
        });
    }

    let count = pages.len();
    let streams: Vec<Vec<u8>> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let page_no = format!("Page {} of {count}", i + 1);
            let mut s: Vec<u8> = Vec::new();
            let text_at = |s: &mut Vec<u8>, y: f64, text: &str, bold: bool| {
                if text.is_empty() {
                    return;
                }
                let font = if bold { "F2" } else { "F1" };
                s.extend(format!("BT /{font} {size:.2} Tf {MARGIN:.2} {y:.2} Td ").bytes());
                s.extend(literal(text));
                s.extend(b" Tj ET\n");
            };
            let bar = |b: &Bar| {
                let [l, c, r] =
                    [&b.left, &b.center, &b.right].map(|slot| slot.resolve(&p.section, &page_no));
                bar_line(&l, &c, &r, width_chars)
            };
            let top = ph - MARGIN - size;
            if let Some(h) = doc.header.as_ref().filter(|h| !h.first_page_only || i == 0) {
                text_at(&mut s, top, &bar(h), false);
            }
            let mut y = top - header_h;
            for l in &p.lines {
                text_at(&mut s, y, &l.text, l.bold);
                y -= lead;
            }
            if let Some(f) = doc.footer.as_ref().filter(|f| !f.first_page_only || i == 0) {
                text_at(&mut s, MARGIN, &bar(f), false);
            }
            s
        })
        .collect();

    // Objects: 1 catalog, 2 page tree, 3 and 4 fonts, 5 info, then a page
    // and its content stream for each page.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", 6 + 2 * i)).collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            kids.join(" ")
        )
        .into_bytes(),
    );
    for name in ["Courier", "Courier-Bold"] {
        objects.push(
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>"
            )
            .into_bytes(),
        );
    }
    let mut info = b"<< /Title ".to_vec();
    info.extend(literal(&doc.title));
    info.extend(b" /Producer (Method Budget) >>");
    objects.push(info);
    for (i, stream) in streams.into_iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.0} {ph:.0}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                7 + 2 * i
            )
            .into_bytes(),
        );
        let mut obj = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        obj.extend(stream);
        obj.extend(b"endstream");
        objects.push(obj);
    }

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(table, "{o:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend(table.bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_escape_and_encode() {
        assert_eq!(literal("a(b)\\"), b"(a\\(b\\)\\\\)".to_vec());
        assert_eq!(literal("caf\u{e9}"), b"(caf\\351)".to_vec());
        assert_eq!(literal("\u{2013}\u{4e2d}"), b"(\\226?)".to_vec());
    }
}
