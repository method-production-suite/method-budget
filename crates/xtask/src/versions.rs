//! Where the version lives, checking that every place agrees, and bumping
//! it everywhere at once.
//!
//! The one source is `version` under `[workspace.package]` in the root
//! Cargo.toml. Copies of it, which must agree:
//!
//! - every workspace member's Cargo.toml inherits it (`version.workspace =
//!   true`, never a number of its own);
//! - Cargo.lock records each member at that version (CI builds `--locked`);
//! - `crates/method-app/tauri.conf.json` carries it as `version` (the bundle's
//!   version, the About box, and what the updater compares against).
//!
//! And the version has release notes, `docs/releases/<version>.md`: readable
//! always (a draft with placeholders is fine between releases), ready to
//! publish when a tag names it.

use std::path::{Path, PathBuf};

use method_release::notes;
use method_release::version::{Part, Version};

use crate::repo;

const ROOT_TOML: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";

/// Paths a bump may find changed without refusing: the release's own notes
/// and what `scripts/site.py` regenerates from them (the release skill
/// drafts and regenerates before it bumps; docs/RELEASING.md).
fn may_be_dirty(path: &str, notes_rel: &str) -> bool {
    path == notes_rel
        || path == "README.md"
        || path == "docs/features.toml"
        || path.starts_with("site/")
}

/// The text of `[section]` lines: each line with the section it is in.
fn sections(text: &str) -> Vec<(String, &str)> {
    let mut current = String::new();
    text.split_inclusive('\n')
        .map(|line| {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                current = t.to_string();
            }
            (current.clone(), line)
        })
        .collect()
}

/// `key = "value"` on one line (spaces around `=` optional), or `None`.
fn string_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(key)?;
    let rest = rest.trim_start().strip_prefix('=')?.trim();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The workspace version as written in the root Cargo.toml, and the index of
/// its line.
fn workspace_version_line(text: &str) -> Result<(String, usize), String> {
    for (i, (section, line)) in sections(text).into_iter().enumerate() {
        if section == "[workspace.package]" {
            if let Some(v) = string_value(line, "version") {
                return Ok((v.to_string(), i));
            }
        }
    }
    Err("the root Cargo.toml has no version under [workspace.package]".into())
}

/// The workspace version, parsed.
pub fn current(root: &Path) -> Result<Version, String> {
    let text = repo::read(root, ROOT_TOML)?;
    let (v, _) = workspace_version_line(&text)?;
    Version::parse(&v).map_err(|e| format!("Cargo.toml [workspace.package] version: {e}"))
}

/// The workspace members, as the root Cargo.toml lists them.
fn members(text: &str) -> Result<Vec<String>, String> {
    let start = text
        .find("members")
        .ok_or("the root Cargo.toml lists no workspace members")?;
    let rest = &text[start..];
    let open = rest.find('[').ok_or("members isn't a list")?;
    let close = rest.find(']').ok_or("members isn't a closed list")?;
    let list = &rest[open + 1..close];
    let out: Vec<String> = list
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    if out.is_empty() {
        return Err("the root Cargo.toml lists no workspace members".into());
    }
    Ok(out)
}

/// A member's package name, and whether it inherits the workspace version
/// (`None` when it sets none at all, `Some(false)` when it has its own).
fn member_package(text: &str) -> (Option<String>, Option<bool>) {
    let mut name = None;
    let mut inherits = None;
    for (section, line) in sections(text) {
        if section != "[package]" {
            continue;
        }
        if let Some(n) = string_value(line, "name") {
            name = Some(n.to_string());
        }
        let t: String = line
            .split('#')
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if t == "version.workspace=true" || t == "version={workspace=true}" {
            inherits = Some(true);
        } else if t.starts_with("version=") || t.starts_with("version.") {
            inherits = Some(false);
        }
    }
    (name, inherits)
}

