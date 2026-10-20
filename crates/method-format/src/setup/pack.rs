//! Setup packs (decisions.md, entry 50; docs/FORMAT.md, "Setup packs"):
//! making one from a budget's picked items, describing one, and importing
//! one with its stamp. A pack is a budget file with setup and no lines whose
//! provenance says `setup_pack` (method_core::pack), so making one is Import
//! Setup into an empty budget (the same dependency walk), and importing one
//! is Import Setup from it (the same review and conflict choices) followed
//! by stamping what came in with the pack's title, version and effective
//! date.

use method_core::pack::{self as core_pack, ItemSource, PackInfo};
use method_core::{folders, Budget, Provenance, ProvenanceKind};
use serde::{Deserialize, Serialize};

use super::import::{self, all_items, Imported, ItemRef, Resolution, What};

/// Longest `created` stamp, in characters.
const MAX_CREATED_CHARS: usize = 64;

/// A pack's details as the page and the command line send and see them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PackDetails {
    pub title: String,
    pub description: String,
    pub author: String,
    pub version: String,
    /// `YYYY-MM-DD`, or empty.
    pub effective: String,
    pub region: String,
    pub notes: String,
    pub urls: Vec<String>,
}

impl From<&PackInfo> for PackDetails {
    fn from(p: &PackInfo) -> PackDetails {
        PackDetails {
            title: p.title.clone(),
            description: p.description.clone(),
            author: p.author.clone(),
            version: p.version.clone(),
            effective: p.effective.clone(),
            region: p.region.clone(),
            notes: p.notes.clone(),
            urls: p.urls.clone(),
        }
    }
}

impl From<&PackDetails> for PackInfo {
    fn from(d: &PackDetails) -> PackInfo {
        PackInfo {
            title: d.title.clone(),
            description: d.description.clone(),
            author: d.author.clone(),
            version: d.version.clone(),
            effective: d.effective.clone(),
            region: d.region.clone(),
            notes: d.notes.clone(),
            urls: d.urls.clone(),
        }
    }
}

/// Whether kind `w` can go in a pack: every kind Import Setup knows but the
/// budget settings, which belong to a production.
pub fn pack_kind(w: What) -> bool {
    !w.is_setting()
}

fn check_created(created: &str) -> Result<String, String> {
    let t = created.trim();
    if t.is_empty() || t.chars().count() > MAX_CREATED_CHARS || t.chars().any(char::is_control) {
        return Err("A setup pack needs the date it was made.".into());
    }
    Ok(t.to_string())
}

/// Makes a setup pack from budget `src`: the `picks` and everything they
/// need (a fringe's globals and unit, a charge's groups, a tag preset's
/// tags, however deep), in the folders `src` keeps them in, with where each
/// came from, stamped with `details`, `created` (an ISO 8601 date or
/// date-time from the caller) and `app_version`. Refused when nothing is
/// picked, a budget setting is picked or the details don't check.
pub fn build(
    src: &Budget,
    picks: &[ItemRef],
    details: &PackDetails,
    created: &str,
    app_version: &str,
) -> Result<(Budget, Imported), String> {
    if picks.is_empty() {
        return Err("Pick at least one setup item for the pack.".into());
    }
    for p in picks {
        if !pack_kind(What::parse(&p.kind)?) {
            return Err(format!(
                "A setup pack can't hold the budget setting {}.",
                p.kind.replace('_', " ")
            ));
        }
    }
    let info = core_pack::check_info(&PackInfo::from(details))?;
    let created = check_created(created)?;
    let mut pack = Budget::new(info.title.clone(), src.base_currency.clone());
    pack.units.clear(); // only what was picked
    let done = import::import(&mut pack, src, picks, &[])?;
    // Folders come along, so a pack keeps the maker's sorting.
    for (w, from, here) in &done.written {
        let kind = w.key();
        if let Some(f) = folders::folder_of(src, kind, from) {
            pack.setup_folders
                .0
                .entry(kind.to_string())
                .or_default()
                .entry(f.to_string())
                .or_default()
                .push(here.clone());
        }
    }
    pack.provenance = Some(Provenance {
        kind: ProvenanceKind::SetupPack,
        source_title: src.title.trim().to_string(),
        source_version: None,
        saved: created,
        app_version: app_version.to_string(),
        note: None,
        pack: Some(info),
    });
    Ok((pack, done))
}

/// Stamps the items an import `written` into `b` with pack `info`: the
/// pack's title and version, and its effective date unless the item has its
/// own. Source notes and links the item came with stay.
pub fn stamp(b: &mut Budget, info: &PackInfo, written: &[(What, String, String)]) {
    for (w, _, here) in written {
        let kind = w.key();
        let mut s: ItemSource = b.setup_sources.get(kind, here).cloned().unwrap_or_default();
        s.pack = info.title.clone();
        s.pack_version = info.version.clone();
        if s.effective.is_empty() {
            s.effective = info.effective.clone();
        }
        b.setup_sources.set(kind, here, s);
    }
}

