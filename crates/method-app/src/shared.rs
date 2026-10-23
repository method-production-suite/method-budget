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
//! - From the first role on, every change is signed with this computer's
//!   key ([`SharedDoc::use_key`]; decisions.md, entry 89), and a change
//!   whose signature is missing or doesn't match is left out and listed;
//!   removing a person ([`SharedDoc::remove_person`]) leaves out their
//!   changes the remover hadn't seen.
//!
//! The words for the interface: changes are "shared", never synced; a
//! conflict is "changed by two people"; there are no actors, logs or
//! frontiers on screen (design, 9.10).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use method_core::log::{
    ActorId, ChangeId, Field, Hint, Ids, Kind, LogError, Projection, Review, Role, Signer, Stamp,
    Uid, Value,
};
use method_core::{Budget, RowId};
use method_format::log::{self as flog, AccessRequest, LoadReport, Presence, Scan, Shared};
use serde::{Deserialize, Serialize};

use crate::i18n::{tr, tr_with};
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
    /// What they may change: "owner", "co_owner", "admin", "editor", "department",
    /// "actuals", "comment" or "read_only".
    pub role: String,
    /// The role in words: "Edits everything", "Edits 2000 Production".
    pub role_text: String,
    /// A department editor's categories and accounts, by number.
    pub scope: Vec<String>,
    /// Their computer's key is in the budget: their changes are signed.
    pub signs: bool,
    /// Removed by the owner, a co-owner or an admin, and not given a role
    /// since: their later changes are left out.
    pub removed: bool,
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
    /// The latest of them, newest last (at most [`REFUSED_SHOWN`]).
    pub refused_changes: Vec<RefusedDto>,
    /// Something to tell the person about the file (an edit made outside
    /// the app kept aside), until the next one.
    pub notice: Option<String>,
    /// Changes in this budget are signed (the owner's key is in it).
    pub signed: bool,
    /// What signing means here, in a sentence.
    pub signed_text: String,
    /// A key in this person's name is in the budget and it isn't this
    /// computer's (someone else put it in): nothing here is signed or
    /// shared until that is sorted out. In a sentence.
    pub key_warning: Option<String>,
    /// This is the owner's computer and a key in the owner's name isn't its
    /// own: Start Again is offered (docs/research/owner-key.md, part 1).
    pub can_start_again: bool,
    /// The owner's key isn't the one this computer saw first (part 2): what
    /// to tell the person, and Trust the New Key is offered.
    pub owner_key_warning: Option<String>,
    /// The owner started this budget again: what to tell the person, who
    /// reopens it to join.
    pub replaced: Option<String>,
    /// This person's changes that weren't shared before the budget was
    /// started again, by label, to make again by hand.
    pub not_in_new: Vec<String>,
    /// Access requests this person could allow, oldest first, leaving out
    /// the ones declined on this computer (docs/APP.md, "Asking for
    /// access").
    pub requests: Vec<RequestDto>,
    /// This person's own request, while it is waiting.
    pub my_request: Option<RequestDto>,
    /// This person may change less than everything once roles are set, so
    /// the page offers to ask for access.
    pub can_ask: bool,
    /// Who to ask, by name: the owner.
    pub ask_name: String,
    /// A department editor's categories with nothing in their scope, by
    /// number: the topsheet shows a lock beside them. Empty for every other
    /// role.
    pub locked_categories: Vec<String>,
    /// A department editor's accounts outside their scope, by number: their
    /// rows and lines are muted, with a lock. Empty for every other role.
    pub locked_accounts: Vec<String>,
    /// Whether this person may invite someone (decisions.md, entry 97).
    pub can_invite: bool,
    /// Invites waiting to be used, and used ones with who joined: only for
    /// someone who may give roles.
    pub invites: Vec<InviteDto>,
}

/// One invite, as the Share dialog lists it (decisions.md, entry 97).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InviteDto {
    /// The invite's id (32 hex characters): what Cancel names.
    pub id: String,
    /// Who made it, and their color.
    pub by: String,
    pub by_color: String,
    /// The role it gives, as a key and in words.
    pub role: String,
    pub role_text: String,
    /// When it was made and when it expires (ISO 8601 UTC).
    pub made: String,
    pub expires: String,
    /// "open" (waiting to be used) or "used".
    pub state: String,
    /// Who joined with it, once used.
    pub joined: Option<String>,
    pub joined_color: Option<String>,
    /// Whether this person may cancel it.
    pub can_cancel: bool,
}

/// An invite just made: the link, shown once and never kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatedInvite {
    pub link: String,
    /// The role it gives, in words.
    pub role_text: String,
    /// When it expires (ISO 8601 UTC).
    pub expires: String,
}

/// How long an invite may last, in days: the Share dialog's choices.
pub const INVITE_DAYS: [u64; 3] = [1, 7, 30];

/// One access request, as the status panel shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RequestDto {
    /// Who asks (64 hex characters).
    pub actor: String,
    pub name: String,
    pub color: String,
    /// The role asked for ("editor", "department", "actuals", "comment").
    pub role: String,
    /// What Allow gives, in words: "Edits everything", "Edits 2400 Grip".
    pub role_text: String,
    /// The category or account numbers it is about.
    pub codes: Vec<String>,
    /// Those, in words: "2400 Grip".
    pub codes_text: String,
    /// Allow those categories or accounts only is a choice of its own (it
    /// gives less than Allow).
    pub only: bool,
    pub note: String,
    /// When they asked (ISO 8601 UTC); with `actor`, what names it.
    pub at: String,
}

/// How long a note on an access request may be, in characters.
pub const REQUEST_NOTE_MAX_CHARS: usize = 200;

/// The roles a person may ask for.
const ASKABLE: [&str; 4] = ["editor", "department", "actuals", "comment"];

/// A request declined on this computer, as remembered.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Declined {
    actor: String,
    at: String,
}

/// How many refused changes the status lists.
pub const REFUSED_SHOWN: usize = 20;