/// The workspace's own packages in Cargo.lock (those without a `source`):
/// name, version and the index of the version line.
fn lock_entries(lock: &str) -> Vec<(String, String, usize)> {
    let lines: Vec<&str> = lock.split_inclusive('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "[[package]]" {
            i += 1;
            continue;
        }
        let mut name = None;
        let mut version = None;
        let mut source = false;
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim() != "[[package]]" {
            if let Some(n) = string_value(lines[j], "name") {
                name = Some(n.to_string());
            } else if let Some(v) = string_value(lines[j], "version") {
                version = Some((v.to_string(), j));
            } else if lines[j].trim_start().starts_with("source") {
                source = true;
            }
            j += 1;
        }
        if let (Some(n), Some((v, at)), false) = (name, version, source) {
            out.push((n, v, at));
        }
        i = j;
    }
    out
}

/// `text` with line `at` replaced by `key = "value"`, keeping its indent and
/// line ending.
fn replace_line(text: &str, at: usize, key: &str, value: &str) -> String {
    text.split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            if i != at {
                return line.to_string();
            }
            let indent: String = line
                .chars()
                .take_while(|c| c.is_whitespace() && *c != '\n')
                .collect();
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            format!("{indent}{key} = \"{value}\"{ending}")
        })
        .collect()
}

/// The version `tauri.conf.json` carries, if any.
fn tauri_version(conf: &str) -> Result<Option<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(conf).map_err(|e| format!("{}: not JSON: {e}", repo::TAURI_CONF))?;
    Ok(v.get("version")
        .and_then(|s| s.as_str())
        .map(str::to_string))
}

/// `conf` with its top-level `version` changed from `old` to `new`, nothing
/// else touched (checked by reading both back).
fn set_tauri_version(conf: &str, old: &str, new: &str) -> Result<String, String> {
    let needle = format!("\"version\": \"{old}\"");
    if conf.matches(&needle).count() != 1 {
        return Err(format!(
            "{}: expected exactly one {needle} to change",
            repo::TAURI_CONF
        ));
    }
    let out = conf.replacen(&needle, &format!("\"version\": \"{new}\""), 1);
    let mut want: serde_json::Value =
        serde_json::from_str(conf).map_err(|e| format!("{}: {e}", repo::TAURI_CONF))?;
    want["version"] = serde_json::Value::String(new.to_string());
    let got: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| format!("{}: {e}", repo::TAURI_CONF))?;
    if got != want {
        return Err(format!(
            "{}: changing the version would change something else too",
            repo::TAURI_CONF
        ));
    }
    Ok(out)
}

/// Every place the version lives, read: the problems with them (empty when
/// they all say `version` and the notes for it can be read).
pub fn problems(root: &Path) -> Result<(Version, Vec<String>), String> {
    let toml = repo::read(root, ROOT_TOML)?;
    let (written, _) = workspace_version_line(&toml)?;
    let version = Version::parse(&written)
        .map_err(|e| format!("Cargo.toml [workspace.package] version: {e}"))?;
    let v = version.to_string();
    let mut problems = Vec::new();

    let mut names = Vec::new();
    for m in members(&toml)? {
        let rel = format!("{m}/Cargo.toml");
        let text = repo::read(root, &rel)?;
        let (name, inherits) = member_package(&text);
        match inherits {
            Some(true) => {}
            Some(false) => problems.push(format!(
                "{rel} sets its own version; it must say version.workspace = true"
            )),
            None => problems.push(format!(
                "{rel} has no version; it must say version.workspace = true"
            )),
        }
        match name {
            Some(n) => names.push(n),
            None => problems.push(format!("{rel} has no package name")),
        }
    }

    let lock = repo::read(root, LOCK)?;
    let entries = lock_entries(&lock);
    for n in &names {
        match entries.iter().find(|(name, _, _)| name == n) {
            Some((_, lv, _)) if *lv == v => {}
            Some((_, lv, _)) => problems.push(format!(
                "Cargo.lock has {n} at {lv}, not {v} (run a cargo build, or cargo xtask bump)"
            )),
            None => problems.push(format!("Cargo.lock has no entry for {n}")),
        }
    }

    let conf = repo::read(root, repo::TAURI_CONF)?;
    match tauri_version(&conf)? {
        Some(tv) if tv == v => {}
        Some(tv) => problems.push(format!("{} says {tv}, not {v}", repo::TAURI_CONF)),
        None => problems.push(format!(
            "{} has no version (it must say {v})",
            repo::TAURI_CONF
        )),
    }

    let notes_rel = format!("{}/{v}.md", repo::RELEASES_DIR);
    match std::fs::read_to_string(repo::notes_path(root, &v)) {
        Err(_) => problems.push(format!(
            "{notes_rel} is missing: every version has release notes (cargo xtask bump starts them)"
        )),
        Ok(text) => match notes::parse(&text) {
            Err(e) => problems.push(format!("{notes_rel}: {e}")),
            Ok(n) if n.title.rsplit(' ').next() != Some(v.as_str()) => problems.push(format!(
                "{notes_rel}: the title \"{}\" should end with the version, {v}",
                n.title
            )),
            Ok(_) => {}
        },
    }
    Ok((version, problems))
}

