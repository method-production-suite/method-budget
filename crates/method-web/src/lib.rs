//! Method Budget in a browser: [`method_session::Host`] compiled to
//! WebAssembly with a small JS API, for the existing page
//! (`crates/method-app/ui/`) to run against through `js/tauri-shim.js`
//! (docs/research/web-session.md, phase 2).
//!
//! The JS API (wasm-bindgen, `--target web`):
//!
//! ```js
//! import init, { Host, commands } from './method_web.js';
//! await init();
//! const host = new Host();                     // one untitled starter budget
//! const view = JSON.parse(host.dispatch('get_view', '{}'));
//! host.openMthd('Pilot.mthd', text);           // the view, as JSON
//! const { name, text } = host.saveMthd();      // the budget in front
//! host.setReadOnly(true);                      // a viewer
//! JSON.parse(host.hostInfo());                 // {kind: "web", commands_version, read_only}
//! commands();                                  // every command dispatch runs
//! host.setCharts(indexJson);                   // the chart list (charts/index.json)
//! host.addChart('feature', text);              // a chart's text, once it is picked
//! ```
//!
//! The chart-of-accounts templates aren't compiled in (6 MB): `cargo xtask
//! web` puts them beside the page as `charts/index.json` and
//! `charts/<id>.mthd`, and the shim fetches the list when New From Template
//! asks for it and a chart's file when one is picked.
//!
//! `dispatch` and the others throw an `Error` whose `message` is the text the
//! page shows. The clock is the browser's (`Date.now()`), installed by the
//! first `new Host()`; names that must be unique are seeded from
//! `crypto.getRandomValues`.
//!
//! On the desktop the crate holds only [`mthd_name`]: everything else is for
//! wasm32 alone.

#[cfg(target_arch = "wasm32")]
mod web;

/// The name a budget is downloaded under: `name` with the `.mthd`
/// extension (an opened export or a budget saved as `.json` saves as a
/// budget file), and a plain name when it has none.
pub fn mthd_name(name: &str) -> String {
    let name = name.trim();
    let ext = method_format::EXTENSION;
    if name.is_empty() {
        return format!("Untitled.{ext}");
    }
    let stem = match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() && e.eq_ignore_ascii_case(ext) => return name.into(),
        Some((stem, e)) if !stem.is_empty() && !e.contains(' ') && e.len() <= 5 => stem,
        _ => name,
    };
    format!("{stem}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_download_as_mthd_files() {
        assert_eq!(mthd_name("Pilot.mthd"), "Pilot.mthd");
        assert_eq!(mthd_name("Pilot.MTHD"), "Pilot.MTHD");
        assert_eq!(mthd_name("pilot.json"), "pilot.mthd");
        assert_eq!(mthd_name("Feature budget.xml"), "Feature budget.mthd");
        assert_eq!(mthd_name("Draft 2.0 notes"), "Draft 2.0 notes.mthd");
        assert_eq!(mthd_name("Untitled"), "Untitled.mthd");
        assert_eq!(mthd_name("  "), "Untitled.mthd");
        assert_eq!(mthd_name(".mthd"), ".mthd.mthd");
    }
}
