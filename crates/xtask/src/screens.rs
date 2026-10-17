//! `cargo xtask capture-screens` and `cargo xtask review-screens`: the README
//! and site pictures, retaken from the invented Field Station budget
//! (SUITE-STANDARDS.md, section 7; docs/WEBSITE.md, "Screenshots").
//!
//! `capture-screens` builds the browser build into `target/screens/web`
//! (the same code as `cargo xtask web`), builds the command line and runs
//! its totals command on the budget for the terminal picture, then runs
//! `scripts/capture-screens.mjs`, which drives the real page in headless
//! Chrome and writes each picture, framed, light and dark. Each is then
//! written to `assets/screens/` (the README) and `site/img/` (the site, which
//! is published from `site/` alone), and only where its bytes differ, so a
//! run that changes nothing changes no file.
//!
//! `review-screens --out <page.html>` writes one page showing each picture
//! that differs from the last commit, old beside new, and lists them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::repo;

/// The four pictures, in the suite's order: the main view, the signature
/// feature, open or import, and the file you own.
pub const SHOTS: [&str; 4] = ["hero", "explain", "import", "file"];
pub const THEMES: [&str; 2] = ["light", "dark"];
/// Where each picture goes: the README's folder and the site's.
pub const DIRS: [&str; 2] = ["assets/screens", "site/img"];

const FIXTURES: &str = "fixtures/field-station";
const BUDGET: &str = "field-station.mthd";
const SHEET: &str = "camera-department.csv";
const DRIVER: &str = "scripts/capture-screens.mjs";
const CLI_TOML: &str = "crates/method-cli/Cargo.toml";
/// Everything a run makes that isn't a picture in the repository.
const WORK: &str = "target/screens";

/// Each picture's file name (`hero-light.png`), in order.
pub fn file_names() -> Vec<String> {
    SHOTS
        .iter()
        .flat_map(|s| THEMES.iter().map(move |t| format!("{s}-{t}.png")))
        .collect()
}

/// Every path a capture writes, relative to the root, in order.
#[cfg(test)]
pub fn outputs() -> Vec<String> {
    file_names()
        .iter()
        .flat_map(|f| DIRS.iter().map(move |d| format!("{d}/{f}")))
        .collect()
}

/// Writes `bytes` to `path` unless it already holds exactly them. True when
/// it wrote.
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(true)
}

/// The command line's program name, from its Cargo.toml's `[[bin]]`.
pub fn cli_name(toml: &str) -> Option<String> {
    let at = toml.find("[[bin]]")?;
    toml[at..]
        .lines()
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "name").then(|| v.trim().trim_matches('"').to_string())
        })
}

/// What the terminal picture shows: the command as typed in the budget's
/// folder, and what it printed.
pub fn terminal_json(command: &str, output: &str) -> String {
    serde_json::json!({ "command": command, "output": output }).to_string()
}

