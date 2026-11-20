//! New Budget: the form, charts to start from, setup and Save In.

use super::*;

// ---------------------------------------------------------------------------
// New Budget

/// The kinds of production the form offers.
const KINDS: &[(&str, &str)] = &[
    ("feature", "Feature"),
    ("tv_episode", "TV episode"),
    ("tv_season", "TV season"),
    ("commercial", "Commercial"),
    ("documentary", "Documentary"),
    ("other", "Other"),
];

/// New Budget's form. Every text is as typed; blank leaves a field out.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewBudgetSpec {
    pub title: String,
    /// `feature`, `tv_episode`, `tv_season`, `commercial`, `documentary` or
    /// `other`.
    pub kind: String,
    /// TV only.
    pub season: String,
    /// A TV episode's number.
    pub episode: String,
    /// A TV season's episode count.
    pub episodes: String,
    /// Dates, `YYYY-MM-DD`.
    pub prep_start: String,
    pub shoot_start: String,
    pub shoot_end: String,
    pub wrap_end: String,
    /// The schedule's length: globals `PREP_WEEKS`, `SHOOT_DAYS`,
    /// `WRAP_WEEKS`.
    pub prep_weeks: String,
    pub shoot_days: String,
    pub wrap_weeks: String,
    pub country: String,
    pub region: String,
    pub city: String,
    /// The base currency's code (blank: USD).
    pub currency: String,
    /// `starter`, `blank`, `chart` (with `chart`) or `file` (with
    /// `template`, from `new_budget_pick_template`).
    pub start: String,
    pub chart: String,
    pub template: Option<u64>,
    /// `budget`, `production_level`, `category` or `account` (blank: as the
    /// starting point has it).
    pub fringe_posting: String,
    /// `binary` or `exact` (blank: as the starting point has it).
    pub arithmetic: String,
    /// Save In: the folder's token (from `new_budget_folder` or
    /// `new_budget_pick_folder`). `None`: the budget opens with no file.
    pub folder: Option<u64>,
    /// The file's name, with or without `.mthd` (blank: from the title).
    pub file_name: String,
    /// Setup: bring in everything in the setup library.
    pub library: bool,
    /// Setup: the packs to bring in, by their tokens from
    /// `new_budget_setup`'s listing.
    pub packs: Vec<u64>,
    /// Save In: make a project folder named like the file in the folder,
    /// save the budget in it and list it with the projects.
    pub project: bool,
    /// Link a Schedule: the token of a schedule days file picked with
    /// `schedule_days_pick` (its shoot days and dates come in, linked).
    pub schedule: Option<u64>,
}

fn text(field: &str, value: &str, max: usize) -> Result<String, String> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(format!("{field} is too long: at most {max} characters."));
    }
    if v.chars().any(char::is_control) {
        return Err(format!("{field} has to fit on one line."));
    }
    Ok(v.to_string())
}

/// An edit function's refusal `e` as a sentence about `field`.
fn said(field: &str, e: &str) -> String {
    format!("{field}: {}.", e.trim().trim_end_matches('.'))
}

/// The budget the form describes, starting from `template` when the form
/// says `file`, with the `setup` ticked (the library, packs) brought in
/// before the form's own details, and a sentence saying what it holds.
pub fn build(
    spec: &NewBudgetSpec,
    template: Option<&Budget>,
    setup: &[Ticked],
) -> Result<(Budget, String), String> {
    build_with(spec, template, setup, None)
}

