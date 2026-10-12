//! The native menu bar.
//!
//! Every command item carries an id and keys from the UI's shortcut list,
//! `ui/js/shortcuts.js` (a test below fails if the two drift apart). Choosing
//! one sends its id to the web view as a [`EVENT`] event, and `main.js` runs
//! the same handler the keyboard shortcut runs, so a menu click and a key
//! press always do the same thing. While the tools window is in front the
//! event goes to it instead (`toolswin.js` answers its own text fields and tab
//! items and passes the rest to `main.js`; src/tools.rs). Two ids are answered
//! here in Rust instead: [`QUIT`] goes through the unsaved-changes guard, and
//! [`CLOSE`] closes the window in front (the main one asks first). [`CLOSE_TAB`]
//! closes the tools window when it is in front, else goes to `main.js`, which
//! closes the budget's tab. The Window menu ends with the open budgets
//! (src/tabs.rs fills that list in).
//!
//! Who answers a key press: on macOS the web page sees a key first and the
//! menu's key equivalent fires only if the page didn't handle it. So `main.js`
//! leaves app-level keys (Save, Undo...) to these items there, and handles
//! grid keys (arrows, fill down, insert line...) itself, which keeps the menu
//! from firing a second time. On Windows and Linux `main.js` handles every
//! key and ignores a menu event that repeats a key it just handled. Cut, Copy
//! and Paste are the stock items: text fields edit natively, and the grid
//! answers the browser's clipboard events. See docs/APP.md (Keyboard).
//!
//! Items are never disabled: Undo and Redo also undo typing in a text field,
//! and every handler in `main.js` simply does nothing where it doesn't apply.

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The event the web view listens for; the payload is the item's id.
pub const EVENT: &str = "menu";
/// Quit, after asking about unsaved changes (handled in `app.rs`).
pub const QUIT: &str = "app.quit";
/// Close the window, which asks about unsaved changes on its way out.
pub const CLOSE: &str = "file.close";
/// Close the budget's tab (the window, when it is the only one; the tools
/// window, when it is in front).
pub const CLOSE_TAB: &str = "file.close_tab";
/// The Window menu's id (on macOS, the one the system adds its window list to).
#[cfg(target_os = "macos")]
pub const WINDOW_MENU: &str = tauri::menu::WINDOW_SUBMENU_ID;
#[cfg(not(target_os = "macos"))]
pub const WINDOW_MENU: &str = "window";
/// The last of the Window menu's own items; the open budgets follow it,
/// after a separator.
pub const LAST_WINDOW_ITEM: &str = "window.move_to_other_pane";

/// One menu entry. Keys use the shortcut list's syntax (`mod+shift+s`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Item {
        id: &'static str,
        label: &'static str,
        mac: Option<&'static str>,
        other: Option<&'static str>,
    },
    Separator,
    /// Stock text editing items (native in text fields; clipboard events in the grid).
    Cut,
    Copy,
    Paste,
    /// The tools panel: show or hide it, and where it goes, as a submenu.
    ToolsPanel,
    /// The tools panel's tabs, as a submenu.
    SetupTabs,
    /// Format > Text Color, as a submenu.
    TextColors,
    /// File > Export, as a submenu.
    Export,
}

use Entry::Separator as SEP;

const fn item(id: &'static str, label: &'static str, keys: &'static str) -> Entry {
    Entry::Item {
        id,
        label,
        mac: Some(keys),
        other: Some(keys),
    }
}

const fn per_platform(
    id: &'static str,
    label: &'static str,
    mac: &'static str,
    other: &'static str,
) -> Entry {
    Entry::Item {
        id,
        label,
        mac: Some(mac),
        other: Some(other),
    }
}

const fn plain(id: &'static str, label: &'static str) -> Entry {
    Entry::Item {
        id,
        label,
        mac: None,
        other: None,
    }
}

