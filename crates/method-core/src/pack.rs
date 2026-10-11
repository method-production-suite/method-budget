//! Setup packs and where setup items came from (docs/FORMAT.md, "Setup
//! packs"; decisions.md, entry 50).
//!
//! - **A setup pack is a budget file** with setup and no lines, stamped with
//!   a [`crate::Provenance`] of kind [`crate::ProvenanceKind::SetupPack`]
//!   whose `pack` holds the pack's own details ([`PackInfo`]): a title, a
//!   description, who made it (free text), a version, the date its rates
//!   take effect, the region or jurisdiction they apply to, and source notes
//!   and links. It opens in the application like any budget.
//! - **Where an item came from** is [`SetupSources`]: per kind and item name,
//!   an [`ItemSource`] (the pack it came from, that pack's version and
//!   effective date, and source notes and links). A pack carries one for each
//!   item its maker documented; importing a pack stamps every item it brings
//!   with the pack's title, version and effective date, so a budget can show
//!   where a rate came from. About the items, not content: versions don't
//!   carry it, no amount depends on it, and a writer keeps only entries for
//!   items that exist ([`normalized`]).
//! - **Links are text.** A source link must be `http://` or `https://` and is
//!   shown, never fetched or opened by the application.

use std::collections::BTreeMap;

use crate::model::Budget;

/// Longest title, author, version, effective date or region, in characters.
pub const MAX_PACK_FIELD_CHARS: usize = 200;
/// Longest description or source notes, in characters.
pub const MAX_PACK_TEXT_CHARS: usize = 4_000;
/// Most source links on a pack or an item.
pub const MAX_SOURCE_URLS: usize = 20;
/// Longest source link, in characters.
pub const MAX_URL_CHARS: usize = 2_000;

/// A setup pack's own details. Every field is free text but the effective
/// date (`YYYY-MM-DD` or empty) and the links (`http(s)://`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackInfo {
    pub title: String,
    pub description: String,
    /// Who made it: free text (a name, a union local, a payroll house).
    pub author: String,
    /// The pack's own version, as its maker writes it (`"2026.1"`).
    pub version: String,
    /// When its rates take effect, `YYYY-MM-DD`, or empty.
    pub effective: String,
    /// The region or jurisdiction its rates apply to, as text.
    pub region: String,
    /// Where the pack's figures come from, in words.
    pub notes: String,
    /// Links to the sources (shown as text, never opened by the app).
    pub urls: Vec<String>,
}

/// Where one setup item came from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemSource {
    /// The title of the setup pack it was imported from, or empty.
    pub pack: String,
    /// That pack's version, or empty.
    pub pack_version: String,
    /// When its figure takes effect (`YYYY-MM-DD`), or empty.
    pub effective: String,
    /// Where its figure comes from, in words.
    pub notes: String,
    /// Links to its sources (text only).
    pub urls: Vec<String>,
}

impl ItemSource {
    /// True when it says nothing.
    pub fn is_empty(&self) -> bool {
        *self == ItemSource::default()
    }
}

/// Where setup items came from: kind (`global`, `fringe`, ... as in
/// [`crate::folders::KINDS`]) -> item name -> its source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupSources(pub BTreeMap<String, BTreeMap<String, ItemSource>>);

impl SetupSources {
    /// True when no item has a source.
    pub fn is_empty(&self) -> bool {
        self.0
            .values()
            .all(|m| m.values().all(ItemSource::is_empty))
    }

    /// The source of item `name` of `kind`, if it has one.
    pub fn get(&self, kind: &str, name: &str) -> Option<&ItemSource> {
        self.0
            .get(kind)
            .and_then(|m| m.get(name))
            .filter(|s| !s.is_empty())
    }

    /// Sets (or, with an empty source, removes) item `name`'s source.
    pub fn set(&mut self, kind: &str, name: &str, source: ItemSource) {
        if source.is_empty() {
            self.remove(kind, name);
            return;
        }
        self.0
            .entry(kind.to_string())
            .or_default()
            .insert(name.to_string(), source);
    }