/// [`build`], with a linked schedule's shoot days and dates
/// (`new_budget::link_schedule`) after the ticked setup and before the
/// form's own details, so what is typed in the form wins.
pub fn build_with(
    spec: &NewBudgetSpec,
    template: Option<&Budget>,
    setup: &[Ticked],
    schedule: Option<&method_format::import::schedule::ScheduleDays>,
) -> Result<(Budget, String), String> {
    let title = text("The title", &spec.title, TITLE_MAX)?;
    if title.is_empty() {
        return Err(crate::i18n::tr("start.give_a_title"));
    }
    let kind = KINDS
        .iter()
        .find(|(k, _)| *k == spec.kind.trim())
        .map(|(_, label)| *label)
        .ok_or_else(|| format!("Unknown production type {:?}.", spec.kind))?;
    let tv = kind.starts_with("TV");
    let currency = match text("The currency", &spec.currency, 10)?.to_uppercase() {
        c if c.is_empty() => "USD".to_string(),
        c => c,
    };

    let mut b = match spec.start.trim() {
        "starter" | "" => starter::budget(),
        "blank" => starter::blank(title.as_str(), currency.as_str()),
        "chart" => {
            let chart = method_format::charts::find(&spec.chart)
                .ok_or("That chart of accounts isn't bundled with this version.")?;
            let b = chart
                .budget()
                .map_err(|e| format!("The {} chart can't be read: {e}.", chart.name))?;
            crate::templates::strip(&b, false, &title)
        }
        "file" => crate::templates::strip(
            template.ok_or("Choose the budget file to start from again.")?,
            false,
            &title,
        ),
        other => return Err(format!("Unknown starting point {other:?}.")),
    };
    edit::set_title(&mut b, &title)?;
    if b.base_currency != currency {
        if b.currencies.contains_key(&currency) {
            return Err(format!(
                "The budget you start from lists {currency} as a foreign currency, so it can't be the base currency here. Pick another currency, or start from something else."
            ));
        }
        edit::set_base_currency(&mut b, &currency).map_err(|e| said("Currency", &e))?;
    }

    // The ticked setup, before the details typed here, so those win (a
    // library's SHOOT_DAYS gives way to the Shoot days typed).
    let brought = new_budget::bring_in(&mut b, setup)?;
    let linked = schedule
        .map(|days| new_budget::link_schedule(&mut b, days))
        .transpose()?;

    // Budget info, in plain words.
    let mut info = |name: &str, kind: &str, value: &str| -> Result<(), String> {
        let value = text(name, value, TEXT_MAX)?;
        if value.is_empty() {
            return Ok(());
        }
        let input = InfoInput {
            kind: kind.into(),
            value,
        };
        edit::set_info(&mut b, name, &input).map_err(|e| said(name, &e))
    };
    info("Production type", "text", kind)?;
    if tv {
        info("Season", "text", &spec.season)?;
    }
    if kind == "TV episode" {
        info("Episode", "text", &spec.episode)?;
    }
    if kind == "TV season" && !spec.episodes.trim().is_empty() {
        let n: u32 = spec
            .episodes
            .trim()
            .parse()
            .ok()
            .filter(|n| (1..=999).contains(n))
            .ok_or("Episodes is a whole number from 1 to 999.")?;
        info("Episodes", "number", &n.to_string())?;
    }
    for (name, value) in [
        ("Prep start", &spec.prep_start),
        ("Shoot start", &spec.shoot_start),
        ("Shoot end", &spec.shoot_end),
        ("Wrap end", &spec.wrap_end),
    ] {
        info(name, "date", value)?;
    }
    for (name, value) in [
        ("Country", &spec.country),
        ("Region", &spec.region),
        ("City", &spec.city),
    ] {
        info(name, "text", value)?;
    }

    // The schedule's length, as the starter's globals.
    for (label, global, value) in [
        ("Prep weeks", "PREP_WEEKS", &spec.prep_weeks),
        ("Shoot days", "SHOOT_DAYS", &spec.shoot_days),
        ("Wrap weeks", "WRAP_WEEKS", &spec.wrap_weeks),
    ] {
        let v = value.trim();
        if v.is_empty() {
            continue;
        }
        let n: Dec = v
            .replace(',', "")
            .parse()
            .map_err(|_| format!("{label} is a number, like 4 or 2.5."))?;
        if n < Dec::ZERO {
            return Err(format!("{label} can't be below 0."));
        }
        edit::set_global(&mut b, global, &n.to_string()).map_err(|e| said(label, &e))?;
    }

    if !spec.fringe_posting.trim().is_empty() {
        edit::set_fringe_posting(&mut b, spec.fringe_posting.trim(), None)
            .map_err(|e| said("Fringe posting", &e))?;
    }
    if !spec.arithmetic.trim().is_empty() {
        edit::set_arithmetic(&mut b, &spec.arithmetic).map_err(|e| said("Arithmetic", &e))?;
    }

    let categories = b.categories().count();
    let accounts: usize = b.categories().map(|c| c.accounts.len()).sum();
    let lines: usize = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.details.len())
        .sum();
    let n = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let message = if lines > 0 {
        format!(
            "New budget \"{title}\" with {}, {} and {}.",
            n(categories, "category", "categories"),
            n(accounts, "account", "accounts"),
            n(lines, "line", "lines")
        )
    } else {
        format!(
            "New budget \"{title}\" with {} and {}.",
            n(categories, "category", "categories"),
            n(accounts, "account", "accounts")
        )
    };
    let message = match brought.sentence() {
        Some(more) => format!("{message} {more}"),
        None => message,
    };
    let message = match linked {
        Some(more) => format!("{message} {more}"),
        None => message,
    };
    Ok((b, message))
}

