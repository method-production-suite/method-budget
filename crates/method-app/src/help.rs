//! The Help menu's links and its Licenses and Notices window (docs/APP.md,
//! "The Help menu").
//!
//! User Guide, Report a Bug… and methodproduction.com open a page in the
//! default browser; they are answered here, in Rust. The app has no general
//! way to open a link: [`open_url`] opens only the addresses below (the bug
//! report by its address and a body checked character by character), with
//! the system's own opener, so nothing a page or a budget says can reach it.
//!
//! Report a Bug… pre-fills a new GitHub issue (ported from Method Reader's
//! `feedback.rs`): an instruction, then a footer with the app's version and
//! the system's. No budget content is ever put in it. The body is cut to
//! [`MAX_BODY`] and any context passed is cleaned of file paths and the
//! home folder first ([`redact`]); the menu passes none.
//!
//! Licenses and Notices is answered by the page (ui/js/licenses.js), which
//! asks for the texts with the [`licenses`] command when it opens. The three
//! files are built into the app from the repository root.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// Help > User Guide.
pub const USER_GUIDE_ITEM: &str = "help.user_guide";
/// Help > Report a Bug….
pub const REPORT_BUG_ITEM: &str = "help.report_bug";
/// Help > Licenses and Notices (answered by the page).
pub const LICENSES_ITEM: &str = "help.licenses";
/// Help > methodproduction.com.
pub const SUITE_SITE_ITEM: &str = "help.suite_site";

/// The user guide.
pub const USER_GUIDE: &str =
    "https://github.com/method-production-suite/method-budget/blob/main/docs/APP.md";
/// A new issue on the app's repository; the body follows as `?body=`.
pub const NEW_ISSUE_BASE: &str =
    "https://github.com/method-production-suite/method-budget/issues/new";
/// The suite's website.
pub const SUITE_SITE: &str = "https://methodproduction.com";
/// The product's name, as the footer says it.
pub const PRODUCT: &str = "Method Budget";
/// About the longest body a report carries.
pub const MAX_BODY: usize = 2000;

const MIT: &str = include_str!("../../../LICENSE-MIT");
const APACHE: &str = include_str!("../../../LICENSE-APACHE");
const NOTICES: &str = include_str!("../../../THIRD-PARTY-NOTICES.md");

// ---------------------------------------------------------------------------
// The bug report

