//! A small, strict XML tokenizer for the SpreadsheetML parts of an .xlsx.
//!
//! It reads elements, attributes and text, and nothing it doesn't need:
//!
//! - **No DTDs.** A `<!DOCTYPE` (or any other `<!` declaration) is an error,
//!   so entities are never declared, let alone fetched from a file or URL.
//!   Only the five predefined entities (`&lt; &gt; &amp; &quot; &apos;`) and
//!   character references (`&#233;`, `&#xE9;`) are expanded; any other
//!   `&name;` is an error.
//! - Comments, processing instructions and the XML declaration are skipped;
//!   CDATA sections are text.
//! - Tags must nest: an end tag that doesn't match the open element is an
//!   error, as is input ending inside an element. Nesting deeper than
//!   [`MAX_DEPTH`] and more than [`MAX_ATTRS`] attributes on one element are
//!   errors, so a hostile part can't use up memory with structure alone.
//! - Element and attribute names are given without their namespace prefix
//!   (`x:row` is `row`): Office files use different prefixes (and the Strict
//!   variant different namespaces) for the same elements.
//! - It is a loop over the input, never recursive.

use std::borrow::Cow;

use super::ReadError;

/// Deepest element nesting accepted.
pub const MAX_DEPTH: usize = 128;
/// Most attributes accepted on one element.
pub const MAX_ATTRS: usize = 64;

/// One thing read from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// A start tag (or an empty element, `empty`), with its attributes'
    /// local names and decoded values.
    Start {
        name: &'a str,
        attrs: Vec<(&'a str, Cow<'a, str>)>,
        empty: bool,
    },
    /// An end tag (also sent right after an empty element's start).
    End { name: &'a str },
    /// Text between tags, entities expanded (CDATA as it is).
    Text(Cow<'a, str>),
}

/// The text of an XML part: UTF-8 (a byte order mark is dropped), or UTF-16
/// with a byte order mark.
pub fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, ReadError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(rest)
            .map(Cow::Borrowed)
            .map_err(|_| ReadError::new("a part of the workbook isn't valid UTF-8 text"));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return super::csvread::decode_utf16(bytes)
            .map(Cow::Owned)
            .ok_or_else(|| ReadError::new("a part of the workbook isn't valid UTF-16 text"));
    }
    std::str::from_utf8(bytes)
        .map(Cow::Borrowed)
        .map_err(|_| ReadError::new("a part of the workbook isn't valid UTF-8 text"))
}

/// Reads events from XML text.
pub struct Reader<'a> {
    src: &'a str,
    pos: usize,
    /// Open elements' qualified names.
    stack: Vec<&'a str>,
    /// An empty element's end, sent on the next call.
    pending_end: Option<&'a str>,
    /// The root element has closed: nothing more is read.
    done: bool,
}

fn err(msg: impl Into<String>) -> ReadError {
    ReadError::new(msg)
}

/// The part of a qualified name after its prefix.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

fn is_name_char(c: char) -> bool {
    !(c.is_whitespace() || matches!(c, '/' | '>' | '=' | '<' | '"' | '\'' | '&'))
}