/// Import Setup from `src` into `b` (method_format::setup::import), then, when
/// `src` is a setup pack, the stamp on everything that came in.
pub fn import_pack(
    b: &mut Budget,
    src: &Budget,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> Result<Imported, String> {
    let done = import::import(b, src, picks, resolutions)?;
    if let Some(info) = core_pack::info(src).cloned() {
        stamp(b, &info, &done.written);
    }
    Ok(done)
}

/// A kind as the command line names it: `fringes`, `fringe`, `tag-presets`,
/// `tag_preset`, `ot-patterns`... (`None`: no such kind).
pub fn parse_kind(s: &str) -> Option<What> {
    let k = s.trim().to_ascii_lowercase().replace('-', "_");
    let k = match k.as_str() {
        "currencies" => "currency".to_string(),
        "line_blocks" | "line_block" | "blocks" => "line_preset".to_string(),
        "overtime" | "overtime_patterns" | "ot" => "ot_pattern".to_string(),
        "rates" | "rate_tables" | "rate" => "rate_table".to_string(),
        _ => k.strip_suffix('s').map(str::to_string).unwrap_or(k),
    };
    What::parse(&k).ok().filter(|w| pack_kind(*w))
}

/// The items of `src` of `kinds` (every pack kind when empty), only those
/// named in `names` when given (`Name`, or `kind:Name` to say which kind).
/// Refused when a name isn't there.
pub fn picks_for(src: &Budget, kinds: &[What], names: &[String]) -> Result<Vec<ItemRef>, String> {
    let kinds: Vec<What> = if kinds.is_empty() {
        What::ALL.into_iter().filter(|w| pack_kind(*w)).collect()
    } else {
        kinds.to_vec()
    };
    let items: Vec<(What, String)> = all_items(src)
        .into_iter()
        .filter(|(w, _)| kinds.contains(w))
        .collect();
    if names.is_empty() {
        return Ok(items.iter().map(import::item_ref).collect());
    }
    let mut out = Vec::new();
    for n in names {
        let (kind, name) = match n.split_once(':') {
            Some((k, rest)) if parse_kind(k).is_some() => (parse_kind(k), rest.trim()),
            _ => (None, n.trim()),
        };
        let found: Vec<&(What, String)> = items
            .iter()
            .filter(|(w, iname)| kind.is_none_or(|k| k == *w) && iname == name)
            .collect();
        if found.is_empty() {
            return Err(format!(
                "no setup item named \u{201c}{name}\u{201d} to put in the pack"
            ));
        }
        out.extend(found.into_iter().map(import::item_ref));
    }
    Ok(out)
}

/// What a pack says about itself, for a review or `method-budget pack import`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PackSummary {
    /// It is a setup pack (else an ordinary budget used as one).
    pub is_pack: bool,
    pub details: Option<PackDetails>,
    /// The budget it was made from.
    pub source_title: String,
    /// When it was made.
    pub created: String,
    /// Where each item came from, in words (items with a source only).
    pub sources: Vec<ItemSourceText>,
}

/// One item's source, in words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemSourceText {
    pub kind: String,
    pub name: String,
    pub text: String,
}

/// The summary of `src`.
pub fn summary(src: &Budget) -> PackSummary {
    let sources = core_pack::normalized(src)
        .0
        .iter()
        .flat_map(|(kind, m)| {
            m.iter().map(move |(name, s)| ItemSourceText {
                kind: kind.clone(),
                name: name.clone(),
                text: core_pack::describe_source(s),
            })
        })
        .collect();
    let prov = src
        .provenance
        .as_ref()
        .filter(|p| p.kind == ProvenanceKind::SetupPack);
    PackSummary {
        is_pack: prov.is_some(),
        details: core_pack::info(src).map(PackDetails::from),
        source_title: prov.map(|p| p.source_title.clone()).unwrap_or_default(),
        created: prov.map(|p| p.saved.clone()).unwrap_or_default(),
        sources,
    }
}