/// `application/x-www-form-urlencoded`, as URLSearchParams writes it.
pub fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A pre-filled issue: an instruction, the context cut to fit, and the
/// versions, which are never cut. `context` is `None` from the menu.
pub fn new_issue_url(
    app_version: &str,
    os_version: &str,
    context: Option<&str>,
    home: Option<&str>,
) -> String {
    let head = "<!-- Describe the problem: what you did, what you expected, and what happened. \
                Please don't paste anything from a budget you need to keep private. -->\n\n";
    let foot = format!("\n---\n{PRODUCT} {app_version} · {os_version}\n");
    let mut block = String::new();
    if let Some(context) = context.filter(|c| !c.trim().is_empty()) {
        let open = "\n**What happened:**\n";
        // Lengths in UTF-16 units, as the browser counts them.
        let units = |s: &str| s.encode_utf16().count();
        let room = MAX_BODY.saturating_sub(units(head) + units(&foot) + units(open) + 1);
        let mut said = redact(context, home);
        if units(&said) > room {
            let keep = room.saturating_sub(1);
            let utf16: Vec<u16> = said.encode_utf16().take(keep).collect();
            said = format!("{}…", String::from_utf16_lossy(&utf16));
        }
        block = format!("{open}{said}\n");
    }
    let body = format!("{head}{block}{foot}");
    format!("{NEW_ISSUE_BASE}?body={}", form_encode(&body))
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replace every match of a home-folder pattern (`/Users/name`,
/// `/home/name`, `C:\Users\name`) not preceded by a word char or `~`.
fn fold_homes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let free = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '~');
        if free {
            if let Some(end) = home_at(&chars, i) {
                out.push('~');
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Where a home-folder pattern starting at `i` ends, if one does.
fn home_at(chars: &[char], i: usize) -> Option<usize> {
    let rest: String = chars[i..].iter().take(12).collect();
    let (prefix_len, stops): (usize, &[char]) = if rest.starts_with("/Users/") {
        (7, &['/', '\'', '"', '`'])
    } else if rest.starts_with("/home/") {
        (6, &['/', '\'', '"', '`'])
    } else if chars.len() > i + 9
        && chars[i].is_ascii_alphabetic()
        && chars[i + 1..i + 9].iter().collect::<String>() == ":\\Users\\"
    {
        (9, &['\\', '\'', '"', '`'])
    } else {
        return None;
    };
    let start = i + prefix_len;
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() && !stops.contains(&chars[end]) {
        end += 1;
    }
    (end > start).then_some(end)
}

/// The length of the file name at the start of `rest`: up to its extension
/// when it has one (a name may hold spaces), else up to the first space.
fn name_length(rest: &[char]) -> usize {
    for (i, &c) in rest.iter().enumerate() {
        if matches!(c, '\\' | '/' | '\r' | '\n') {
            break;
        }
        if c != '.' {
            continue;
        }
        let k = rest[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric())
            .count();
        if !(1..=8).contains(&k) {
            continue;
        }
        let after = i + 1 + k;
        let ok = match rest.get(after) {
            None => true,
            Some(c) if c.is_whitespace() || ",;:)]".contains(*c) => true,
            Some('.') => rest.get(after + 1).is_none_or(|c| c.is_whitespace()),
            _ => false,
        };
        if ok {
            return after;
        }
    }
    rest.iter().take_while(|c| !c.is_whitespace()).count()
}

/// A message, safe to put in a public URL: the home folder becomes `~`, and
/// every path becomes `<path>` and its extension.
pub fn redact(message: &str, home: Option<&str>) -> String {
    let mut out = message.to_string();
    if let Some(home) = home.filter(|h| h.trim().chars().count() > 1) {
        let h = home.trim_end_matches(['/', '\\']);
        if !h.is_empty() {
            out = out.split(h).collect::<Vec<_>>().join("~");
        }
    }
    let out = fold_homes(&out);
    let chars: Vec<char> = out.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let free = i == 0 || chars[i - 1].is_whitespace() || "'\"`([".contains(chars[i - 1]);
        let mut j = i;
        if chars[j] == '~' {
            j += 1;
        } else if chars.len() > j + 1 && chars[j].is_ascii_alphabetic() && chars[j + 1] == ':' {
            j += 2;
        }
        if free && j < chars.len() && matches!(chars[j], '/' | '\\') {
            let mut end = j;
            while end < chars.len() && !"\r\n'\"`<>|".contains(chars[end]) {
                end += 1;
            }
            let path = &chars[i..end];
            let at = path
                .iter()
                .rposition(|c| matches!(c, '/' | '\\'))
                .unwrap_or(0);
            let rest = &path[at + 1..];
            let n = name_length(rest);
            let name: String = rest[..n].iter().collect();
            let ext = name
                .rfind('.')
                .map(|d| &name[d..])
                .filter(|e| {
                    (2..=9).contains(&e.len()) && e[1..].chars().all(|c| c.is_ascii_alphanumeric())
                })
                .unwrap_or("");
            result.push_str("<path>");
            result.push_str(ext);
            result.extend(&rest[n..]);
            i = end;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// The system's name and version for the footer, e.g. `macOS 15.6 (aarch64)`.
/// Asks the system the cheap way (`sw_vers`, `ver`, `/etc/os-release`); when
/// that fails, the name alone.
pub fn os_label() -> String {
    let arch = std::env::consts::ARCH;
    let name = os_version()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| os_name(std::env::consts::OS).to_string());
    format!("{name} ({arch})")
}

/// A friendlier name for `std::env::consts::OS`.
fn os_name(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

#[cfg(target_os = "macos")]
fn os_version() -> Option<String> {
    let out = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    let v = String::from_utf8(out.stdout).ok()?;
    Some(format!("macOS {}", v.trim()))
}

#[cfg(target_os = "windows")]
fn os_version() -> Option<String> {
    use std::os::windows::process::CommandExt;
    /// No console window flashes up from the app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    windows_version(&String::from_utf8_lossy(&out.stdout))
}

/// `Microsoft Windows [Version 10.0.22631.4317]` as `Windows 10.0.22631.4317`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_version(ver: &str) -> Option<String> {
    let v = ver.split("Version ").nth(1)?.split(']').next()?.trim();
    (!v.is_empty()).then(|| format!("Windows {v}"))
}

#[cfg(target_os = "linux")]
fn os_version() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    os_release_name(&text)
}

/// `PRETTY_NAME` from an os-release file, e.g. `Ubuntu 24.04.1 LTS`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn os_release_name(text: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))?;
    let v = line.trim().trim_matches(['"', '\'']).trim();
    (!v.is_empty()).then(|| v.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn os_version() -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// Opening a link

/// Whether `url` is one the app may open: the user guide, the website, or
/// a new issue whose body is form-encoded text and nothing else.
pub fn allowed_url(url: &str) -> bool {
    if url == USER_GUIDE || url == SUITE_SITE {
        return true;
    }
    let Some(body) = url
        .strip_prefix(NEW_ISSUE_BASE)
        .and_then(|q| q.strip_prefix("?body="))
    else {
        return false;
    };
    if body.is_empty() || body.len() > 6 * MAX_BODY {
        return false;
    }
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' | b'+' => i += 1,
            b'%' if b.len() > i + 2
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit() =>
            {
                i += 3
            }
            _ => return false,
        }
    }
    true
}

/// The program and arguments that open `url` in the default browser here.
fn opener(url: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "macos") {
        ("/usr/bin/open".into(), vec![url.into()])
    } else if cfg!(target_os = "windows") {
        // rundll32 hands the address to the default browser with no shell
        // in between, so nothing in it is read as a command.
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        (
            format!(r"{root}\System32\rundll32.exe"),
            vec!["url.dll,FileProtocolHandler".into(), url.into()],
        )
    } else {
        ("xdg-open".into(), vec![url.into()])
    }
}

/// Opens `url` in the default browser, if it is one [`allowed_url`] allows.
/// Waits for the opener (call it off the main thread).
pub fn open_url(url: &str) -> Result<(), String> {
    if !allowed_url(url) {
        return Err(crate::i18n::tr("help.link_not_opened"));
    }
    let (program, args) = opener(url);
    let status = std::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("Couldn't open the browser: {e}"))?;
    // rundll32 says nothing useful in its exit code.
    if status.success() || cfg!(target_os = "windows") {
        Ok(())
    } else {
        Err(crate::i18n::tr_with(
            "help.browser_not_opened",
            &[("status", status.to_string().as_str())],
        ))
    }
}