/// `cargo xtask check-version [--tag vX.Y.Z]`.
pub fn check_command(root: &Path, tag: Option<&str>) -> Result<String, String> {
    let (version, mut problems) = problems(root)?;
    if let Some(tag) = tag {
        match Version::from_tag(tag) {
            Err(e) => problems.push(e),
            Ok(t) if t != version => problems.push(format!(
                "the tag {tag} names {t}, but the workspace version is {version}"
            )),
            Ok(_) => {
                if let Err(more) = ready_notes(root, &version.to_string()) {
                    problems.extend(more);
                }
            }
        }
    }
    if problems.is_empty() {
        let what = if tag.is_some() {
            "agrees everywhere and its notes are ready to publish"
        } else {
            "agrees everywhere and has notes"
        };
        Ok(format!("{version}: {what}\n"))
    } else {
        Err(format!(
            "the version is out of step:\n  - {}",
            problems.join("\n  - ")
        ))
    }
}

/// The notes for `version`, checked as a release needs them.
fn ready_notes(root: &Path, version: &str) -> Result<(), Vec<String>> {
    let rel = format!("{}/{version}.md", repo::RELEASES_DIR);
    let text = std::fs::read_to_string(repo::notes_path(root, version))
        .map_err(|_| vec![format!("{rel} is missing")])?;
    notes::check_for_release(version, &text)
        .map(|_| ())
        .map_err(|ps| ps.into_iter().map(|p| format!("{rel}: {p}")).collect())
}

/// `cargo xtask notes-check [X.Y.Z]`.
pub fn notes_command(root: &Path, version: Option<&str>) -> Result<String, String> {
    let v = match version {
        Some(s) => Version::parse(s)?.to_string(),
        None => current(root)?.to_string(),
    };
    match ready_notes(root, &v) {
        Ok(()) => Ok(format!(
            "{}/{v}.md is ready to publish\n",
            repo::RELEASES_DIR
        )),
        Err(ps) => Err(format!(
            "the notes for {v} aren't ready:\n  - {}",
            ps.join("\n  - ")
        )),
    }
}

/// The skeleton block of the notes template, with `{{version}}` filled in:
/// the text between `<!-- skeleton:start -->` and `<!-- skeleton:end -->`,
/// without the code fence around it.
pub fn skeleton(template: &str, version: &str) -> Result<String, String> {
    const START: &str = "<!-- skeleton:start -->";
    const END: &str = "<!-- skeleton:end -->";
    let a = template
        .find(START)
        .ok_or_else(|| format!("{}: no {START} marker", repo::NOTES_TEMPLATE))?;
    let b = template
        .find(END)
        .ok_or_else(|| format!("{}: no {END} marker", repo::NOTES_TEMPLATE))?;
    if b < a {
        return Err(format!(
            "{}: the skeleton markers are out of order",
            repo::NOTES_TEMPLATE
        ));
    }
    let block = &template[a + START.len()..b];
    let lines: Vec<&str> = block.lines().collect();
    let first = lines.iter().position(|l| l.trim_start().starts_with("```"));
    let last = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("```"));
    let body = match (first, last) {
        (Some(f), Some(l)) if l > f => &lines[f + 1..l],
        _ => {
            return Err(format!(
                "{}: the skeleton isn't in a code fence",
                repo::NOTES_TEMPLATE
            ))
        }
    };
    let mut out = body.join("\n").replace("{{version}}", version);
    out.push('\n');
    Ok(out)
}

