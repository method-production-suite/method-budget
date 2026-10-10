//! A report as one self-contained, print-ready HTML page: inline styles, no
//! scripts, no outside files. Printing it from a browser gives Letter pages
//! in the setup's orientation; browsers repeat each table's column titles on
//! every page. Page numbers are left to the browser's own print header and
//! footer, since a page has no way to number itself reliably (decision 28).

use std::fmt::Write as _;

use fwb_core::FontSize;

use super::{display, Doc, Row, Slot, Style, Table};

/// `s` with `&`, `<`, `>`, `"` and `'` escaped.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `#rrggbb` and nothing else, so a color from a file can't inject CSS.
fn safe_color(c: &str) -> Option<&str> {
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit());
    ok.then_some(c)
}

fn row_html(out: &mut String, t: &Table, row: &Row) {
    let n = t.columns.len();
    match row.style {
        Style::Heading => {
            let _ = writeln!(
                out,
                "<tr class=\"heading\"><td colspan=\"{n}\">{}</td></tr>",
                escape(&row.span_text())
            );
        }
        Style::Note => {
            let _ = writeln!(
                out,
                "<tr class=\"note\"><td colspan=\"{n}\">{}</td></tr>",
                escape(&row.span_text())
            );
        }
        Style::Rule => {
            let _ = writeln!(out, "<tr class=\"rule\"><td colspan=\"{n}\"></td></tr>");
        }
        Style::Normal | Style::Total => {
            let mut classes = Vec::new();
            if row.style == Style::Total {
                classes.push("total");
            }
            if row.bold {
                classes.push("b");
            }
            if row.italic {
                classes.push("i");
            }
            let mut attrs = String::new();
            if !classes.is_empty() {
                let _ = write!(attrs, " class=\"{}\"", classes.join(" "));
            }
            if let Some(c) = row.color.as_deref().and_then(safe_color) {
                let _ = write!(attrs, " style=\"color:{c}\"");
            }
            out.push_str(&format!("<tr{attrs}>"));
            for i in 0..n {
                let text = row.cells.get(i).map(display).unwrap_or_default();
                let mut style = String::new();
                if i == 1 && row.indent > 0 {
                    let _ = write!(style, " style=\"padding-left:{}em\"", row.indent);
                }
                let class = if t.columns[i].numeric {
                    " class=\"num\""
                } else {
                    ""
                };
                let _ = write!(out, "<td{class}{style}>{}</td>", escape(&text));
            }
            out.push_str("</tr>\n");
        }
    }
}

fn bar_html(out: &mut String, class: &str, slots: [&Slot; 3]) {
    let [l, c, r] = slots.map(|s| s.resolve("", ""));
    if l.is_empty() && c.is_empty() && r.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "<div class=\"{class}\"><span>{}</span><span>{}</span><span>{}</span></div>",
        escape(&l),
        escape(&c),
        escape(&r)
    );
}

/// The whole report as an HTML document.
pub fn to_html(doc: &Doc) -> String {
    let size = match doc.font_size {
        FontSize::Small => "8pt",
        FontSize::Medium => "9pt",
        FontSize::Large => "10.5pt",
    };
    let orientation = if doc.landscape {
        "landscape"
    } else {
        "portrait"
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"generator\" content=\"Film Wizard Budgeting\">\n\
         <title>{}</title>\n<style>\n\
         @page {{ size: letter {orientation}; margin: 0.5in; }}\n\
         body {{ font-family: -apple-system, \"Helvetica Neue\", Helvetica, Arial, sans-serif; \
         font-size: {size}; color: #000; background: #fff; margin: 0; }}\n\
         .bar {{ display: flex; justify-content: space-between; color: #444; margin: 0 0 1em; }}\n\
         .bar span {{ flex: 1; }} .bar span:nth-child(2) {{ text-align: center; }} \
         .bar span:nth-child(3) {{ text-align: right; }}\n\
         .foot {{ margin: 1.5em 0 0; }}\n\
         section {{ break-before: page; }} section:first-of-type {{ break-before: auto; }}\n\
         h2 {{ font-size: 1.3em; margin: 0 0 0.6em; }}\n\
         table {{ border-collapse: collapse; width: 100%; margin: 0 0 1.2em; }}\n\
         th {{ text-align: left; border-bottom: 1px solid #000; padding: 2px 6px; }}\n\
         td {{ padding: 1px 6px; vertical-align: top; }}\n\
         th.num, td.num {{ text-align: right; white-space: nowrap; }}\n\
         tr {{ break-inside: avoid; }}\n\
         tr.heading td {{ font-weight: bold; padding-top: 0.8em; }}\n\
         tr.total td {{ font-weight: bold; border-top: 1px solid #888; }}\n\
         tr.note td {{ font-size: 0.85em; color: #333; padding-left: 2.5em; white-space: pre-wrap; }}\n\
         tr.rule td {{ border-top: 1px solid #000; padding: 0; height: 2px; }}\n\
         tr.b td {{ font-weight: bold; }} tr.i td {{ font-style: italic; }}\n",
        escape(&doc.title)
    );
    if doc.shading {
        out.push_str(
            "tbody tr:nth-child(even) td { background: #f2f2f2; }\n\
             tr.total td { background: #e0e0e0; }\n",
        );
    }
    if doc.vertical_rules {
        out.push_str("td + td, th + th { border-left: 1px solid #bbb; }\n");
    }
    out.push_str("</style>\n</head>\n<body>\n");
    if let Some(h) = &doc.header {
        bar_html(&mut out, "bar", [&h.left, &h.center, &h.right]);
    }
    for s in &doc.sections {
        let _ = writeln!(out, "<section>\n<h2>{}</h2>", escape(&s.title));
        for t in &s.tables {
            out.push_str("<table>\n<thead><tr>");
            for c in &t.columns {
                let class = if c.numeric { " class=\"num\"" } else { "" };
                let _ = write!(out, "<th{class}>{}</th>", escape(&c.title));
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            for row in &t.rows {
                row_html(&mut out, t, row);
            }
            out.push_str("</tbody>\n</table>\n");
        }
        out.push_str("</section>\n");
    }
    if let Some(f) = &doc.footer {
        bar_html(&mut out, "bar foot", [&f.left, &f.center, &f.right]);
    }
    out.push_str("</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_colors() {
        assert_eq!(
            escape("<a href='x'>&\"</a>"),
            "&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;"
        );
        assert_eq!(safe_color("#a0B0c0"), Some("#a0B0c0"));
        assert_eq!(safe_color("red;x"), None);
        assert_eq!(safe_color("#12345g"), None);
    }
}
