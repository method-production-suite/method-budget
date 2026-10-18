//! The update manifest (`latest.json`) the app reads, built from the signed
//! updater artifacts of a release, and the question "does the app trust a
//! real updater key yet?".
//!
//! `latest.json` is what the updater plugin fetches from the endpoint in
//! `tauri.conf.json` (`plugins.updater.endpoints`, the one place it is
//! written): the version, the notes as plain text, the date, and for each
//! platform the artifact's URL and the content of its `.sig`. The app
//! downloads the artifact for its platform and checks it against the
//! signature with the public key it was built with, so everything here is
//! about catching mistakes before they ship (a stale artifact, a signature
//! from another key, a file for a platform nobody asked for), not about
//! trust: a wrong manifest can only make an update fail, never install one.

use std::path::{Path, PathBuf};

use method_release::minisign::{self, PLACEHOLDER_PUBLIC_KEY};
use method_release::notes;
use method_release::version::Version;
use serde_json::{json, Map, Value};

use crate::versions;

/// The artifact names the manifest knows, by what follows `_<version>_` in
/// the name, and the platform keys each one serves. The updater looks for
/// `{os}-{arch}-{installer}` and then `{os}-{arch}`. Anything else in the
/// folder is refused.
///
/// The Mac build is Apple silicon only (the Method suite standard, 2026-10-16), so
/// the release workflow ships `aarch64.app.tar.gz`. A universal archive is
/// refused on purpose: it would offer the update to Intel Macs, which keep
/// the version they have (the app says so when asked: src/updater.rs).
const PLATFORMS: &[(&str, &[&str])] = &[
    ("aarch64.app.tar.gz", &["darwin-aarch64"]),
    ("x64.app.tar.gz", &["darwin-x86_64"]),
    ("x64-setup.exe", &["windows-x86_64", "windows-x86_64-nsis"]),
    ("x64_en-US.msi", &["windows-x86_64-msi"]),
    ("amd64.AppImage", &["linux-x86_64", "linux-x86_64-appimage"]),
];

pub struct Options {
    pub root: PathBuf,
    pub dir: PathBuf,
    pub version: String,
    pub out: PathBuf,
    pub config: PathBuf,
    pub base_url: Option<String>,
    pub pub_date: Option<String>,
}

/// The updater settings `tauri.conf.json` carries: the public key as
/// written, and the endpoints.
pub(crate) fn updater_config(config: &Path) -> Result<(String, Vec<String>), String> {
    let text = std::fs::read_to_string(config)
        .map_err(|e| format!("can't read {}: {e}", config.display()))?;
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", config.display()))?;
    let u = &v["plugins"]["updater"];
    let key = u["pubkey"]
        .as_str()
        .ok_or_else(|| format!("{}: no plugins.updater.pubkey", config.display()))?
        .to_string();
    let endpoints = u["endpoints"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok((key, endpoints))
}

/// The key id the app trusts, `None` for the placeholder. A value that is
/// neither is an error: a broken key is a mistake, not "no key yet".
pub fn trusted_key(config: &Path) -> Result<Option<String>, String> {
    let (key, _) = updater_config(config)?;
    if key.trim() == PLACEHOLDER_PUBLIC_KEY {
        return Ok(None);
    }
    minisign::parse_public_key_box(&key)
        .map(|k| Some(k.key_id))
        .map_err(|e| format!("{}: plugins.updater.pubkey: {e}", config.display()))
}

/// `cargo xtask updater-key [--config <path>] [--require]`.
pub fn key_command(config: &Path, require: bool) -> Result<String, String> {
    match trusted_key(config)? {
        Some(id) => Ok(format!("trusted {id}\n")),
        None if require => Err(format!(
            "{} still has the placeholder updater key (docs/RELEASING.md, \"The updater key\")",
            config.display()
        )),
        None => Ok("none\n".to_string()),
    }
}

/// Where a release's files are downloaded from, worked out from the
/// endpoint: `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`
/// becomes `https://github.com/<owner>/<repo>/releases/download/v<version>/`.
pub fn release_base(endpoint: &str, version: &str) -> Result<String, String> {
    let rest = endpoint
        .strip_prefix("https://github.com/")
        .and_then(|r| r.strip_suffix("/releases/latest/download/latest.json"))
        .ok_or_else(|| {
            format!("the endpoint {endpoint} isn't a GitHub latest-release URL; pass --base-url")
        })?;
    let parts: Vec<&str> = rest.split('/').collect();
    let ok = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    if parts.len() != 2 || !parts.iter().all(|p| ok(p)) {
        return Err(format!(
            "the endpoint {endpoint} doesn't name owner/repository"
        ));
    }
    Ok(format!(
        "https://github.com/{rest}/releases/download/v{version}/"
    ))
}

/// The platform keys an artifact named `name` serves, for `version`.
pub fn platforms_of(name: &str, version: &str) -> Result<&'static [&'static str], String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(format!(
            "{name:?}: artifact names may only have letters, digits, dots, dashes and underscores (the URL is the name)"
        ));
    }
    let marker = format!("_{version}_");
    let at = name.find(&marker).ok_or_else(|| {
        format!("{name}: doesn't carry the version ({marker}); is it from another release?")
    })?;
    let suffix = &name[at + marker.len()..];
    PLATFORMS
        .iter()
        .find(|(s, _)| *s == suffix)
        .map(|(_, keys)| *keys)
        .ok_or_else(|| format!("{name}: an unknown platform ({suffix})"))
}