/// A change left out: who made it, when, what it was and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefusedDto {
    pub actor: String,
    pub name: String,
    pub color: String,
    pub time: String,
    pub label: String,
    /// Why, in a sentence.
    pub why: String,
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
    /// Signed with its author's key.
    pub signed: bool,
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
    /// What happened to an outside edit kept aside, for the status.
    outside: Option<String>,
    /// Where this computer's tool actor pools are (`None`: nowhere known).
    tools_root: Option<PathBuf>,
    /// This person's own programs' actors on this computer.
    local_tools: BTreeSet<ActorId>,
    /// This computer's key, once given (kept to sign a budget started
    /// again or opened afresh).
    signer: Option<std::sync::Arc<dyn Signer>>,
    /// The owner started this budget again: this person's changes that
    /// weren't shared yet, by label. Nothing is written until they reopen.
    replaced: Option<Vec<String>>,
    /// Where this computer remembers owner keys (`None`: nowhere, so none
    /// is remembered; docs/research/owner-key.md, part 2).
    owner_keys: Option<PathBuf>,
    /// This person's access request, written into the heartbeat while it
    /// waits.
    request: Option<AccessRequest>,
    /// Where this computer remembers requests it declined (`None`: only
    /// for as long as the budget is open).
    declined_dir: Option<PathBuf>,
    /// Requests declined here, by who and when they asked.
    declined: BTreeSet<Declined>,
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
            outside: None,
            tools_root: flog::actor_id_path().map(|p| flog::tools_root(&p)),
            local_tools: BTreeSet::new(),
            signer: None,
            replaced: None,
            owner_keys: None,
            request: None,
            declined_dir: None,
            declined: BTreeSet::new(),
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
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
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
        if self.replaced.is_some() {
            return out;
        }
        if !self.shared.dir.exists() {
            self.error = Some(tr("shared.cant_reach"));
            return out;
        }
        // The owner started the budget again: nothing more goes into the
        // folder from this log (docs/research/owner-key.md, part 1).
        if flog::replaced(&self.shared) {
            self.replaced = Some(
                self.shared
                    .log
                    .own_after(self.written_seq)
                    .iter()
                    .map(|c| c.label.clone())
                    .collect(),
            );
            self.error = None;
            return out;
        }
        self.flush(now, false);
        // Who is here (their names go on the changes that arrive).
        if self
            .presence_read_at
            .is_none_or(|t| now.duration_since(t) >= PRESENCE_EVERY)
        {
            self.people = self.shared.dir.presences();
            self.read_local_tools();
            self.presence_read_at = Some(now);
        }
        // What others wrote.
        let before = self.shared.log.len();
        let folds = self.shared.log.folds();
        let report = flog::absorb(&mut self.shared.log, &self.shared.dir, &mut self.scan);
        if self.shared.log.folds() != folds {
            // A removal (or the owner's first role or key) left out changes
            // this computer had applied: the budget is made again.
            self.reproject();
            *budget = self.proj.budget.clone();
            out.budget_changed = true;
            self.last_change = now;
        }
        self.last_sync = Some(now);
        self.waiting = report.pending;
        // The owner's key arrived: this computer's goes in too, and the
        // owner's is remembered.
        if report.applied > 0 && self.shared.log.signer_key().is_some() {
            if let Err(e) = self.publish_key() {
                self.error = Some(e);
            }
            self.remember_owner_key();
        }
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
        // Invites used or past their date are sealed by whoever may give
        // their role (decisions.md, entry 97), so a link that reaches someone
        // later can't take the place of the person who joined.
        if self.seal_invites() {
            self.reproject();
            *budget = self.proj.budget.clone();
        }
        if self.keeper && self.keep_file(now, budget) {
            out.budget_changed = true;
        }
        // Given what was asked for: the request goes.
        if self
            .request
            .as_ref()
            .is_some_and(|r| self.has_asked(self.actor(), r))
        {
            self.request = None;
            self.place_changed = true;
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
                self.error = Some(tr_with(
                    "shared.cant_write_folder",
                    &[("why", e.0.as_str())],
                ));
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
            if let Ok(bytes) = std::fs::read(&path) {
                if bytes == flog::snapshot_text(&self.shared, &self.proj).as_bytes() {
                    // Another program of this person's wrote what the log
                    // holds (the owner's command line): nothing to do.
                    self.snapshot_len = self.shared.log.len();
                    self.disk = Some(DiskStamp::of(&path, &bytes));
                } else if self.shared.dir.is_written(&bytes) {
                    // An older version of the budget put back (a sync
                    // service's history, a backup): behind the log, so it
                    // is written again rather than taken in.
                    self.write_file(&path);
                    self.outside = Some(format!(
                        "{} was put back to an older version of the budget, so it was written again from everyone\u{2019}s changes. Nothing was lost.",
                        self.shared.info.file
                    ));
                    return changed;
                }
            }
        }
        if self.disk.is_some() && !self.file_matches(&path) && self.shared.log.roles_started() {
            // Roles are set: nobody can say whose edit the file holds, so it
            // isn't taken in as the keeper's (decisions.md, entry 78). It is
            // kept beside the file and the file is written again.
            match flog::set_aside(&self.shared, &iso_now()) {
                Ok(Some(kept)) => {
                    self.outside = Some(format!(
                        "Someone changed {} outside Method Budget. Because roles are set, that change wasn\u{2019}t taken in: it was kept as \u{201c}{}\u{201d} beside it, and the file was written again from everyone\u{2019}s changes.",
                        self.shared.info.file,
                        kept.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    ));
                    self.snapshot_len = self.shared.log.len();
                    self.note_snapshot(&path);
                }
                Ok(None) => {}
                Err(e) => self.error = Some(e.0),
            }
            self.disk = std::fs::read(&path).ok().map(|b| DiskStamp::of(&path, &b));
        } else if self.disk.is_some() && !self.file_matches(&path) {
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
                self.error = Some(tr_with("shared.cant_write_file", &[("why", e.0.as_str())]));
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
            request: self.request.clone(),
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
    /// Uses `root` for this computer's tool actor pools (tests).
    pub fn set_tools_root(&mut self, root: PathBuf) {
        self.tools_root = Some(root);
        self.read_local_tools();
    }

    fn read_local_tools(&mut self) {
        self.local_tools = self
            .tools_root
            .as_deref()
            .map(|r| flog::tool_actors(r, self.actor()).into_iter().collect())
            .unwrap_or_default();
    }

    /// The person a program's actor writes for: from the log once it says
    /// so, and for this person's own programs from this computer's pool
    /// (which only this computer can see).
    fn person_for(&self, actor: ActorId) -> Option<ActorId> {
        let person = self.shared.log.person_of(actor);
        if person != actor {
            return Some(person);
        }
        self.local_tools.contains(&actor).then(|| self.actor())
    }

    /// An actor that is surely a program whose person isn't known here
    /// yet: it made changes but never wrote a heartbeat (every app does),
    /// has no role and didn't start sharing. Other people's programs say
    /// whose they are only once roles are set (`acts_for`), so until then
    /// this computer can't honestly name them.
    fn unknown_program(&self, actor: ActorId) -> bool {
        actor != self.actor()
            && self.shared.log.owner() != Some(actor)
            && self.person_for(actor).is_none()
            && !self.people.contains_key(&actor)
            && !self.shared.log.state().grants().contains_key(&actor)
    }

    pub fn name_of(&self, actor: ActorId) -> String {
        // A command line or agent writing for someone shows as them.
        if let Some(person) = self.person_for(actor) {
            return format!("{} (command line or agent)", self.name_of(person));
        }
        if self.unknown_program(actor) {
            return "A command line or agent (whose isn\u{2019}t known yet)".into();
        }
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
        if let Some(person) = self.person_for(actor) {
            return self.color_of(person);
        }
        if self.unknown_program(actor) {
            return "slate".into();
        }
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
        let removed = self.shared.log.removed(actor);
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
            role_text: if removed {
                "Removed: reads only, and later changes are left out".into()
            } else {
                self.role_text(&role)
            },
            scope: role
                .scope()
                .map(|s| self.scope_codes(s))
                .unwrap_or_default(),
            signs: self.shared.log.key_of(actor).is_some(),
            removed,
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
            .filter(|a| **a != me && self.person_for(**a).is_none() && !self.unknown_program(**a))
            .map(|a| self.person_dto(*a, now))
            .collect();
        let review = self.review();
        let (locked_categories, locked_accounts) = self.locked();
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
            refused: self.shared.log.rejected().len() + self.shared.log.forks().len(),
            refused_changes: self.refused_changes(),
            notice: self.outside.clone(),
            signed: self.shared.log.signing_started(),
            signed_text: self.signed_text(),
            key_warning: self.key_warning(),
            can_start_again: self.signer.is_some()
                && self.shared.log.owner() == Some(me)
                && self.shared.log.key_conflict().is_some(),
            owner_key_warning: self.owner_key_warning(),
            replaced: self.replaced_text(),
            not_in_new: self.replaced.clone().unwrap_or_default(),
            requests: self.requests(),
            my_request: self.request.as_ref().map(|r| self.request_dto(me, r)),
            can_ask: self.can_ask(),
            ask_name: self.owner_name(),
            locked_categories,
            locked_accounts,
            can_invite: self.can_invite(),
            invites: self.invites(),
        }
    }

    /// When a key in this person's name is in the budget and it isn't this
    /// computer's, what to tell them (decisions.md, entry 89).
    pub fn key_warning(&self) -> Option<String> {
        self.shared.log.key_conflict()?;
        Some(if self.shared.log.owner() == Some(self.actor()) {
            tr("shared.key_taken_owner")
        } else {
            tr_with("shared.key_taken", &[("owner", self.owner_name().as_str())])
        })
    }

    /// What signing means in this budget now, in a sentence.
    fn signed_text(&self) -> String {
        if self.shared.log.signing_started() {
            "Changes are signed: each person\u{2019}s computer signs what they change with a key kept on that computer, and a change whose signature doesn\u{2019}t match is left out.".into()
        } else if self.shared.log.roles_started() {
            "Changes aren\u{2019}t signed yet: they will be once the owner opens this budget in this version.".into()
        } else {
            "Changes are signed once the owner sets the first role.".into()
        }
    }

    /// The changes left out, newest last, with why in plain words.
    fn refused_changes(&self) -> Vec<RefusedDto> {
        // Changes refused, then other changes claiming the same number as
        // another (MReview, pull request #73): both listed.
        let all: Vec<&(method_core::log::Change, String)> = self
            .shared
            .log
            .rejected()
            .iter()
            .chain(self.shared.log.forks())
            .collect();
        all[all.len().saturating_sub(REFUSED_SHOWN)..]
            .iter()
            .copied()
            .map(|(c, reason)| RefusedDto {
                actor: c.actor.hex(),
                name: self.name_of(c.actor),
                color: self.color_of(c.actor),
                time: c.time.clone(),
                label: c.label.clone(),
                why: refused_why(reason),
            })
            .collect()
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
            .ok_or_else(|| tr("shared.entity_gone"))?;
        let field = Field::parse(field).ok_or_else(|| tr("shared.unknown_field"))?;
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
            .map_or_else(|| tr("shared.the_owner"), |o| self.name_of(o))
    }

    /// Why this person can't change anything, when their role reads only.
    pub fn refusal(&self) -> Option<String> {
        if let Some(w) = self.replaced_text() {
            return Some(w);
        }
        if let Some(w) = self.key_warning() {
            return Some(w);
        }
        if let Some(w) = self.owner_key_warning() {
            return Some(w);
        }
        if self.shared.log.removed(self.actor()) {
            return Some(tr_with(
                "shared.removed_ask",
                &[("owner", self.owner_name().as_str())],
            ));
        }
        self.role().reads_only().then(|| {
            tr_with(
                "shared.read_only_ask",
                &[("owner", self.owner_name().as_str())],
            )
        })
    }

    /// A refused change in the words the page shows: what this person's
    /// role allows, and who to ask.
    fn refused_text(&self, e: &LogError) -> String {
        match e {
            LogError::Unsigned { reason, .. }
                if reason == method_core::log::sign::why::KEY_TAKEN =>
            {
                return self.key_warning().unwrap_or_default()
            }
            LogError::Unsigned { reason, .. }
                if reason == method_core::log::sign::why::OWNER_KEY_CHANGED =>
            {
                return self.owner_key_warning().unwrap_or_default()
            }
            LogError::Unsigned { .. } => return tr("shared.unsigned"),
            LogError::Removed { .. } => {
                return self.refusal().unwrap_or_else(|| tr("shared.removed"))
            }
            LogError::NotAllowed { .. } => {}
            _ => return tr_with("shared.not_shared", &[("why", e.to_string().as_str())]),
        }
        let owner = self.owner_name();
        let ask = tr_with("shared.ask", &[("owner", owner.as_str())]);
        match self.role() {
            Role::Department(scope) => tr_with(
                "shared.department_only",
                &[
                    ("scope", self.scope_words(&scope).as_str()),
                    ("owner", owner.as_str()),
                ],
            ),
            Role::Actuals => tr_with("shared.actuals_only", &[("owner", owner.as_str())]),
            Role::Comment => tr_with("shared.notes_only", &[("owner", owner.as_str())]),
            Role::ReadOnly => self.refusal().unwrap_or(ask),
            Role::Editor => tr("shared.editor_limits"),
            Role::Admin => tr("shared.only_make_admins"),
            Role::CoOwner => tr("shared.only_make_coowners"),
            Role::Owner => tr_with("shared.not_shared", &[("why", e.to_string().as_str())]),
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
        self.codes_words(&self.scope_codes(scope))
    }

    /// Category and account numbers with their names: "2000 Production and
    /// 2400 Grip".
    fn codes_words(&self, codes: &[String]) -> String {
        let b = &self.proj.budget;
        let names: Vec<String> = codes
            .iter()
            .cloned()
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
            Role::CoOwner => {
                "Co-owner: edits everything and sets what people may change, admins included".into()
            }
            Role::Admin => "Edits everything and sets what people may change".into(),
            Role::Editor => "Edits everything".into(),
            Role::Department(scope) => format!("Edits {}", self.scope_words(scope)),
            Role::Actuals => "Actuals and notes".into(),
            Role::Comment => "Notes only".into(),
            Role::ReadOnly => "Read only".into(),
        }
    }

    /// Sets what the person `actor` (64 hex characters) may change:
    /// `role` is "co_owner", "admin", "editor", "department", "actuals", "comment" or
    /// "read_only"; `scope` lists a department editor's categories or
    /// accounts by number. One change by this person, shared like any
    /// edit; not an undo step (set the role again to change it back).
    pub fn set_role(&mut self, actor: &str, role: &str, scope: &[String]) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        if Some(who) == self.shared.log.owner() || role == "owner" {
            return Err(tr("shared.one_owner"));
        }
        if role != "department" && !scope.is_empty() {
            return Err(tr("shared.scope_only_department"));
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
                .ok_or_else(|| tr_with("shared.no_code", &[("code", code)]))?;
            uids.insert(*uid);
        }
        let role = Role::parse(role, uids).ok_or_else(|| tr("shared.not_a_role"))?;
        if role.scope().is_some_and(|s| s.is_empty()) {
            return Err(tr("shared.pick_category"));
        }
        let label = format!("{}: {}", self.name_of(who), self.role_text(&role));
        let me = self.role();
        self.shared
            .log
            .grant(who, role, &label, &iso_now())
            .map_err(|e| match me {
                Role::Admin => tr("shared.only_make_admins"),
                Role::CoOwner => tr("shared.only_make_coowners"),
                Role::Owner => tr_with("shared.role_not_set", &[("why", e.to_string().as_str())]),
                _ => tr_with(
                    "shared.only_set_roles",
                    &[("owner", self.owner_name().as_str())],
                ),
            })?;
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Signs this person's changes with `key` (this computer's key pair,
    /// kept in the app's data folder, never in the shared folder;
    /// decisions.md, entry 89). When the budget's changes are signed, or
    /// this is the owner of a budget with roles, the key goes into the
    /// log now, as a change of its own.
    /// Where this computer remembers owner keys (the app passes
    /// [`flog::owner_keys_dir`]; tests pass a folder of their own).
    pub fn set_owner_keys_dir(&mut self, dir: Option<PathBuf>) {
        self.owner_keys = dir;
    }

    pub fn use_key(&mut self, key: std::sync::Arc<dyn Signer>) -> Result<(), String> {
        self.shared.log.set_signer(key.clone());
        self.signer = Some(key);
        self.publish_key()?;
        self.remember_owner_key();
        Ok(())
    }

    /// Remembers the owner's key the first time this computer sees it
    /// (docs/research/owner-key.md, part 2).
    fn remember_owner_key(&mut self) {
        let Some(dir) = self.owner_keys.clone() else {
            return;
        };
        if self.signer.is_none() {
            return;
        }
        let me = self.actor();
        if let Err(e) = flog::remember_owner_key(&mut self.shared, &dir, me, &iso_now()) {
            self.error = Some(tr_with(
                "shared.cant_remember_key",
                &[("why", e.0.as_str())],
            ));
        }
    }

    /// Start Again (docs/research/owner-key.md, part 1): on the owner's
    /// computer, when a key in the owner's name isn't this computer's, a new
    /// shared budget made from this one as it stands, signed with this
    /// computer's key and with everyone's roles, takes the log folder's
    /// place; the old one is kept aside. The budget itself doesn't change;
    /// who changed what and undo start fresh, and everyone else reopens it.
    pub fn start_again(&mut self) -> Result<(), String> {
        if self.shared.log.owner() != Some(self.actor()) || self.shared.log.key_conflict().is_none()
        {
            return Err(tr("shared.start_again_owner"));
        }
        let signer = self.signer.clone().ok_or_else(|| tr("shared.no_key"))?;
        // Everything of this person's is in the folder first, so the old
        // log kept aside is whole.
        self.flush(Instant::now(), true);
        let shared = flog::start_again(
            &self.shared,
            signer,
            self.owner_keys.as_deref(),
            Ids::from_os(),
            &iso_now(),
        )
        .map_err(|e| e.0)?;
        self.adopt_log(shared);
        self.keeper = true;
        let path = self.shared.path.clone();
        self.note_snapshot(&path);
        Ok(())
    }

    /// Opens this budget afresh from its folder: joins the budget the owner
    /// started again, or takes in what a newly trusted owner key kept
    /// aside. Returns the budget as it now stands.
    pub fn reopen(&mut self) -> Result<Budget, String> {
        let actor = self.actor();
        let path = self.shared.path.clone();
        let (shared, report) =
            flog::open_in(&path, actor, Ids::from_os(), self.owner_keys.as_deref())
                .map_err(|e| e.0)?;
        self.keeper = shared.info.genesis.actor == actor;
        self.adopt_log(shared);
        self.waiting = report.pending;
        if let Some(signer) = self.signer.clone() {
            self.use_key(signer)?;
        }
        Ok(self.proj.budget.clone())
    }

    /// Trust the New Key (docs/research/owner-key.md, part 2): this
    /// computer remembers the owner's new key in place of the one it saw
    /// before (an owner who moved to a new computer), and the budget is
    /// opened afresh so what the old key kept aside comes in.
    pub fn trust_owner_key(&mut self) -> Result<Budget, String> {
        let key = self
            .shared
            .log
            .owner_key_changed()
            .ok_or_else(|| tr("shared.owner_key_same"))?;
        let dir = self
            .owner_keys
            .clone()
            .ok_or_else(|| tr("shared.nowhere_to_remember"))?;
        flog::trust_owner_key(&dir, &self.shared.path, &self.shared.info, key, &iso_now())
            .map_err(|e| e.0)?;
        self.reopen()
    }

    /// Takes `shared` in place of this budget's log: a budget started again,
    /// or the same budget read afresh. Undo, the recent list and the
    /// counts start over.
    fn adopt_log(&mut self, shared: Shared) {
        self.shared = shared;
        if let Some(signer) = self.signer.clone() {
            self.shared.log.set_signer(signer);
        }
        self.written_seq = self.shared.dir.highest_written(self.actor());
        self.snapshot_len = self.shared.log.len();
        self.steps.clear();
        self.redo.clear();
        self.recent.clear();
        self.replaced = None;
        self.error = None;
        self.waiting = 0;
        self.scan = self.shared.dir.scan();
        self.last_sync = Some(Instant::now());
        self.reproject();
    }

    /// When the owner's key isn't the one this computer saw first, what to
    /// tell the person (docs/research/owner-key.md, part 2).
    pub fn owner_key_warning(&self) -> Option<String> {
        self.shared.log.owner_key_changed()?;
        Some(tr_with(
            "shared.owner_key_changed",
            &[("owner", self.owner_name().as_str())],
        ))
    }

    /// What to tell the person once the owner started the budget again.
    fn replaced_text(&self) -> Option<String> {
        self.replaced.as_ref()?;
        Some(tr("shared.replaced"))
    }

    /// Puts this computer's key into the log when it should be there and
    /// isn't (after the owner's key arrives, for everyone else).
    fn publish_key(&mut self) -> Result<(), String> {
        let added = self
            .shared
            .log
            .publish_key("Added this computer\u{2019}s key", &iso_now())
            .map_err(|e| self.refused_text(&e))?;
        if added.is_some() {
            self.reproject();
            let now = Instant::now();
            self.last_edit = Some(now);
            self.unwritten_since.get_or_insert(now);
            self.last_change = now;
        }
        Ok(())
    }

    /// Removes the person `actor` (64 hex characters): they read only, and
    /// their changes this computer hasn't seen are left out on every
    /// computer (decisions.md, entry 89). One change by this person, shared
    /// like any edit; giving them a role again lets them back in from then on.
    pub fn remove_person(&mut self, actor: &str) -> Result<(), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        if Some(who) == self.shared.log.owner() {
            return Err(tr("shared.owner_stays"));
        }
        if who == self.actor() {
            return Err(tr("shared.not_yourself"));
        }
        let label = format!("Removed {}", self.name_of(who));
        let me = self.role();
        self.shared
            .log
            .revoke(who, &label, &iso_now())
            .map_err(|e| match (me, e) {
                (_, e @ (LogError::Unsigned { .. } | LogError::Removed { .. })) => {
                    self.refused_text(&e)
                }
                (Role::Admin, _) => tr("shared.only_remove_admins"),
                (Role::CoOwner, _) => tr("shared.only_remove_coowners"),
                (Role::Owner, e) => {
                    tr_with("shared.not_removed", &[("why", e.to_string().as_str())])
                }
                _ => tr_with(
                    "shared.only_remove_people",
                    &[("owner", self.owner_name().as_str())],
                ),
            })?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    // Access requests (docs/research/collaboration.md, 9.7; docs/APP.md,
    // "Asking for access")

    /// The category an account is in, by number.
    fn category_of_account(&self, account: &str) -> Option<String> {
        self.proj
            .budget
            .categories()
            .find(|c| c.accounts.iter().any(|a| a.code == account))
            .map(|c| c.code.clone())
    }

    /// Whether `role` lets its person change lines in category or account
    /// `code`.
    fn covers(&self, role: &Role, code: &str) -> bool {
        match role {
            Role::Owner | Role::CoOwner | Role::Admin | Role::Editor => true,
            Role::Department(scope) => {
                let codes = self.scope_codes(scope);
                codes.iter().any(|c| c == code)
                    || (self.proj.binding.accounts.contains_key(code)
                        && self
                            .category_of_account(code)
                            .is_some_and(|cat| codes.contains(&cat)))
            }
            _ => false,
        }
    }

    /// A department editor's categories with no part in their scope, and
    /// accounts outside it; empty for every other role.
    fn locked(&self) -> (Vec<String>, Vec<String>) {
        let role = self.role();
        if !matches!(role, Role::Department(_)) || self.shared.log.removed(self.actor()) {
            return (Vec::new(), Vec::new());
        }
        let mut cats = Vec::new();
        let mut accounts = Vec::new();
        for c in self.proj.budget.categories() {
            let mut any = self.covers(&role, &c.code);
            for a in &c.accounts {
                if self.covers(&role, &a.code) {
                    any = true;
                } else {
                    accounts.push(a.code.clone());
                }
            }
            if !any {
                cats.push(c.code.clone());
            }
        }
        (cats, accounts)
    }

    /// Whether `actor` already has what `req` asks for: the categories or
    /// accounts it names, for a department; the role or more, otherwise.
    fn has_asked(&self, actor: ActorId, req: &AccessRequest) -> bool {
        if self.shared.log.removed(actor) {
            return false;
        }
        let role = self.shared.log.role_of(actor);
        let covered = !req.codes.is_empty() && req.codes.iter().all(|c| self.covers(&role, c));
        let full = matches!(
            role,
            Role::Owner | Role::CoOwner | Role::Admin | Role::Editor
        );
        match req.role.as_str() {
            "department" => covered || full,
            "editor" => full || covered,
            "actuals" => full || role == Role::Actuals || covered,
            "comment" => full || matches!(role, Role::Actuals | Role::Comment) || covered,
            _ => true,
        }
    }

    /// What Allow gives for `req` by `actor` (`only`: the categories or
    /// accounts asked about, as a department editor), as a role key and
    /// scope for [`SharedDoc::set_role`].
    fn granted_for(
        &self,
        actor: ActorId,
        req: &AccessRequest,
        only: bool,
    ) -> (String, Vec<String>) {
        if only || req.role == "department" {
            let mut scope = match self.shared.log.role_of(actor) {
                Role::Department(s) if !self.shared.log.removed(actor) => self.scope_codes(&s),
                _ => Vec::new(),
            };
            for c in &req.codes {
                if !scope.contains(c) {
                    scope.push(c.clone());
                }
            }
            ("department".into(), scope)
        } else {
            (req.role.clone(), Vec::new())
        }
    }

    /// A scope's numbers as the uids a role holds (unknown numbers left out).
    fn scope_uids(&self, codes: &[String]) -> BTreeSet<Uid> {
        codes
            .iter()
            .filter_map(|c| {
                self.proj
                    .binding
                    .categories
                    .get(c)
                    .or_else(|| self.proj.binding.accounts.get(c))
                    .copied()
            })
            .collect()
    }

    /// A request as the panel shows it.
    fn request_dto(&self, actor: ActorId, req: &AccessRequest) -> RequestDto {
        let (key, scope) = self.granted_for(actor, req, false);
        let given = Role::parse(&key, self.scope_uids(&scope)).unwrap_or(Role::ReadOnly);
        RequestDto {
            actor: actor.hex(),
            name: self.name_of(actor),
            color: self.color_of(actor),
            role: req.role.clone(),
            role_text: self.role_text(&given),
            codes: req.codes.clone(),
            codes_text: self.codes_words(&req.codes),
            only: !req.codes.is_empty() && req.role != "department",
            note: req.note.clone(),
            at: req.at.clone(),
        }
    }

    /// Whether this person could allow `req` by `actor` under the rules for
    /// setting roles.
    fn may_allow(&self, actor: ActorId, req: &AccessRequest) -> bool {
        if actor == self.actor() || Some(actor) == self.shared.log.owner() {
            return false;
        }
        let (key, scope) = self.granted_for(actor, req, false);
        let Some(given) = Role::parse(&key, self.scope_uids(&scope)) else {
            return false;
        };
        method_core::log::may_give(&self.role(), &self.shared.log.role_of(actor), &given)
    }

    /// The requests waiting that this person could allow, oldest first:
    /// not declined here, not yet given, from people (never programs).
    pub fn requests(&self) -> Vec<RequestDto> {
        if self.replaced.is_some() || !self.role().sets_roles() {
            return Vec::new();
        }
        let mut out: Vec<(String, RequestDto)> = self
            .people
            .iter()
            .filter_map(|(actor, p)| {
                let req = p.request.as_ref()?;
                if !ASKABLE.contains(&req.role.as_str())
                    || self.person_for(*actor).is_some()
                    || self.declined.contains(&Declined {
                        actor: actor.hex(),
                        at: req.at.clone(),
                    })
                    || self.has_asked(*actor, req)
                    || !self.may_allow(*actor, req)
                {
                    return None;
                }
                Some((req.at.clone(), self.request_dto(*actor, req)))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// Whether this person may ask for access: roles are set and theirs is
    /// less than everything.
    pub fn can_ask(&self) -> bool {
        self.replaced.is_none()
            && self.shared.log.roles_started()
            && self.shared.log.owner() != Some(self.actor())
            && matches!(
                self.role(),
                Role::Department(_) | Role::Actuals | Role::Comment | Role::ReadOnly
            )
    }

    /// Asks the owner, co-owners and admins for `role` ("editor",
    /// "department", "actuals" or "comment"), about the categories or
    /// accounts `codes`, with a short `note`: written into this computer's
    /// heartbeat now, replacing any request before it. Nothing goes into the
    /// change log and nothing leaves the budget's folder.
    pub fn request_access(
        &mut self,
        role: &str,
        codes: &[String],
        note: &str,
    ) -> Result<(), String> {
        if !self.shared.log.roles_started() {
            return Err(tr("shared.nothing_to_ask"));
        }
        if !self.can_ask() {
            return Err(tr("shared.can_change_all"));
        }
        if !ASKABLE.contains(&role) {
            return Err(tr("shared.not_askable"));
        }
        let mut list: Vec<String> = Vec::new();
        for code in codes {
            let code = code.trim().to_string();
            if code.is_empty() || list.contains(&code) {
                continue;
            }
            if !self.proj.binding.categories.contains_key(&code)
                && !self.proj.binding.accounts.contains_key(&code)
            {
                return Err(tr_with("shared.no_code", &[("code", code.as_str())]));
            }
            list.push(code);
        }
        if role == "department" && list.is_empty() {
            return Err(tr("shared.say_which"));
        }
        let note: String = note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(REQUEST_NOTE_MAX_CHARS)
            .collect();
        let req = AccessRequest {
            role: role.to_string(),
            codes: list,
            note,
            at: iso_now(),
        };
        if self.has_asked(self.actor(), &req) {
            return Err(tr("shared.can_change_that"));
        }
        self.request = Some(req);
        self.heartbeat(Instant::now(), true);
        Ok(())
    }

    /// Takes this person's request back.
    pub fn withdraw_request(&mut self) {
        if self.request.take().is_some() {
            self.heartbeat(Instant::now(), true);
        }
    }

    /// The request by `actor` (64 hex characters) made at `at`, as its
    /// heartbeat holds it now.
    fn request_of(&self, actor: &str, at: &str) -> Result<(ActorId, AccessRequest), String> {
        let who = ActorId::parse(actor).ok_or_else(|| tr("shared.which_person"))?;
        let req = self
            .people
            .get(&who)
            .and_then(|p| p.request.clone())
            .filter(|r| r.at == at)
            .ok_or_else(|| tr("shared.request_gone"))?;
        Ok((who, req))
    }

    /// Allows the request by `actor` made at `at`: gives what it asks for,
    /// or with `only` the categories or accounts it names as a department
    /// editor (added to the ones they have), through [`SharedDoc::set_role`],
    /// so it is checked and signed like any role set in the Share dialog.
    pub fn allow_request(&mut self, actor: &str, at: &str, only: bool) -> Result<(), String> {
        let (who, req) = self.request_of(actor, at)?;
        if only && req.codes.is_empty() {
            return Err(tr("shared.request_no_codes"));
        }
        let (key, scope) = self.granted_for(who, &req, only);
        self.set_role(actor, &key, &scope)
    }

    /// Declines the request by `actor` made at `at`: not shown again on
    /// this computer (remembered beside this computer's sharing id, never in
    /// the shared folder); nothing is written to the change log. A request
    /// made again later shows again.
    pub fn decline_request(&mut self, actor: &str, at: &str) -> Result<(), String> {
        let (who, req) = self.request_of(actor, at)?;
        self.declined.insert(Declined {
            actor: who.hex(),
            at: req.at,
        });
        self.write_declined()
    }

    /// Where this computer remembers declined requests (the app passes a
    /// folder beside the sharing id; tests pass their own). Reads what is
    /// remembered there for this budget.
    pub fn set_declined_dir(&mut self, dir: Option<PathBuf>) {
        self.declined_dir = dir;
        if let Some(path) = self.declined_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(list) = serde_json::from_str::<Vec<Declined>>(&text) {
                    self.declined.extend(list);
                }
            }
        }
    }

    fn declined_path(&self) -> Option<PathBuf> {
        let dir = self.declined_dir.as_ref()?;
        Some(dir.join(format!("{}.json", self.shared.info.budget_id)))
    }

    fn write_declined(&self) -> Result<(), String> {
        let Some(path) = self.declined_path() else {
            return Ok(());
        };
        let list: Vec<&Declined> = self.declined.iter().collect();
        let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| tr_with("shared.cant_remember", &[("why", e.to_string().as_str())]))?;
        }
        std::fs::write(&path, text)
            .map_err(|e| tr_with("shared.cant_remember", &[("why", e.to_string().as_str())]))
    }

    // Invites (decisions.md, entry 97; docs/APP.md, "Inviting someone")

    /// Whether this person may invite someone now.
    pub fn can_invite(&self) -> bool {
        self.replaced.is_none()
            && self.shared.log.key_conflict().is_none()
            && self.shared.log.owner_key_changed().is_none()
            && !self.shared.log.removed(self.actor())
            && self.role().sets_roles()
    }

    /// Records an invite giving `role` ("editor", "department" with `scope`'s
    /// category or account numbers, "actuals", "comment", "read_only", or for
    /// the owner and co-owners "admin", and the owner "co_owner") for `days`
    /// days, and returns its link. The link holds the invite's secret, which
    /// no computer keeps: the log holds only its public key, so the link is
    /// shown once.
    pub fn create_invite(
        &mut self,
        role: &str,
        scope: &[String],
        days: u64,
    ) -> Result<CreatedInvite, String> {
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
        if !self.can_invite() {
            return Err(self.refusal().unwrap_or_else(|| {
                tr_with(
                    "shared.only_invite",
                    &[("owner", self.owner_name().as_str())],
                )
            }));
        }
        if !INVITE_DAYS.contains(&days) {
            return Err(tr("shared.invite_days"));
        }
        if role == "owner" {
            return Err(tr("shared.one_owner_invite"));
        }
        if role != "department" && !scope.is_empty() {
            return Err(tr("shared.scope_only_department"));
        }
        let mut uids = BTreeSet::new();
        for code in scope {
            let code = code.trim();
            let uid = self
                .proj
                .binding
                .categories
                .get(code)
                .or_else(|| self.proj.binding.accounts.get(code))
                .ok_or_else(|| tr_with("shared.no_code", &[("code", code)]))?;
            uids.insert(*uid);
        }
        let given = Role::parse(role, uids).ok_or_else(|| tr("shared.not_a_role_invite"))?;
        if given.scope().is_some_and(|s| s.is_empty()) {
            return Err(tr("shared.pick_category"));
        }
        if !method_core::log::may_give(&self.role(), &Role::ReadOnly, &given) {
            return Err(match self.role() {
                Role::CoOwner => tr("shared.only_invite_coowner"),
                _ => tr("shared.only_invite_admin"),
            });
        }
        let secret = flog::sign::new_invite_secret().map_err(|e| e.0)?;
        let id = flog::sign::new_invite_id().map_err(|e| e.0)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let expires = method_core::log::invite::rfc3339_at((now + days * 86_400) as i64);
        let role_text = self.role_text(&given);
        let label = format!("Invited someone: {role_text}");
        self.shared
            .log
            .invite(
                id,
                given,
                &expires,
                flog::sign::invite_key(&secret),
                &label,
                &iso_now(),
            )
            .map_err(|e| self.refused_text(&e))?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        let link = crate::invite_link::InviteLink {
            budget: self.shared.info.budget_id.clone(),
            invite: id,
            secret,
            file: self.file_name(),
        };
        Ok(CreatedInvite {
            link: link.to_text(),
            role_text,
            expires,
        })
    }

    /// Seals the invites this person may cancel that were used, or are past
    /// their date by this computer's clock: one cancel each, written like any
    /// change. True when anything was recorded.
    pub fn seal_invites(&mut self) -> bool {
        if !self.can_invite() {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let sealed = self.shared.log.seal_invites(now, &iso_now());
        if sealed.is_empty() {
            return false;
        }
        let at = Instant::now();
        self.last_edit = Some(at);
        self.unwritten_since.get_or_insert(at);
        self.last_change = at;
        true
    }

    /// The budget's file name, as the link names it.
    fn file_name(&self) -> String {
        self.shared
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.shared.info.file.clone())
    }

    /// The shared budget's id (`budget.json`'s `budget_id`).
    pub fn budget_id(&self) -> &str {
        &self.shared.info.budget_id
    }

    /// The invites waiting to be used and the ones used, oldest first, for
    /// someone who may give roles.
    pub fn invites(&self) -> Vec<InviteDto> {
        if !self.can_invite() {
            return Vec::new();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let me = self.actor();
        let mine = self.role();
        let mut out: Vec<InviteDto> = self
            .shared
            .log
            .state()
            .invites()
            .values()
            .filter_map(|rec| {
                let used = rec.claimed.map(|c| c.actor);
                let open = method_core::log::invite::open(rec, now);
                if used.is_none() && !open {
                    return None;
                }
                let by = self.shared.log.person_of(rec.by);
                Some(InviteDto {
                    id: rec.id.hex(),
                    by: self.name_of(rec.by),
                    by_color: self.color_of(rec.by),
                    role: rec.role.key().to_string(),
                    role_text: self.role_text(&rec.role),
                    made: rec.time.clone(),
                    expires: rec.expires.clone(),
                    state: if used.is_some() { "used" } else { "open" }.into(),
                    joined: used.map(|a| self.name_of(a)),
                    joined_color: used.map(|a| self.color_of(a)),
                    can_cancel: open
                        && (by == me
                            || method_core::log::may_give(&mine, &Role::ReadOnly, &rec.role)),
                })
            })
            .collect();
        out.sort_by(|a, b| a.made.cmp(&b.made).then(a.id.cmp(&b.id)));
        out
    }

    /// Cancels invite `id` (32 hex characters): a claim this computer
    /// hasn't seen is refused on every computer. A used invite may be
    /// cancelled too, which seals it: the use this computer has seen stands.
    pub fn cancel_invite(&mut self, id: &str) -> Result<(), String> {
        let uid = Uid::parse(id).ok_or_else(|| tr("shared.which_invite"))?;
        let rec = self
            .shared
            .log
            .state()
            .invites()
            .get(&uid)
            .cloned()
            .ok_or_else(|| tr("shared.invite_gone"))?;
        if rec.cancelled.is_some() {
            return Ok(());
        }
        let label = format!("Cancelled an invite: {}", self.role_text(&rec.role));
        self.shared
            .log
            .cancel_invite(uid, &label, &iso_now())
            .map_err(|e| match e {
                LogError::NotAllowed { .. } => tr("shared.only_cancel"),
                e => self.refused_text(&e),
            })?;
        self.reproject();
        let now = Instant::now();
        self.last_edit = Some(now);
        self.unwritten_since.get_or_insert(now);
        self.last_change = now;
        Ok(())
    }

    /// Takes up the invite in `link` as this person (File > Join from
    /// Link…): checks it against the budget as it reached this computer,
    /// records the claim (signed, with this computer's key in the log first),
    /// and writes it to the folder now. Returns the role it gave, in words.
    /// The errors are sentences for the person.
    pub fn join_invite(&mut self, link: &crate::invite_link::InviteLink) -> Result<String, String> {
        if let Some(t) = self.replaced_text() {
            return Err(t);
        }
        if link.budget != self.shared.info.budget_id {
            return Err(wrong_budget(&link.file));
        }
        if let Some(w) = self.key_warning().or_else(|| self.owner_key_warning()) {
            return Err(w);
        }
        let Some(rec) = self.shared.log.state().invites().get(&link.invite).cloned() else {
            return Err(invite_words(method_core::log::invite::why::NOT_FOUND, None));
        };
        let me = self.actor();
        let role = self.role();
        let removed = self.shared.log.removed(me);
        if self.shared.log.owner() == Some(me) || (!removed && !role.reads_only()) {
            return Err(tr_with(
                "shared.already_member",
                &[("role", self.role_text(&role).as_str())],
            ));
        }
        if rec.claimed.is_some() {
            return Err(invite_words(method_core::log::invite::why::USED, None));
        }
        if rec.cancelled.is_some() {
            return Err(invite_words(method_core::log::invite::why::CANCELLED, None));
        }
        let now = iso_now();
        let expired = match (
            method_core::log::invite::rfc3339_secs(&now),
            method_core::log::invite::rfc3339_secs(&rec.expires),
        ) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        };
        if expired {
            return Err(invite_words(
                method_core::log::invite::why::EXPIRED,
                Some(&rec.expires),
            ));
        }
        // This computer's key is in the log before the claim (it goes in
        // when the budget is opened; once more here in case it didn't).
        self.publish_key()?;
        let proof = flog::sign::claim_proof(&link.secret, link.invite, me);
        let label = format!("Joined from an invite: {}", self.role_text(&rec.role));
        self.shared
            .log
            .claim(link.invite, proof, &label, &now)
            .map_err(|e| match e {
                LogError::Invite { reason, .. } => invite_words(&reason, Some(&rec.expires)),
                e => self.refused_text(&e),
            })?;
        self.reproject();
        let at = Instant::now();
        self.last_change = at;
        self.last_edit = Some(at);
        self.unwritten_since.get_or_insert(at);
        self.flush(at, true);
        Ok(self.role_text(&self.role()))
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
            .ok_or_else(|| tr("shared.entity_gone"))?;
        let f = Field::parse(field).ok_or_else(|| tr("shared.unknown_field"))?;
        let history = self.shared.log.blame(uid, f);
        let (_, _, value) = history.get(index).ok_or_else(|| tr("shared.value_gone"))?;
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
            .ok_or_else(|| tr("shared.entity_gone"))?;
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
                    signed: c.sig.is_some(),
                }
            })
            .collect()
    }
}

