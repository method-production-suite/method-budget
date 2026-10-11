//! Folders of setup items, per kind (roadmap 2.15, the personal setup
//! library).
//!
//! A budget can sort its setup items into named folders, one set of folders
//! per kind of item: `setup_folders` maps a kind (`global`, `fringe`,
//! `group`, `unit`, `set`, `location`, `currency`, `charge`, `credit`,
//! `tag_preset`, `line_preset`, `report_preset`, `ot_pattern`) to its folders, and each
//! folder to the names of the items in it. An item is in at most one folder
//! of its kind; an item in none is unfiled. A folder can be empty.
//!
//! The library (a budget file used as a library) is what shows folders
//! today; any budget may carry them. Layout, not content, like the setup
//! order: versions don't carry them, and no amount depends on them. A writer
//! keeps only names that exist, each once ([`normalized`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::Budget;
use crate::rename::SetupKind;

/// The kinds that can have folders, in the order the library lists them.
pub const KINDS: [&str; 13] = [
    "global",
    "fringe",
    "group",
    "unit",
    "set",
    "location",
    "currency",
    "charge",
    "credit",
    "tag_preset",
    "line_preset",
    "report_preset",
    "ot_pattern",
];

/// Longest folder name, in characters.
pub const MAX_FOLDER_NAME_CHARS: usize = 60;

/// Folders per kind: kind -> folder name -> item names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupFolders(pub BTreeMap<String, BTreeMap<String, Vec<String>>>);

impl SetupFolders {
    /// True when no kind has a folder.
    pub fn is_empty(&self) -> bool {
        self.0.values().all(|f| f.is_empty())
    }

    /// `kind`'s folders (empty when it has none).
    pub fn of(&self, kind: &str) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.0.get(kind).into_iter().flatten()
    }
}

/// Whether `kind` is one that can have folders.
pub fn is_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// The folder kind of a setup table.
pub fn kind_of(k: SetupKind) -> &'static str {
    match k {
        SetupKind::Global => "global",
        SetupKind::Fringe => "fringe",
        SetupKind::Group => "group",
        SetupKind::Unit => "unit",
        SetupKind::Set => "set",
        SetupKind::Location => "location",
        SetupKind::Currency => "currency",
    }
}

/// Whether `b` has an item of `kind` named exactly `name`.
pub fn exists(b: &Budget, kind: &str, name: &str) -> bool {
    match kind {
        "global" => b.globals.contains_key(name),
        "fringe" => b.fringes.contains_key(name),
        "group" => b.groups.contains_key(name),
        "unit" => b.units.contains_key(name),
        "set" => b.sets.contains_key(name),
        "location" => b.locations.contains_key(name),
        "currency" => b.currencies.contains_key(name),
        "charge" => b.charges.iter().any(|c| c.name == name),
        "credit" => b.credits.iter().any(|c| c.name == name),
        "tag_preset" => b.tag_presets.iter().any(|p| p.name == name),
        "line_preset" => b.line_presets.iter().any(|p| p.name == name),
        "report_preset" => b.report_presets.iter().any(|p| p.name == name),
        "ot_pattern" => b.ot_patterns.iter().any(|p| p.name == name),
        _ => false,
    }
}

/// The folders a file keeps: known kinds only, names that exist, each in
/// its first folder only; empty folders stay, kinds with none are dropped.
pub fn normalized(b: &Budget) -> SetupFolders {
    let mut out = BTreeMap::new();
    for (kind, folders) in &b.setup_folders.0 {
        if !is_kind(kind) || folders.is_empty() {
            continue;
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut kept = BTreeMap::new();
        for (folder, names) in folders {
            if valid_folder_name(folder).is_err() {
                continue;
            }
            let names: Vec<String> = names
                .iter()
                .filter(|n| exists(b, kind, n) && seen.insert(n.as_str()))
                .cloned()
                .collect();
            kept.insert(folder.clone(), names);
        }
        if !kept.is_empty() {
            out.insert(kind.clone(), kept);
        }
    }
    SetupFolders(out)
}

/// The folder item `name` of `kind` is in, if any.
pub fn folder_of<'a>(b: &'a Budget, kind: &str, name: &str) -> Option<&'a str> {
    b.setup_folders
        .of(kind)
        .find(|(_, names)| names.iter().any(|n| n == name))
        .map(|(f, _)| f.as_str())
}