/// Now, as RFC 3339 in UTC to the second.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339(secs)
}

/// `secs` since 1970 as `YYYY-MM-DDTHH:MM:SSZ` (days to a civil date:
/// Howard Hinnant's algorithm).
pub fn rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// A `--pub-date`, which must be `YYYY-MM-DDTHH:MM:SSZ` (the updater
/// refuses a manifest whose date it can't read).
fn checked_date(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| r.into_iter().all(|i| b[i].is_ascii_digit());
    let ok = b.len() == 20
        && digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'T'
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19)
        && b[19] == b'Z';
    if ok {
        Ok(s.to_string())
    } else {
        Err(format!("--pub-date {s:?} isn't YYYY-MM-DDTHH:MM:SSZ"))
    }
}

/// Builds the manifest as JSON. Every problem found is reported together.
pub fn build(o: &Options) -> Result<Value, String> {
    let version = Version::parse(&o.version)?;
    let v = version.to_string();
    let current = versions::current(&o.root)?;
    if current != version {
        return Err(format!(
            "--version {v} isn't the workspace version, {current}"
        ));
    }
    let trusted = trusted_key(&o.config)?.ok_or_else(|| {
        format!(
            "{} has the placeholder updater key, so the app would refuse every update: no manifest",
            o.config.display()
        )
    })?;
    let base = match &o.base_url {
        Some(b) => {
            if !b.starts_with("https://") {
                return Err(format!("--base-url {b} must be https"));
            }
            if b.ends_with('/') {
                b.clone()
            } else {
                format!("{b}/")
            }
        }
        None => {
            let (_, endpoints) = updater_config(&o.config)?;
            let first = endpoints
                .first()
                .ok_or_else(|| format!("{}: no plugins.updater.endpoints", o.config.display()))?;
            release_base(first, &v)?
        }
    };
    let pub_date = match &o.pub_date {
        Some(d) => checked_date(d)?,
        None => now_rfc3339(),
    };
    let notes_rel = format!("{}/{v}.md", crate::repo::RELEASES_DIR);
    let notes_text = std::fs::read_to_string(o.root.join(&notes_rel))
        .map_err(|_| format!("{notes_rel} is missing"))?;
    let parsed = notes::check_for_release(&v, &notes_text)
        .map_err(|ps| format!("{notes_rel} isn't ready:\n  - {}", ps.join("\n  - ")))?;

    let mut names: Vec<String> = std::fs::read_dir(&o.dir)
        .map_err(|e| format!("can't read {}: {e}", o.dir.display()))?
        .map(|e| {
            e.map_err(|e| e.to_string()).and_then(|e| {
                if e.file_type().map_err(|e| e.to_string())?.is_dir() {
                    Err(format!(
                        "{}: a folder; only artifacts and their .sig files belong here",
                        e.file_name().to_string_lossy()
                    ))
                } else {
                    Ok(e.file_name().to_string_lossy().into_owned())
                }
            })
        })
        .collect::<Result<_, _>>()?;
    names.sort();

    let mut problems = Vec::new();
    let mut platforms = Map::new();
    let mut used_sigs = Vec::new();
    for name in names.iter().filter(|n| !n.ends_with(".sig")) {
        let keys = match platforms_of(name, &v) {
            Ok(k) => k,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        let sig_name = format!("{name}.sig");
        if !names.contains(&sig_name) {
            problems.push(format!(
                "{name}: no {sig_name} beside it (is updater signing on?)"
            ));
            continue;
        }
        used_sigs.push(sig_name.clone());
        let sig = match std::fs::read_to_string(o.dir.join(&sig_name)) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                problems.push(format!("{sig_name}: {e}"));
                continue;
            }
        };
        match minisign::parse_signature_box(&sig) {
            Err(e) => {
                problems.push(format!("{sig_name}: {e}"));
                continue;
            }
            Ok(s) if s.key_id != trusted => {
                problems.push(format!(
                    "{sig_name}: signed with key {}, but the app trusts {trusted}",
                    s.key_id
                ));
                continue;
            }
            // The app's updater (method-update) refuses a signature that
            // doesn't name the version the manifest announces.
            Ok(s) if s.version.as_deref() != Some(v.as_str()) => {
                problems.push(format!(
                    "{sig_name}: signed for {}, not version {v}; sign it with `cargo xtask sign-update`",
                    s.version
                        .map_or_else(|| "no version".to_string(), |s| format!("version {s}"))
                ));
                continue;
            }
            Ok(_) => {}
        }
        for key in keys {
            if platforms.contains_key(*key) {
                problems.push(format!("{name}: a second artifact for {key}"));
                continue;
            }
            platforms.insert(
                (*key).to_string(),
                json!({ "signature": sig, "url": format!("{base}{name}") }),
            );
        }
    }
    for n in names.iter().filter(|n| n.ends_with(".sig")) {
        let artifact = n.trim_end_matches(".sig").to_string();
        if !used_sigs.contains(n) && !names.contains(&artifact) {
            problems.push(format!("{n}: a signature with no artifact"));
        }
    }
    if platforms.is_empty() && problems.is_empty() {
        problems.push(format!("{}: no updater artifacts", o.dir.display()));
    }
    if !problems.is_empty() {
        return Err(format!(
            "no manifest was written:\n  - {}",
            problems.join("\n  - ")
        ));
    }
    Ok(json!({
        "version": v,
        "notes": notes::to_plain_text(&parsed),
        "pub_date": pub_date,
        "platforms": platforms,
    }))
}