/// The link is for another shared budget than the one picked.
pub fn wrong_budget(file: &str) -> String {
    tr_with("shared.wrong_budget", &[("file", file)])
}

/// Why an invite can't be taken up, in a sentence for the person joining
/// (`expires`: the invite's expiry, for the date in words).
pub fn invite_words(reason: &str, expires: Option<&str>) -> String {
    use method_core::log::invite::why;
    match reason {
        why::NOT_FOUND => tr("shared.invite.not_found"),
        why::USED => tr("shared.invite.used"),
        why::CANCELLED => tr("shared.invite.cancelled"),
        why::EXPIRED | why::BAD_TIME => match expires.and_then(|e| e.get(..10)) {
            Some(day) => tr_with("shared.invite.expired_on", &[("day", day)]),

            None => tr("shared.invite.expired"),
        },
        why::WRONG_SECRET => tr("shared.invite.wrong_secret"),
        why::NOT_ALLOWED => tr("shared.invite.not_allowed"),
        why::OWNER => tr("shared.invite.owner"),
        _ => tr("shared.invite.unchecked"),
    }
}

/// Why a change was left out, in a sentence for the status panel.
pub fn refused_why(reason: &str) -> String {
    if let Some(r) = reason.strip_prefix(method_core::log::refused::INVITE) {
        use method_core::log::invite::why;
        return match r {
            why::NOT_FOUND => "It joined through an invite this budget doesn\u{2019}t have.".into(),
            why::WRONG_SECRET => {
                "It joined through an invite without the invite\u{2019}s secret.".into()
            }
            why::EXPIRED | why::BAD_TIME => {
                "It joined through an invite after the invite expired.".into()
            }
            why::CANCELLED => {
                "It joined through an invite that was cancelled before it arrived.".into()
            }
            why::USED => "It joined through an invite someone else had already used.".into(),
            why::NOT_ALLOWED => {
                "It joined through an invite whose maker could no longer give that role.".into()
            }
            why::OWNER => "The owner doesn\u{2019}t join through an invite.".into(),
            _ => "Its invite couldn\u{2019}t be checked.".into(),
        };
    }
    use method_core::log::{refused, sign::why};
    if reason.starts_with(refused::FORK) || reason == method_core::log::FORK_SECOND {
        return "Two different changes claim to be the same change by this person, so neither was taken in.".into();
    }
    if reason == method_core::log::FORK_REPLACED {
        return "It claimed the place of one of this person\u{2019}s changes, which was taken in instead.".into();
    }
    if reason == method_core::log::FORK_REFUSED_TOO {
        return "It claimed the same place as another of this person\u{2019}s changes, and neither fits.".into();
    }
    if reason == method_core::log::FORK_WAITING {
        return "It claims the place of one of this person\u{2019}s changes and waits for changes that haven\u{2019}t arrived.".into();
    }
    if let Some(r) = reason.strip_prefix(refused::NOT_ALLOWED) {
        return format!("Their role doesn\u{2019}t allow it ({r}).");
    }
    if let Some(r) = reason.strip_prefix(refused::SIGNATURE) {
        return match r {
            why::MISMATCH => "Its signature doesn\u{2019}t match the person\u{2019}s key, so it may have been changed in the folder or written in their name.".into(),
            why::MISSING => "It isn\u{2019}t signed, and changes in this budget must be.".into(),
            why::NO_KEY => "It is signed, but its author\u{2019}s key isn\u{2019}t in this budget.".into(),
            why::NOT_PERSONS_KEY | why::PERSON_HAS_NO_KEY => "It came from a command line or agent that doesn\u{2019}t have its person\u{2019}s key.".into(),
            why::TWO_KEYS => "It tried to give its author a second key.".into(),
            _ => "Its signature couldn\u{2019}t be checked.".into(),
        };
    }
    if reason.starts_with(refused::REMOVED) {
        return "It was made after its author was removed.".into();
    }
    if reason.contains("first change") {
        return "It wasn\u{2019}t made from this budget\u{2019}s shared history.".into();
    }
    "It didn\u{2019}t fit the budget as it stood.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_for_leaving_a_change_out_are_plain() {
        use method_core::log::{refused, sign::why};
        let sig = |w: &str| refused_why(&format!("{}{w}", refused::SIGNATURE));
        assert!(sig(why::MISMATCH).starts_with("Its signature doesn\u{2019}t match"));
        assert!(sig(why::MISSING).contains("isn\u{2019}t signed"));
        assert!(refused_why(&format!("{}made after", refused::REMOVED)).contains("removed"));
        assert!(refused_why("not allowed: this person reads only").contains("role"));
        assert!(refused_why(method_core::log::FORK_REPLACED).contains("taken in instead"));
        assert!(refused_why(&format!("{}x", refused::FORK)).starts_with("Two different changes"));
        for text in [sig(why::MISMATCH), sig(why::NO_KEY), sig(why::CANT_CHECK)] {
            assert!(!text.contains('\u{2014}') && !text.contains('!'), "{text}");
        }
    }

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
