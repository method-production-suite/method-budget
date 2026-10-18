//! Working together on one budget through a shared folder (roadmap 3.18;
//! docs/research/collaboration.md; decisions.md, entries 62 and 64): the
//! engine side of a shared session, with no window.
//!
//! A [`SharedDoc`] rides inside a [`crate::session::Session`] once the
//! budget is shared (File > Share…, or opening a budget that has a
//! `Feature.mthd-log/` folder beside it). From then on:
//!
//! - every edit is recorded as a change in the log
//!   ([`SharedDoc::record`]), with the rename and renumber hints the
//!   commands pass, and the projection is kept beside the budget so the
//!   next capture never rebuilds it from the log;
//! - rows the user makes get random ids from the shared range, so two
//!   people adding lines apart never make the same id;
//! - the folder is polled by the app's one-second timer
//!   ([`SharedDoc::tick`]): new segments from other computers are
//!   absorbed, this computer's changes are flushed to its own segment
//!   files once typing pauses, the keeper writes the merged budget as the
//!   `.mthd` and records edits made to it by other programs, and the
//!   heartbeat file says who is where;
//! - Undo takes back only this person's changes ([`SharedDoc::undo`]),
//!   and says what couldn't be put back because someone changed it since.
//! - The owner (who started sharing) and admins set what each person may
//!   change ([`SharedDoc::set_role`]; decisions.md, entry 78); an edit
//!   beyond this person's role is refused in plain words before anything
//!   is written, and someone else's is refused when it arrives.
//!
//! The words for the interface: changes are "shared", never synced; a
//! conflict is "changed by two people"; there are no actors, logs or
//! frontiers on screen (design, 9.10).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use method_core::log::{
    ActorId, ChangeId, Field, Hint, Ids, Kind, LogError, Projection, Review, Role, Stamp, Uid,
    Value,
};
use method_core::{Budget, RowId};
use method_format::log::{self as flog, LoadReport, Presence, Scan, Shared};
use serde::{Deserialize, Serialize};

use crate::safety::{DiskStamp, DiskStampExt};

/// How long typing must pause before this computer's changes are written
/// to a segment file (a segment per keystroke would be hundreds of files).
pub const FLUSH_QUIET: Duration = Duration::from_millis(1500);
/// Changes are written at least this often while someone keeps typing.
pub const FLUSH_LATEST: Duration = Duration::from_secs(6);
/// How long the budget must be quiet before the keeper rewrites the `.mthd`.
pub const SNAPSHOT_QUIET: Duration = Duration::from_secs(2);
/// How often the heartbeat file is rewritten.
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(20);
/// How often other people's heartbeat files are read.
pub const PRESENCE_EVERY: Duration = Duration::from_secs(5);
/// A heartbeat older than this means the person is away.
pub const AWAY_AFTER: Duration = Duration::from_secs(120);
/// How many remote changes the status keeps for the page to flash.
const RECENT_KEPT: usize = 40;
/// The longest display name kept.
pub const NAME_MAX_CHARS: usize = 60;

/// The palette of person colors, by name; the page maps names to colors.
pub const COLORS: [&str; 8] = [
    "teal", "blue", "purple", "orange", "rose", "green", "amber", "slate",
];

/// This person's name and color, as other people see them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Person {
    pub name: String,
    pub color: String,
}

impl Default for Person {
    fn default() -> Person {
        Person {
            name: String::new(),
            color: COLORS[0].into(),
        }
    }
}

impl Person {
    /// A person with `name` and `color` tidied: one line, at most
    /// [`NAME_MAX_CHARS`] characters, a color from the palette.
    pub fn tidy(name: &str, color: &str, actor: ActorId) -> Person {
        let one_line: String = name
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let name: String = one_line.trim().chars().take(NAME_MAX_CHARS).collect();
        let color = if COLORS.contains(&color) {
            color.to_string()
        } else {
            default_color(actor).to_string()
        };
        Person { name, color }
    }
}

/// The color an actor gets before anyone picks one: spread over the
/// palette by its id.
pub fn default_color(actor: ActorId) -> &'static str {
    COLORS[(actor.0[0] as usize) % COLORS.len()]
}

/// A name when a person hasn't given one: "Person 7f3a".
pub fn default_name(actor: ActorId) -> String {
    format!("Person {}", &actor.hex()[..4])
}

/// Initials for an avatar: the first letters of up to two words.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().take(2).collect();
    let mut out: String = words
        .iter()
        .filter_map(|w| w.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect();
    if out.is_empty() {
        out.push('?');
    }
    out
}

/// Where this computer's person file lives (`~/.method/person.json`,
/// beside the actor id): the name and color this person uses in every
/// shared budget.
pub fn person_path() -> Option<PathBuf> {
    flog::actor_id_path().map(|p| p.with_file_name("person.json"))
}

/// The person kept in `path`, or the default for `actor`.
pub fn read_person(path: Option<&Path>, actor: ActorId) -> Person {
    let stored: Option<Person> = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok());
    match stored {
        Some(p) => Person::tidy(&p.name, &p.color, actor),
        None => Person {
            name: String::new(),
            color: default_color(actor).into(),
        },
    }
}

/// Writes the person to `path` (best effort: a failed write only means
/// the name is asked again next time).
pub fn write_person(path: Option<&Path>, p: &Person) {
    let Some(path) = path else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(path, text + "\n");
    }
}

/// The time now as ISO 8601 UTC to the second (what changes and
/// heartbeats carry, for display only).
pub fn iso_now() -> String {
    iso_at(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    )
}

/// `secs` since the Unix epoch as ISO 8601 UTC.
pub fn iso_at(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Seconds since the Unix epoch of an ISO 8601 UTC time written by
/// [`iso_at`] (`None` for anything else).
pub fn iso_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m as u32, d as u32);
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs).ok()
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// public-domain `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

