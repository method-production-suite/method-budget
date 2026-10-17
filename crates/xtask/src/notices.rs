//! `cargo xtask notices [--check]`: THIRD-PARTY-NOTICES.md, the third-party
//! code the desktop app and the command line ship, with each crate's licence
//! and the licence and notice files it comes with.
//!
//! Which crates: `cargo metadata --locked` once per desktop platform
//! ([`PLATFORMS`], with `--filter-platform` so target-specific dependencies
//! are resolved as Cargo resolves them for that platform), then a walk of the
//! `resolve` graph from each program's package ([`PROGRAMS`]) along normal
//! dependency edges only. Build and dev dependencies, and everything only
//! they pull in, are left out; procedural macros are normal dependencies and
//! are listed. The workspace's own crates are left out.
//!
//! The output is the same on every machine: sorted, no timestamps, no paths
//! but a crate's own file names, line endings normalized. It needs no
//! network once `cargo fetch` has downloaded the crates.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The file written, relative to the repository root.
pub const FILE: &str = "THIRD-PARTY-NOTICES.md";

/// The programs that ship: (package, what the file calls it).
pub const PROGRAMS: &[(&str, &str)] = &[("method-app", "app"), ("method-cli", "command line")];

/// The platforms the programs are built for: (target triple, name).
pub const PLATFORMS: &[(&str, &str)] = &[
    ("aarch64-apple-darwin", "macOS"),
    ("x86_64-apple-darwin", "macOS"),
    ("x86_64-pc-windows-msvc", "Windows"),
    ("x86_64-unknown-linux-gnu", "Linux"),
];

/// Third-party material that is not a Rust crate and ships in the app (the
/// page in crates/method-app/ui, the icons, the bundled resources):
/// (what, where, licence). Maintained by hand. Checked 2026-10-16: the page's
/// scripts, styles and fonts are the project's own (system fonts only), and
/// the icons are drawn by crates/method-app/icons/source/make_icon.py, so
/// there is nothing to list.
pub const OTHER: &[(&str, &str, &str)] = &[];

/// One third-party crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub repository: Option<String>,
    /// Names from [`PROGRAMS`], sorted.
    pub programs: BTreeSet<String>,
    /// Names from [`PLATFORMS`], sorted.
    pub platforms: BTreeSet<String>,
    /// The licence and notice files: (file name, normalized text), sorted by name.
    pub files: Vec<(String, String)>,
}

impl Crate {
    pub fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

/// One distinct licence or notice text and who ships it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    /// (crate label, file name), sorted.
    pub users: Vec<(String, String)>,
}

/// Whether a file name is a licence or notice file: it starts with LICENSE,
/// LICENCE, COPYING or NOTICE, in any case.
pub fn is_licence_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
        .iter()
        .any(|p| upper.starts_with(p))
}

/// A text as the file shows it: Unix line endings, no trailing spaces on a
/// line, no blank lines before or after.
pub fn normalize(text: &str) -> String {
    let unix = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = unix.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|l| !l.is_empty());
    let end = lines.iter().rposition(|l| !l.is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].join("\n"),
        _ => String::new(),
    }
}

/// A sort key for versions: the numeric parts of `1.2.3` as numbers (so
/// 0.10 sorts after 0.9), then the whole string.
pub fn version_key(v: &str) -> (Vec<u64>, String) {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let nums = core
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    (nums, v.to_string())
}

/// Sorts crates by name, then version.
pub fn sort_crates(crates: &mut [Crate]) {
    crates.sort_by(|a, b| {
        (a.name.as_str(), version_key(&a.version)).cmp(&(b.name.as_str(), version_key(&b.version)))
    });
}

/// The distinct texts, each with every crate and file that has it. Ordered by
/// their first user (crates in the order given, files by name), so with
/// sorted crates the order is fixed. Empty texts are skipped.
pub fn group_texts(crates: &[Crate]) -> Vec<Text> {
    let mut out: Vec<Text> = Vec::new();
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for c in crates {
        for (file, text) in &c.files {
            if text.is_empty() {
                continue;
            }
            let user = (c.label(), file.clone());
            match index.get(text.as_str()) {
                Some(&i) => out[i].users.push(user),
                None => {
                    index.insert(text.as_str(), out.len());
                    out.push(Text {
                        text: text.clone(),
                        users: vec![user],
                    });
                }
            }
        }
    }
    out
}