impl<'a> Reader<'a> {
    pub fn new(src: &'a str) -> Reader<'a> {
        Reader {
            src,
            pos: 0,
            stack: Vec::new(),
            pending_end: None,
            done: false,
        }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    /// Skips past `end`, or fails with `what` when it never comes.
    fn skip_past(&mut self, end: &str, what: &str) -> Result<(), ReadError> {
        match self.rest().find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => Err(err(format!("the workbook's XML has an unfinished {what}"))),
        }
    }

    /// The next event, or `None` once the root element has closed (or the
    /// input held no element at all).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Event<'a>>, ReadError> {
        if let Some(name) = self.pending_end.take() {
            if self.stack.is_empty() {
                self.done = true;
            }
            return Ok(Some(Event::End { name: local(name) }));
        }
        loop {
            if self.done {
                self.check_trailing()?;
                return Ok(None);
            }
            let rest = self.rest();
            if rest.is_empty() {
                return match self.stack.last() {
                    Some(open) => Err(err(format!(
                        "the workbook's XML ends inside <{}>",
                        local(open)
                    ))),
                    None => Ok(None),
                };
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let raw = &rest[..end];
                self.pos += end;
                if self.stack.is_empty() {
                    if raw.trim().is_empty() {
                        continue;
                    }
                    return Err(err("the workbook's XML has text outside its root element"));
                }
                return Ok(Some(Event::Text(unescape(raw)?)));
            }
            if rest.starts_with("<?") {
                self.skip_past("?>", "processing instruction")?;
                continue;
            }
            if rest.starts_with("<!--") {
                self.pos += 4;
                self.skip_past("-->", "comment")?;
                continue;
            }
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = body
                    .find("]]>")
                    .ok_or_else(|| err("the workbook's XML has an unfinished CDATA section"))?;
                if self.stack.is_empty() {
                    return Err(err("the workbook's XML has text outside its root element"));
                }
                self.pos += 9 + end + 3;
                return Ok(Some(Event::Text(Cow::Borrowed(&body[..end]))));
            }
            if rest.starts_with("<!") {
                return Err(err(
                    "the workbook has a document type declaration (DTD), which isn't allowed",
                ));
            }
            if let Some(body) = rest.strip_prefix("</") {
                let end = body
                    .find('>')
                    .ok_or_else(|| err("the workbook's XML has an unfinished end tag"))?;
                let name = body[..end].trim_end();
                self.pos += 2 + end + 1;
                match self.stack.pop() {
                    Some(open) if open == name => {}
                    Some(open) => {
                        return Err(err(format!(
                            "the workbook's XML closes <{}> with </{}>",
                            local(open),
                            local(name)
                        )))
                    }
                    None => {
                        return Err(err("the workbook's XML closes an element it never opened"))
                    }
                }
                if self.stack.is_empty() {
                    self.done = true;
                }
                return Ok(Some(Event::End { name: local(name) }));
            }
            return self.start_tag().map(Some);
        }
    }

    /// After the root element only space, comments and processing
    /// instructions may follow.
    fn check_trailing(&mut self) -> Result<(), ReadError> {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if trimmed.is_empty() {
                return Ok(());
            }
            if trimmed.starts_with("<?") {
                self.skip_past("?>", "processing instruction")?;
            } else if trimmed.starts_with("<!--") {
                self.pos += 4;
                self.skip_past("-->", "comment")?;
            } else {
                return Err(err("the workbook's XML has more after its root element"));
            }
        }
    }

    fn start_tag(&mut self) -> Result<Event<'a>, ReadError> {
        let src = self.src;
        let mut i = self.pos + 1;
        let name_end = src[i..]
            .find(|c: char| !is_name_char(c))
            .map_or(src.len(), |n| i + n);
        let name = &src[i..name_end];
        if name.is_empty() || name.len() > 256 {
            return Err(err("the workbook's XML has a malformed tag"));
        }
        i = name_end;
        let mut attrs = Vec::new();
        let bytes = src.as_bytes();
        loop {
            while i < src.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            match bytes.get(i) {
                None => return Err(err("the workbook's XML has an unfinished tag")),
                Some(b'>') => {
                    i += 1;
                    if self.stack.len() >= MAX_DEPTH {
                        return Err(err("the workbook's XML is nested too deeply"));
                    }
                    self.stack.push(name);
                    self.pos = i;
                    return Ok(Event::Start {
                        name: local(name),
                        attrs,
                        empty: false,
                    });
                }
                Some(b'/') => {
                    if bytes.get(i + 1) != Some(&b'>') {
                        return Err(err("the workbook's XML has a malformed tag"));
                    }
                    self.pos = i + 2;
                    if self.stack.len() >= MAX_DEPTH {
                        return Err(err("the workbook's XML is nested too deeply"));
                    }
                    self.pending_end = Some(name);
                    return Ok(Event::Start {
                        name: local(name),
                        attrs,
                        empty: true,
                    });
                }
                Some(_) => {
                    let an_end = src[i..]
                        .find(|c: char| !is_name_char(c))
                        .map_or(src.len(), |n| i + n);
                    let aname = &src[i..an_end];
                    if aname.is_empty() {
                        return Err(err("the workbook's XML has a malformed attribute"));
                    }
                    i = an_end;
                    while i < src.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if bytes.get(i) != Some(&b'=') {
                        return Err(err("the workbook's XML has an attribute without a value"));
                    }
                    i += 1;
                    while i < src.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let quote = match bytes.get(i) {
                        Some(&q @ (b'"' | b'\'')) => q as char,
                        _ => return Err(err("the workbook's XML has an unquoted attribute")),
                    };
                    i += 1;
                    let vend = src[i..]
                        .find(quote)
                        .map(|n| i + n)
                        .ok_or_else(|| err("the workbook's XML has an unfinished attribute"))?;
                    let raw = &src[i..vend];
                    if raw.contains('<') {
                        return Err(err("the workbook's XML has a '<' inside an attribute"));
                    }
                    if attrs.len() >= MAX_ATTRS {
                        return Err(err(
                            "the workbook's XML has too many attributes on one element",
                        ));
                    }
                    attrs.push((local(aname), unescape(raw)?));
                    i = vend + 1;
                }
            }
        }
    }
}