pub const FILE: &[Entry] = &[
    item("file.new", "New", "mod+n"),
    item("file.new_tab", "New Tab", "mod+t"),
    item("file.open", "Open…", "mod+o"),
    // Starting from another budget (src/templates.rs, src/import_setup.rs).
    plain("file.new_from_template", "New From Template…"),
    // A filtered copy in a new tab (src/sharing.rs).
    plain("file.new_sub_budget", "New Sub-Budget…"),
    SEP,
    item("file.save", "Save", "mod+s"),
    item("file.save_as", "Save As…", "mod+shift+s"),
    plain("file.save_as_template", "Save as Template…"),
    // A snapshot for someone else, opened read-only (src/sharing.rs).
    plain("file.save_share_copy", "Save a Copy for Sharing…"),
    // Working together through a shared folder (src/shared_app.rs).
    plain("file.share", "Share…"),
    SEP,
    plain("file.import_setup", "Import Setup From…"),
    // Setup packs: setup items to share, with where they came from (src/packs.rs).
    plain("file.import_setup_pack", "Import Setup Pack…"),
    plain("file.export_setup_pack", "Export Setup Pack…"),
    // Detail lines from a spreadsheet, with a column mapping (src/sheet_import.rs).
    plain("file.import_spreadsheet", "Import Spreadsheet…"),
    // A chart of accounts as a new budget, or applied to this one (src/coa_import.rs).
    plain("file.import_chart", "Import Chart of Accounts…"),
    // A whole budget from another program's export, reconciled (src/export_import.rs).
    plain("file.import_budget_export", "Import Budget Export…"),
    // Setup lists from a workbook or CSV files (src/setup_sheets.rs).
    plain("file.import_setup_lists", "Import Setup Lists…"),
    // Actual costs from an accounting export (src/actuals.rs).
    plain("file.import_actuals", "Import Actuals…"),
    SEP,
    // Named versions inside the budget, and comparing budgets (src/versions.rs,
    // src/compare.rs).
    plain("file.versions", "Versions…"),
    plain("file.compare", "Compare Budgets…"),
    // What changed since another budget, and merging a returned copy
    // (src/diffmerge.rs).
    plain("file.what_changed", "What Changed…"),
    plain("file.merge", "Merge Changes From…"),
    SEP,
    item("file.print_budget", "Print…", "mod+p"),
    item("file.print_topsheet", "Print Topsheet…", "mod+shift+p"),
    Entry::Export,
    SEP,
    item(CLOSE_TAB, "Close Tab", "mod+w"),
    item(CLOSE, "Close Window", "mod+shift+w"),
];

/// File > Export: the budget as spreadsheets, each through a Save dialog
/// (src/export.rs).
pub const EXPORT: &[Entry] = &[
    plain("file.export_xlsx", "Excel Workbook…"),
    plain("file.export_csv", "CSV Files…"),
    plain("file.export_tsv", "Tab-Delimited Detail…"),
    // Setup lists, a sheet or CSV file per kind (src/setup_sheets.rs).
    plain("file.export_setup_lists", "Setup Lists…"),
];

/// Windows and Linux end the File menu with Quit; macOS has it in the app menu.
pub const FILE_QUIT: &[Entry] = &[SEP, item(QUIT, "Quit", "mod+q")];

pub const EDIT: &[Entry] = &[
    item("edit.undo", "Undo", "mod+z"),
    per_platform("edit.redo", "Redo", "mod+shift+z", "mod+y"),
    SEP,
    Entry::Cut,
    Entry::Copy,
    Entry::Paste,
    // Whole rows with everything on them (ui/js/clipboard.js).
    plain("edit.cut_rows", "Cut Rows"),
    plain("edit.copy_rows", "Copy Rows"),
    plain("edit.paste_rows", "Paste Rows"),
    item("edit.select_all", "Select All", "mod+a"),
    SEP,
    item("edit.fill_down", "Fill Down", "mod+shift+d"),
    item("edit.delete_rows", "Delete Rows", "mod+d"),
    // Rows one place up or down (src/moverows.rs).
    item("edit.move_up", "Move Up", "alt+up"),
    item("edit.move_down", "Move Down", "alt+down"),
    SEP,
    item("edit.note", "Add Note…", "mod+shift+space"),
    item("edit.make_range", "Make Fringe Range", "mod+r"),
    item("edit.remove_range", "Remove Fringe Range", "mod+shift+r"),
    // How a total is worked out, and the overtime helper (ui/js/explain.js,
    // ui/js/overtime.js; the tools panel's Explain and Overtime tabs).
    item("edit.explain", "Explain This Number…", "mod+e"),
    plain("edit.overtime", "Overtime Helper…"),
    // Who changed a cell of a shared budget (src/shared_app.rs).
    plain("share.blame", "Who Changed This…"),
    SEP,
    // Find and replace: the Find tab of the tools panel (ui/js/find.js).
    item("edit.find", "Find…", "mod+f"),
    item("edit.replace", "Find and Replace…", "mod+shift+f"),
    item("edit.find_next", "Find Next", "f3"),
    item("edit.find_previous", "Find Previous", "shift+f3"),
];