/// A code fence longer than any run of backticks in `text`.
pub fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

/// A table cell: pipes escaped, line breaks flattened.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\r', '\n'], " ")
}

/// The platforms in prose, each with its targets: "macOS (a and b), Windows
/// (c) and Linux (d)".
pub fn platform_list() -> String {
    let mut names: Vec<(&str, Vec<&str>)> = Vec::new();
    for (target, name) in PLATFORMS {
        match names.iter_mut().find(|(n, _)| n == name) {
            Some((_, ts)) => ts.push(target),
            None => names.push((name, vec![target])),
        }
    }
    let parts: Vec<String> = names
        .iter()
        .map(|(n, ts)| format!("{n} ({})", and_list(ts)))
        .collect();
    and_list(&parts)
}

/// "a", "a and b", "a, b and c".
fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

/// The whole file.
pub fn render(crates: &[Crate], texts: &[Text], other: &[(&str, &str, &str)]) -> String {
    let all_platforms: BTreeSet<&str> = PLATFORMS.iter().map(|(_, n)| *n).collect();
    let programs: Vec<&str> = PROGRAMS.iter().map(|(_, n)| *n).collect();
    let mut s = String::new();
    s.push_str("# Third-party notices\n\n");
    s.push_str(
        "Method Budget's own code is licensed under MIT OR Apache-2.0, at your option \
         (LICENSE-MIT, LICENSE-APACHE). The desktop app and the command line also ship \
         code written by others. This file lists that code, with each part's licence and \
         the licence and notice files it comes with.\n\n",
    );
    s.push_str(&format!(
        "This file is generated by `cargo xtask notices` from Cargo.lock. Don't edit it by \
         hand: run the command again when dependencies change (CI runs `cargo xtask notices \
         --check`). It lists the crates the desktop app and the command line need when \
         built for {}, leaving out build and development dependencies and the \
         workspace's own crates. Procedural macros are listed although they run only \
         while compiling.\n\n",
        platform_list(),
    ));

    let text_ids: BTreeMap<(String, String), usize> = texts
        .iter()
        .enumerate()
        .flat_map(|(i, t)| t.users.iter().map(move |u| (u.clone(), i + 1)))
        .collect();

    s.push_str("## Rust crates\n\n");
    s.push_str(&format!(
        "{} crates. \"Texts\" points at the licence and notice files below; \"none \
         shipped\" means the crate's source has no such file.\n\n",
        crates.len()
    ));
    s.push_str("| Crate | Version | Licence | Ships in | Platforms | Repository | Texts |\n");
    s.push_str("|---|---|---|---|---|---|---|\n");
    for c in crates {
        let ships = programs
            .iter()
            .filter(|p| c.programs.contains(**p))
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let platforms = if c
            .platforms
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == all_platforms
        {
            "all".to_string()
        } else {
            c.platforms.iter().cloned().collect::<Vec<_>>().join(", ")
        };
        let mut ids: Vec<usize> = c
            .files
            .iter()
            .filter_map(|(f, _)| text_ids.get(&(c.label(), f.clone())).copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let refs = if ids.is_empty() {
            "none shipped".to_string()
        } else {
            ids.iter()
                .map(|i| format!("[T{i}](#t{i})"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            cell(&c.name),
            cell(&c.version),
            cell(c.license.as_deref().unwrap_or("none given")),
            ships,
            platforms,
            cell(c.repository.as_deref().unwrap_or("none given")),
            refs,
        ));
    }

    s.push_str("\n## Other third-party material\n\n");
    s.push_str(
        "Maintained by hand in crates/xtask/src/notices.rs (`OTHER`): anything that ships \
         in the app and is not a Rust crate, such as scripts, styles, fonts or images in \
         crates/method-app/ui and crates/method-app/icons.\n\n",
    );
    if other.is_empty() {
        s.push_str(
            "None. The app's page, its styles and its icons are Method Budget's own, and \
             it uses the system's fonts.\n",
        );
    } else {
        s.push_str("| What | Where | Licence |\n|---|---|---|\n");
        let mut rows = other.to_vec();
        rows.sort();
        for (what, at, licence) in rows {
            s.push_str(&format!(
                "| {} | {} | {} |\n",
                cell(what),
                cell(at),
                cell(licence)
            ));
        }
    }

    s.push_str("\n## Licence and notice texts\n\n");
    s.push_str(&format!(
        "{} distinct texts. Identical files are shown once, with every crate that ships \
         them.\n",
        texts.len()
    ));
    for (i, t) in texts.iter().enumerate() {
        let n = i + 1;
        let users = t
            .users
            .iter()
            .map(|(c, f)| format!("{c} ({f})"))
            .collect::<Vec<_>>()
            .join(", ");
        let fence = fence_for(&t.text);
        s.push_str(&format!(
            "\n### T{n}\n\nShipped by: {users}\n\n{fence}text\n{}\n{fence}\n",
            t.text
        ));
    }
    s
}

/// The packages reached from `root` (a package id) along normal dependency
/// edges of `meta`'s resolve graph, `root` included.
pub fn reachable(meta: &Value, root: &str) -> Result<BTreeSet<String>, String> {
    let nodes = meta["resolve"]["nodes"]
        .as_array()
        .ok_or("cargo metadata has no resolve graph")?;
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for node in nodes {
        let id = node["id"].as_str().ok_or("a resolve node has no id")?;
        let mut deps = Vec::new();
        for d in node["deps"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let normal = d["dep_kinds"]
                .as_array()
                .map(|ks| ks.iter().any(|k| k["kind"].is_null()))
                .unwrap_or(false);
            if normal {
                deps.push(d["pkg"].as_str().ok_or("a dependency has no package id")?);
            }
        }
        edges.insert(id, deps);
    }
    let mut seen = BTreeSet::new();
    let mut todo = vec![root];
    while let Some(id) = todo.pop() {
        if seen.insert(id.to_string()) {
            todo.extend(edges.get(id).map(Vec::as_slice).unwrap_or(&[]));
        }
    }
    Ok(seen)
}

/// The package id of the workspace member named `name`.
fn member_id(meta: &Value, name: &str) -> Result<String, String> {
    let members: HashSet<&str> = meta["workspace_members"]
        .as_array()
        .ok_or("cargo metadata has no workspace members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    meta["packages"]
        .as_array()
        .ok_or("cargo metadata has no packages")?
        .iter()
        .find(|p| {
            p["name"].as_str() == Some(name)
                && p["id"].as_str().is_some_and(|id| members.contains(id))
        })
        .and_then(|p| p["id"].as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("the workspace has no package {name}"))
}

fn metadata(root: &Path, target: &str) -> Result<Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(&cargo)
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed for {target}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata's output: {e}"))
}

/// The licence and notice files at the top of a crate's folder, plus the
/// manifest's `license-file` when it names another file inside the crate.
fn licence_files(dir: &Path, license_file: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let mut names = BTreeSet::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("can't read a crate's folder ({e}); run `cargo fetch` first"))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if is_licence_name(&name) && e.path().is_file() {
            names.insert(name);
        }
    }
    if let Some(f) = license_file {
        let p = Path::new(f);
        let inside = p
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        // Only a file whose name matches exactly, so a case-insensitive file
        // system finds what a case-sensitive one does.
        let exact = p.file_name().is_some_and(|name| {
            p.parent()
                .and_then(|parent| std::fs::read_dir(dir.join(parent)).ok())
                .is_some_and(|mut es| {
                    es.any(|e| e.is_ok_and(|e| e.file_name() == name && e.path().is_file()))
                })
        });
        if inside && exact {
            names.insert(f.replace('\\', "/"));
        }
    }
    let mut out = Vec::new();
    for name in names {
        let bytes = std::fs::read(dir.join(&name))
            .map_err(|e| format!("can't read {name} in a crate's folder: {e}"))?;
        out.push((name, normalize(&String::from_utf8_lossy(&bytes))));
    }
    Ok(out)
}