/// `cargo xtask update-manifest ...`: builds the manifest and writes it
/// (whole or not at all).
pub fn command(o: Options) -> Result<String, String> {
    let manifest = build(&o)?;
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n";
    let name = o
        .out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "latest.json".into());
    let tmp = o.out.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&tmp, &text).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &o.out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("can't write {}: {e}", o.out.display())
    })?;
    let keys: Vec<&String> = manifest["platforms"]
        .as_object()
        .map(|m| m.keys().collect())
        .unwrap_or_default();
    Ok(format!(
        "Wrote {} for {} ({})\n",
        o.out.display(),
        manifest["version"].as_str().unwrap_or(""),
        keys.iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TempRepo;
    use method_release::minisign::fixture;

    const KEY: u64 = 0xE4D0_668D_54F6_EB34;
    const ENDPOINT: &str =
        "https://github.com/example/method-budget/releases/latest/download/latest.json";

    fn repo(pubkey: &str) -> TempRepo {
        let r = TempRepo::new("manifest");
        r.write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.2.0\"\n",
        );
        r.write(
            "conf.json",
            &json!({ "version": "0.2.0", "plugins": { "updater": { "pubkey": pubkey, "endpoints": [ENDPOINT] } } }).to_string(),
        );
        r.write(
            "docs/releases/0.2.0.md",
            "# Method Budget 0.2.0\n\nUpdates arrive in the app.\n\n## New\n\n- **Updates.** They download quietly.\n",
        );
        r
    }

    fn opts(r: &TempRepo) -> Options {
        Options {
            root: r.root().to_path_buf(),
            dir: r.root().join("dist"),
            version: "0.2.0".into(),
            out: r.root().join("latest.json"),
            config: r.root().join("conf.json"),
            base_url: None,
            pub_date: Some("2026-10-10T12:00:00Z".into()),
        }
    }

    fn artifact(r: &TempRepo, name: &str, key: u64) {
        r.write(&format!("dist/{name}"), "bytes");
        r.write(
            &format!("dist/{name}.sig"),
            &fixture::signature_for_version(key, name, "0.2.0"),
        );
    }

    #[test]
    fn a_good_manifest() {
        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        let out = command(opts(&r)).unwrap();
        assert!(out.contains("darwin-aarch64"), "{out}");
        assert!(!out.contains("darwin-x86_64"), "{out}");
        let m: Value = serde_json::from_str(&r.read("latest.json")).unwrap();
        assert_eq!(m["version"], "0.2.0");
        assert_eq!(m["pub_date"], "2026-10-10T12:00:00Z");
        assert!(m["notes"]
            .as_str()
            .unwrap()
            .starts_with("Updates arrive in the app."));
        let url = "https://github.com/example/method-budget/releases/download/v0.2.0/Method-Budget_0.2.0_aarch64.app.tar.gz";
        assert_eq!(m["platforms"]["darwin-aarch64"]["url"], url);
        assert_eq!(
            m["platforms"]["darwin-aarch64"]["signature"],
            fixture::signature_for_version(KEY, "Method-Budget_0.2.0_aarch64.app.tar.gz", "0.2.0")
        );
        // Intel Macs find no entry and keep the version they have.
        assert_eq!(m["platforms"].as_object().unwrap().len(), 1);
        assert!(!r.read_dir("").iter().any(|n| n.ends_with(".tmp")));
    }

    #[test]
    fn malformed_signatures_are_refused() {
        let r = repo(&fixture::public_key(KEY));
        r.write("dist/Method-Budget_0.2.0_aarch64.app.tar.gz", "bytes");
        r.write(
            "dist/Method-Budget_0.2.0_aarch64.app.tar.gz.sig",
            "not a signature at all",
        );
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("aarch64.app.tar.gz.sig: signature: not base64"),
            "{e}"
        );
        // A public key where a signature belongs.
        r.write(
            "dist/Method-Budget_0.2.0_aarch64.app.tar.gz.sig",
            &fixture::public_key(KEY),
        );
        assert!(command(opts(&r))
            .unwrap_err()
            .contains("expected four lines"));
        // A real-looking signature from another key.
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", 42);
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("signed with key 2A, but the app trusts E4D0668D54F6EB34"),
            "{e}"
        );
        assert!(!r.root().join("latest.json").exists());
    }

    /// The app refuses a signature that doesn't name the manifest's version,
    /// so the manifest is never written with one (a Tauri CLI signature,
    /// not yet re-signed, or one for another release).
    #[test]
    fn a_signature_must_name_the_version() {
        let r = repo(&fixture::public_key(KEY));
        let name = "Method-Budget_0.2.0_aarch64.app.tar.gz";
        r.write(&format!("dist/{name}"), "bytes");
        r.write(&format!("dist/{name}.sig"), &fixture::signature(KEY, name));
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("signed for no version, not version 0.2.0"),
            "{e}"
        );
        assert!(e.contains("cargo xtask sign-update"), "{e}");
        r.write(
            &format!("dist/{name}.sig"),
            &fixture::signature_for_version(KEY, name, "0.1.9"),
        );
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("signed for version 0.1.9, not version 0.2.0"),
            "{e}"
        );
        assert!(!r.root().join("latest.json").exists());
        artifact(&r, name, KEY);
        assert!(command(opts(&r)).is_ok());
    }

    #[test]
    fn unknown_platforms_and_strays_are_refused() {
        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        artifact(&r, "Method-Budget_0.2.0_riscv64.app.tar.gz", KEY);
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("an unknown platform (riscv64.app.tar.gz)"),
            "{e}"
        );

        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.1.0_aarch64.app.tar.gz", KEY);
        assert!(command(opts(&r))
            .unwrap_err()
            .contains("doesn't carry the version"));

        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method Budget_0.2.0_aarch64.app.tar.gz", KEY);
        assert!(command(opts(&r)).unwrap_err().contains("only have letters"));

        let r = repo(&fixture::public_key(KEY));
        r.write("dist/Method-Budget_0.2.0_aarch64.app.tar.gz", "bytes");
        r.write(
            "dist/Method-Budget_0.2.0_x64-setup.exe.sig",
            &fixture::signature(KEY, "x"),
        );
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("no Method-Budget_0.2.0_aarch64.app.tar.gz.sig"),
            "{e}"
        );
        assert!(
            e.contains("x64-setup.exe.sig: a signature with no artifact"),
            "{e}"
        );

        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        artifact(&r, "Method-Budget_0.2.0_universal.app.tar.gz", KEY);
        let e = command(opts(&r)).unwrap_err();
        assert!(
            e.contains("an unknown platform (universal.app.tar.gz)"),
            "a universal archive would update Intel Macs: {e}"
        );

        let r = repo(&fixture::public_key(KEY));
        std::fs::create_dir_all(r.root().join("dist")).unwrap();
        assert!(command(opts(&r))
            .unwrap_err()
            .contains("no updater artifacts"));
    }

    #[test]
    fn every_known_platform() {
        let r = repo(&fixture::public_key(KEY));
        for n in [
            "Method-Budget_0.2.0_aarch64.app.tar.gz",
            "Method-Budget_0.2.0_x64.app.tar.gz",
            "Method-Budget_0.2.0_x64-setup.exe",
            "Method-Budget_0.2.0_x64_en-US.msi",
            "Method-Budget_0.2.0_amd64.AppImage",
        ] {
            artifact(&r, n, KEY);
        }
        let m = build(&opts(&r)).unwrap();
        let keys: Vec<&String> = m["platforms"].as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "darwin-aarch64",
                "darwin-x86_64",
                "linux-x86_64",
                "linux-x86_64-appimage",
                "windows-x86_64",
                "windows-x86_64-msi",
                "windows-x86_64-nsis"
            ]
        );
    }

    #[test]
    fn no_manifest_without_a_real_key_ready_notes_and_the_right_version() {
        let r = repo(PLACEHOLDER_PUBLIC_KEY);
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        assert!(command(opts(&r))
            .unwrap_err()
            .contains("placeholder updater key"));
        assert_eq!(
            key_command(&r.root().join("conf.json"), false).unwrap(),
            "none\n"
        );
        assert!(key_command(&r.root().join("conf.json"), true).is_err());

        let r = repo("garbage");
        assert!(key_command(&r.root().join("conf.json"), false).is_err());

        let r = repo(&fixture::public_key(KEY));
        assert_eq!(
            key_command(&r.root().join("conf.json"), true).unwrap(),
            "trusted E4D0668D54F6EB34\n"
        );
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        r.write(
            "docs/releases/0.2.0.md",
            "# Method Budget 0.2.0\n\n{{Headline.}}\n\n## S\n\n- x\n",
        );
        assert!(command(opts(&r)).unwrap_err().contains("isn't ready"));

        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.3.0_aarch64.app.tar.gz", KEY);
        let mut o = opts(&r);
        o.version = "0.3.0".into();
        assert!(command(o)
            .unwrap_err()
            .contains("isn't the workspace version"));

        let r = repo(&fixture::public_key(KEY));
        artifact(&r, "Method-Budget_0.2.0_aarch64.app.tar.gz", KEY);
        let mut o = opts(&r);
        o.pub_date = Some("yesterday".into());
        assert!(command(o).is_err());
    }

    #[test]
    fn base_urls() {
        assert_eq!(
            release_base(ENDPOINT, "1.2.3").unwrap(),
            "https://github.com/example/method-budget/releases/download/v1.2.3/"
        );
        for bad in [
            "http://github.com/a/b/releases/latest/download/latest.json",
            "https://example.com/a/b/releases/latest/download/latest.json",
            "https://github.com/a/releases/latest/download/latest.json",
            "https://github.com/a/../releases/latest/download/latest.json",
            "https://github.com/a/b/releases/download/v1/latest.json",
        ] {
            assert!(release_base(bad, "1.0.0").is_err(), "{bad}");
        }
        // The repository's own endpoint has the expected shape.
        let conf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../method-app/tauri.conf.json");
        let (_, endpoints) = updater_config(&conf).unwrap();
        assert_eq!(endpoints.len(), 1);
        assert!(release_base(&endpoints[0], "0.1.0").is_ok());
    }

    #[test]
    fn dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_791_633_599), "2026-10-10T11:59:59Z");
        assert!(checked_date(&now_rfc3339()).is_ok());
        assert!(checked_date("2026-10-10 12:00:00Z").is_err());
    }
}