    /// Removes item `name`'s source.
    pub fn remove(&mut self, kind: &str, name: &str) {
        if let Some(m) = self.0.get_mut(kind) {
            m.remove(name);
            if m.is_empty() {
                self.0.remove(kind);
            }
        }
    }
}

/// Whether `b` is a setup pack (its provenance says so).
pub fn is_pack(b: &Budget) -> bool {
    b.provenance
        .as_ref()
        .is_some_and(|p| p.kind == crate::ProvenanceKind::SetupPack)
}

/// The pack details of `b`, when it is a setup pack.
pub fn info(b: &Budget) -> Option<&PackInfo> {
    b.provenance
        .as_ref()
        .filter(|p| p.kind == crate::ProvenanceKind::SetupPack)
        .and_then(|p| p.pack.as_ref())
}

/// `b`'s sources, keeping only entries for items that exist and say
/// something: what a writer writes.
pub fn normalized(b: &Budget) -> SetupSources {
    let mut out = BTreeMap::new();
    for (kind, items) in &b.setup_sources.0 {
        let kept: BTreeMap<String, ItemSource> = items
            .iter()
            .filter(|(n, s)| !s.is_empty() && crate::folders::exists(b, kind, n))
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        if !kept.is_empty() {
            out.insert(kind.clone(), kept);
        }
    }
    SetupSources(out)
}

/// Item `from` of `kind` is now called `to`: its source follows it.
pub fn rename_item(b: &mut Budget, kind: &str, from: &str, to: &str) {
    if let Some(m) = b.setup_sources.0.get_mut(kind) {
        if let Some(s) = m.remove(from) {
            m.insert(to.to_string(), s);
        }
    }
}

fn field(what: &str, s: &str, most: usize) -> Result<String, String> {
    let t = s.trim();
    if t.chars().count() > most {
        return Err(format!("The {what} can be at most {most} characters."));
    }
    if t.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(format!("The {what} has characters that can't be kept."));
    }
    Ok(t.to_string())
}

fn one_line(what: &str, s: &str) -> Result<String, String> {
    let t = field(what, s, MAX_PACK_FIELD_CHARS)?;
    if t.contains('\n') || t.contains('\t') {
        return Err(format!("The {what} must be one line."));
    }
    Ok(t)
}

/// An effective date: empty, or a real `YYYY-MM-DD` date.
pub fn check_date(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    let bad = || {
        format!("The effective date must be a date written YYYY-MM-DD, not \u{201c}{t}\u{201d}.")
    };
    let b = t.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| t[r].parse::<u32>().map_err(|_| bad());
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(bad()),
    };
    if y < 1900 || d == 0 || d > days {
        return Err(bad());
    }
    Ok(t.to_string())
}

/// Source links: trimmed, blanks dropped, each `http://` or `https://`
/// with no spaces, at most [`MAX_SOURCE_URLS`].
pub fn check_urls(urls: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for u in urls {
        let t = u.trim();
        if t.is_empty() {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        let scheme = lower.starts_with("https://") || lower.starts_with("http://");
        if !scheme || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(format!(
                "A source link must start with https:// or http:// and have no spaces: \u{201c}{t}\u{201d}."
            ));
        }
        if t.chars().count() > MAX_URL_CHARS {
            return Err(format!(
                "A source link can be at most {MAX_URL_CHARS} characters."
            ));
        }
        out.push(t.to_string());
    }
    if out.len() > MAX_SOURCE_URLS {
        return Err(format!("At most {MAX_SOURCE_URLS} source links."));
    }
    Ok(out)
}

/// `info` checked and tidied (trimmed); refused with a sentence when a
/// field is too long, the date isn't a date or a link isn't a web link. A
/// pack needs a title.
pub fn check_info(info: &PackInfo) -> Result<PackInfo, String> {
    let title = one_line("title", &info.title)?;
    if title.is_empty() {
        return Err("A setup pack needs a title.".into());
    }
    Ok(PackInfo {
        title,
        description: field("description", &info.description, MAX_PACK_TEXT_CHARS)?,
        author: one_line("author", &info.author)?,
        version: one_line("version", &info.version)?,
        effective: check_date(&info.effective)?,
        region: one_line("region", &info.region)?,
        notes: field("source notes", &info.notes, MAX_PACK_TEXT_CHARS)?,
        urls: check_urls(&info.urls)?,
    })
}

