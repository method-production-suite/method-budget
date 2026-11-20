//! A person's signed name and color (decisions.md, entry 103).
//!
//! Every computer checks a `profile` the same way, so the rules are here:
//! a name in [`normal_name`] form, up to [`NAME_MAX_CHARS`] characters, and
//! a color from [`COLORS`].

/// The longest a name may be, in characters.
pub const NAME_MAX_CHARS: usize = 60;

/// The person colors a profile may name (the app's palette).
pub const COLORS: [&str; 8] = [
    "teal", "blue", "purple", "orange", "rose", "green", "amber", "slate",
];

/// Characters a name may not hold, besides control characters: the ones
/// that change the direction text runs in, or that show nothing.
fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180E}'
    )
}

/// `name` as a profile holds it: hidden and control characters left out,
/// every run of white space one space, trimmed, and cut to
/// [`NAME_MAX_CHARS`] characters.
pub fn normal_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !hidden(*c))
        .collect();
    let folded = kept
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    folded.chars().take(NAME_MAX_CHARS).collect()
}

/// Why a profile isn't one every computer takes, or `None` when it is.
pub fn refusal(name: &str, color: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("a profile names its person");
    }
    if normal_name(name) != name {
        return Some("a name is one line of visible characters, up to 60");
    }
    if !COLORS.contains(&color) {
        return Some("a color is one of the person colors");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_folded_to_one_visible_line() {
        assert_eq!(normal_name("  Jordan \t Lee \n"), "Jordan Lee");
        assert_eq!(normal_name("Jor\u{200B}dan\u{202E} Lee"), "Jordan Lee");
        assert_eq!(normal_name(&"x".repeat(80)).chars().count(), NAME_MAX_CHARS);
        assert_eq!(normal_name("Zoë Ångström"), "Zoë Ångström");
    }

    #[test]
    fn only_a_normal_name_and_a_known_color_are_taken() {
        assert_eq!(refusal("Jordan Lee", "blue"), None);
        assert!(refusal("", "blue").is_some());
        assert!(refusal("Jordan  Lee", "blue").is_some());
        assert!(refusal("Jordan\u{200D}Lee", "blue").is_some());
        assert!(refusal("Jordan Lee", "chartreuse").is_some());
    }
}
