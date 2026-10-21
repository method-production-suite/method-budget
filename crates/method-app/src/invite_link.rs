//! Invite links (decisions.md, entry 97; docs/APP.md, "Inviting someone").
//!
//! A link is plain text the inviter copies and sends any way they like;
//! nothing is sent anywhere by the app, and nothing on the network reads it:
//!
//! ```text
//! methodbudget:join?v=1&b=<budget id>&i=<invite id>&s=<secret>&f=<file name>
//! ```
//!
//! - `v`: the link's version, 1. A newer one is refused in words.
//! - `b`: the shared budget's id (64 hex characters, `budget.json`'s
//!   `budget_id`), so the app can tell the right budget from another.
//! - `i`: the invite's id (32 hex characters, as the log writes it).
//! - `s`: the invite's 32-byte secret, base64url without padding (43
//!   characters). Only the link has it.
//! - `f`: the budget's file name, percent-encoded, so the person knows which
//!   `.mthd` to pick in their copy of the shared folder.
//!
//! File > Join from Link… reads one pasted on its own or inside a message
//! (the first `methodbudget:` word in the text), and `methodbudget://join?…`
//! too, so the later clickable links read the same.

use method_core::log::Uid;

/// What a link starts with.
pub const SCHEME: &str = "methodbudget:";
/// The version of the links this build writes and reads.
pub const LINK_VERSION: u32 = 1;

/// What an invite link carries.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteLink {
    pub budget: String,
    pub invite: Uid,
    pub secret: [u8; 32],
    pub file: String,
}

impl std::fmt::Debug for InviteLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secret.
        f.debug_struct("InviteLink")
            .field("budget", &self.budget)
            .field("invite", &self.invite)
            .field("file", &self.file)
            .finish()
    }
}

/// Not a link at all.
pub const NOT_A_LINK: &str = "That isn\u{2019}t a Method Budget invite link. It starts with \u{201c}methodbudget:join\u{201d}; paste the whole link you were sent.";
/// A link missing a part (cut short when it was copied).
pub const INCOMPLETE: &str =
    "This invite link is incomplete, perhaps cut short when it was copied. Ask for it again.";
/// A link from a newer version.
pub const NEWER: &str = "This invite link was made by a newer version of Method Budget. Update Method Budget, then join from the link again.";

impl InviteLink {
    /// The link's text.
    pub fn to_text(&self) -> String {
        format!(
            "{SCHEME}join?v={LINK_VERSION}&b={}&i={}&s={}&f={}",
            self.budget,
            self.invite.hex(),
            base64url(&self.secret),
            percent_encode(&self.file)
        )
    }