pub const INSERT: &[Entry] = &[
    item("insert.line", "Line", "mod+i"),
    item("insert.subtotal", "Subtotal", "mod+="),
    item("insert.adjustment", "Adjustment", "mod+-"),
    plain("insert.separator", "Separator"),
    SEP,
    plain("insert.account", "Account…"),
    plain("insert.category", "Category…"),
    plain("insert.production_total", "Production Total…"),
];

/// Text style for the selected detail lines (ui/js/rowformat.js). Italic is
/// not on Cmd+I, which inserts a line.
pub const FORMAT: &[Entry] = &[
    item("format.bold", "Bold", "mod+b"),
    item("format.italic", "Italic", "mod+shift+i"),
    SEP,
    item("format.indent_more", "Indent More", "mod+alt+right"),
    item("format.indent_less", "Indent Less", "mod+alt+left"),
    SEP,
    Entry::TextColors,
    plain("format.clear", "Clear Formatting"),
];

/// The text colors, in the order of the palette in ui/js/rowformat.js.
pub const TEXT_COLORS: &[Entry] = &[
    plain("format.color.none", "No Color"),
    SEP,
    plain("format.color.red", "Red"),
    plain("format.color.orange", "Orange"),
    plain("format.color.gold", "Gold"),
    plain("format.color.green", "Green"),
    plain("format.color.teal", "Teal"),
    plain("format.color.blue", "Blue"),
    plain("format.color.purple", "Purple"),
    plain("format.color.pink", "Pink"),
];

pub const GO: &[Entry] = &[
    item("go.topsheet", "Topsheet", "mod+shift+up"),
    item("go.up", "Up a Level", "mod+up"),
    item("go.down", "Down a Level", "mod+down"),
    SEP,
    item("go.next_account", "Next Account", "mod+right"),
    item("go.prev_account", "Previous Account", "mod+left"),
];

pub const VIEW: &[Entry] = &[
    Entry::ToolsPanel,
    Entry::SetupTabs,
    SEP,
    // Split view: two panes of budget tabs side by side (ui/js/pane.js,
    // src/docs.rs). Cmd+\ is the Credits tab.
    item("view.split_right", "Split Right", "mod+shift+backslash"),
    plain("view.close_split", "Close Split"),
    SEP,
    // Per-user display choices (ui/js/display.js). Cmd+= inserts a subtotal,
    // so zooming is Option+Cmd+= and Option+Cmd+-.
    item(
        "view.formulas",
        "Show or Hide Formulas in Selection",
        "mod+alt+f",
    ),
    plain("view.hide_zero", "Show or Hide Zero Accounts"),
    plain("view.totals_cents", "Totals With or Without Cents"),
    SEP,
    item("view.zoom_in", "Zoom In", "mod+alt+="),
    item("view.zoom_out", "Zoom Out", "mod+alt+-"),
    item("view.zoom_reset", "Actual Size", "mod+alt+0"),
];