/// Opens `b` in front, with no file, as one undo step ("New budget"): in
/// the place of an untouched Untitled budget alone in the window, else in a
/// new tab. Its id.
pub fn open_new(state: &AppState, safety: &Safety, b: Budget) -> Result<DocId, String> {
    let mut s = Session::with_budget(
        starter::blank(b.title.as_str(), b.base_currency.as_str()),
        None,
    );
    s.edit(
        method_session::messages::word("engine.undo.new_budget"),
        |x| {
            *x = b;
            Ok(())
        },
    )?;
    let instead = state.docs().lone_untouched();
    crate::safety_app::put_in_front(state, safety, s, instead)
}

// ---------------------------------------------------------------------------
// Charts of accounts to start from

/// A bundled chart of accounts, as New Budget lists it.
#[derive(Clone, Debug, Serialize)]
pub struct ChartChoice {
    pub id: &'static str,
    pub name: &'static str,
    /// `Feature`, `Television`, `Commercial` or `Other`.
    pub group: &'static str,
    pub description: &'static str,
    /// Our own chart, not one that follows a studio's or network's
    /// (those are numbers, names and levels only, decision 51).
    pub ours: bool,
    pub categories: usize,
    pub accounts: usize,
}

/// Every bundled chart with its size, read once per run.
pub fn chart_choices() -> &'static [ChartChoice] {
    static CHOICES: OnceLock<Vec<ChartChoice>> = OnceLock::new();
    CHOICES.get_or_init(|| {
        method_format::charts::all()
            .iter()
            .map(|c| {
                let (categories, accounts) = c
                    .budget()
                    .map(|b| {
                        (
                            b.categories().count(),
                            b.categories().map(|c| c.accounts.len()).sum(),
                        )
                    })
                    .unwrap_or((0, 0));
                ChartChoice {
                    id: c.id,
                    name: c.name,
                    group: c.group,
                    description: c.description,
                    // Our charts are described as ours by the manifest
                    // (templates/charts/charts.json); the studios' as
                    // following one.
                    ours: c.description.starts_with("Our own"),
                    categories,
                    accounts,
                }
            })
            .collect()
    })
}

// ---------------------------------------------------------------------------
// Setup: the library and setup packs to bring in

/// The library, as New Budget's setup step offers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryChoice {
    /// Its file name without the extension.
    pub name: String,
    /// How many setup items it holds.
    pub items: usize,
    /// Why it can't be read (moved, deleted, not a budget).
    pub problem: Option<String>,
}

/// What New Budget's setup step offers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SetupChoices {
    /// The setup library (`None`: no library picked yet).
    pub library: Option<LibraryChoice>,
    /// The Packs list (the user's Packs folder, then the bundled packs),
    /// each named by its token.
    pub packs: Vec<ListedPack>,
}

/// How many setup items `b` holds that New Budget brings (no settings).
fn setup_items(b: &Budget) -> usize {
    crate::import_setup::all_items(b)
        .into_iter()
        .filter(|(w, _)| !w.is_setting())
        .count()
}

