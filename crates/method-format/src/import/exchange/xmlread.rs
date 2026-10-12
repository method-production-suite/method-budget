//! The budget exchange XML layouts, basic and advanced.
//!
//! Both are one `<budget>` element (in a namespace) holding `<metadata>`,
//! `<categories>`, `<accounts>` and `<details>`, with one element per field,
//! named as the JSON export names its keys (`<cNumber>`, `<aTotal>`,
//! `<dSubtotal>`...). Lists are an element per item: `<category>`,
//! `<account>`, `<detail>`; `<fringeDetail>` holds `<pctFringe>` and
//! `<flatFringe>` elements; `<dFringes>`, `<dGroups>` and `<dGlobalsUsed>`
//! hold `<dFringe>`, `<dGroup>` and `<dGlobalUsed>`.
//!
//! - **Advanced** carries what the JSON export does, figures to the cent.
//! - **Basic** has no metadata (no currencies, no title), leaves the
//!   fringes, groups and globals off the detail rows, and gives every figure
//!   in whole units.
//!
//! The XML is read with our own tokenizer (no DTDs, no external entities)
//! into the same tree the JSON reader takes, so both layouts go through one
//! reader of the layout.

use super::json::{from_tree, J};
use super::model::{Export, ExportFormat};
use crate::import::xml::{decode, Event, Reader};
use crate::import::ReadError;

/// Elements whose children are list items.
const LISTS: [&str; 8] = [
    "categories",
    "accounts",
    "details",
    "fringeDetail",
    "dFringes",
    "dGroups",
    "dGlobalsUsed",
    "customfields",
];

/// Lists of plain text items.
const TEXT_LISTS: [&str; 3] = ["dFringes", "dGroups", "dGlobalsUsed"];

/// One element as read.
#[derive(Default)]
struct Node {
    name: String,
    text: String,
    children: Vec<Node>,
}

/// The tree of elements in `src`.
fn tree(src: &str) -> Result<Node, ReadError> {
    let wrap = |e: ReadError| {
        ReadError::new(format!(
            "the file isn't valid XML ({})",
            e.0.replace("the workbook's XML", "it")
        ))
    };
    let mut r = Reader::new(src);
    let mut stack: Vec<Node> = vec![Node::default()];
    while let Some(ev) = r.next().map_err(wrap)? {
        match ev {
            Event::Start { name, .. } => stack.push(Node {
                name: name.to_string(),
                ..Node::default()
            }),
            Event::End { .. } => {
                let done = stack
                    .pop()
                    .ok_or_else(|| ReadError::new("unbalanced XML"))?;
                stack
                    .last_mut()
                    .ok_or_else(|| ReadError::new("unbalanced XML"))?
                    .children
                    .push(done);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t);
                }
            }
        }
    }
    let mut top = stack.pop().unwrap_or_default();
    top.children
        .pop()
        .ok_or_else(|| ReadError::new("the file holds no XML element"))
}

fn value(n: &Node) -> J {
    if LISTS.contains(&n.name.as_str()) {
        let text_items = TEXT_LISTS.contains(&n.name.as_str());
        return J::Arr(
            n.children
                .iter()
                .map(|c| {
                    if text_items {
                        J::Str(c.text.trim().to_string())
                    } else if n.name == "fringeDetail" {
                        // The element's name is the fringe's type.
                        let mut fields = vec![("type".to_string(), J::Str(c.name.clone()))];
                        if let J::Obj(rest) = value(c) {
                            fields.extend(rest);
                        }
                        J::Obj(fields)
                    } else {
                        value(c)
                    }
                })
                .collect(),
        );
    }
    if n.children.is_empty() {
        return J::Str(n.text.trim().to_string());
    }
    J::Obj(
        n.children
            .iter()
            .map(|c| (c.name.clone(), value(c)))
            .collect(),
    )
}

/// Whether `bytes` look like a budget exchange XML file, and which layout:
/// `Some(true)` for advanced (it has budget data or fringes on its lines),
/// `Some(false)` for basic.
pub fn looks_like(bytes: &[u8]) -> Option<bool> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let t = head.trim_start_matches('\u{feff}').trim_start();
    if !(t.starts_with("<?xml") || t.starts_with("<budget")) || !head.contains("<budget") {
        return None;
    }
    let all = String::from_utf8_lossy(bytes);
    Some(all.contains("<budgetdata") || all.contains("<dFringes"))
}