// ---------------------------------------------------------------------------
// What the page sees

/// One of this person's undoable steps.
#[derive(Clone, Debug)]
struct Step {
    id: ChangeId,
    label: String,
    at: u64,
}

/// Where in the budget an entity is, for the page to find its cell.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PlaceDto {
    /// "row", "account", "category", "production_total", "fringe",
    /// "global"... (the entity kind's name).
    pub kind: String,
    /// The row id, account or category code, or item name.
    pub key: String,
    /// A row's account code, an account's category code.
    pub parent: Option<String>,
    /// The field, as the file names it ("rate"), when the item is about one.
    pub field: Option<String>,
    /// What it is called in sentences: "line 4 in 2300".
    pub text: String,
}

/// A change by someone else that arrived, for the page to flash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RemoteChange {
    pub actor: String,
    pub name: String,
    pub color: String,
    pub label: String,
    pub time: String,
    pub places: Vec<PlaceDto>,
}

/// Who is in the budget, as the toolbar shows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersonDto {
    pub actor: String,
    pub name: String,
    pub initials: String,
    pub color: String,
    pub me: bool,
    /// Keeps the file (writes the `.mthd`).
    pub keeper: bool,
    /// Seconds since their last heartbeat, `None` when they never wrote one.
    pub seen_secs: Option<u64>,
    pub away: bool,
    pub category: Option<String>,
    pub account: Option<String>,
    /// How many changes of theirs this computer has.
    pub changes: u64,
    /// What they may change: "owner", "admin", "editor", "department",
    /// "actuals", "comment" or "read_only".
    pub role: String,
    /// The role in words: "Edits everything", "Edits 2000 Production".
    pub role_text: String,
    /// A department editor's categories and accounts, by number.
    pub scope: Vec<String>,
}

/// The status item and its panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ShareStatus {
    /// "folder" for now.
    pub mode: String,
    pub folder: String,
    pub file: String,
    pub me: PersonDto,
    pub keeper: bool,
    pub people: Vec<PersonDto>,
    /// Seconds since the folder was last read successfully.
    pub synced_secs: Option<u64>,
    /// This computer's changes not yet in the folder.
    pub unshared: usize,
    /// Changes waiting for ones that haven't arrived.
    pub waiting: usize,
    pub review_count: usize,
    /// The problem reaching or writing the folder, if any.
    pub error: Option<String>,
    /// When the keeper last wrote the `.mthd` (ISO 8601 UTC), if known.
    pub file_written: Option<String>,
    pub changes: usize,
    /// Changes by others that arrived lately, newest last.
    pub recent: Vec<RemoteChange>,
    /// The review items that are about a cell on screen: the page marks
    /// them with a badge.
    pub conflicts: Vec<ReviewDto>,
    /// This person sets other people's roles (the owner and admins).
    pub can_set_roles: bool,
    /// Someone has been given a role, so people without one read only.
    pub roles_started: bool,
    /// Changes in the folder this computer didn't take in: beyond their
    /// author's role, or unreadable.
    pub refused: usize,
}

/// One thing to review (design, 9.5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewDto {
    pub index: usize,
    pub key: String,
    pub sentence: String,
    pub place: Option<PlaceDto>,
    /// Changed by two people: the value kept and the ones that lost, with
    /// who wrote each and the index to pick it back by (`share_use_value`).
    pub values: Vec<ValueDto>,
}

/// One write to a field, for blame and conflicts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValueDto {
    /// The value as text.
    pub value: String,
    pub actor: String,
    pub name: String,
    pub color: String,
    pub me: bool,
    pub time: String,
    pub label: String,
    /// The current value.
    pub kept: bool,
    /// Its place in the field's history, oldest first (what
    /// `share_use_value` takes).
    pub index: usize,
}

/// One change in the Activity tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActivityDto {
    pub actor: String,
    pub name: String,
    pub color: String,
    pub me: bool,
    pub time: String,
    pub label: String,
    pub ops: usize,
    pub place: Option<PlaceDto>,
}

/// What a tick did, for the window to tell the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickOut {
    /// The budget changed (changes arrived, or an outside edit was taken in).
    pub budget_changed: bool,
    /// The changes by others that were applied.
    pub remote: Vec<RemoteChange>,
}

// ---------------------------------------------------------------------------
// The shared session

/// A shared budget's log, folder and bookkeeping, inside a session.
pub struct SharedDoc {
    shared: Shared,
    scan: Scan,
    proj: Projection,
    ids: Ids,
    me: Person,
    keeper: bool,
    steps: Vec<Step>,
    redo: Vec<Step>,
    /// The highest seq of this actor in a segment file.
    written_seq: u64,
    /// When this person last edited, and when the first unwritten edit was.
    last_edit: Option<Instant>,
    unwritten_since: Option<Instant>,
    /// When the budget last changed from any side (the keeper's quiet time).
    last_change: Instant,
    last_sync: Option<Instant>,
    error: Option<String>,
    /// How many changes the log had when the `.mthd` was last written.
    snapshot_len: usize,
    file_written: Option<String>,
    disk: Option<DiskStamp>,
    heartbeat_at: Option<Instant>,
    presence_read_at: Option<Instant>,
    place: (Option<String>, Option<String>),
    place_changed: bool,
    people: BTreeMap<ActorId, Presence>,
    recent: Vec<RemoteChange>,
    /// Changes waiting for ones that haven't arrived, from the last read.
    waiting: usize,
}

impl std::fmt::Debug for SharedDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedDoc")
            .field("folder", &self.shared.dir.root())
            .field("changes", &self.shared.log.len())
            .field("keeper", &self.keeper)
            .finish()
    }
}

/// The keeper's note of the `.mthd` it last wrote, in its own folder, so
/// the next start tells an outside edit from a stale file.
#[derive(Serialize, Deserialize)]
struct SnapshotNote {
    hash: String,
    written: String,
}