/// View > Tools Panel: show or hide it wherever it is, and its three places
/// (docked at the right, docked at the bottom, its own window). The panel
/// header has the same three as buttons (ui/js/tools.js).
pub const TOOLS_PANEL: &[Entry] = &[
    item("view.toggle_panel", "Show or Hide Tools Panel", "mod+alt+s"),
    SEP,
    plain("view.panel.right", "Dock at Right"),
    plain("view.panel.bottom", "Dock at Bottom"),
    plain("view.panel.window", "Pop Out into a Window"),
    SEP,
    // The panel's tabs, in its default order (ui/js/tools.js): the compact
    // apply lists show here; their full editors are in Budget Setup.
    plain("view.tool.fringes", "Fringes"),
    plain("view.tool.groups", "Groups"),
    plain("view.tool.globals", "Globals"),
    plain("view.tool.units", "Units"),
    plain("view.tab.presets", "Presets"),
    SEP,
    plain("view.tool.sets", "Sets"),
    plain("view.tool.locations", "Locations"),
    plain("view.tool.currencies", "Currency"),
    plain("view.tab.notes", "Notes"),
    plain("view.tab.library", "Library"),
    plain("view.tab.history", "History"),
    plain("view.tab.actuals", "Actuals"),
];

/// View > Budget Setup: the Budget Setup window (src/setupwin.rs), and each
/// of its tabs. Their keys are the ones long-time users of other budgeting
/// tools already know (decisions.md, entries 15 and 42): there they open the
/// setup window on that table too.
pub const SETUP_TABS: &[Entry] = &[
    plain("view.setup", "Open Budget Setup"),
    SEP,
    item("view.tab.fringes", "Fringes", "mod+3"),
    item("view.tab.groups", "Groups", "mod+4"),
    item("view.tab.globals", "Globals", "mod+g"),
    item("view.tab.units", "Units", "mod+7"),
    item("view.tab.sets", "Sets", "mod+8"),
    item("view.tab.locations", "Locations", "mod+5"),
    item("view.tab.currencies", "Currency", "mod+6"),
    SEP,
    item("view.tab.charges", "Charges", "mod+/"),
    item("view.tab.credits", "Credits", "mod+backslash"),
    plain("view.tab.info", "Info"),
    item("view.tab.settings", "Settings", "mod+9"),
    plain("view.tab.baseline", "Baseline"),
];

/// Budgets open as tabs: the next and previous tab, and the first two
/// (Cmd+3 to Cmd+9 are the Budget Setup tabs). The open budgets are listed after
/// these (src/tabs.rs). Ctrl+Tab and Ctrl+Shift+Tab work too (shortcuts.js).
pub const WINDOW: &[Entry] = &[
    item("window.next_tab", "Show Next Tab", "mod+shift+]"),
    item("window.prev_tab", "Show Previous Tab", "mod+shift+["),
    item("window.first_tab", "First Tab", "mod+1"),
    item("window.second_tab", "Second Tab", "mod+2"),
    SEP,
    // Split view (ui/js/pane.js): the budget in front to the other pane.
    plain(LAST_WINDOW_ITEM, "Move Tab to Other Pane"),
    SEP,
];

pub const HELP: &[Entry] = &[
    item("help.shortcuts", "Keyboard Shortcuts", "mod+shift+/"),
    // This version's notes, in the page (ui/js/updates.js). Check for
    // Updates and its automatic switch follow (src/updater.rs).
    plain("help.release_notes", "Release Notes"),
];