fn run(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("couldn't run {what}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{what} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

pub fn capture_command(root: &Path) -> Result<String, String> {
    let work = root.join(WORK);
    let web = work.join("web");
    crate::web::command(root, Some(web.clone()), false)?;

    // The command line, for the terminal picture: its real output.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(&cargo);
    build
        .args(["build", "--release", "--locked", "-p", "method-cli"])
        .current_dir(root);
    let status = build
        .status()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !status.success() {
        return Err("the command line doesn't build".into());
    }
    let name = cli_name(&repo::read(root, CLI_TOML)?)
        .ok_or_else(|| format!("{CLI_TOML} names no [[bin]]"))?;
    let bin = root.join("target").join("release").join(&name);
    let fixtures = root.join(FIXTURES);
    let output = run(
        Command::new(&bin)
            .args(["totals", BUDGET])
            .current_dir(&fixtures),
        &format!("{name} totals"),
    )?;
    let output = String::from_utf8(output).map_err(|_| "the totals aren't text".to_string())?;
    fs::create_dir_all(&work).map_err(|e| format!("couldn't make {WORK}: {e}"))?;
    let terminal = work.join("terminal.json");
    fs::write(
        &terminal,
        terminal_json(&format!("{name} totals {BUDGET}"), &output),
    )
    .map_err(|e| format!("couldn't write {}: {e}", terminal.display()))?;

    // The pictures.
    let pictures = work.join("pictures");
    if pictures.exists() {
        fs::remove_dir_all(&pictures)
            .map_err(|e| format!("couldn't clear {}: {e}", pictures.display()))?;
    }
    let status = Command::new("node")
        .arg(root.join(DRIVER))
        .arg("--web")
        .arg(&web)
        .arg("--budget")
        .arg(fixtures.join(BUDGET))
        .arg("--sheet")
        .arg(fixtures.join(SHEET))
        .arg("--terminal")
        .arg(&terminal)
        .arg("--out")
        .arg(&pictures)
        .current_dir(root)
        .status()
        .map_err(|e| format!("couldn't run node (Node 18 or later is needed): {e}"))?;
    if !status.success() {
        return Err(format!("{DRIVER} failed; nothing was copied"));
    }
    install(root, &pictures)
}

/// Copies each picture from `pictures` to its places, where it changed.
fn install(root: &Path, pictures: &Path) -> Result<String, String> {
    // Every picture must be there before any is copied.
    let mut all = Vec::new();
    for f in file_names() {
        let p = pictures.join(&f);
        let bytes = fs::read(&p).map_err(|e| format!("{} wasn't made: {e}", p.display()))?;
        all.push((f, bytes));
    }
    let mut changed = Vec::new();
    for (f, bytes) in &all {
        for d in DIRS {
            let rel = format!("{d}/{f}");
            if write_if_changed(&root.join(&rel), bytes)? {
                changed.push(rel);
            }
        }
    }
    Ok(if changed.is_empty() {
        format!("{} pictures taken; none changed.\n", file_names().len())
    } else {
        format!(
            "{} pictures taken; {} files changed:\n{}\nCompare them with: cargo xtask review-screens --out target/screens/review.html\n",
            file_names().len(),
            changed.len(),
            changed
                .iter()
                .map(|c| format!("  {c}\n"))
                .collect::<String>()
                .trim_end()
        )
    })
}

// ---------------------------------------------------------------------------
// review-screens

/// One picture that differs from the last commit, with every path holding
/// the same change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub paths: Vec<String>,
    /// The bytes at HEAD, or None for a new picture.
    pub before: Option<Vec<u8>>,
    /// The bytes on disk, or None for a removed one.
    pub after: Option<Vec<u8>>,
}

impl Changed {
    pub fn kind(&self) -> &'static str {
        match (&self.before, &self.after) {
            (None, _) => "new",
            (_, None) => "removed",
            _ => "changed",
        }
    }
}

/// A picture's bytes at HEAD and on disk: what a change is grouped by.
type Pair = (Option<Vec<u8>>, Option<Vec<u8>>);

/// The pictures in [`DIRS`] that differ from HEAD (staged or not), the
/// same change in two places listed once, in path order.
pub fn changed_pictures(root: &Path) -> Result<Vec<Changed>, String> {
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    args.extend(DIRS);
    let out = repo::git(root, &args)?;
    let mut paths = Vec::new();
    let mut records = out.split('\0').filter(|r| r.len() > 3);
    while let Some(r) = records.next() {
        let (code, path) = r.split_at(3);
        paths.push(path.to_string());
        // A rename or copy is followed by its old path, which changed too.
        if code.starts_with('R') || code.starts_with('C') {
            if let Some(old) = records.next() {
                paths.push(old.to_string());
            }
        }
    }
    paths.retain(|p| p.ends_with(".png"));
    paths.sort();
    paths.dedup();

    let mut groups: BTreeMap<Pair, Vec<String>> = BTreeMap::new();
    let mut order = Vec::new();
    for path in paths {
        let before = head_bytes(root, &path)?;
        let after = fs::read(root.join(&path)).ok();
        if before == after {
            continue;
        }
        let key = (before, after);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(path);
    }
    let mut all: Vec<Changed> = order
        .into_iter()
        .filter_map(|key| {
            let paths = groups.remove(&key)?;
            Some(Changed {
                paths,
                before: key.0,
                after: key.1,
            })
        })
        .collect();
    all.sort_by(|a, b| a.paths.cmp(&b.paths));
    Ok(all)
}