/// Every third-party crate the programs ship, sorted.
pub fn collect(root: &Path) -> Result<Vec<Crate>, String> {
    let mut found: BTreeMap<String, Crate> = BTreeMap::new();
    let mut dirs: BTreeMap<String, (std::path::PathBuf, Option<String>)> = BTreeMap::new();
    for (target, platform) in PLATFORMS {
        let meta = metadata(root, target)?;
        let members: HashSet<&str> = meta["workspace_members"]
            .as_array()
            .ok_or("cargo metadata has no workspace members")?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let packages: BTreeMap<&str, &Value> = meta["packages"]
            .as_array()
            .ok_or("cargo metadata has no packages")?
            .iter()
            .filter_map(|p| p["id"].as_str().map(|id| (id, p)))
            .collect();
        for (package, program) in PROGRAMS {
            let start = member_id(&meta, package)?;
            for id in reachable(&meta, &start)? {
                if members.contains(id.as_str()) {
                    continue;
                }
                let p = packages
                    .get(id.as_str())
                    .ok_or("a dependency is missing from cargo metadata's packages")?;
                let text = |k: &str| p[k].as_str().map(str::to_string);
                let entry = found.entry(id.clone()).or_insert_with(|| Crate {
                    name: text("name").unwrap_or_default(),
                    version: text("version").unwrap_or_default(),
                    license: text("license"),
                    repository: text("repository"),
                    programs: BTreeSet::new(),
                    platforms: BTreeSet::new(),
                    files: vec![],
                });
                entry.programs.insert((*program).to_string());
                entry.platforms.insert((*platform).to_string());
                if !dirs.contains_key(&id) {
                    let manifest = p["manifest_path"]
                        .as_str()
                        .ok_or("a package has no manifest path")?;
                    let dir = Path::new(manifest)
                        .parent()
                        .ok_or("a manifest path has no folder")?
                        .to_path_buf();
                    dirs.insert(id.clone(), (dir, text("license_file")));
                }
            }
        }
    }
    let mut crates = Vec::new();
    for (id, mut c) in found {
        if let Some((dir, lf)) = dirs.get(&id) {
            c.files = licence_files(dir, lf.as_deref())?;
        }
        crates.push(c);
    }
    sort_crates(&mut crates);
    Ok(crates)
}