/// A shortcut-list key spec as a Tauri accelerator: `mod+shift+s` is
/// `CmdOrCtrl+Shift+S`.
pub fn accelerator(spec: &str) -> String {
    spec.split('+')
        .map(|part| match part {
            "mod" => "CmdOrCtrl".to_string(),
            "ctrl" => "Ctrl".to_string(),
            "tab" => "Tab".to_string(),
            "shift" => "Shift".to_string(),
            "alt" => "Alt".to_string(),
            "enter" => "Enter".to_string(),
            "backspace" => "Backspace".to_string(),
            "up" => "Up".to_string(),
            "down" => "Down".to_string(),
            "left" => "Left".to_string(),
            "right" => "Right".to_string(),
            "space" => "Space".to_string(),
            "backslash" => "Backslash".to_string(),
            "=" => "Equal".to_string(),
            "-" => "Minus".to_string(),
            key => key.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn append<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Submenu<R>,
    entries: &[Entry],
) -> tauri::Result<()> {
    for entry in entries {
        match *entry {
            Entry::Item {
                id,
                label,
                mac,
                other,
            } => {
                let keys = if cfg!(target_os = "macos") {
                    mac
                } else {
                    other
                };
                let accel = keys.map(accelerator);
                menu.append(&MenuItem::with_id(app, id, label, true, accel.as_deref())?)?;
            }
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Entry::Cut => menu.append(&PredefinedMenuItem::cut(app, None)?)?,
            Entry::Copy => menu.append(&PredefinedMenuItem::copy(app, None)?)?,
            Entry::Paste => menu.append(&PredefinedMenuItem::paste(app, None)?)?,
            Entry::ToolsPanel => {
                let panel = Submenu::new(app, "Tools Panel", true)?;
                append(app, &panel, TOOLS_PANEL)?;
                menu.append(&panel)?;
            }
            Entry::SetupTabs => {
                let tabs = Submenu::new(app, "Budget Setup", true)?;
                append(app, &tabs, SETUP_TABS)?;
                menu.append(&tabs)?;
            }
            Entry::TextColors => {
                let colors = Submenu::new(app, "Text Color", true)?;
                append(app, &colors, TEXT_COLORS)?;
                menu.append(&colors)?;
            }
            Entry::Export => {
                let export = Submenu::new(app, "Export", true)?;
                append(app, &export, EXPORT)?;
                menu.append(&export)?;
            }
        }
    }
    Ok(())
}

fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    entries: &[Entry],
) -> tauri::Result<Submenu<R>> {
    let menu = Submenu::new(app, title, true)?;
    append(app, &menu, entries)?;
    Ok(menu)
}

/// Builds the menu bar. `&` marks the Alt-key letter on Windows and Linux
/// (macOS ignores it).
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let name = app.package_info().name.clone();
    // Labels name the product, not the executable (which differs in dev builds).
    let about = PredefinedMenuItem::about(
        app,
        Some(&format!("About {name}")),
        Some(AboutMetadata {
            name: Some(name.clone()),
            version: Some(app.package_info().version.to_string()),
            // A link on Windows and Linux; macOS shows the credits line.
            website: Some(crate::app::WEBSITE.into()),
            website_label: Some("methodbudget.com".into()),
            credits: Some("methodbudget.com".into()),
            ..Default::default()
        }),
    )?;
    let menu = Menu::new(app)?;

    #[cfg(target_os = "macos")]
    {
        // The app menu. Our own Quit item goes through the unsaved-changes
        // guard; the stock one would quit without asking.
        let app_menu = Submenu::new(app, &name, true)?;
        app_menu.append(&about)?;
        app_menu.append(&PredefinedMenuItem::separator(app)?)?;
        app_menu.append(&PredefinedMenuItem::hide(
            app,
            Some(&format!("Hide {name}")),
        )?)?;
        app_menu.append(&PredefinedMenuItem::hide_others(app, None)?)?;
        app_menu.append(&PredefinedMenuItem::separator(app)?)?;
        app_menu.append(&MenuItem::with_id(
            app,
            QUIT,
            format!("Quit {name}"),
            true,
            Some(accelerator("mod+q")),
        )?)?;
        menu.append(&app_menu)?;
    }

    let file = submenu(app, "&File", FILE)?;
    // Open Recent, Autosave and Revert to Backup… (src/safety_app.rs).
    crate::safety_app::extend_file_menu(app, &file)?;
    if !cfg!(target_os = "macos") {
        append(app, &file, FILE_QUIT)?;
    }
    menu.append(&file)?;
    menu.append(&submenu(app, "&Edit", EDIT)?)?;
    menu.append(&submenu(app, "&Insert", INSERT)?)?;
    menu.append(&submenu(app, "F&ormat", FORMAT)?)?;
    menu.append(&submenu(app, "&Go", GO)?)?;
    menu.append(&submenu(app, "&View", VIEW)?)?;

    let window = Submenu::with_id(app, WINDOW_MENU, "&Window", true)?;
    #[cfg(target_os = "macos")]
    {
        window.append(&PredefinedMenuItem::minimize(app, None)?)?;
        window.append(&PredefinedMenuItem::maximize(app, None)?)?;
        window.append(&PredefinedMenuItem::fullscreen(app, None)?)?;
        window.append(&PredefinedMenuItem::separator(app)?)?;
    }
    append(app, &window, WINDOW)?;
    menu.append(&window)?;

    let help = Submenu::with_id(app, HELP_SUBMENU_ID, "&Help", true)?;
    append(app, &help, HELP)?;
    crate::updater::extend_help_menu(app, &help)?;
    if !cfg!(target_os = "macos") {
        help.append(&PredefinedMenuItem::separator(app)?)?;
        help.append(&about)?;
    }
    menu.append(&help)?;
    Ok(menu)
}