/// Reads a budget exchange XML file, in the layout `format` names (basic
/// or advanced).
pub fn read(bytes: &[u8], format: ExportFormat) -> Result<Export, ReadError> {
    let src = decode(bytes)?;
    let root = tree(&src)?;
    if root.name != "budget" {
        return Err(ReadError::new(
            "this XML file isn't a budget export (its root isn't <budget>)",
        ));
    }
    let places = if format == ExportFormat::XmlBasic {
        0
    } else {
        2
    };
    from_tree(&value(&root), format, places)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADV: &str = r#"<?xml version="1.0"?>
<budget xmlns="urn:x"><metadata><budgetdata><BaseCurrency>CAD</BaseCurrency></budgetdata><defaultfields><Title>A &amp; B</Title></defaultfields><customfields></customfields></metadata>
<categories><category><cID>0</cID><cNumber>1100</cNumber><cDescription>Story</cDescription><cFringe>10.00</cFringe><cOriginal>0.00</cOriginal><cTotal>110.00</cTotal></category>
<category><cID>1</cID><cNumber></cNumber><cDescription>Fee</cDescription><cFringe></cFringe><cOriginal>0.00</cOriginal><cTotal>5.00</cTotal></category></categories>
<accounts><account><categoryID>0</categoryID><aID>0</aID><aNumber>1101</aNumber><aDescription>Rights</aDescription><aFringe>10.00</aFringe><aOriginal>0.00</aOriginal><aTotal>100.00</aTotal></account>
<account><categoryID>0</categoryID><aID>1</aID><aNumber>1199</aNumber><aDescription>Total Fringes</aDescription><aFringe></aFringe><aOriginal>0.00</aOriginal><aTotal>10.00</aTotal><fringeDetail><pctFringe><name>P10</name><rate>10</rate><amount>100</amount><total>10</total></pctFringe></fringeDetail></account></accounts>
<details><detail><accountID>0</accountID><dAggPercent>10%</dAggPercent><dAggSumPercent>10%</dAggSumPercent><dFringes><dFringe>P10</dFringe></dFringes><dGroups><dGroup>G</dGroup></dGroups><dGlobalsUsed></dGlobalsUsed><dDescription>Option</dDescription><dAmount>1</dAmount><dRate>100</dRate><dSubtotal>100.00</dSubtotal></detail></details></budget>"#;

    #[test]
    fn advanced_xml_reads_as_the_json_does() {
        assert_eq!(looks_like(ADV.as_bytes()), Some(true));
        let x = read(ADV.as_bytes(), ExportFormat::XmlAdvanced).unwrap();
        assert_eq!(x.base_currency, "CAD");
        assert_eq!(x.name, "A & B");
        assert!(x.line_fringes && x.line_groups && x.fringe_figures);
        assert_eq!(x.places, 2);
        let super::super::model::TopRow::Category(c) = &x.rows[0] else {
            panic!("{:?}", x.rows[0])
        };
        assert_eq!(c.accounts.len(), 2);
        assert_eq!(c.accounts[0].details[0].fringes, vec!["P10"]);
        assert_eq!(c.accounts[0].details[0].groups, vec!["G"]);
        let f = c.accounts[1].fringe_account.as_ref().unwrap();
        assert!(f[0].percent && f[0].rate == "10");
        assert!(matches!(x.rows[1], super::super::model::TopRow::Charge(_)));
    }

    #[test]
    fn hostile_or_foreign_xml_is_refused() {
        assert_eq!(looks_like(b"<other/>"), None);
        assert!(read(b"<other/>", ExportFormat::XmlBasic).is_err());
        let dtd = "<?xml version=\"1.0\"?><!DOCTYPE budget [<!ENTITY x \"y\">]><budget/>";
        assert!(read(dtd.as_bytes(), ExportFormat::XmlBasic)
            .unwrap_err()
            .0
            .contains("XML"));
        assert!(read(b"<budget><categories>", ExportFormat::XmlBasic).is_err());
    }
}
