//! Formulas bound by id (docs/research/collaboration.md, 3.7): inside the
//! log a formula keeps its text as typed, spacing and all, with each global
//! name replaced by a marker `{g:<uid>}`. Writing it out puts the globals'
//! current names back, so a rename is one write to the global's name and
//! every formula follows.

use super::ids::Uid;
use crate::expr::name_spans;

const OPEN: &str = "{g:";

/// Binds `text`: every name `lookup` knows becomes its marker. Names it
/// doesn't know (a typo, a unit) stay as typed.
pub fn bind(text: &str, lookup: &dyn Fn(&str) -> Option<Uid>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut kept = 0;
    for r in name_spans(text) {
        if let Some(uid) = lookup(&text[r.clone()]) {
            out.push_str(&text[kept..r.start]);
            out.push_str(OPEN);
            out.push_str(&uid.hex());
            out.push('}');
            kept = r.end;
        }
    }
    out.push_str(&text[kept..]);
    out
}

/// Unbinds `bound`: every marker becomes the name `names` gives its id.
/// A marker whose id resolves to nothing is left as it is, which reads as
/// an unknown name (the usual formula error).
pub fn unbind(bound: &str, names: &dyn Fn(Uid) -> Option<String>) -> String {
    let mut out = String::with_capacity(bound.len());
    let mut rest = bound;
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + OPEN.len()..];
        match after.find('}') {
            Some(j) => match Uid::parse(&after[..j]).and_then(names) {
                Some(name) => {
                    out.push_str(&name);
                    rest = &after[j + 1..];
                }
                None => {
                    out.push_str(&rest[i..i + OPEN.len() + j + 1]);
                    rest = &after[j + 1..];
                }
            },
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The ids a bound formula names.
pub fn refs(bound: &str) -> Vec<Uid> {
    let mut out = Vec::new();
    let mut rest = bound;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        match after.find('}') {
            Some(j) => {
                if let Some(uid) = Uid::parse(&after[..j]) {
                    out.push(uid);
                }
                rest = &after[j + 1..];
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_names_and_keeps_spacing() {
        let days = Uid(7);
        let weeks = Uid(9);
        let lookup = |n: &str| match n {
            "DAYS" => Some(days),
            "WEEKS" => Some(weeks),
            _ => None,
        };
        let bound = bind("DAYS * 5 +  WEEKS/2 + 2DAYS + Day", &lookup);
        assert_eq!(
            bound,
            format!(
                "{{g:{}}} * 5 +  {{g:{}}}/2 + 2DAYS + Day",
                days.hex(),
                weeks.hex()
            )
        );
        assert_eq!(refs(&bound), vec![days, weeks]);
        let names = |u: Uid| match u {
            Uid(7) => Some("SHOOT_DAYS".to_string()),
            Uid(9) => Some("WEEKS".to_string()),
            _ => None,
        };
        assert_eq!(
            unbind(&bound, &names),
            "SHOOT_DAYS * 5 +  WEEKS/2 + 2DAYS + Day"
        );
        assert_eq!(bind("", &lookup), "");
        assert_eq!(unbind("1.5 * 2", &names), "1.5 * 2");
    }

    #[test]
    fn unknown_markers_stay() {
        let text = format!("{{g:{}}} + 1", Uid(5).hex());
        assert_eq!(unbind(&text, &|_| None), text);
        assert_eq!(unbind("{g:zz} + 1", &|_| Some("X".into())), "{g:zz} + 1");
        assert_eq!(unbind("{g:", &|_| None), "{g:");
    }
}