/// Answers a menu item (other than Quit): Close Window here, everything else
/// in a web view: the tools window's or the Budget Setup window's when it is
/// in front (each answers its own items, undo in its text fields among them,
/// and passes the rest to the main window), else the main window's.
pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let tools_in_front = crate::tools::tools_in_front(app);
    // The Budget Setup window, like the tools window (src/setupwin.rs).
    let setup_in_front = crate::setupwin::in_front(app);
    let target = crate::setupwin::menu_target(tools_in_front, setup_in_front);
    if id == CLOSE || (id == CLOSE_TAB && (tools_in_front || setup_in_front)) {
        // The window in front closes: the main one asks about unsaved
        // changes on its way out (CloseRequested); the tools one docks the
        // panel back.
        if let Some(w) = app.get_webview_window(target) {
            let _ = w.close();
        }
        return;
    }
    if id.starts_with(crate::tabs::DOC_PREFIX) {
        // Window > a budget: the main window brings its tab forward. The
        // item ticked or unticked itself, so the list is drawn again.
        crate::tabs::redraw_window_menu(app);
        let _ = app.emit_to(crate::tools::MAIN, EVENT, id);
        return;
    }
    let _ = app.emit_to(target, EVENT, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORTCUTS_JS: &str = include_str!("../ui/js/shortcuts.js");

    /// Every command item in the menus, with its keys.
    fn items() -> Vec<Entry> {
        [
            FILE,
            EXPORT,
            FILE_QUIT,
            EDIT,
            INSERT,
            FORMAT,
            TEXT_COLORS,
            GO,
            VIEW,
            TOOLS_PANEL,
            SETUP_TABS,
            WINDOW,
            HELP,
        ]
        .iter()
        .flat_map(|m| m.iter().copied())
        .filter(|e| matches!(e, Entry::Item { .. }))
        .collect()
    }

    /// The text after `key: '` (or `key: ['`) up to the next quote.
    fn quoted_after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        let at = line.find(&format!("{key}: "))? + key.len() + 2;
        let rest = line[at..].trim_start_matches('[');
        let rest = rest.strip_prefix('\'')?;
        Some(&rest[..rest.find('\'')?])
    }

    /// (mac keys, other keys, scope) for an id in shortcuts.js; the first
    /// alternative when there are several.
    fn js_entry(id: &str) -> Option<(Option<&str>, Option<&str>, &str)> {
        let needle = format!("{{ id: '{id}',");
        let line = SHORTCUTS_JS.lines().find(|l| l.contains(&needle))?;
        let scope = quoted_after(line, "scope").expect("every entry has a scope");
        if let Some(k) = quoted_after(line, "keys") {
            return Some((Some(k), Some(k), scope));
        }
        Some((
            quoted_after(line, "mac"),
            quoted_after(line, "other"),
            scope,
        ))
    }

    #[test]
    fn menu_keys_match_the_shortcut_list() {
        for e in items() {
            let Entry::Item { id, mac, other, .. } = e else {
                unreachable!()
            };
            let (js_mac, js_other, _) =
                js_entry(id).unwrap_or_else(|| panic!("{id} is missing from shortcuts.js"));
            assert_eq!(mac, js_mac, "macOS keys for {id}");
            assert_eq!(other, js_other, "Windows/Linux keys for {id}");
        }
    }

    #[test]
    fn every_app_and_grid_shortcut_has_a_menu_item() {
        // App-level keys only work through the menu on macOS, so each one
        // needs an item; grid keys get one so they can be found and clicked.
        let ids: Vec<&str> = items()
            .into_iter()
            .map(|e| match e {
                Entry::Item { id, .. } => id,
                _ => unreachable!(),
            })
            .collect();
        let mut checked = 0;
        for line in SHORTCUTS_JS.lines() {
            let Some(id) = quoted_after(line, "{ id") else {
                continue;
            };
            let scope = quoted_after(line, "scope").unwrap();
            if matches!(scope, "app" | "grid" | "menu") {
                assert!(ids.contains(&id), "{id} ({scope}) has no menu item");
                checked += 1;
            }
        }
        assert!(checked > 30, "only found {checked} entries in shortcuts.js");
    }

    #[test]
    fn no_two_items_share_keys() {
        for (mac, pick) in [(true, 0), (false, 1)] {
            let mut seen = std::collections::BTreeMap::new();
            for e in items() {
                let Entry::Item {
                    id, mac: m, other, ..
                } = e
                else {
                    unreachable!()
                };
                let keys = [m, other][pick];
                if let Some(k) = keys {
                    if let Some(prev) = seen.insert(k, id) {
                        panic!("{prev} and {id} both use {k} (mac: {mac})");
                    }
                }
            }
        }
    }

    #[test]
    fn accelerators() {
        assert_eq!(accelerator("mod+shift+s"), "CmdOrCtrl+Shift+S");
        assert_eq!(accelerator("mod+alt+t"), "CmdOrCtrl+Alt+T");
        assert_eq!(accelerator("mod+enter"), "CmdOrCtrl+Enter");
        assert_eq!(accelerator("mod+backspace"), "CmdOrCtrl+Backspace");
        assert_eq!(accelerator("mod+="), "CmdOrCtrl+Equal");
        assert_eq!(accelerator("mod+shift+up"), "CmdOrCtrl+Shift+Up");
        assert_eq!(accelerator("mod+]"), "CmdOrCtrl+]");
        assert_eq!(accelerator("mod+/"), "CmdOrCtrl+/");
        assert_eq!(accelerator("mod+1"), "CmdOrCtrl+1");
        assert_eq!(accelerator("mod+-"), "CmdOrCtrl+Minus");
        assert_eq!(accelerator("mod+shift+space"), "CmdOrCtrl+Shift+Space");
        assert_eq!(accelerator("mod+left"), "CmdOrCtrl+Left");
        assert_eq!(accelerator("mod+backslash"), "CmdOrCtrl+Backslash");
        assert_eq!(accelerator("mod+shift+/"), "CmdOrCtrl+Shift+/");
        assert_eq!(accelerator("mod+g"), "CmdOrCtrl+G");
        assert_eq!(accelerator("mod+alt+right"), "CmdOrCtrl+Alt+Right");
        assert_eq!(accelerator("mod+shift+i"), "CmdOrCtrl+Shift+I");
        assert_eq!(accelerator("mod+t"), "CmdOrCtrl+T");
        assert_eq!(accelerator("mod+shift+]"), "CmdOrCtrl+Shift+]");
        assert_eq!(accelerator("ctrl+shift+tab"), "Ctrl+Shift+Tab");
        assert_eq!(accelerator("f3"), "F3");
        assert_eq!(accelerator("shift+f3"), "Shift+F3");
        assert_eq!(accelerator("mod+shift+f"), "CmdOrCtrl+Shift+F");
        assert_eq!(accelerator("alt+down"), "Alt+Down");
        assert_eq!(accelerator("mod+alt+="), "CmdOrCtrl+Alt+Equal");
        assert_eq!(accelerator("mod+alt+-"), "CmdOrCtrl+Alt+Minus");
        assert_eq!(accelerator("mod+alt+0"), "CmdOrCtrl+Alt+0");
    }

    #[test]
    fn text_colors_match_the_palette() {
        // Each Format > Text Color item names a color rowformat.js knows.
        const ROWFORMAT_JS: &str = include_str!("../ui/js/rowformat.js");
        let mut n = 0;
        for e in TEXT_COLORS {
            let Entry::Item { id, .. } = e else { continue };
            let color = id.strip_prefix("format.color.").unwrap();
            if color != "none" {
                assert!(
                    ROWFORMAT_JS.contains(&format!("{{ id: '{color}',")),
                    "{color} is missing from rowformat.js"
                );
                n += 1;
            }
        }
        assert_eq!(n, 8);
    }
}