/// `raw` with entity and character references expanded.
pub fn unescape(raw: &str) -> Result<Cow<'_, str>, ReadError> {
    if !raw.contains('&') {
        return Ok(Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after
            .find(';')
            .filter(|&e| e <= 12)
            .ok_or_else(|| err("the workbook's XML has a '&' that isn't a reference"))?;
        let name = &after[..end];
        let c = match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = name.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    return Err(err(format!(
                        "the workbook's XML uses an undeclared entity &{name};"
                    )));
                };
                code.filter(|&c| c != 0)
                    .and_then(char::from_u32)
                    .ok_or_else(|| err("the workbook's XML has an invalid character reference"))?
            }
        };
        out.push(c);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

/// The value of attribute `name` (local name) in `attrs`.
pub fn attr<'b>(attrs: &'b [(&str, Cow<'_, str>)], name: &str) -> Option<&'b str> {
    attrs
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(src: &str) -> Result<Vec<String>, ReadError> {
        let mut r = Reader::new(src);
        let mut out = Vec::new();
        while let Some(e) = r.next()? {
            out.push(match e {
                Event::Start { name, attrs, empty } => {
                    let a: Vec<String> = attrs.iter().map(|(n, v)| format!("{n}={v}")).collect();
                    format!(
                        "<{name}{}{}>",
                        if a.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", a.join(" "))
                        },
                        if empty { "/" } else { "" }
                    )
                }
                Event::End { name } => format!("</{name}>"),
                Event::Text(t) => format!("'{t}'"),
            });
        }
        Ok(out)
    }

    #[test]
    fn reads_elements_attributes_and_text() {
        let src = "<?xml version=\"1.0\"?>\n<!-- c --><x:a xmlns:x='u' b=\"1 &amp; 2\"><c/>t&lt;&#233;&#x1F3AC;<![CDATA[<raw>]]></x:a>\n<!-- after -->\n";
        assert_eq!(
            events(src).unwrap(),
            [
                "<a x=u b=1 & 2>",
                "<c/>",
                "</c>",
                "'t<\u{e9}\u{1f3ac}'",
                "'<raw>'",
                "</a>",
            ]
        );
    }

    #[test]
    fn refuses_dtds_and_unknown_entities() {
        let dtd = "<!DOCTYPE a [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><a>&x;</a>";
        assert!(events(dtd).unwrap_err().0.contains("DTD"));
        assert!(events("<a>&x;</a>").unwrap_err().0.contains("entity"));
        assert!(events("<a b=\"&x;\"/>").unwrap_err().0.contains("entity"));
        assert!(events("<a>&#0;</a>").is_err());
        assert!(events("<a>&#xD800;</a>").is_err());
        assert!(events("<a>& b</a>").is_err());
    }

    #[test]
    fn malformed_xml_is_an_error() {
        for bad in [
            "<a><b></a></b>",
            "<a>",
            "<a",
            "<a b></a>",
            "<a b=c></a>",
            "<a b=\"c></a>",
            "<a b=\"<\"></a>",
            "</a>",
            "<a><!-- x </a>",
            "<a><![CDATA[x</a>",
            "<a><?pi </a>",
            "text<a/>",
            "<a/><b/>",
            "<a></a><b/>",
            "< a/>",
        ] {
            assert!(events(bad).is_err(), "{bad}");
        }
        // Nothing but a declaration: no events.
        assert!(events("<?xml version=\"1.0\"?>  ").unwrap().is_empty());
    }

    #[test]
    fn limits_hold() {
        let deep = "<a>".repeat(MAX_DEPTH + 1);
        assert!(events(&deep).unwrap_err().0.contains("deeply"));
        let ok = format!("{}{}", "<a>".repeat(MAX_DEPTH), "</a>".repeat(MAX_DEPTH));
        assert!(events(&ok).is_ok());
        let attrs: String = (0..=MAX_ATTRS).map(|i| format!(" a{i}=\"1\"")).collect();
        assert!(events(&format!("<a{attrs}/>"))
            .unwrap_err()
            .0
            .contains("attributes"));
    }

    #[test]
    fn random_input_never_panics() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let alphabet = [
            '<', '>', '/', '!', '?', '&', ';', '#', 'x', 'a', '"', '\'', '=', ' ', '[', ']', '-',
            'C', 'D', ':',
        ];
        for _ in 0..20_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let n = (seed % 40) as usize;
            let mut s = String::from("<r>");
            let mut k = seed;
            for _ in 0..n {
                k = k.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                s.push(alphabet[(k >> 33) as usize % alphabet.len()]);
            }
            let _ = events(&s);
        }
    }

    #[test]
    fn decodes_utf8_and_utf16() {
        assert_eq!(decode(b"\xEF\xBB\xBF<a/>").unwrap(), "<a/>");
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("<a>\u{e9}</a>".encode_utf16().flat_map(|u| u.to_le_bytes()))
            .collect();
        assert_eq!(decode(&utf16).unwrap(), "<a>\u{e9}</a>");
        assert!(decode(b"<a>\xFF</a>").is_err());
    }
}