/// The library and the packs to offer, listing the packs again (their
/// tokens are the Packs list's).
pub fn setup_choices(docs: &Docs, library: &Library, packs: &Packs) -> SetupChoices {
    let library = library.path().map(|path| {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match crate::library::read(docs, &path) {
            Ok((b, _)) => LibraryChoice {
                name,
                items: setup_items(&b),
                problem: None,
            },
            Err(e) => LibraryChoice {
                name,
                items: 0,
                problem: Some(e),
            },
        }
    });
    SetupChoices {
        library,
        packs: crate::packs::list(packs).packs,
    }
}

/// The setup the form ticked, read: the library first (from its tab when it
/// is open), then the packs in the order listed.
pub fn ticked_setup(
    docs: &Docs,
    library: &Library,
    packs: &Packs,
    spec: &NewBudgetSpec,
) -> Result<Vec<Ticked>, String> {
    let mut out = Vec::new();
    if spec.library {
        let path = library.path().ok_or(
            "There's no setup library yet. Untick it, or make one in the Library tab first.",
        )?;
        let (budget, _) = crate::library::read(docs, &path)
            .map_err(|e| format!("Your library can't be read: {}.", e.trim_end_matches('.')))?;
        out.push(Ticked {
            label: "your library".into(),
            budget,
        });
    }
    let mut seen = Vec::new();
    for &token in &spec.packs {
        if seen.contains(&token) {
            continue;
        }
        seen.push(token);
        let path = packs.listed(token)?;
        let (budget, _) = crate::session::read_budget(&path)?;
        let label = method_core::pack::info(&budget)
            .map(|i| i.title.trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "a pack".into());
        out.push(Ticked { label, budget });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Save In

/// The folder New Budget offers first: the one the last new budget was
/// saved in, else Documents, else the home folder (each only while it is a
/// folder).
pub(super) fn default_folder(
    last: Option<PathBuf>,
    documents: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    [last, documents, home]
        .into_iter()
        .flatten()
        .find(|d| d.is_dir())
}

/// The file name's stem from what was typed (blank: from `title`), without
/// the budget extension. Refused when it can't be a file's name.
pub fn file_stem_for(typed: &str, title: &str) -> Result<String, String> {
    let typed = typed.trim();
    let ext = format!(".{}", method_format::EXTENSION);
    let typed = match typed.len().checked_sub(ext.len()) {
        Some(at) if typed.is_char_boundary(at) && typed[at..].eq_ignore_ascii_case(&ext) => {
            typed[..at].trim()
        }
        _ => typed,
    };
    if typed.is_empty() {
        return Ok(safe_file_stem(title));
    }
    if typed.chars().count() > TITLE_MAX {
        return Err(format!(
            "The file name is too long: at most {TITLE_MAX} characters."
        ));
    }
    if typed
        .chars()
        .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return Err(crate::i18n::tr("start.name_bad_characters"));
    }
    if typed.starts_with('.') {
        return Err(crate::i18n::tr("start.name_starts_with_dot"));
    }
    Ok(typed.to_string())
}

/// `stem` with the budget extension.
pub(super) fn file_name(stem: &str) -> String {
    format!("{stem}.{}", method_format::EXTENSION)
}

/// The first of "`stem` 2", "`stem` 3"… with no file of that name in `dir`.
pub(super) fn free_stem(dir: &Path, stem: &str) -> String {
    free_name(stem, |s| dir.join(file_name(s)))
}

/// The first of "`stem` 2", "`stem` 3"… for which nothing is at `at(name)`.
fn free_name(stem: &str, at: impl Fn(&str) -> PathBuf) -> String {
    (2..10_000)
        .map(|n| format!("{stem} {n}"))
        .find(|s| std::fs::symlink_metadata(at(s)).is_err())
        .unwrap_or_else(|| format!("{stem} {}", unique_suffix()))
}

/// A number no other free name took, for when 2 to 9,999 are all taken.
fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// What saving a new budget into a folder came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Made {
    /// Written and opened in front, as a saved budget.
    Opened(DocId, PathBuf),
    /// A file of that name is there already; nothing was written. A free
    /// name to offer instead.
    Taken(String),
}