/// The link a Help item opens, if it is one of the link items.
fn link_for(id: &str) -> Option<String> {
    match id {
        USER_GUIDE_ITEM => Some(USER_GUIDE.into()),
        SUITE_SITE_ITEM => Some(SUITE_SITE.into()),
        REPORT_BUG_ITEM => Some(new_issue_url(
            env!("CARGO_PKG_VERSION"),
            &os_label(),
            None,
            None,
        )),
        _ => None,
    }
}

/// Menu items answered here: User Guide, Report a Bug… and
/// methodproduction.com. False for anything else (the caller passes it on).
/// A link that can't be opened is noted on standard error and nothing else.
pub fn handle_menu<R: Runtime>(_app: &AppHandle<R>, id: &str) -> bool {
    if !matches!(id, USER_GUIDE_ITEM | REPORT_BUG_ITEM | SUITE_SITE_ITEM) {
        return false;
    }
    let id = id.to_string();
    // Off the main thread: the version check and the opener both wait on a
    // program.
    std::thread::spawn(move || {
        if let Some(url) = link_for(&id) {
            if let Err(e) = open_url(&url) {
                eprintln!("{id}: {e}");
            }
        }
    });
    true
}

// ---------------------------------------------------------------------------
// Licenses and Notices

/// The texts Licenses and Notices shows, as plain text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LicensesDto {
    pub mit: &'static str,
    pub apache: &'static str,
    pub notices: &'static str,
}