/// How the session (method-session) uses the shared folder's log.
impl method_session::collab::SharedLog for SharedDoc {
    fn is_dirty(&self) -> bool {
        SharedDoc::is_dirty(self)
    }

    fn can_undo(&self) -> bool {
        SharedDoc::can_undo(self)
    }

    fn can_redo(&self) -> bool {
        SharedDoc::can_redo(self)
    }

    fn randomize_new_rows(&mut self, before: &Budget, after: &mut Budget) -> HashMap<RowId, RowId> {
        SharedDoc::randomize_new_rows(self, before, after)
    }

    fn record(
        &mut self,
        after: &Budget,
        hints: &[Hint],
        label: &str,
    ) -> Result<Option<Budget>, String> {
        SharedDoc::record(self, after, hints, label)
    }

    fn undo(&mut self) -> Option<(Budget, Option<String>)> {
        SharedDoc::undo(self)
    }

    fn redo(&mut self) -> Option<(Budget, Option<String>)> {
        SharedDoc::redo(self)
    }

    fn history(&self) -> crate::session::History {
        SharedDoc::history(self)
    }

    fn history_len(&self) -> usize {
        SharedDoc::history_len(self)
    }

    fn applied_steps(&self) -> usize {
        SharedDoc::applied_steps(self)
    }