/// Why `name` can't be a folder name, or the name trimmed.
pub fn valid_folder_name(name: &str) -> Result<String, String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("Enter a folder name.".into());
    }
    if t.chars().count() > MAX_FOLDER_NAME_CHARS {
        return Err(format!(
            "A folder name can be at most {MAX_FOLDER_NAME_CHARS} characters."
        ));
    }
    if t.chars().any(char::is_control) {
        return Err("A folder name can't contain tabs or line breaks.".into());
    }
    Ok(t.to_string())
}

fn check_kind(kind: &str) -> Result<(), String> {
    if is_kind(kind) {
        Ok(())
    } else {
        Err(format!("unknown kind of setup item `{kind}`"))
    }
}

/// The folder of `kind` named `name` ignoring case, as it is spelled.
fn find_folder(b: &Budget, kind: &str, name: &str) -> Option<String> {
    let key = name.trim().to_lowercase();
    b.setup_folders
        .of(kind)
        .map(|(f, _)| f)
        .find(|f| f.to_lowercase() == key)
        .cloned()
}

/// Adds an empty folder to `kind`. Names are unique ignoring case.
pub fn add_folder(b: &mut Budget, kind: &str, name: &str) -> Result<String, String> {
    check_kind(kind)?;
    let name = valid_folder_name(name)?;
    if let Some(f) = find_folder(b, kind, &name) {
        return Err(format!("There is already a folder named {f}."));
    }
    b.setup_folders
        .0
        .entry(kind.to_string())
        .or_default()
        .insert(name.clone(), Vec::new());
    Ok(name)
}

/// Renames `kind`'s folder `from` (its items stay in it).
pub fn rename_folder(b: &mut Budget, kind: &str, from: &str, to: &str) -> Result<String, String> {
    check_kind(kind)?;
    let to = valid_folder_name(to)?;
    if !b.setup_folders.of(kind).any(|(f, _)| f == from) {
        return Err(format!("There is no folder named {from}."));
    }
    if let Some(f) = find_folder(b, kind, &to) {
        if f != from {
            return Err(format!("There is already a folder named {f}."));
        }
    }
    let folders = b.setup_folders.0.entry(kind.to_string()).or_default();
    let names = folders.remove(from).unwrap_or_default();
    folders.insert(to.clone(), names);
    Ok(to)
}

/// Deletes `kind`'s folder `name`; its items become unfiled (they stay).
/// Returns how many items it held.
pub fn delete_folder(b: &mut Budget, kind: &str, name: &str) -> Result<usize, String> {
    check_kind(kind)?;
    let Some(folders) = b.setup_folders.0.get_mut(kind) else {
        return Err(format!("There is no folder named {name}."));
    };
    let Some(names) = folders.remove(name) else {
        return Err(format!("There is no folder named {name}."));
    };
    if folders.is_empty() {
        b.setup_folders.0.remove(kind);
    }
    Ok(names.iter().filter(|n| exists(b, kind, n)).count())
}

/// Puts items `names` of `kind` in `folder` (`None`: unfiled), out of any
/// other folder. Every name must be an item here and the folder must exist.
pub fn move_items(
    b: &mut Budget,
    kind: &str,
    names: &[String],
    folder: Option<&str>,
) -> Result<(), String> {
    check_kind(kind)?;
    for n in names {
        if !exists(b, kind, n) {
            return Err(format!("{} {n} is not here.", kind.replace('_', " ")));
        }
    }
    if let Some(f) = folder {
        if !b.setup_folders.of(kind).any(|(x, _)| x == f) {
            return Err(format!("There is no folder named {f}."));
        }
    }
    let moving: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let Some(folders) = b.setup_folders.0.get_mut(kind) else {
        return Ok(()); // no folders: everything is unfiled already
    };
    for list in folders.values_mut() {
        list.retain(|n| !moving.contains(n.as_str()));
    }
    if let Some(f) = folder.and_then(|f| folders.get_mut(f)) {
        for n in names {
            if !f.contains(n) {
                f.push(n.clone());
            }
        }
    }
    Ok(())
}