    /// Reads the first invite link in `text` (a link alone, or a message
    /// holding one). The error is a sentence for the person.
    pub fn parse(text: &str) -> Result<InviteLink, String> {
        let lower = text.to_ascii_lowercase();
        let at = lower.find(SCHEME).ok_or(NOT_A_LINK)?;
        let word: String = text[at..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '<' | '>' | '"' | '\u{201d}'))
            .collect();
        let rest = &word[SCHEME.len()..];
        let rest = rest.strip_prefix("//").unwrap_or(rest);
        let query = rest
            .strip_prefix("join?")
            .or_else(|| rest.strip_prefix("join/?"))
            .ok_or(NOT_A_LINK)?;
        let mut v = None;
        let (mut b, mut i, mut s, mut f) = (None, None, None, None);
        for pair in query.split('&') {
            let Some((k, val)) = pair.split_once('=') else {
                continue;
            };
            match k {
                "v" => v = val.parse::<u32>().ok(),
                "b" => b = Some(val.to_ascii_lowercase()),
                "i" => i = Some(val.to_string()),
                "s" => s = Some(val.to_string()),
                "f" => f = percent_decode(val),
                _ => {}
            }
        }
        match v {
            Some(LINK_VERSION) => {}
            Some(n) if n > LINK_VERSION => return Err(NEWER.into()),
            _ => return Err(INCOMPLETE.into()),
        }
        let budget = b
            .filter(|b| b.len() == 64 && b.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or(INCOMPLETE)?;
        let invite = i.and_then(|i| Uid::parse(&i)).ok_or(INCOMPLETE)?;
        let secret = s
            .and_then(|s| unbase64url(&s))
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or(INCOMPLETE)?;
        let file = f.filter(|f| !f.trim().is_empty()).ok_or(INCOMPLETE)?;
        // A file name, never a path.
        if file.contains('/') || file.contains('\\') || file == "." || file == ".." {
            return Err(INCOMPLETE.into());
        }
        Ok(InviteLink {
            budget,
            invite,
            secret,
            file,
        })
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding (RFC 4648, section 5).
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// Reads base64url without padding (padding tolerated). `None` for anything
/// else.
pub fn unbase64url(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = B64.iter().position(|x| *x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // Leftover bits must be zero, so one text means one value.
    (acc == 0).then_some(out)
}

/// Percent-encodes everything but letters, digits and `-._~` (as UTF-8).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Reads percent-encoding (`+` stays a plus). `None` when it isn't UTF-8 or
/// a `%` isn't followed by two hex digits.
pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> InviteLink {
        InviteLink {
            budget: "ab".repeat(32),
            invite: Uid(0x0123_4567_89ab_cdef_0011_2233_4455_6677),
            secret: core::array::from_fn(|i| (i * 7 + 3) as u8),
            file: "Feature Film (v2) é.mthd".into(),
        }
    }

    #[test]
    fn a_link_round_trips_and_reads_inside_a_message() {
        let l = link();
        let text = l.to_text();
        assert!(text.starts_with("methodbudget:join?v=1&b="), "{text}");
        assert!(!text.contains(' '));
        assert_eq!(InviteLink::parse(&text).unwrap(), l);
        let message = format!("Hi Riley,\n\nhere is the budget: <{text}>\nThanks");
        assert_eq!(InviteLink::parse(&message).unwrap(), l);
        let deep = text.replacen("methodbudget:", "methodbudget://", 1);
        assert_eq!(InviteLink::parse(&deep).unwrap(), l);
        // The secret never shows in Debug.
        assert!(!format!("{l:?}").contains(&base64url(&l.secret)));
    }

    #[test]
    fn broken_links_are_refused_in_words() {
        let text = link().to_text();
        assert_eq!(InviteLink::parse("hello").unwrap_err(), NOT_A_LINK);
        assert_eq!(
            InviteLink::parse("https://example.com/join").unwrap_err(),
            NOT_A_LINK
        );
        let cut = &text[..text.len() - 20];
        assert_eq!(InviteLink::parse(cut).unwrap_err(), INCOMPLETE);
        let newer = text.replace("v=1", "v=2");
        assert_eq!(InviteLink::parse(&newer).unwrap_err(), NEWER);
        let short_secret = text.replace("&s=", "&s=AA");
        assert_eq!(InviteLink::parse(&short_secret).unwrap_err(), INCOMPLETE);
        let path = text.replace("&f=", "&f=..%2F");
        assert_eq!(InviteLink::parse(&path).unwrap_err(), INCOMPLETE);
        for words in [NOT_A_LINK, INCOMPLETE, NEWER] {
            assert!(!words.contains('\u{2014}') && !words.contains('!'));
        }
    }

    #[test]
    fn base64url_matches_the_rfc_vectors() {
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64url(plain.as_bytes()), enc);
            assert_eq!(unbase64url(enc).unwrap(), plain.as_bytes());
        }
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(unbase64url("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(base64url(&[0u8; 32]).len(), 43);
        assert!(unbase64url("Zh").is_none(), "stray bits");
        assert!(unbase64url("Z").is_none());
        assert!(unbase64url("Zm9v!").is_none());
    }
}