/// `s` checked and tidied, as [`check_info`] does.
pub fn check_source(s: &ItemSource) -> Result<ItemSource, String> {
    Ok(ItemSource {
        pack: one_line("pack title", &s.pack)?,
        pack_version: one_line("pack version", &s.pack_version)?,
        effective: check_date(&s.effective)?,
        notes: field("source notes", &s.notes, MAX_PACK_TEXT_CHARS)?,
        urls: check_urls(&s.urls)?,
    })
}

/// Where an item came from, in words: "From CA Rates (2026.1), effective
/// 2026-01-01. Union scale sheet. https://...". Empty for an empty source.
pub fn describe_source(s: &ItemSource) -> String {
    let mut parts = Vec::new();
    if !s.pack.is_empty() {
        let mut p = format!("From {}", s.pack);
        if !s.pack_version.is_empty() {
            p.push_str(&format!(" ({})", s.pack_version));
        }
        if !s.effective.is_empty() {
            p.push_str(&format!(", effective {}", s.effective));
        }
        parts.push(p);
    } else if !s.effective.is_empty() {
        parts.push(format!("Effective {}", s.effective));
    }
    if !s.notes.trim().is_empty() {
        parts.push(s.notes.trim().to_string());
    }
    parts.extend(s.urls.iter().cloned());
    parts.join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_is_checked_and_tidied() {
        let ok = check_info(&PackInfo {
            title: "  Rates  ".into(),
            effective: "2026-02-29".into(),
            urls: vec![" https://example.org/a ".into(), "".into()],
            ..PackInfo::default()
        });
        assert!(ok.is_err(), "2026 is not a leap year");
        let ok = check_info(&PackInfo {
            title: "  Rates  ".into(),
            effective: "2028-02-29".into(),
            urls: vec![" https://example.org/a ".into(), "".into()],
            ..PackInfo::default()
        })
        .unwrap();
        assert_eq!(ok.title, "Rates");
        assert_eq!(ok.urls, ["https://example.org/a"]);
        assert!(check_info(&PackInfo::default()).is_err(), "needs a title");
        for bad in [
            "javascript:alert(1)",
            "file:///etc",
            "https://a b",
            "ftp://x",
        ] {
            assert!(check_urls(&[bad.into()]).is_err(), "{bad}");
        }
        for bad in ["2026-13-01", "2026-1-01", "yesterday", "1800-01-01"] {
            assert!(check_date(bad).is_err(), "{bad}");
        }
        assert!(check_info(&PackInfo {
            title: "a\nb".into(),
            ..PackInfo::default()
        })
        .is_err());
        assert!(check_info(&PackInfo {
            title: "x".repeat(MAX_PACK_FIELD_CHARS + 1),
            ..PackInfo::default()
        })
        .is_err());
    }

    #[test]
    fn sources_follow_renames_and_keep_only_items_that_exist() {
        let mut b = Budget::new("Pack", "USD");
        b.globals.insert("RATE".into(), "7".into());
        let s = ItemSource {
            notes: "Scale sheet".into(),
            ..ItemSource::default()
        };
        b.setup_sources.set("global", "RATE", s.clone());
        b.setup_sources.set("global", "GONE", s.clone());
        b.setup_sources
            .set("global", "EMPTY", ItemSource::default());
        let n = normalized(&b);
        assert_eq!(n.0["global"].keys().collect::<Vec<_>>(), ["RATE"]);
        rename_item(&mut b, "global", "RATE", "RATE2");
        assert_eq!(b.setup_sources.get("global", "RATE2"), Some(&s));
        b.setup_sources.remove("global", "RATE2");
        b.setup_sources.remove("global", "GONE");
        assert!(b.setup_sources.is_empty());
        assert_eq!(
            describe_source(&ItemSource {
                pack: "CA".into(),
                pack_version: "2026.1".into(),
                effective: "2026-01-01".into(),
                notes: "Scale".into(),
                urls: vec!["https://example.org".into()],
            }),
            "From CA (2026.1), effective 2026-01-01. Scale. https://example.org"
        );
    }
}