/// What a bump will do: the versions, and every file with its new content.
pub struct Plan {
    pub from: Version,
    pub to: Version,
    /// (path relative to the root, new content).
    pub writes: Vec<(String, String)>,
    /// The notes were started from the template (they didn't exist).
    pub started_notes: bool,
}

/// Works out a bump without writing anything. Refuses (with every reason)
/// when the tree has other changes, when the places disagree already, when
/// the target isn't higher, or when existing notes for it aren't ready.
pub fn plan(root: &Path, target: &str) -> Result<Plan, String> {
    let (from, existing) = problems(root)?;
    // The notes for the current version may still be a draft; everything
    // else must agree before a bump moves it.
    let disagree: Vec<String> = existing
        .into_iter()
        .filter(|p| !p.starts_with(&format!("{}/", repo::RELEASES_DIR)))
        .collect();
    if !disagree.is_empty() {
        return Err(format!(
            "the version is already out of step, so nothing was changed (fix these first):\n  - {}",
            disagree.join("\n  - ")
        ));
    }
    let to = match target {
        "major" => from.bump(Part::Major),
        "minor" => from.bump(Part::Minor),
        "patch" => from.bump(Part::Patch),
        other => {
            let v = Version::parse(other)?;
            if v < from {
                return Err(format!("{v} isn't higher than the current version, {from}"));
            }
            v
        }
    };
    let v = to.to_string();
    let notes_rel = format!("{}/{v}.md", repo::RELEASES_DIR);

    let dirty: Vec<String> = repo::dirty_paths(root)?
        .into_iter()
        .filter(|p| !may_be_dirty(p, &notes_rel))
        .collect();
    if !dirty.is_empty() {
        const SHOWN: usize = 20;
        let mut list = dirty
            .iter()
            .take(SHOWN)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  - ");
        if dirty.len() > SHOWN {
            list.push_str(&format!("\n  ... and {} more", dirty.len() - SHOWN));
        }
        return Err(format!(
            "the working tree has changes, so nothing was changed. Commit or stash these first:\n  - {list}"
        ));
    }

    if to == from {
        // Already there (the first release, or a bump run twice): nothing to
        // change, as long as the notes are ready.
        let text = std::fs::read_to_string(repo::notes_path(root, &v))
            .map_err(|_| format!("already at {v}, but {notes_rel} is missing"))?;
        notes::check_for_release(&v, &text).map_err(|ps| {
            format!(
                "already at {v}, but {notes_rel} isn't ready:\n  - {}",
                ps.join("\n  - ")
            )
        })?;
        return Ok(Plan {
            from,
            to,
            writes: Vec::new(),
            started_notes: false,
        });
    }

    let mut writes = Vec::new();
    let toml = repo::read(root, ROOT_TOML)?;
    let (_, at) = workspace_version_line(&toml)?;
    writes.push((
        ROOT_TOML.to_string(),
        replace_line(&toml, at, "version", &v),
    ));

    let mut names = Vec::new();
    for m in members(&toml)? {
        if let (Some(n), _) = member_package(&repo::read(root, &format!("{m}/Cargo.toml"))?) {
            names.push(n);
        }
    }
    let mut lock = repo::read(root, LOCK)?;
    for (name, _, at) in lock_entries(&lock) {
        if names.contains(&name) {
            lock = replace_line(&lock, at, "version", &v);
        }
    }
    writes.push((LOCK.to_string(), lock));

    let conf = repo::read(root, repo::TAURI_CONF)?;
    writes.push((
        repo::TAURI_CONF.to_string(),
        set_tauri_version(&conf, &from.to_string(), &v)?,
    ));

    let started_notes = match std::fs::read_to_string(root.join(&notes_rel)) {
        Ok(text) => {
            notes::check_for_release(&v, &text).map_err(|ps| {
                format!(
                    "{notes_rel} exists but isn't ready, so nothing was changed:\n  - {}",
                    ps.join("\n  - ")
                )
            })?;
            false
        }
        Err(_) => {
            let template = repo::read(root, repo::NOTES_TEMPLATE)?;
            let text = skeleton(&template, &v)?;
            // A fresh draft must at least be readable, or check-version
            // would fail the moment it lands.
            notes::parse(&text).map_err(|e| {
                format!("{}: its skeleton can't be read: {e}", repo::NOTES_TEMPLATE)
            })?;
            writes.push((notes_rel, text));
            true
        }
    };
    Ok(Plan {
        from,
        to,
        writes,
        started_notes,
    })
}