/// The app's licences and the third-party notices (Help > Licenses and
/// Notices). Registered in app.rs, build.rs and capabilities/main.json; the
/// main window only.
#[tauri::command]
pub fn licenses() -> LicensesDto {
    LicensesDto {
        mit: MIT,
        apache: APACHE,
        notices: NOTICES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encoding_matches_url_search_params() {
        assert_eq!(form_encode("C++ a/b"), "C%2B%2B+a%2Fb");
        assert_eq!(form_encode("·"), "%C2%B7");
        assert_eq!(form_encode("a-b_c.d*"), "a-b_c.d*");
    }

    #[test]
    fn the_report_carries_the_versions_and_no_budget() {
        let url = new_issue_url("0.2.1", "macOS 15.6 (aarch64)", None, None);
        assert!(url.starts_with(
            "https://github.com/method-production-suite/method-budget/issues/new?body="
        ));
        assert!(url.contains("%3C%21--+Describe+the+problem"), "{url}");
        assert!(
            url.ends_with("Method+Budget+0.2.1+%C2%B7+macOS+15.6+%28aarch64%29%0A"),
            "{url}"
        );
        assert!(!url.contains("What+happened"));
        let blank = new_issue_url("0.2.1", "x", Some("  "), None);
        assert!(!blank.contains("What+happened"));
        let said = new_issue_url("0.2.1", "x", Some("couldn't save: disk full"), None);
        assert!(said.contains("couldn%27t+save%3A+disk+full"));
        // Everything the app builds, it may open.
        assert!(allowed_url(&url));
        assert!(allowed_url(&said));
    }

    #[test]
    fn a_long_context_is_cut_and_the_footer_never_is() {
        let long = "x".repeat(5000);
        let url = new_issue_url("0.2.1", "Linux", Some(&long), None);
        assert!(url.contains("%E2%80%A6"), "an ellipsis where it was cut");
        assert!(url.ends_with("Method+Budget+0.2.1+%C2%B7+Linux%0A"));
        let body = url.split("?body=").nth(1).unwrap();
        let x = body.matches('x').count();
        assert!(x < MAX_BODY && x > MAX_BODY / 2, "{x}");
        assert!(allowed_url(&url));
    }

    #[test]
    fn every_path_becomes_path_and_its_extension() {
        assert_eq!(
            redact("cannot read /Users/ann/Budgets/My Film.mthd (EACCES)", None),
            "cannot read <path>.mthd (EACCES)"
        );
        assert_eq!(
            redact("wrote C:\\Users\\ann\\Budgets\\Big Title.xlsx, done", None),
            "wrote <path>.xlsx, done"
        );
        assert_eq!(redact("in /srv/library", None), "in <path>");
        assert_eq!(redact("and/or https://x.y/z", None), "and/or https://x.y/z");
        assert_eq!(redact("'/a/b.mthd'", None), "'<path>.mthd'");
    }

    #[test]
    fn the_home_folder_becomes_a_tilde_first() {
        assert_eq!(redact("home is /Users/ann", None), "home is ~");
        assert_eq!(redact("home is /home/ann", None), "home is ~");
        assert_eq!(
            redact("home is /Users/ann", Some("/Users/ann/")),
            "home is ~"
        );
        assert_eq!(
            redact("in /data/riley/x.mthd", Some("/data/riley")),
            "in <path>.mthd"
        );
        let url = new_issue_url("0.2.1", "x", Some("at /Users/ann/Secret Film.mthd"), None);
        assert!(!url.contains("ann") && !url.contains("Secret"), "{url}");
    }

    #[test]
    fn only_the_help_links_may_be_opened() {
        assert!(allowed_url(USER_GUIDE));
        assert!(allowed_url(SUITE_SITE));
        let bug = |body: &str| format!("{NEW_ISSUE_BASE}?body={body}");
        assert!(allowed_url(&bug("Hello+%C2%B7+there")));
        for no in [
            "",
            "https://methodproduction.com/",
            "https://methodproduction.com.evil.example",
            "http://methodproduction.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            NEW_ISSUE_BASE,
            "https://github.com/method-production-suite/method-budget/issues/new?title=x",
            "https://github.com/someone-else/method-budget/issues/new?body=x",
        ] {
            assert!(!allowed_url(no), "{no}");
        }
        for body in [
            "",
            "a b",
            "a&labels=x",
            "a\"b",
            "a'b",
            "a;b",
            "a%2",
            "a%zz",
            "a#b",
            "a\nb",
            "%C2%B7%",
        ] {
            assert!(!allowed_url(&bug(body)), "{body:?}");
        }
        assert!(!allowed_url(&bug(&"a".repeat(6 * MAX_BODY + 1))));
        // The opener is never run for anything else.
        assert!(open_url("https://example.com").is_err());
    }

    #[test]
    fn the_opener_gets_the_link_as_one_argument() {
        let (program, args) = opener(SUITE_SITE);
        assert_eq!(args.last().map(String::as_str), Some(SUITE_SITE));
        if cfg!(target_os = "macos") {
            assert_eq!(program, "/usr/bin/open");
            assert_eq!(args.len(), 1);
        } else if cfg!(target_os = "windows") {
            assert!(program.ends_with(r"\System32\rundll32.exe"), "{program}");
            assert_eq!(args[0], "url.dll,FileProtocolHandler");
        } else {
            assert_eq!(program, "xdg-open");
        }
    }

    #[test]
    fn the_menu_links_are_the_allowed_ones() {
        for id in [USER_GUIDE_ITEM, REPORT_BUG_ITEM, SUITE_SITE_ITEM] {
            let url = link_for(id).unwrap();
            assert!(allowed_url(&url), "{id}: {url}");
        }
        assert_eq!(link_for(LICENSES_ITEM), None);
        assert_eq!(link_for("help.shortcuts"), None);
    }

    #[test]
    fn the_system_is_named() {
        let label = os_label();
        assert!(
            label.ends_with(&format!("({})", std::env::consts::ARCH)),
            "{label}"
        );
        assert!(label.len() > 3, "{label}");
        assert_eq!(
            windows_version("\r\nMicrosoft Windows [Version 10.0.22631.4317]\r\n").as_deref(),
            Some("Windows 10.0.22631.4317")
        );
        assert_eq!(windows_version("nothing"), None);
        assert_eq!(
            os_release_name("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n").as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert_eq!(os_release_name("NAME=x"), None);
        assert_eq!(os_name("macos"), "macOS");
        assert_eq!(os_name("freebsd"), "freebsd");
    }

    #[test]
    fn the_licenses_are_built_in() {
        let l = licenses();
        assert!(l.mit.contains("MIT License") || l.mit.contains("Permission is hereby granted"));
        assert!(l.apache.contains("Apache License"));
        assert!(l.notices.starts_with("# Third-party notices"));
        assert!(l.notices.len() > 100_000, "the whole notices file");
    }

    #[test]
    fn licenses_is_the_main_windows_only_and_drawn_as_text() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        const SETUP_CAPS: &str = include_str!("../capabilities/setup.json");
        const LICENSES_JS: &str = include_str!("../ui/js/licenses.js");
        const MAIN_JS: &str = include_str!("../ui/js/main.js");
        assert!(APP_RS.contains("crate::help::licenses,"));
        assert!(APP_RS.contains("crate::help::handle_menu(app, id)"));
        assert!(BUILD_RS.contains("\"licenses\","));
        assert!(MAIN_CAPS.contains("\"allow-licenses\""));
        assert!(!TOOLS_CAPS.contains("\"allow-licenses\""));
        assert!(!SETUP_CAPS.contains("\"allow-licenses\""));
        // The page answers Licenses and Notices; it never opens a link.
        assert!(LICENSES_JS.contains(&format!("'{LICENSES_ITEM}'")));
        assert!(MAIN_JS.contains("initLicenses"));
        for markup in ["innerHTML", "insertAdjacentHTML", "outerHTML"] {
            assert!(!LICENSES_JS.contains(markup), "{markup}");
        }
    }
}