/// A file's bytes at HEAD, or None when HEAD hasn't got it.
fn head_bytes(root: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "blob", &format!("HEAD:{path}")])
        .output()
        .map_err(|e| format!("can't run git: {e}"))?;
    Ok(out.status.success().then_some(out.stdout))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn side(label: &str, bytes: Option<&[u8]>, missing: &str) -> String {
    let body = match bytes {
        None => format!("<p class=\"none\">{missing}</p>"),
        Some(b) => format!(
            "<img alt=\"{label}\" src=\"data:image/png;base64,{}\">",
            method_release::minisign::encode_base64(b)
        ),
    };
    format!("<figure><figcaption>{label}</figcaption>{body}</figure>")
}

/// One self-contained page: every picture is inside it, so it opens from
/// anywhere with nothing beside it.
pub fn review_page(pictures: &[Changed]) -> String {
    let list: String = pictures
        .iter()
        .map(|p| {
            format!(
                "<li><code>{}</code> {}</li>\n",
                p.paths
                    .iter()
                    .map(|x| escape(x))
                    .collect::<Vec<_>>()
                    .join(", "),
                p.kind()
            )
        })
        .collect();
    let sections: String = pictures
        .iter()
        .map(|p| {
            format!(
                "<section>\n<h2>{} <span>{}</span></h2>\n<div class=\"pair\">\n{}\n{}\n</div>\n</section>\n",
                p.paths.iter().map(|x| escape(x)).collect::<Vec<_>>().join("<br>"),
                p.kind(),
                side("Before", p.before.as_deref(), "No picture before. This one is new."),
                side("Now", p.after.as_deref(), "Removed. The capture no longer takes this picture."),
            )
        })
        .collect();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pictures to check</title>
<style>
body{{margin:0;padding:24px;font:15px/1.5 -apple-system,system-ui,sans-serif;background:#f3f2ee;color:#1d1b16}}
h1{{font-size:20px;margin:0 0 6px}}
.lead{{max-width:46em;margin:0 0 12px}}
ul.changed{{margin:0 0 28px}}
section{{margin:0 0 36px}}
h2{{font-size:13px;font-family:ui-monospace,monospace;font-weight:600;margin:0 0 10px}}
h2 span{{font-family:-apple-system,system-ui,sans-serif;font-weight:400;color:#6d6960;margin-left:8px}}
.pair{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}
figure{{margin:0;background:#cfccc4;padding:10px;border-radius:6px}}
figcaption{{font-size:12px;color:#4a453a;margin-bottom:6px}}
img{{display:block;width:100%;height:auto}}
.none{{margin:40px 0;text-align:center;color:#6d6960}}
@media (prefers-color-scheme: dark){{body{{background:#1c1b19;color:#ece9e2}}figure{{background:#3a3833}}figcaption,h2 span,.none{{color:#b3ada2}}}}
</style>
</head>
<body>
<h1>Pictures to check</h1>
<p class="lead">These differ from the last commit. Before on the left, now on the right.
Check that each shows what it should, and that nothing real is in frame: every picture
shows Field Station, the invented production.</p>
<ul class="changed">
{list}</ul>
{sections}</body>
</html>
"#
    )
}

pub fn review_command(root: &Path, out: Option<PathBuf>) -> Result<String, String> {
    let out = out.ok_or("review-screens needs --out <page.html>: where to write the page")?;
    let pictures = changed_pictures(root)?;
    if pictures.is_empty() {
        return Ok("No picture changed.\n".into());
    }
    fs::write(&out, review_page(&pictures))
        .map_err(|e| format!("couldn't write {}: {e}", out.display()))?;
    let n = pictures.len();
    let mut text = format!(
        "{n} picture{} changed. Old beside new: {}\n",
        if n == 1 { "" } else { "s" },
        out.display()
    );
    for p in &pictures {
        text.push_str(&format!("  {:<7}  {}\n", p.kind(), p.paths.join(" ")));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TempRepo;

    #[test]
    fn every_picture_goes_to_the_readme_and_the_site() {
        let out = outputs();
        assert_eq!(out.len(), 16);
        assert_eq!(out[0], "assets/screens/hero-light.png");
        assert_eq!(out[1], "site/img/hero-light.png");
        assert_eq!(out[15], "site/img/file-dark.png");
        // The site's copy is under site/, which is all the website publishes.
        assert!(out.iter().filter(|p| p.starts_with("site/")).count() == 8);
        // The driver writes the names this copies.
        let driver = include_str!("../../../scripts/capture-screens.mjs");
        for s in SHOTS {
            assert!(driver.contains(&format!("'{s}'")), "{s}");
        }
    }

    #[test]
    fn a_picture_is_written_only_when_it_changed() {
        let r = TempRepo::new("screens-write");
        let p = r.root().join("site/img/hero-light.png");
        assert!(write_if_changed(&p, b"one").unwrap());
        assert!(!write_if_changed(&p, b"one").unwrap());
        assert!(write_if_changed(&p, b"two").unwrap());
        assert_eq!(fs::read(&p).unwrap(), b"two");
    }

    #[test]
    fn install_needs_every_picture_and_reports_what_changed() {
        let r = TempRepo::new("screens-install");
        let pics = r.root().join("target/pictures");
        fs::create_dir_all(&pics).unwrap();
        for f in file_names().iter().skip(1) {
            fs::write(pics.join(f), f.as_bytes()).unwrap();
        }
        assert!(install(r.root(), &pics)
            .unwrap_err()
            .contains("hero-light.png"));
        assert!(!r.root().join("site/img").exists(), "nothing copied");
        fs::write(pics.join("hero-light.png"), b"hero").unwrap();
        let first = install(r.root(), &pics).unwrap();
        assert!(first.contains("16 files changed"), "{first}");
        assert_eq!(r.read("assets/screens/hero-light.png"), "hero");
        assert!(install(r.root(), &pics).unwrap().contains("none changed"));
    }

    #[test]
    fn the_command_line_is_named_by_its_cargo_toml() {
        let toml = "[package]\nname = \"method-cli\"\n\n[[bin]]\nname = \"method\"\npath = \"src/main.rs\"\n";
        assert_eq!(cli_name(toml).as_deref(), Some("method"));
        assert_eq!(cli_name("[package]\nname = \"x\"\n"), None);
        // The first [[bin]] is the command line itself; `method` after it is the old name's alias.
        assert_eq!(
            cli_name(include_str!("../../method-cli/Cargo.toml")).as_deref(),
            Some("method-budget")
        );
        let t: serde_json::Value =
            serde_json::from_str(&terminal_json("method totals a.mthd", "x\t1\n")).unwrap();
        assert_eq!(t["output"], "x\t1\n");
    }

    #[test]
    fn review_lists_each_change_once_with_both_places() {
        let r = TempRepo::new("screens-review");
        r.write("assets/screens/hero-light.png", "old");
        r.write("site/img/hero-light.png", "old");
        r.write("assets/screens/explain-light.png", "same");
        r.write("README.md", "x");
        r.commit();
        assert!(changed_pictures(r.root()).unwrap().is_empty());
        assert_eq!(
            review_command(r.root(), Some(r.root().join("p.html"))).unwrap(),
            "No picture changed.\n"
        );

        r.write("assets/screens/hero-light.png", "new");
        r.write("site/img/hero-light.png", "new");
        r.write("site/img/file-dark.png", "added");
        r.write("site/img/notes.txt", "not a picture");
        fs::remove_file(r.root().join("assets/screens/explain-light.png")).unwrap();
        let c = changed_pictures(r.root()).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].paths, ["assets/screens/explain-light.png"]);
        assert_eq!(c[0].kind(), "removed");
        assert_eq!(
            c[1].paths,
            ["assets/screens/hero-light.png", "site/img/hero-light.png"]
        );
        assert_eq!(c[1].kind(), "changed");
        assert_eq!(c[2].kind(), "new");

        let out = r.root().join("review.html");
        let text = review_command(r.root(), Some(out.clone())).unwrap();
        assert!(text.starts_with("3 pictures changed"), "{text}");
        let page = fs::read_to_string(out).unwrap();
        // "old" and "new" in base64, each inside the page.
        assert!(page.contains("data:image/png;base64,b2xk"));
        assert!(page.contains("data:image/png;base64,bmV3"));
        assert!(page.contains("This one is new."));
        assert!(!page.contains('\u{2014}'));
        assert!(review_command(r.root(), None).is_err());
    }

    #[test]
    fn the_page_escapes_paths() {
        let page = review_page(&[Changed {
            paths: vec!["site/img/<b>.png".into()],
            before: None,
            after: Some(b"x".to_vec()),
        }]);
        assert!(page.contains("site/img/&lt;b&gt;.png"));
        assert!(!page.contains("<b>.png"));
    }
}