/// The pack's details in lines of text: title, version, effective date and
/// so on, for the command line and the review.
pub fn describe(s: &PackSummary) -> Vec<String> {
    let mut out = Vec::new();
    let Some(d) = &s.details else {
        return out;
    };
    let mut head = d.title.clone();
    if !d.version.is_empty() {
        head.push_str(&format!(" ({})", d.version));
    }
    out.push(head);
    for (label, v) in [
        ("Effective", &d.effective),
        ("Region", &d.region),
        ("Made by", &d.author),
        ("Made", &s.created),
        ("From the budget", &s.source_title),
        ("Description", &d.description),
        ("Sources", &d.notes),
    ] {
        if !v.trim().is_empty() {
            out.push(format!("{label}: {}", v.trim()));
        }
    }
    for u in &d.urls {
        out.push(format!("Link: {u}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{
        starter, Charge, ChargeKind, Dec, Fringe, FringeKind, Group, TagPreset, UnitDef,
    };
    use std::collections::BTreeSet;

    fn r(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn res(kind: &str, name: &str, choice: &str) -> Resolution {
        Resolution {
            kind: kind.into(),
            name: name.into(),
            choice: choice.into(),
        }
    }

    /// A budget with a fringe needing a unit and two globals, a charge
    /// needing a group, and a tag preset needing a fringe and a group.
    fn source() -> Budget {
        let mut b = starter::budget();
        b.title = "Show".into();
        b.globals.insert("DAYS".into(), "20".into());
        b.globals.insert("WEEKS".into(), "DAYS / 5".into());
        b.globals.insert("LONELY".into(), "1".into());
        b.global_info.insert(
            "DAYS".into(),
            method_core::GlobalInfo {
                description: "Shoot days".into(),
                ..Default::default()
            },
        );
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                short_code: None,
                description: "Kit".into(),
                rate: "WEEKS * 10".into(),
                kind: FringeKind::flat(Some("Shift".into())),
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Overtime".into(),
                in_budget_total: true,
                color: None,
            },
        );
        b.charges.push(Charge {
            name: "Fee".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Percent(Dec::from_int(5)),
            excluded_amounts: Vec::new(),
            excluded_groups: ["OT".to_string()].into(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.tag_presets.push(TagPreset {
            name: "Crew".into(),
            fringes: ["KIT".to_string()].into(),
            groups: ["OT".to_string()].into(),
            ..TagPreset::default()
        });
        b.setup_sources.set(
            "fringe",
            "KIT",
            ItemSource {
                notes: "Rental house sheet".into(),
                urls: vec!["https://example.org/kit".into()],
                ..ItemSource::default()
            },
        );
        folders::add_folder(&mut b, "fringe", "Union").unwrap();
        folders::move_items(&mut b, "fringe", &["KIT".into()], Some("Union")).unwrap();
        b
    }

    fn details() -> PackDetails {
        PackDetails {
            title: "Test Rates".into(),
            description: "Made for the tests".into(),
            author: "Payroll desk".into(),
            version: "2026.1".into(),
            effective: "2026-01-01".into(),
            region: "California".into(),
            notes: "Scale sheets".into(),
            urls: vec!["https://example.org/rates".into()],
        }
    }

    #[test]
    fn a_pack_holds_the_picks_and_everything_they_need() {
        let src = source();
        let (pack, done) = build(
            &src,
            &[r("charge", "Fee"), r("tag_preset", "Crew")],
            &details(),
            "2026-10-10T09:00:00Z",
            "0.1.0",
        )
        .unwrap();
        assert!(method_core::pack::is_pack(&pack));
        assert!(pack.topsheet.is_empty(), "no lines");
        assert_eq!(pack.charges.len(), 1);
        assert!(pack.groups.contains_key("OT"), "the charge's group");
        assert!(pack.fringes.contains_key("KIT"), "the preset's fringe");
        assert!(pack.units.contains_key("Shift"), "the fringe's unit");
        assert!(pack.globals.contains_key("WEEKS") && pack.globals.contains_key("DAYS"));
        assert!(
            !pack.globals.contains_key("LONELY"),
            "not picked, not needed"
        );
        assert_eq!(pack.units.len(), 1, "no starter units");
        assert_eq!(pack.global_info["DAYS"].description, "Shoot days");
        assert_eq!(done.added, 7, "{done:?}");
        assert_eq!(folders::folder_of(&pack, "fringe", "KIT"), Some("Union"));
        assert_eq!(
            pack.setup_sources.get("fringe", "KIT").unwrap().notes,
            "Rental house sheet"
        );
        let p = pack.provenance.as_ref().unwrap();
        assert_eq!(p.source_title, "Show");
        assert_eq!(p.saved, "2026-10-10T09:00:00Z");
        assert_eq!(p.pack.as_ref().unwrap().region, "California");

        // It round-trips through the file format exactly.
        let text = crate::to_string(&pack);
        assert!(text.contains("\"setup_pack\""), "{text}");
        assert!(text.contains("\"effective\": \"2026-01-01\""), "{text}");
        assert!(text.contains("\"setup_sources\""), "{text}");
        let back = crate::from_str(&text).unwrap();
        assert_eq!(back, pack);
        assert_eq!(crate::to_string(&back), text);

        // The summary says what it is.
        let s = summary(&back);
        assert!(s.is_pack);
        assert_eq!(s.details.as_ref().unwrap().title, "Test Rates");
        assert_eq!(s.sources.len(), 1);
        assert!(describe(&s).iter().any(|l| l == "Effective: 2026-01-01"));
    }

    #[test]
    fn a_pack_refuses_settings_bad_details_and_nothing() {
        let src = source();
        let d = details();
        assert!(build(&src, &[], &d, "2026-10-10", "x").is_err());
        assert!(build(&src, &[r("fringe_posting", "")], &d, "2026-10-10", "x").is_err());
        assert!(build(&src, &[r("fringe", "NOPE")], &d, "2026-10-10", "x").is_err());
        let bad = PackDetails {
            effective: "soon".into(),
            ..d.clone()
        };
        assert!(build(&src, &[r("fringe", "KIT")], &bad, "2026-10-10", "x").is_err());
        let untitled = PackDetails {
            title: " ".into(),
            ..d.clone()
        };
        assert!(build(&src, &[r("fringe", "KIT")], &untitled, "2026-10-10", "x").is_err());
        assert!(build(&src, &[r("fringe", "KIT")], &d, " ", "x").is_err());
    }

    #[test]
    fn importing_a_pack_stamps_what_came_in_and_resolves_conflicts() {
        let (pack, _) = build(
            &source(),
            &[r("fringe", "KIT"), r("charge", "Fee")],
            &details(),
            "2026-10-10",
            "0.1.0",
        )
        .unwrap();
        // The target has its own DAYS and OT.
        let mut b = starter::budget();
        b.globals.insert("DAYS".into(), "30".into());
        b.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Ours".into(),
                in_budget_total: false,
                color: None,
            },
        );
        let all = picks_for(&pack, &[], &[]).unwrap();
        let mut ours = b.clone();
        let done = import_pack(&mut ours, &pack, &all, &[]).unwrap();
        assert_eq!(done.kept, 2, "DAYS and OT keep this budget's version");
        assert_eq!(ours.globals["DAYS"].0, "30");
        let kit = ours.setup_sources.get("fringe", "KIT").unwrap();
        assert_eq!(kit.pack, "Test Rates");
        assert_eq!(kit.pack_version, "2026.1");
        assert_eq!(kit.effective, "2026-01-01");
        assert_eq!(kit.notes, "Rental house sheet", "the item's own notes stay");
        assert!(
            ours.setup_sources.get("global", "DAYS").is_none(),
            "kept items aren't stamped"
        );

        // Theirs everywhere.
        let mut theirs = b.clone();
        import_pack(
            &mut theirs,
            &pack,
            &all,
            &[
                res("global", "DAYS", "imported"),
                res("group", "OT", "imported"),
            ],
        )
        .unwrap();
        assert_eq!(theirs.globals["DAYS"].0, "20");
        assert!(theirs.groups["OT"].in_budget_total);
        assert_eq!(
            theirs.setup_sources.get("group", "OT").unwrap().pack,
            "Test Rates"
        );

        // A new name: the pack's items use it.
        let mut renamed = b.clone();
        import_pack(&mut renamed, &pack, &all, &[res("group", "OT", "rename")]).unwrap();
        let fee = renamed.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert!(fee.excluded_groups.contains("OT (2)"));
        assert_eq!(
            renamed.setup_sources.get("group", "OT (2)").unwrap().pack,
            "Test Rates"
        );
        assert!(renamed.setup_sources.get("group", "OT").is_none());

        // An ordinary budget used as a pack stamps nothing.
        let mut plain = starter::budget();
        import_pack(&mut plain, &source(), &[r("fringe", "KIT")], &[]).unwrap();
        assert_eq!(
            plain.setup_sources.get("fringe", "KIT").unwrap().pack,
            "",
            "the item's own source comes along, unstamped"
        );
    }

    #[test]
    fn kinds_and_names_as_the_command_line_gives_them() {
        assert_eq!(parse_kind("fringes"), Some(What::Fringe));
        assert_eq!(parse_kind("Currencies"), Some(What::Currency));
        assert_eq!(parse_kind("tag-presets"), Some(What::TagPreset));
        assert_eq!(parse_kind("ot-patterns"), Some(What::OtPattern));
        assert_eq!(parse_kind("line-blocks"), Some(What::LinePreset));
        assert_eq!(parse_kind("fringe-posting"), None, "settings never");
        assert_eq!(parse_kind("nope"), None);
        let src = source();
        let fringes = picks_for(&src, &[What::Fringe], &[]).unwrap();
        assert!(fringes.iter().all(|p| p.kind == "fringe"));
        let one = picks_for(&src, &[], &["global:DAYS".into()]).unwrap();
        assert_eq!(one, [r("global", "DAYS")]);
        assert!(picks_for(&src, &[What::Fringe], &["DAYS".into()]).is_err());
    }
}