/// `from` becomes `to` in `kind`'s folders (the item was renamed), keeping
/// its folder.
pub fn rename_item(b: &mut Budget, kind: &str, from: &str, to: &str) {
    if let Some(folders) = b.setup_folders.0.get_mut(kind) {
        for n in folders.values_mut().flatten() {
            if n == from {
                *n = to.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SetDef;

    fn budget() -> Budget {
        let mut b = Budget::new("T", "USD");
        for n in ["a", "b", "c"] {
            b.sets.insert(n.into(), SetDef::default());
        }
        b
    }

    fn names(b: &Budget, kind: &str, folder: &str) -> Vec<String> {
        b.setup_folders.0[kind][folder].clone()
    }

    #[test]
    fn folders_are_made_renamed_and_deleted() {
        let mut b = budget();
        assert_eq!(add_folder(&mut b, "set", " Stage ").unwrap(), "Stage");
        assert!(
            add_folder(&mut b, "set", "stage").is_err(),
            "unique ignoring case"
        );
        assert!(add_folder(&mut b, "set", "  ").is_err());
        assert!(add_folder(&mut b, "nonsense", "X").is_err());
        assert!(add_folder(&mut b, "set", &"x".repeat(61)).is_err());
        move_items(&mut b, "set", &["a".into(), "b".into()], Some("Stage")).unwrap();
        assert_eq!(folder_of(&b, "set", "a"), Some("Stage"));
        // A case change is a rename of the same folder.
        rename_folder(&mut b, "set", "Stage", "STAGE").unwrap();
        assert_eq!(names(&b, "set", "STAGE"), ["a", "b"]);
        add_folder(&mut b, "set", "Loc").unwrap();
        assert!(rename_folder(&mut b, "set", "Loc", "stage").is_err());
        move_items(&mut b, "set", &["a".into()], Some("Loc")).unwrap();
        assert_eq!(names(&b, "set", "STAGE"), ["b"]);
        assert_eq!(names(&b, "set", "Loc"), ["a"]);
        move_items(&mut b, "set", &["a".into()], None).unwrap();
        assert_eq!(folder_of(&b, "set", "a"), None);
        assert!(move_items(&mut b, "set", &["zz".into()], None).is_err());
        assert!(move_items(&mut b, "set", &["a".into()], Some("Nope")).is_err());
        assert_eq!(delete_folder(&mut b, "set", "STAGE").unwrap(), 1);
        assert!(b.sets.contains_key("b"), "items stay");
        delete_folder(&mut b, "set", "Loc").unwrap();
        assert!(b.setup_folders.is_empty());
    }

    #[test]
    fn a_file_keeps_names_that_exist_once_and_empty_folders() {
        let mut b = budget();
        let mut sets = BTreeMap::new();
        sets.insert(
            "One".to_string(),
            vec!["a".into(), "gone".into(), "a".into()],
        );
        sets.insert("Two".to_string(), vec!["a".into(), "b".into()]);
        sets.insert("Empty".to_string(), vec![]);
        b.setup_folders.0.insert("set".into(), sets);
        b.setup_folders.0.insert("nonsense".into(), BTreeMap::new());
        b.setup_folders.0.insert("fringe".into(), BTreeMap::new());
        let n = normalized(&b);
        assert_eq!(n.0.keys().collect::<Vec<_>>(), ["set"]);
        assert_eq!(n.0["set"]["One"], ["a"]);
        assert_eq!(n.0["set"]["Two"], ["b"]);
        assert!(n.0["set"]["Empty"].is_empty());
    }

    #[test]
    fn renaming_an_item_keeps_its_folder() {
        let mut b = budget();
        add_folder(&mut b, "set", "F").unwrap();
        move_items(&mut b, "set", &["a".into()], Some("F")).unwrap();
        rename_item(&mut b, "set", "a", "aa");
        assert_eq!(names(&b, "set", "F"), ["aa"]);
    }
}