/// Writes every file in `writes` or none: each goes to a temporary file
/// beside it first, then they are renamed into place; if a rename fails, the
/// files already replaced get their old content back.
pub fn write_all(root: &Path, writes: &[(String, String)]) -> Result<(), String> {
    let mut staged: Vec<(PathBuf, PathBuf, Option<Vec<u8>>)> = Vec::new();
    let cleanup = |staged: &[(PathBuf, PathBuf, Option<Vec<u8>>)]| {
        for (_, tmp, _) in staged {
            let _ = std::fs::remove_file(tmp);
        }
    };
    for (rel, content) in writes {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                cleanup(&staged);
                return Err(format!("can't create the folder for {rel}: {e}"));
            }
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = path.with_file_name(format!(".{name}.xtask.tmp"));
        let old = std::fs::read(&path).ok();
        if let Err(e) = std::fs::write(&tmp, content) {
            cleanup(&staged);
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("can't write {rel}: {e}"));
        }
        staged.push((path, tmp, old));
    }
    for (i, (path, tmp, _)) in staged.iter().enumerate() {
        if let Err(e) = std::fs::rename(tmp, path) {
            for (p, _, old) in &staged[..i] {
                match old {
                    Some(bytes) => {
                        let _ = std::fs::write(p, bytes);
                    }
                    None => {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
            cleanup(&staged[i..]);
            return Err(format!(
                "can't replace {}: {e} (nothing was changed)",
                path.display()
            ));
        }
    }
    Ok(())
}

/// `cargo xtask bump <major|minor|patch|X.Y.Z> [--check]`.
pub fn bump_command(root: &Path, target: &str, check_only: bool) -> Result<String, String> {
    let plan = plan(root, target)?;
    let v = plan.to.to_string();
    if plan.writes.is_empty() {
        return Ok(format!(
            "Already at {v}: nothing to change, and its notes are ready. Next: commit, then tag v{v} (docs/RELEASING.md).\n"
        ));
    }
    let mut out = format!(
        "{} {} -> {v}\n",
        if check_only { "Would bump" } else { "Bumped" },
        plan.from
    );
    for (rel, _) in &plan.writes {
        let note = if plan.started_notes && rel.ends_with(&format!("{v}.md")) {
            format!(" (new, from {}: write the notes)", repo::NOTES_TEMPLATE)
        } else {
            String::new()
        };
        out.push_str(&format!("  {rel}{note}\n"));
    }
    if check_only {
        out.push_str("Nothing was written (--check).\n");
        return Ok(out);
    }
    write_all(root, &plan.writes)?;
    if plan.started_notes {
        out.push_str(&format!(
            "Next: write {}/{v}.md, check it with `cargo xtask notes-check`, then commit and tag v{v} (docs/RELEASING.md).\n",
            repo::RELEASES_DIR
        ));
    } else {
        out.push_str(&format!(
            "Next: commit, then tag v{v} (docs/RELEASING.md).\n"
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TempRepo;

    const TEMPLATE: &str = "# Writing release notes\n\nGuidance.\n\n<!-- skeleton:start -->\n```markdown\n# Method Budget {{version}}\n\n{{One sentence.}}\n\n## {{A heading}}\n\n- **{{A claim.}}** {{More.}}\n```\n<!-- skeleton:end -->\n";

    fn notes_for(v: &str) -> String {
        format!("# Method Budget {v}\n\nThe headline.\n\n## New\n\n- **A thing.** It works.\n")
    }

    /// A tiny workspace in the shape of this one, committed.
    fn workspace(version: &str) -> TempRepo {
        let r = TempRepo::new("versions");
        r.write(
            "Cargo.toml",
            &format!("[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/a\",\n    \"crates/method-app\",\n]\n\n[workspace.package]\nedition = \"2021\"\n# The one place.\nversion = \"{version}\"\nlicense = \"MIT\"\n"),
        );
        r.write(
            "crates/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\nserde = { version = \"=1.0.0\" }\n",
        );
        r.write(
            "crates/method-app/Cargo.toml",
            "[package]\nname = \"method-app\"\nversion.workspace = true\n",
        );
        r.write(
            "Cargo.lock",
            &format!("version = 4\n\n[[package]]\nname = \"a\"\nversion = \"{version}\"\ndependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"method-app\"\nversion = \"{version}\"\n\n[[package]]\nname = \"serde\"\nversion = \"{version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"00\"\n"),
        );
        r.write(
            "crates/method-app/tauri.conf.json",
            &format!("{{\n  \"productName\": \"Method Budget\",\n  \"version\": \"{version}\",\n  \"plugins\": {{ \"x\": {{ \"version\": \"9\" }} }}\n}}\n"),
        );
        r.write("docs/release-notes-template.md", TEMPLATE);
        r.write(&format!("docs/releases/{version}.md"), &notes_for(version));
        r.commit();
        r
    }

    #[test]
    fn the_real_repository_agrees_with_itself() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (v, problems) = problems(&root).unwrap();
        assert!(problems.is_empty(), "{problems:#?}");
        // The app and `method --version` report the same version.
        assert_eq!(v.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn finds_every_place_that_disagrees() {
        let r = workspace("0.1.0");
        assert!(check_command(r.root(), None).is_ok());
        r.write(
            "crates/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.0.9\"\n",
        );
        let lock = r.read("Cargo.lock").replacen(
            "name = \"method-app\"\nversion = \"0.1.0\"",
            "name = \"method-app\"\nversion = \"0.0.9\"",
            1,
        );
        r.write("Cargo.lock", &lock);
        r.write(
            "crates/method-app/tauri.conf.json",
            "{ \"version\": \"0.0.9\" }",
        );
        std::fs::remove_file(r.root().join("docs/releases/0.1.0.md")).unwrap();
        let e = check_command(r.root(), None).unwrap_err();
        for needle in [
            "crates/a/Cargo.toml sets its own version",
            "Cargo.lock has method-app at 0.0.9",
            "tauri.conf.json says 0.0.9",
            "docs/releases/0.1.0.md is missing",
        ] {
            assert!(e.contains(needle), "{needle}: {e}");
        }
        // A registry crate that happens to share the version is not ours.
        assert!(!e.contains("serde"), "{e}");
    }

    #[test]
    fn a_tag_must_name_the_version_and_have_ready_notes() {
        let r = workspace("0.1.0");
        assert!(check_command(r.root(), Some("v0.1.0")).is_ok());
        let e = check_command(r.root(), Some("v0.2.0")).unwrap_err();
        assert!(e.contains("names 0.2.0"), "{e}");
        assert!(check_command(r.root(), Some("0.1.0")).is_err());
        // A draft is fine between releases, not for a tag.
        r.write(
            "docs/releases/0.1.0.md",
            &notes_for("0.1.0").replace("The headline.", "{{One sentence.}}"),
        );
        assert!(check_command(r.root(), None).is_ok());
        let e = check_command(r.root(), Some("v0.1.0")).unwrap_err();
        assert!(e.contains("placeholder"), "{e}");
        assert!(notes_command(r.root(), None).is_err());
    }

    #[test]
    fn bump_sets_every_place_and_starts_the_notes() {
        let r = workspace("0.1.0");
        let out = bump_command(r.root(), "minor", true).unwrap();
        assert!(out.contains("Would bump 0.1.0 -> 0.2.0"), "{out}");
        // --check wrote nothing.
        assert!(r.read("Cargo.toml").contains("version = \"0.1.0\""));
        assert!(!r.root().join("docs/releases/0.2.0.md").exists());

        let out = bump_command(r.root(), "minor", false).unwrap();
        assert!(out.contains("docs/releases/0.2.0.md (new"), "{out}");
        let toml = r.read("Cargo.toml");
        assert!(
            toml.contains("# The one place.\nversion = \"0.2.0\"\n"),
            "{toml}"
        );
        let lock = r.read("Cargo.lock");
        assert!(lock.contains("name = \"a\"\nversion = \"0.2.0\""), "{lock}");
        assert!(
            lock.contains("name = \"method-app\"\nversion = \"0.2.0\""),
            "{lock}"
        );
        // The registry crate keeps its own version.
        assert!(
            lock.contains("name = \"serde\"\nversion = \"0.1.0\""),
            "{lock}"
        );
        let conf = r.read("crates/method-app/tauri.conf.json");
        assert!(conf.contains("\"version\": \"0.2.0\""), "{conf}");
        assert!(conf.contains("{ \"version\": \"9\" }"), "{conf}");
        let notes = r.read("docs/releases/0.2.0.md");
        assert!(
            notes.starts_with("# Method Budget 0.2.0\n\n{{One sentence.}}"),
            "{notes}"
        );
        // Everything agrees, with the draft; the draft isn't ready to tag.
        assert!(check_command(r.root(), None).is_ok());
        assert!(check_command(r.root(), Some("v0.2.0")).is_err());
        // No temporary files are left behind.
        assert!(!r
            .read_dir("crates/method-app")
            .iter()
            .any(|n| n.contains("xtask.tmp")));
    }

    #[test]
    fn bump_refuses_and_writes_nothing() {
        let r = workspace("0.1.0");
        // Lower.
        assert!(bump_command(r.root(), "0.0.5", false)
            .unwrap_err()
            .contains("isn't higher"));
        // The same version (the first release): nothing to write, if its
        // notes are ready.
        let out = bump_command(r.root(), "0.1.0", false).unwrap();
        assert!(
            out.starts_with("Already at 0.1.0: nothing to change"),
            "{out}"
        );
        r.write(
            "docs/releases/0.1.0.md",
            &notes_for("0.1.0").replace("The headline.", "{{Later.}}"),
        );
        assert!(bump_command(r.root(), "0.1.0", true)
            .unwrap_err()
            .contains("isn't ready"));
        r.write("docs/releases/0.1.0.md", &notes_for("0.1.0"));
        assert!(bump_command(r.root(), "sideways", false).is_err());
        // A dirty tree, except for the release's own notes and the site.
        r.write("crates/a/src.rs", "// work in progress\n");
        let e = bump_command(r.root(), "patch", false).unwrap_err();
        assert!(e.contains("crates/a/src.rs"), "{e}");
        assert!(r.read("Cargo.toml").contains("\"0.1.0\""));
        std::fs::remove_file(r.root().join("crates/a/src.rs")).unwrap();
        r.write("README.md", "regenerated\n");
        r.write("site/index.html", "regenerated\n");
        // Notes already drafted for the target must be ready.
        r.write(
            "docs/releases/0.1.1.md",
            &notes_for("0.1.1").replace("The headline.", "{{Later.}}"),
        );
        let e = bump_command(r.root(), "patch", false).unwrap_err();
        assert!(e.contains("isn't ready"), "{e}");
        assert!(r.read("Cargo.toml").contains("\"0.1.0\""));
        r.write("docs/releases/0.1.1.md", &notes_for("0.1.1"));
        let out = bump_command(r.root(), "patch", false).unwrap();
        assert!(out.contains("Bumped 0.1.0 -> 0.1.1"), "{out}");
        assert!(!out.contains("(new"), "{out}");
        assert!(check_command(r.root(), Some("v0.1.1")).is_ok());
    }

    #[test]
    fn bump_refuses_when_already_out_of_step() {
        let r = workspace("0.1.0");
        r.write(
            "crates/method-app/tauri.conf.json",
            "{ \"version\": \"0.0.9\" }",
        );
        r.commit();
        let e = bump_command(r.root(), "patch", false).unwrap_err();
        assert!(e.contains("already out of step"), "{e}");
    }

    #[test]
    fn the_template_skeleton() {
        let s = skeleton(TEMPLATE, "1.2.3").unwrap();
        assert!(s.starts_with("# Method Budget 1.2.3\n"));
        assert!(!s.contains("```"));
        assert!(notes::parse(&s).is_ok());
        assert!(skeleton("no markers", "1.0.0").is_err());
        // The repository's own template starts readable notes.
        let real = include_str!("../../../docs/release-notes-template.md");
        let s = skeleton(real, "9.9.9").unwrap();
        let n = notes::parse(&s).unwrap();
        assert_eq!(n.title, "Method Budget 9.9.9");
        assert!(
            notes::check_for_release("9.9.9", &s).is_err(),
            "a fresh skeleton must not pass as finished notes"
        );
    }
}
