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
//! [`CLOSE`] closes the window in front (the main one asks first).
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
    item("file.open", "Open…", "mod+o"),
    SEP,
    item("file.save", "Save", "mod+s"),
    item("file.save_as", "Save As…", "mod+shift+s"),
    SEP,
    item("file.print_topsheet", "Print Topsheet…", "mod+shift+p"),
    item("file.print_budget", "Print Budget…", "mod+p"),
    SEP,
    item(CLOSE, "Close Window", "mod+w"),
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
    SEP,
    item("edit.note", "Note…", "mod+shift+space"),
    item("edit.make_range", "Make Fringe Range", "mod+r"),
    item("edit.remove_range", "Remove Fringe Range", "mod+shift+r"),
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

pub const VIEW: &[Entry] = &[Entry::ToolsPanel, Entry::SetupTabs];

/// View > Tools Panel: show or hide it wherever it is, and its three places
/// (docked at the right, docked at the bottom, its own window). The panel
/// header has the same three as buttons (ui/js/tools.js).
pub const TOOLS_PANEL: &[Entry] = &[
    item("view.toggle_panel", "Show or Hide Tools Panel", "mod+alt+s"),
    SEP,
    plain("view.panel.right", "Dock at Right"),
    plain("view.panel.bottom", "Dock at Bottom"),
    plain("view.panel.window", "Pop Out into a Window"),
];

/// The tools panel's tabs, in the panel's default order (ui/js/tools.js):
/// the top pane, the bottom pane, then the Budget tab's own tabs. Their keys
/// are the ones long-time users of other budgeting tools already know
/// (decisions.md, entry 15).
pub const SETUP_TABS: &[Entry] = &[
    item("view.tab.fringes", "Fringes", "mod+3"),
    item("view.tab.groups", "Groups", "mod+4"),
    item("view.tab.globals", "Globals", "mod+g"),
    item("view.tab.units", "Units", "mod+7"),
    SEP,
    item("view.tab.sets", "Sets", "mod+8"),
    item("view.tab.locations", "Locations", "mod+5"),
    item("view.tab.currencies", "Currency", "mod+6"),
    plain("view.tab.notes", "Notes"),
    SEP,
    item("view.tab.charges", "Charges", "mod+/"),
    item("view.tab.credits", "Credits", "mod+backslash"),
    plain("view.tab.info", "Info"),
    item("view.tab.settings", "Settings", "mod+9"),
    plain("view.tab.baseline", "Baseline"),
];

pub const HELP: &[Entry] = &[item("help.shortcuts", "Keyboard Shortcuts", "mod+shift+/")];

/// A shortcut-list key spec as a Tauri accelerator: `mod+shift+s` is
/// `CmdOrCtrl+Shift+S`.
pub fn accelerator(spec: &str) -> String {
    spec.split('+')
        .map(|part| match part {
            "mod" => "CmdOrCtrl".to_string(),
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
                let tabs = Submenu::new(app, "Tools Tab", true)?;
                append(app, &tabs, SETUP_TABS)?;
                menu.append(&tabs)?;
            }
            Entry::TextColors => {
                let colors = Submenu::new(app, "Text Color", true)?;
                append(app, &colors, TEXT_COLORS)?;
                menu.append(&colors)?;
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

    #[cfg(target_os = "macos")]
    {
        let window = Submenu::with_id(app, tauri::menu::WINDOW_SUBMENU_ID, "Window", true)?;
        window.append(&PredefinedMenuItem::minimize(app, None)?)?;
        window.append(&PredefinedMenuItem::maximize(app, None)?)?;
        window.append(&PredefinedMenuItem::fullscreen(app, None)?)?;
        menu.append(&window)?;
    }

    let help = Submenu::with_id(app, HELP_SUBMENU_ID, "&Help", true)?;
    append(app, &help, HELP)?;
    if !cfg!(target_os = "macos") {
        help.append(&PredefinedMenuItem::separator(app)?)?;
        help.append(&about)?;
    }
    menu.append(&help)?;
    Ok(menu)
}

/// Answers a menu item (other than Quit): Close Window here, everything else
/// in a web view: the tools window's when it is in front (it answers the
/// panel's own items, undo in its text fields among them, and passes the rest
/// to the main window), else the main window's.
pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let target = crate::tools::menu_target(crate::tools::tools_in_front(app));
    if id == CLOSE {
        // The window in front closes: the main one asks about unsaved
        // changes on its way out (CloseRequested); the tools one docks the
        // panel back.
        if let Some(w) = app.get_webview_window(target) {
            let _ = w.close();
        }
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
            FILE_QUIT,
            EDIT,
            INSERT,
            FORMAT,
            TEXT_COLORS,
            GO,
            VIEW,
            TOOLS_PANEL,
            SETUP_TABS,
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