    fn refusal(&self) -> Option<String> {
        SharedDoc::refusal(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl SharedDoc {
    /// Starts sharing the budget at `path`, which must be saved: the log
    /// folder is made beside it and this computer keeps the file.
    pub fn start(path: &Path, actor: ActorId, me: Person) -> Result<SharedDoc, String> {
        let shared = flog::init(path, actor, Ids::from_os(), &iso_now()).map_err(|e| e.0)?;
        flog::flush(&shared.log, &shared.dir).map_err(|e| e.0)?;
        let mut doc = SharedDoc::wrap(shared, me, true);
        doc.written_seq = doc.shared.log.own_seq();
        doc.snapshot_len = doc.shared.log.len();
        doc.note_snapshot(path);
        doc.heartbeat(Instant::now(), true);
        Ok(doc)
    }

    /// Joins the shared budget at `path` (its log folder is beside it).
    /// Returns the merged budget, which the session adopts.
    pub fn open(
        path: &Path,
        actor: ActorId,
        me: Person,
    ) -> Result<(SharedDoc, LoadReport), String> {
        let (shared, report) = flog::open(path, actor, Ids::from_os()).map_err(|e| e.0)?;
        let keeper = shared.info.genesis.actor == actor;
        let mut doc = SharedDoc::wrap(shared, me, keeper);
        doc.written_seq = doc.shared.dir.highest_written(actor);
        doc.waiting = report.pending;
        // The file as the keeper last wrote it: unchanged means no outside
        // edit to take in; the log decides what the budget is.
        if keeper {
            doc.disk = doc.read_snapshot_note(path);
            if doc.disk.is_some() && doc.file_matches(path) {
                doc.snapshot_len = doc.shared.log.len();
            }
        }
        doc.heartbeat(Instant::now(), true);
        doc.presence_read_at = None;
        Ok((doc, report))
    }

    fn wrap(shared: Shared, me: Person, keeper: bool) -> SharedDoc {
        let proj = shared.log.project();
        SharedDoc {
            shared,
            scan: Scan::default(),
            proj,
            ids: Ids::from_os(),
            me,
            keeper,
            steps: Vec::new(),
            redo: Vec::new(),
            written_seq: 0,
            last_edit: None,
            unwritten_since: None,
            last_change: Instant::now(),
            last_sync: None,
            error: None,
            snapshot_len: 0,
            file_written: None,
            disk: None,
            heartbeat_at: None,
            presence_read_at: None,
            place: (None, None),
            place_changed: false,
            people: BTreeMap::new(),
            recent: Vec::new(),
            waiting: 0,
        }
        .with_scan()
    }

    fn with_scan(mut self) -> SharedDoc {
        self.scan = self.shared.dir.scan();
        self.last_sync = Some(Instant::now());
        self
    }

    pub fn actor(&self) -> ActorId {
        self.shared.log.actor()
    }

    pub fn me(&self) -> &Person {
        &self.me
    }

    pub fn set_me(&mut self, me: Person) {
        self.me = me;
        self.place_changed = true;
    }

    pub fn is_keeper(&self) -> bool {
        self.keeper
    }

    pub fn folder(&self) -> &Path {
        self.shared.dir.root()
    }

    /// The budget as the log projects it.
    pub fn budget(&self) -> &Budget {
        &self.proj.budget
    }

    pub fn projection(&self) -> &Projection {
        &self.proj
    }

    pub fn log(&self) -> &method_core::log::Log {
        &self.shared.log
    }

    /// The `.mthd`'s path moved (Save As while shared isn't allowed, but
    /// the session may learn its path).
    pub fn path(&self) -> &Path {
        &self.shared.path
    }

    /// Changes this computer made that aren't in the folder yet.
    pub fn unshared(&self) -> usize {
        self.shared
            .log
            .own_seq()
            .saturating_sub(self.written_seq)
            .try_into()
            .unwrap_or(usize::MAX)
    }

    /// Whether anything of this person's is not yet in the folder, or (for
    /// the keeper) the file is behind the log: what Save and the close
    /// question look at.
    pub fn is_dirty(&self) -> bool {
        self.unshared() > 0 || (self.keeper && self.snapshot_len != self.shared.log.len())
    }

    /// Where this person is, for the heartbeat.
    pub fn set_place(&mut self, category: Option<String>, account: Option<String>) {
        let place = (category, account);
        if place != self.place {
            self.place = place;
            self.place_changed = true;
        }
    }

    // Recording

    /// Gives rows new to `after` (not in `before`) random ids from the
    /// shared range. Returns old id -> new id.
    pub fn randomize_new_rows(
        &mut self,
        before: &Budget,
        after: &mut Budget,
    ) -> HashMap<RowId, RowId> {
        let had: HashSet<RowId> = crate::edit::details(before).map(|d| d.id).collect();
        let mut taken: HashSet<RowId> = crate::edit::details(after).map(|d| d.id).collect();
        let mut map = HashMap::new();
        for d in after
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if had.contains(&d.id) || map.contains_key(&d.id) {
                continue;
            }
            let fresh =
                method_core::log::ids::random_row_id(&mut self.ids, |id| taken.contains(&id));
            taken.insert(fresh);
            map.insert(d.id, fresh);
            d.id = fresh;
        }
        map
    }

    /// Records the edit that took the projected budget to `after` as one
    /// change by this person. Returns the budget as the log now projects
    /// it (normally `after` itself) when anything was recorded.
    pub fn record(
        &mut self,
        after: &Budget,
        hints: &[Hint],
        label: &str,
    ) -> Result<Option<Budget>, String> {
        let id = self
            .shared
            .log
            .record(&self.proj, after, hints, label, &iso_now())
            .map_err(|e| self.refused_text(&e))?;
        let Some(id) = id else {
            return Ok(None);
        };
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        self.steps.push(Step {
            id,
            label: label.to_string(),
            at: now_ms(),
        });
        self.redo.clear();
        Ok(Some(self.proj.budget.clone()))
    }

    fn reproject(&mut self) {
        self.proj = self.shared.log.project();
    }

    // Undo and redo, this person's only

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes this person's latest step. Returns the budget to adopt and
    /// a note when something couldn't be put back; `None` when there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<(Budget, Option<String>)> {
        let step = self.steps.pop()?;
        let (budget, note) = self.reverse(&step, true);
        Some((budget, note))
    }

    pub fn redo(&mut self) -> Option<(Budget, Option<String>)> {
        let step = self.redo.pop()?;
        let (budget, note) = self.reverse(&step, false);
        Some((budget, note))
    }

    /// Records the inverse of `step`, moving it to the other stack.
    fn reverse(&mut self, step: &Step, undoing: bool) -> (Budget, Option<String>) {
        let what = if undoing { "Undid" } else { "Redid" };
        let (new, inv) = match self.shared.log.undo(step.id, &iso_now()) {
            Ok(r) => r,
            Err(e @ LogError::NotAllowed { .. }) => {
                return (self.proj.budget.clone(), Some(self.refused_text(&e)))
            }
            Err(e) => {
                return (
                    self.proj.budget.clone(),
                    Some(format!("Couldn't {} this step: {e}", what.to_lowercase())),
                )
            }
        };
        let mut note = None;
        match new {
            Some(id) => {
                let moved = Step {
                    id,
                    label: step.label.clone(),
                    at: step.at,
                };
                if undoing {
                    self.redo.push(moved);
                } else {
                    self.steps.push(moved);
                }
                self.reproject();
                let now = Instant::now();
                self.last_edit = Some(now);
                self.unwritten_since.get_or_insert(now);
                self.last_change = now;
                if !inv.skipped.is_empty() {
                    note = Some(format!(
                        "{what} \u{201c}{}\u{201d}. {}",
                        step.label,
                        self.skips(&inv.skipped)
                    ));
                }
            }
            None => {
                note = Some(format!(
                    "Nothing to put back for \u{201c}{}\u{201d}: {}",
                    step.label,
                    if inv.skipped.is_empty() {
                        "it has been changed since.".to_string()
                    } else {
                        self.skips(&inv.skipped)
                    }
                ));
            }
        }
        (self.proj.budget.clone(), note)
    }

    /// The skip sentences with people's names instead of actor ids.
    fn skips(&self, skipped: &[String]) -> String {
        let mut names: Vec<(String, String)> = Vec::new();
        for actor in self.shared.log.frontier().0.keys() {
            names.push((actor.short(), self.name_of(*actor)));
        }
        let mut out: Vec<String> = skipped
            .iter()
            .map(|s| {
                let mut s = s.clone();
                for (short, name) in &names {
                    s = s.replace(&format!(": {short} changed"), &format!(": {name} changed"));
                }
                s
            })
            .collect();
        out.dedup();
        out.join(" ")
    }

    /// This person's steps as the History list shows them.
    pub fn history(&self) -> crate::session::History {
        let steps = self
            .steps
            .iter()
            .map(|s| (s, true))
            .chain(self.redo.iter().rev().map(|s| (s, false)))
            .map(|(s, done)| crate::session::HistoryStep {
                label: s.label.clone(),
                at: s.at,
                done,
            })
            .collect();
        crate::session::History {
            steps,
            current: self.steps.len(),
            trimmed: false,
        }
    }

    pub fn history_len(&self) -> usize {
        self.steps.len() + self.redo.len()
    }

    pub fn applied_steps(&self) -> usize {
        self.steps.len()
    }

    // The timer

    /// One pass for the app's timer: writes this computer's changes once
    /// typing pauses, reads what others wrote, keeps the file (the
    /// keeper), and the heartbeat. `budget` is the session's budget, which
    /// is replaced when changes arrive.
    pub fn tick(&mut self, now: Instant, budget: &mut Budget) -> TickOut {
        let mut out = TickOut::default();
        if !self.shared.dir.exists() {
            self.error = Some("Can't reach the shared folder.".into());
            return out;
        }
        self.flush(now, false);
        // Who is here (their names go on the changes that arrive).
        if self
            .presence_read_at
            .is_none_or(|t| now.duration_since(t) >= PRESENCE_EVERY)
        {
            self.people = self.shared.dir.presences();
            self.presence_read_at = Some(now);
        }
        // What others wrote.
        let before = self.shared.log.len();
        let report = flog::absorb(&mut self.shared.log, &self.shared.dir, &mut self.scan);
        self.last_sync = Some(now);
        self.waiting = report.pending;
        if report.applied > 0 {
            let me = self.actor();
            let arrived: Vec<ChangeId> = self
                .shared
                .log
                .changes_from(before)
                .filter(|c| c.actor != me)
                .map(|c| c.id())
                .collect();
            if !arrived.is_empty() {
                self.reproject();
                for id in arrived {
                    if let Some(rc) = self.remote_change(id) {
                        self.recent.push(rc.clone());
                        out.remote.push(rc);
                    }
                }
                if self.recent.len() > RECENT_KEPT {
                    let cut = self.recent.len() - RECENT_KEPT;
                    self.recent.drain(..cut);
                }
                *budget = self.proj.budget.clone();
                out.budget_changed = true;
                self.last_change = now;
            }
        }
        if self.keeper && self.keep_file(now, budget) {
            out.budget_changed = true;
        }
        self.heartbeat(now, false);
        out
    }

    /// Writes this computer's unwritten changes to a segment when typing
    /// has paused (or `force`).
    pub fn flush(&mut self, now: Instant, force: bool) -> bool {
        if self.unshared() == 0 {
            self.unwritten_since = None;
            return false;
        }
        let quiet = self
            .last_edit
            .is_none_or(|t| now.duration_since(t) >= FLUSH_QUIET);
        let long = self
            .unwritten_since
            .is_some_and(|t| now.duration_since(t) >= FLUSH_LATEST);
        if !(force || quiet || long) {
            return false;
        }
        match flog::flush(&self.shared.log, &self.shared.dir) {
            Ok(_) => {
                self.written_seq = self.shared.log.own_seq();
                self.unwritten_since = None;
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(format!("Couldn't write to the shared folder: {}", e.0));
                false
            }
        }
    }

    /// The keeper's work: take in an edit made to the `.mthd` by another
    /// program, and rewrite the file from the merged state once the budget
    /// is quiet. True when the budget changed.
    fn keep_file(&mut self, now: Instant, budget: &mut Budget) -> bool {
        let path = self.shared.path.clone();
        let mut changed = false;
        if self.disk.is_some() && !self.file_matches(&path) {
            match flog::ingest(&mut self.shared, &iso_now()) {
                Ok(Some(_)) => {
                    self.reproject();
                    *budget = self.proj.budget.clone();
                    changed = true;
                    self.last_change = now;
                    self.unwritten_since.get_or_insert(now);
                }
                Ok(None) => {}
                Err(e) => self.error = Some(e.0),
            }
            // Whatever it held, the file is now known.
            self.disk = std::fs::read(&path).ok().map(|b| DiskStamp::of(&path, &b));
        }
        if self.snapshot_len != self.shared.log.len()
            && now.duration_since(self.last_change) >= SNAPSHOT_QUIET
        {
            self.write_file(&path);
        }
        changed
    }

    /// Writes the merged budget as the `.mthd` now (the keeper).
    pub fn write_file(&mut self, path: &Path) -> bool {
        match flog::write_snapshot(&self.shared) {
            Ok(_) => {
                self.snapshot_len = self.shared.log.len();
                self.note_snapshot(path);
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(format!("Couldn't write the budget file: {}", e.0));
                false
            }
        }
    }

    /// Remembers the file as just written (its stamp, and a note in this
    /// actor's folder for the next start).
    fn note_snapshot(&mut self, path: &Path) {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let stamp = DiskStamp::of(path, &bytes);
        let written = iso_now();
        let note = SnapshotNote {
            hash: stamp.hash.to_string(),
            written: written.clone(),
        };
        if let Ok(text) = serde_json::to_string(&note) {
            let _ = flog::write_sealed(&self.snapshot_note_path(), text.as_bytes());
        }
        self.disk = Some(stamp);
        self.file_written = Some(written);
    }

    fn snapshot_note_path(&self) -> PathBuf {
        self.shared
            .dir
            .root()
            .join("actors")
            .join(self.actor().hex())
            .join("snapshot.json")
    }

    /// The stamp of the file as this keeper last wrote it, from its note,
    /// when the file still has that content.
    fn read_snapshot_note(&mut self, path: &Path) -> Option<DiskStamp> {
        let text = std::fs::read_to_string(self.snapshot_note_path()).ok()?;
        let note: SnapshotNote = serde_json::from_str(&text).ok()?;
        let hash: u64 = note.hash.parse().ok()?;
        let bytes = std::fs::read(path).ok()?;
        let stamp = DiskStamp::of(path, &bytes);
        self.file_written = Some(note.written);
        (stamp.hash == hash).then_some(stamp)
    }

    /// Whether the `.mthd` still holds what the keeper last wrote.
    fn file_matches(&mut self, path: &Path) -> bool {
        let Some(stamp) = &self.disk else {
            return false;
        };
        match stamp.check(path) {
            crate::safety::DiskCheck::Same(fresh) => {
                if let Some(f) = fresh {
                    self.disk = Some(f);
                }
                true
            }
            crate::safety::DiskCheck::Changed => false,
            // Gone: nothing to take in; the next snapshot puts it back.
            crate::safety::DiskCheck::Missing => true,
        }
    }

    /// Writes the heartbeat when it is due, or where this person is changed.
    fn heartbeat(&mut self, now: Instant, force: bool) {
        let due = self
            .heartbeat_at
            .is_none_or(|t| now.duration_since(t) >= HEARTBEAT_EVERY);
        if !(force || due || self.place_changed) {
            return;
        }
        let p = Presence {
            name: self.me.name.clone(),
            color: self.me.color.clone(),
            category: self.place.0.clone(),
            account: self.place.1.clone(),
            at: iso_now(),
            frontier: self.shared.log.frontier().digest(),
        };
        if self.shared.dir.write_presence(self.actor(), &p).is_ok() {
            self.heartbeat_at = Some(now);
            self.place_changed = false;
        }
    }

    /// Everything for the way out: changes to the folder and (the keeper)
    /// the file.
    pub fn flush_all(&mut self) {
        let now = Instant::now();
        self.flush(now, true);
        if self.keeper && self.snapshot_len != self.shared.log.len() {
            let path = self.shared.path.clone();
            self.write_file(&path);
        }
    }

    // Names and places

    /// The name a person shows with: their own, or "Person 7f3a".
    pub fn name_of(&self, actor: ActorId) -> String {
        if actor == self.actor() {
            if !self.me.name.is_empty() {
                return self.me.name.clone();
            }
            return default_name(actor);
        }
        match self.people.get(&actor) {
            Some(p) if !p.name.trim().is_empty() => {
                p.name.trim().chars().take(NAME_MAX_CHARS).collect()
            }
            _ => default_name(actor),
        }
    }

    pub fn color_of(&self, actor: ActorId) -> String {
        if actor == self.actor() {
            return self.me.color.clone();
        }
        match self.people.get(&actor) {
            Some(p) if COLORS.contains(&p.color.as_str()) => p.color.clone(),
            _ => default_color(actor).to_string(),
        }
    }

    fn person_dto(&self, actor: ActorId, now: Instant) -> PersonDto {
        let role = self.shared.log.role_of(actor);
        let name = self.name_of(actor);
        let me = actor == self.actor();
        let presence = self.people.get(&actor);
        let seen_secs = if me {
            Some(0)
        } else {
            presence.and_then(|p| iso_secs(&p.at)).map(|at| {
                let now_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                now_s.saturating_sub(at)
            })
        };
        let _ = now;
        PersonDto {
            actor: actor.hex(),
            initials: initials(&name),
            name,
            color: self.color_of(actor),
            me,
            keeper: actor == self.shared.info.genesis.actor,
            seen_secs,
            away: !me && seen_secs.is_none_or(|s| s >= AWAY_AFTER.as_secs()),
            category: if me {
                self.place.0.clone()
            } else {
                presence.and_then(|p| p.category.clone())
            },
            account: if me {
                self.place.1.clone()
            } else {
                presence.and_then(|p| p.account.clone())
            },
            changes: self.shared.log.frontier().get(&actor),
            role: role.key().to_string(),
            role_text: self.role_text(&role),
            scope: role
                .scope()
                .map(|s| self.scope_codes(s))
                .unwrap_or_default(),
        }
    }

    /// The status item and panel.
    pub fn status(&self, now: Instant) -> ShareStatus {
        let me = self.actor();
        let mut actors: BTreeSet<ActorId> = self.shared.log.frontier().0.keys().copied().collect();
        actors.extend(self.people.keys().copied());
        // People given a role before they ever opened the budget.
        actors.extend(self.shared.log.members().into_iter().map(|(a, _)| a));
        actors.insert(me);
        let people: Vec<PersonDto> = actors
            .iter()
            .filter(|a| **a != me)
            .map(|a| self.person_dto(*a, now))
            .collect();
        let review = self.review();
        ShareStatus {
            mode: "folder".into(),
            folder: self
                .shared
                .dir
                .root()
                .parent()
                .unwrap_or(self.shared.dir.root())
                .display()
                .to_string(),
            file: self.shared.info.file.clone(),
            me: self.person_dto(me, now),
            keeper: self.keeper,
            people,
            synced_secs: self.last_sync.map(|t| now.duration_since(t).as_secs()),
            unshared: self.unshared(),
            waiting: self.waiting,
            review_count: review.len(),
            error: self.error.clone(),
            file_written: self.file_written.clone(),
            changes: self.shared.log.len(),
            recent: self.recent.clone(),
            conflicts: review
                .into_iter()
                .filter(|r| r.place.as_ref().is_some_and(|p| p.field.is_some()))
                .collect(),
            can_set_roles: self.role().sets_roles(),
            roles_started: self.shared.log.roles_started(),
            refused: self.shared.log.rejected().len(),
        }
    }

    /// Where an entity is in the projected budget, for the page.
    pub fn place_of(&self, uid: Uid, field: Option<Field>) -> Option<PlaceDto> {
        let state = self.shared.log.state();
        let e = state.entity(uid)?;
        let b = &self.proj.binding;
        let text = method_core::log::project::describe(state, b, uid);
        let parent = e
            .place()
            .map(|p| p.list.parent)
            .filter(|p| *p != Uid::ROOT)
            .and_then(|p| b.keys.get(&p).cloned());
        let key = match e.kind {
            Kind::Row => b.row_ids.get(&uid).map(|id| id.to_string())?,
            _ => b.keys.get(&uid).cloned()?,
        };
        Some(PlaceDto {
            kind: e.kind.name().to_string(),
            key,
            parent,
            field: field.map(|f| f.name().to_string()),
            text,
        })
    }

    fn remote_change(&self, id: ChangeId) -> Option<RemoteChange> {
        let c = self.shared.log.change(id)?;
        let mut seen = BTreeSet::new();
        let mut places = Vec::new();
        for op in &c.ops {
            let field = match op {
                method_core::log::Op::Set { field, .. } => Some(*field),
                _ => None,
            };
            let key = (op.entity(), field);
            if !seen.insert(key) {
                continue;
            }
            if let Some(p) = self.place_of(op.entity(), field) {
                places.push(p);
            }
            if places.len() >= 200 {
                break;
            }
        }
        Some(RemoteChange {
            actor: c.actor.hex(),
            name: self.name_of(c.actor),
            color: self.color_of(c.actor),
            label: c.label.clone(),
            time: c.time.clone(),
            places,
        })
    }

    /// A register value as a cell shows it (a formula with its names).
    fn value_text(&self, v: &Value) -> String {
        match v {
            Value::Null => String::new(),
            Value::Text(s) => s.clone(),
            Value::Formula(f) => {
                let keys = &self.proj.binding.keys;
                method_core::log::formula::unbind(f, &|u| keys.get(&u).cloned())
            }
            other => other.brief(),
        }
    }

    fn value_dto(
        &self,
        stamp: Stamp,
        time: &str,
        value: &Value,
        kept: bool,
        index: usize,
    ) -> ValueDto {
        let label = self
            .shared
            .log
            .change(stamp.change())
            .map(|c| c.label.clone())
            .unwrap_or_default();
        ValueDto {
            value: self.value_text(value),
            actor: stamp.actor.hex(),
            name: self.name_of(stamp.actor),
            color: self.color_of(stamp.actor),
            me: stamp.actor == self.actor(),
            time: time.to_string(),
            label,
            kept,
            index,
        }
    }

    /// The review list (design, 9.5).
    pub fn review(&self) -> Vec<ReviewDto> {
        self.proj
            .review
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (place, values) = match &item.review {
                    Review::Overwritten {
                        entity,
                        field,
                        kept,
                        lost,
                    } => {
                        let history = self.shared.log.blame(*entity, *field);
                        let mut values = Vec::new();
                        let find = |s: Stamp| history.iter().position(|(h, _, _)| *h == s);
                        if let Some(i) = find(kept.0) {
                            values.push(self.value_dto(kept.0, &history[i].1, &kept.1, true, i));
                        }
                        for (s, v) in lost {
                            if let Some(i) = find(*s) {
                                values.push(self.value_dto(*s, &history[i].1, v, false, i));
                            }
                        }
                        (self.place_of(*entity, Some(*field)), values)
                    }
                    r => (r.entity().and_then(|u| self.place_of(u, None)), Vec::new()),
                };
                ReviewDto {
                    index,
                    key: item.review.key().to_string(),
                    sentence: self.sentence_with_names(&item.sentence),
                    place,
                    values,
                }
            })
            .collect()
    }

    /// A review sentence with people's names in place of actor ids.
    fn sentence_with_names(&self, s: &str) -> String {
        let mut s = s.to_string();
        for actor in self.shared.log.frontier().0.keys() {
            s = s.replace(&actor.short(), &self.name_of(*actor));
        }
        s
    }

    /// The entity for a place the page names: a row by id, an account or
    /// category by code, a setup item by kind and name.
    pub fn entity_at(&self, kind: &str, key: &str) -> Option<Uid> {
        let b = &self.proj.binding;
        match kind {
            "row" => b.rows.get(&key.parse::<RowId>().ok()?).copied(),
            "account" => b.accounts.get(key).copied(),
            "category" => b.categories.get(key).copied(),
            "budget" => Some(Uid::ROOT),
            other => {
                let k = Kind::parse(other)?;
                b.names.get(&(k, key.to_string())).copied()
            }
        }
    }

    /// Who changed a field, oldest first.
    pub fn blame(&self, kind: &str, key: &str, field: &str) -> Result<Vec<ValueDto>, String> {
        let uid = self
            .entity_at(kind, key)
            .ok_or("That isn't in the shared budget any more.")?;
        let field = Field::parse(field).ok_or("Unknown field.")?;
        let history = self.shared.log.blame(uid, field);
        let last = history.len().saturating_sub(1);
        Ok(history
            .iter()
            .enumerate()
            .map(|(i, (s, time, v))| self.value_dto(*s, time, v, i == last, i))
            .collect())
    }

    // Roles (decisions.md, entry 78)

    /// This person's role.
    pub fn role(&self) -> Role {
        self.shared.log.role_of(self.actor())
    }

    /// The owner's name, for "ask them" sentences.
    fn owner_name(&self) -> String {
        self.shared
            .log
            .owner()
            .map_or_else(|| "The owner".into(), |o| self.name_of(o))
    }

    /// Why this person can't change anything, when their role reads only.
    pub fn refusal(&self) -> Option<String> {
        self.role().reads_only().then(|| {
            format!(
                "You can read this budget but not change it. {} sets who may change what.",
                self.owner_name()
            )
        })
    }

    /// A refused change in the words the page shows: what this person's
    /// role allows, and who to ask.
    fn refused_text(&self, e: &LogError) -> String {
        if !matches!(e, LogError::NotAllowed { .. }) {
            return format!("This change couldn't be shared: {e}");
        }
        let ask = format!("{} sets who may change what.", self.owner_name());
        match self.role() {
            Role::Department(scope) => format!(
                "You can change lines and notes in {} only. {ask}",
                self.scope_words(&scope)
            ),
            Role::Actuals => format!("You can change actuals and notes only. {ask}"),
            Role::Comment => format!("You can add and change notes only. {ask}"),
            Role::ReadOnly => self.refusal().unwrap_or(ask),
            Role::Editor => {
                "Only the owner and admins lock the originals or set what people may change.".into()
            }
            Role::Admin => "Only the owner makes admins or changes an admin's role.".into(),
            Role::Owner => format!("This change couldn't be shared: {e}"),
        }
    }

    /// A department editor's categories and accounts, by number, in
    /// budget order.
    fn scope_codes(&self, scope: &std::collections::BTreeSet<Uid>) -> Vec<String> {
        let b = &self.proj.budget;
        let mut out = Vec::new();
        for c in b.categories() {
            if self
                .proj
                .binding
                .categories
                .get(&c.code)
                .is_some_and(|u| scope.contains(u))
            {
                out.push(c.code.clone());
            }
            for a in &c.accounts {
                if self
                    .proj
                    .binding
                    .accounts
                    .get(&a.code)
                    .is_some_and(|u| scope.contains(u))
                {
                    out.push(a.code.clone());
                }
            }
        }
        out
    }

    /// "2000 Production and 5000 Post".
    fn scope_words(&self, scope: &std::collections::BTreeSet<Uid>) -> String {
        let b = &self.proj.budget;
        let names: Vec<String> = self
            .scope_codes(scope)
            .into_iter()
            .map(|code| {
                let name = b
                    .categories()
                    .find(|c| c.code == code)
                    .map(|c| c.name.clone())
                    .or_else(|| {
                        b.categories()
                            .flat_map(|c| &c.accounts)
                            .find(|a| a.code == code)
                            .map(|a| a.name.clone())
                    })
                    .unwrap_or_default();
                if name.is_empty() {
                    code
                } else {
                    format!("{code} {name}")
                }
            })
            .collect();
        match names.as_slice() {
            [] => "no categories".into(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }

    /// A role in words, as the People list shows it.
    pub fn role_text(&self, role: &Role) -> String {
        match role {
            Role::Owner => "Owner: edits everything and sets what people may change".into(),
            Role::Admin => "Edits everything and sets what people may change".into(),
            Role::Editor => "Edits everything".into(),
            Role::Department(scope) => format!("Edits {}", self.scope_words(scope)),
            Role::Actuals => "Actuals and notes".into(),
            Role::Comment => "Notes only".into(),
            Role::ReadOnly => "Read only".into(),
        }
    }

    /// Sets what the person `actor` (64 hex characters) may change:
    /// `role` is "admin", "editor", "department", "actuals", "comment" or
    /// "read_only"; `scope` lists a department editor's categories or
    /// accounts by number. One change by this person, shared like any
    /// edit; not an undo step (set the role again to change it back).
    pub fn set_role(&mut self, actor: &str, role: &str, scope: &[String]) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or("Couldn't tell which person that is.")?;
        if Some(who) == self.shared.log.owner() || role == "owner" {
            return Err(
                "There is one owner, the person who started sharing, and that doesn't change."
                    .into(),
            );
        }
        let mut uids = std::collections::BTreeSet::new();
        for code in scope {
            let code = code.trim();
            let uid = self
                .proj
                .binding
                .categories
                .get(code)
                .or_else(|| self.proj.binding.accounts.get(code))
                .ok_or_else(|| format!("There is no category or account {code}."))?;
            uids.insert(*uid);
        }
        let role = Role::parse(role, uids).ok_or("That isn't a role a person can have.")?;
        if role.scope().is_some_and(|s| s.is_empty()) {
            return Err("Pick at least one category for this person to edit.".into());
        }
        if role.scope().is_none() && !scope.is_empty() {
            return Err("Only a department editor's role names categories.".into());
        }
        let label = format!("{}: {}", self.name_of(who), self.role_text(&role));
        let me = self.role();
        self.shared
            .log
            .grant(who, role, &label, &iso_now())
            .map_err(|e| match me {
                Role::Admin => {
                    "Only the owner makes admins or changes an admin's role.".to_string()
                }
                Role::Owner => format!("This role couldn't be set: {e}"),
                _ => format!(
                    "Only the owner and admins set what people may change. {} can.",
                    self.owner_name()
                ),
            })?;
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Writes value `index` of a field's history back as this person's
    /// change. Returns the budget to adopt.
    pub fn use_value(
        &mut self,
        kind: &str,
        key: &str,
        field: &str,
        index: usize,
    ) -> Result<Budget, String> {
        let uid = self
            .entity_at(kind, key)
            .ok_or("That isn't in the shared budget any more.")?;
        let f = Field::parse(field).ok_or("Unknown field.")?;
        let history = self.shared.log.blame(uid, f);
        let (_, _, value) = history
            .get(index)
            .ok_or("That value is no longer in the history.")?;
        let place = self.place_of(uid, Some(f));
        let label = format!(
            "Put back {} on {}",
            f.name(),
            place.map(|p| p.text).unwrap_or_else(|| "the budget".into())
        );
        let id = self
            .shared
            .log
            .set_field(uid, f, value.clone(), &label, &iso_now())
            .map_err(|e| self.refused_text(&e))?
            .ok_or("That isn't in the shared budget any more.")?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        self.steps.push(Step {
            id,
            label,
            at: now_ms(),
        });
        self.redo.clear();
        Ok(self.proj.budget.clone())
    }

    /// Recent changes by everyone, newest first.
    pub fn activity(&self, limit: usize) -> Vec<ActivityDto> {
        let changes: Vec<_> = self.shared.log.changes().collect();
        changes
            .iter()
            .rev()
            .take(limit)
            .map(|c| {
                let first = c.ops.first().map(|op| {
                    let field = match op {
                        method_core::log::Op::Set { field, .. } => Some(*field),
                        _ => None,
                    };
                    (op.entity(), field)
                });
                ActivityDto {
                    actor: c.actor.hex(),
                    name: self.name_of(c.actor),
                    color: self.color_of(c.actor),
                    me: c.actor == self.actor(),
                    time: c.time.clone(),
                    label: c.label.clone(),
                    ops: c.ops.len(),
                    place: first.and_then(|(u, f)| self.place_of(u, f)),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_times_round_trip() {
        for secs in [0u64, 86_399, 951_782_400, 1_760_227_200, 4_102_444_800] {
            let s = iso_at(secs);
            assert_eq!(iso_secs(&s), Some(secs), "{s}");
        }
        assert_eq!(iso_at(1_760_227_200), "2025-10-12T00:00:00Z");
        assert_eq!(iso_secs("yesterday"), None);
    }

    #[test]
    fn people_are_tidied() {
        let actor = ActorId([7; 32]);
        let p = Person::tidy("  Jordan\nLee  ", "nope", actor);
        assert_eq!(p.name, "Jordan Lee");
        assert_eq!(p.color, default_color(actor));
        assert_eq!(initials("Jordan Lee"), "JL");
        assert_eq!(initials("priya"), "P");
        assert_eq!(initials(""), "?");
        assert_eq!(default_name(actor), "Person 0707");
    }
}