/// Writes `b` as `stem`.mthd in `dir`, never over a file that is there, and
/// opens it in front as a saved budget (in place of an untouched Untitled
/// budget alone in the window), listed in Open Recent.
pub fn save_new(
    state: &AppState,
    safety: &Safety,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<Made, String> {
    if !dir.is_dir() {
        return Err(crate::i18n::tr("start.folder_gone"));
    }
    let path = dir.join(file_name(stem));
    let text = method_format::to_string(b);
    match safety::write_new(&path, text.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(Made::Taken(free_stem(dir, stem)));
        }
        Err(e) => {
            return Err(format!(
                "Couldn't save in {}: {e}.",
                folder_text(Some(dir), home().as_deref())
            ))
        }
    }
    let id = crate::safety_app::open_path(state, safety, &path)?;
    Ok(Made::Opened(id, path))
}

/// Makes project folder `stem` in `dir`, writes `b` in it as `stem`.mthd
/// and opens it as [`save_new`] does; the folder joins the projects. A
/// folder or file of that name already in `dir` makes nothing: a name free
/// for both is offered.
pub fn save_new_project(
    state: &AppState,
    safety: &Safety,
    start: &StartState,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<Made, String> {
    if !dir.is_dir() {
        return Err(crate::i18n::tr("start.folder_gone"));
    }
    let free = || {
        free_name(stem, |s| {
            // Both the folder and a loose file of the name must be free.
            let folder = dir.join(s);
            if std::fs::symlink_metadata(dir.join(file_name(s))).is_ok() {
                dir.join(file_name(s))
            } else {
                folder
            }
        })
    };
    let project = dir.join(stem);
    if std::fs::symlink_metadata(&project).is_ok()
        || std::fs::symlink_metadata(dir.join(file_name(stem))).is_ok()
    {
        return Ok(Made::Taken(free()));
    }
    match std::fs::create_dir(&project) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(Made::Taken(free()));
        }
        Err(e) => {
            return Err(format!(
                "Couldn't make the folder {stem} in {}: {e}.",
                folder_text(Some(dir), home().as_deref())
            ))
        }
    }
    let made = save_new(state, safety, b, &project, stem);
    if !matches!(made, Ok(Made::Opened(..))) {
        // Nothing was written in it: take the empty folder back.
        let _ = std::fs::remove_dir(&project);
        return made;
    }
    // Best effort: the budget is saved either way.
    let _ = start.add_project(&project);
    made
}

/// Writes `b` in `dir` as `stem`.mthd, or the first free "`stem` 2",
/// "`stem` 3"… when that is taken (never over a file), and opens it as
/// [`save_new`] does. Its id and path.
pub fn save_new_free(
    state: &AppState,
    safety: &Safety,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<(DocId, PathBuf), String> {
    let mut name = stem.to_string();
    for _ in 0..5 {
        match save_new(state, safety, b, dir, &name)? {
            Made::Opened(id, path) => return Ok((id, path)),
            Made::Taken(free) => name = free,
        }
    }
    Err(crate::i18n::tr("start.no_free_name"))
}

/// Writes a duplicate of budget file `path` beside it, as the first free
/// "`stem` 2", "`stem` 3"… (the file's bytes as they are on disk, so an open
/// tab's unsaved changes are not in it), never over a file, and opens it in
/// front as a saved budget listed in Open Recent. Its path.
pub fn duplicate(
    state: &AppState,
    safety: &Safety,
    path: &Path,
) -> Result<(DocId, PathBuf), String> {
    if !is_budget_path(path) {
        return Err(crate::i18n::tr("start.only_budget_duplicated"));
    }
    // Refuse what isn't a readable budget before copying it.
    crate::session::read_budget(path)?;
    let bytes =
        std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}.", path.display()))?;
    let dir = path
        .parent()
        .filter(|d| d.is_dir())
        .ok_or("That folder isn't there any more.")?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Budget".into());
    // Another program may take the free name first: try the next.
    for _ in 0..5 {
        let to = dir.join(file_name(&free_stem(dir, &stem)));
        match safety::write_new(&to, &bytes) {
            Ok(()) => {
                let id = crate::safety_app::open_path(state, safety, &to)?;
                return Ok((id, to));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Couldn't save the duplicate in {}: {e}.",
                    folder_text(Some(dir), home().as_deref())
                ))
            }
        }
    }
    Err(crate::i18n::tr("start.no_free_duplicate_name"))
}