pub fn command(root: &Path, check: bool) -> Result<String, String> {
    let crates = collect(root)?;
    let texts = group_texts(&crates);
    let out = render(&crates, &texts, OTHER);
    let path = root.join(FILE);
    let summary = format!("{} crates, {} licence texts", crates.len(), texts.len());
    if check {
        let now = std::fs::read_to_string(&path).unwrap_or_default();
        if now != out {
            return Err(format!(
                "{FILE} is out of date; run `cargo xtask notices` and commit it"
            ));
        }
        return Ok(format!("{FILE} is up to date ({summary})\n"));
    }
    std::fs::write(&path, &out).map_err(|e| format!("can't write {FILE}: {e}"))?;
    Ok(format!("wrote {FILE} ({summary})\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn krate(name: &str, version: &str, files: &[(&str, &str)]) -> Crate {
        Crate {
            name: name.into(),
            version: version.into(),
            license: Some("MIT".into()),
            repository: Some(format!("https://example.com/{name}")),
            programs: ["app".to_string()].into(),
            platforms: ["Linux", "Windows", "macOS"].map(String::from).into(),
            files: files
                .iter()
                .map(|(f, t)| (f.to_string(), normalize(t)))
                .collect(),
        }
    }

    #[test]
    fn licence_file_names_match_in_any_case() {
        for n in [
            "LICENSE",
            "license-mit",
            "LICENCE.txt",
            "COPYING",
            "Notice.md",
            "LICENSE-APACHE",
        ] {
            assert!(is_licence_name(n), "{n}");
        }
        for n in ["README.md", "Cargo.toml", "src", "UNLICENSE", "mylicense"] {
            assert!(!is_licence_name(n), "{n}");
        }
    }

    #[test]
    fn texts_are_normalized() {
        assert_eq!(
            normalize("\r\n\r\nMIT  \r\nLicense\t\n\n\n"),
            "MIT\nLicense"
        );
        assert_eq!(normalize("a\rb"), "a\nb");
        assert_eq!(normalize(" \n \n"), "");
    }

    #[test]
    fn crates_sort_by_name_then_numeric_version() {
        let mut cs = vec![
            krate("b", "1.0.0", &[]),
            krate("a", "0.10.0", &[]),
            krate("a", "0.9.1", &[]),
            krate("a", "0.9.1-alpha", &[]),
        ];
        sort_crates(&mut cs);
        let labels: Vec<String> = cs.iter().map(Crate::label).collect();
        assert_eq!(labels, ["a 0.9.1", "a 0.9.1-alpha", "a 0.10.0", "b 1.0.0"]);
    }

    #[test]
    fn identical_texts_are_shown_once_with_every_crate() {
        let cs = vec![
            krate(
                "a",
                "1.0.0",
                &[("LICENSE-APACHE", "Apache\r\n"), ("LICENSE-MIT", "MIT a")],
            ),
            krate("b", "2.0.0", &[("LICENSE-APACHE", "Apache  \n")]),
            krate("c", "3.0.0", &[("LICENSE", ""), ("COPYING", "MIT a")]),
        ];
        let texts = group_texts(&cs);
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].text, "Apache");
        assert_eq!(
            texts[0].users,
            [
                ("a 1.0.0".to_string(), "LICENSE-APACHE".to_string()),
                ("b 2.0.0".to_string(), "LICENSE-APACHE".to_string())
            ]
        );
        assert_eq!(texts[1].text, "MIT a");
        assert_eq!(texts[1].users.len(), 2);
        assert_eq!(
            texts[1].users[1],
            ("c 3.0.0".to_string(), "COPYING".to_string())
        );
    }

    #[test]
    fn platforms_read_as_prose() {
        assert_eq!(and_list(&["a"]), "a");
        assert_eq!(and_list(&["a", "b", "c"]), "a, b and c");
        assert_eq!(
            platform_list(),
            "macOS (aarch64-apple-darwin and x86_64-apple-darwin), Windows \
             (x86_64-pc-windows-msvc) and Linux (x86_64-unknown-linux-gnu)"
        );
    }

    #[test]
    fn a_fence_outlasts_backticks_in_the_text() {
        assert_eq!(fence_for("plain"), "```");
        assert_eq!(fence_for("has ``` inside"), "````");
        assert_eq!(fence_for("`````"), "``````");
    }

    #[test]
    fn rendering_lists_crates_texts_and_crates_without_files() {
        let mut cs = vec![
            krate("zed", "1.0.0", &[]),
            krate("alpha", "0.1.0", &[("LICENSE", "The | text")]),
        ];
        cs[0].platforms = ["macOS".to_string()].into();
        cs[0].license = None;
        cs[0].programs.insert("command line".into());
        sort_crates(&mut cs);
        let texts = group_texts(&cs);
        let out = render(&cs, &texts, &[]);
        assert!(out.starts_with(
            "# Third-party notices\n\nMethod Budget's own code is licensed under MIT OR Apache-2.0"
        ));
        assert!(out.contains(
            "| alpha | 0.1.0 | MIT | app | all | https://example.com/alpha | [T1](#t1) |\n"
        ));
        assert!(out.contains("| zed | 1.0.0 | none given | app, command line | macOS | https://example.com/zed | none shipped |\n"));
        assert!(out.find("| alpha").unwrap() < out.find("| zed").unwrap());
        assert!(out
            .contains("### T1\n\nShipped by: alpha 0.1.0 (LICENSE)\n\n```text\nThe | text\n```\n"));
        assert!(out.contains("2 crates."));
        assert!(out.contains("None. The app's page"));
        assert!(!out.contains('\u{2014}'), "no em dashes");
        assert_eq!(
            out,
            render(&cs, &texts, &[]),
            "the same input renders the same"
        );
    }

    #[test]
    fn other_material_is_listed_sorted() {
        let out = render(
            &[],
            &[],
            &[("Zfont", "ui/z", "OFL-1.1"), ("Alib", "ui/a.js", "MIT")],
        );
        let a = out.find("| Alib | ui/a.js | MIT |").unwrap();
        let z = out.find("| Zfont | ui/z | OFL-1.1 |").unwrap();
        assert!(a < z);
    }

    #[test]
    fn only_normal_edges_are_followed() {
        let meta = json!({"resolve": {"nodes": [
            {"id": "app", "deps": [
                {"pkg": "lib", "dep_kinds": [{"kind": null, "target": null}]},
                {"pkg": "builder", "dep_kinds": [{"kind": "build", "target": null}]},
                {"pkg": "tester", "dep_kinds": [{"kind": "dev", "target": null}]},
                {"pkg": "both", "dep_kinds": [{"kind": "dev"}, {"kind": null}]}
            ]},
            {"id": "lib", "deps": [{"pkg": "deep", "dep_kinds": [{"kind": null}]}]},
            {"id": "builder", "deps": [{"pkg": "only-build", "dep_kinds": [{"kind": null}]}]},
            {"id": "tester", "deps": []},
            {"id": "both", "deps": []},
            {"id": "deep", "deps": []},
            {"id": "only-build", "deps": []}
        ]}});
        let got = reachable(&meta, "app").unwrap();
        let want: BTreeSet<String> = ["app", "lib", "deep", "both"].map(String::from).into();
        assert_eq!(got, want);
    }
}
