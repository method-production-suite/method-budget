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
use method_session::messages::{msg, word};
use method_sync::mesh::{Event as LiveEvent, Mesh};
use method_sync::{Cell, Frame, Relay, MOST_SELECTED};
use serde::{Deserialize, Serialize};

use crate::i18n::{tr, tr_with};
use crate::safety::{DiskStamp, DiskStampExt};

mod invites;
mod live;
mod record;
mod requests;
mod roles;
mod status;
mod tick;

use self::live::*;

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
    /// The cell they are on, over a live link (never for this person).
    pub cell: Option<Cell>,
    /// The other cells they have selected, over a live link (never for this
    /// person): empty for none.
    pub selection: Vec<Cell>,
    /// A live link to their computer is open now (never for this person).
    pub linked: bool,
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
    /// Whether this person has live mode on, on this computer.
    pub live_on: bool,
    /// How many people this computer has a live link to.
    pub live_connected: usize,
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
    /// The name is also another person's here (names come from each
    /// computer's own heartbeat), shown as something to check.
    pub name_taken: bool,
    /// No change of theirs is in the budget yet: a computer new to it.
    pub new_here: bool,
    /// A code for the computer asking ("3fa9 c1d2 07be 5e41"): the asker
    /// sees the same code on their own request, so the two can compare.
    pub code: String,
}

/// A computer's code, as people compare it out loud: the first 16 hex
/// characters of the SHA-256 of its whole id, in four groups. A hash, not
/// the id's own first characters, so matching another computer's code takes
/// about 2^64 tries, whatever id is chosen.
pub fn computer_code(actor: ActorId) -> String {
    let hex = method_core::log::hash::sha256_hex(&actor.0);
    format!(
        "{} {} {} {}",
        &hex[..4],
        &hex[4..8],
        &hex[8..12],
        &hex[12..16]
    )
}

/// How long a note on an access request may be, in characters (the same
/// limit every app applies to a note it reads).
pub const REQUEST_NOTE_MAX_CHARS: usize = flog::REQUEST_NOTE_MAX_CHARS;

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
    /// The cell this person is on, for those on a live link.
    cell: Option<Cell>,
    /// The cells this person has selected, for those on a live link.
    selection: Vec<Cell>,
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
    /// The live link, while this person has live mode on (decisions.md,
    /// entry 98). `None` is the default, and nothing touches the network.
    live: Option<Live>,
}

/// The live link and how far it has got.
#[derive(Debug)]
struct Live {
    mesh: Mesh,
    /// How many of the log's changes have been offered to everyone.
    offered: usize,
    /// Who the mesh was last told to connect.
    peers: BTreeMap<String, String>,
    /// Who has a link open now.
    linked: HashSet<String>,
    /// The cell each linked person last said they are on.
    cells: HashMap<String, Cell>,
    /// The cells each linked person last said they have selected.
    selections: HashMap<String, Vec<Cell>>,
}

/// How old a heartbeat may be and still be dialed. Far longer than the time
/// a heartbeat takes to go round, because a folder service that lags is when
/// the link matters most, and a computer's clock may be some way off. A link
/// already open stays, however old the heartbeat.
const LIVE_STALE: Duration = Duration::from_secs(15 * 60);

/// Starts a mesh for `budget`, using `relay`. This is the first thing that
/// touches the network. It can take a moment, so not on the window's thread.
pub fn live_mesh(relay: Relay, budget: &str) -> Result<Mesh, String> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| e.to_string())?;
    Mesh::start(secret, relay, budget).map_err(|e| e.0)
}

/// The largest run of changes in one frame; [`method_sync::MAX_FRAME`] is
/// the hard limit, and a line can grow when it is written into a frame.
const LIVE_FRAME_BYTES: usize = 1024 * 1024;

/// Closing the budget, or the app, ends live mode with it: the link closes
/// (the mesh's own drop) and the address comes out of the heartbeat file.
impl Drop for SharedDoc {
    fn drop(&mut self) {
        self.live_stop();
    }
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
            cell: None,
            selection: Vec::new(),
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
            live: None,
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

    /// The cell this person is on, as checked.
    pub fn cell(&self) -> Option<&Cell> {
        self.cell.as_ref()
    }

    /// The cells this person has selected, as checked.
    pub fn selection(&self) -> &[Cell] {
        &self.selection
    }

    /// The cell this person moved to. It goes only to those on a live link,
    /// and only while live mode is on.
    pub fn set_cell(&mut self, cell: Option<Cell>) {
        self.set_here(cell, self.selection.clone());
    }

    /// The cells this person has selected besides the cursor's (empty for
    /// none). Like the cell, they go only over a live link; more than
    /// [`MOST_SELECTED`] go as none.
    pub fn set_selection(&mut self, selection: Vec<Cell>) {
        self.set_here(self.cell.clone(), selection);
    }

    /// The cell and the selection together: one frame when either changed.
    pub fn set_here(&mut self, cell: Option<Cell>, selection: Vec<Cell>) {
        let cell = cell.and_then(checked_cell);
        let selection = checked_selection(selection);
        if cell == self.cell && selection == self.selection {
            return;
        }
        self.cell = cell;
        self.selection = selection;
        self.send_cursor();
    }

    /// Tells everyone on a live link where this person is now.
    fn send_cursor(&self) {
        if let Some(live) = &self.live {
            live.mesh.broadcast(Frame::Cursor {
                cell: self.cell.clone(),
                selection: self.selection.clone(),
            });
        }
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
            why::NOT_FOUND => tr("shared.refused.invite_not_found"),
            why::WRONG_SECRET => tr("shared.refused.invite_wrong_secret"),
            why::EXPIRED | why::BAD_TIME => tr("shared.refused.invite_expired"),
            why::CANCELLED => tr("shared.refused.invite_cancelled"),
            why::USED => tr("shared.refused.invite_used"),
            why::NOT_ALLOWED => tr("shared.refused.invite_not_allowed"),
            why::OWNER => tr("shared.refused.invite_owner"),
            _ => tr("shared.refused.invite_unchecked"),
        };
    }
    use method_core::log::{refused, sign::why};
    if reason.starts_with(refused::FORK) || reason == method_core::log::FORK_SECOND {
        return tr("shared.refused.fork");
    }
    if reason == method_core::log::FORK_REPLACED {
        return tr("shared.refused.fork_replaced");
    }
    if reason == method_core::log::FORK_REFUSED_TOO {
        return tr("shared.refused.fork_refused_too");
    }
    if reason == method_core::log::FORK_WAITING {
        return tr("shared.refused.fork_waiting");
    }
    if let Some(r) = reason.strip_prefix(refused::NOT_ALLOWED) {
        return tr_with("shared.refused.not_allowed", &[("why", r)]);
    }
    if let Some(r) = reason.strip_prefix(refused::SIGNATURE) {
        return match r {
            why::MISMATCH => tr("shared.refused.sig_mismatch"),
            why::MISSING => tr("shared.refused.sig_missing"),
            why::NO_KEY => tr("shared.refused.sig_no_key"),
            why::NOT_PERSONS_KEY | why::PERSON_HAS_NO_KEY => {
                tr("shared.refused.sig_not_persons_key")
            }
            why::TWO_KEYS => tr("shared.refused.sig_two_keys"),
            _ => tr("shared.refused.sig_unchecked"),
        };
    }
    if reason.starts_with(refused::REMOVED) {
        return tr("shared.refused.removed");
    }
    if reason.contains("first change") {
        return tr("shared.refused.not_this_history");
    }
    tr("shared.refused.didnt_fit")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_cell(kind: &str, key: &str) -> Cell {
        Cell {
            kind: kind.into(),
            key: key.into(),
            parent: Some("50-01".into()),
            field: Some("rate".into()),
        }
    }

    /// What a link brings is refused unless it is one of the grid's kinds with
    /// short names, whichever computer sent it.
    /// A selection from a link keeps the cells that pass, and none at all
    /// once it is longer than a frame may carry.
    #[test]
    fn a_selection_from_a_link_is_checked() {
        let good: Vec<Cell> = (0..3).map(|i| a_cell("row", &i.to_string())).collect();
        assert_eq!(checked_selection(good.clone()), good);
        let mut mixed = good.clone();
        mixed.insert(1, a_cell("script", "9"));
        assert_eq!(checked_selection(mixed), good);
        let most: Vec<Cell> = (0..MOST_SELECTED)
            .map(|i| a_cell("row", &i.to_string()))
            .collect();
        assert_eq!(checked_selection(most.clone()).len(), MOST_SELECTED);
        let mut over = most;
        over.push(a_cell("row", "x"));
        assert!(checked_selection(over).is_empty());
    }

    #[test]
    fn a_cell_from_a_link_is_checked() {
        for kind in ["row", "account", "category", "production_total"] {
            assert!(checked_cell(a_cell(kind, "7")).is_some(), "{kind}");
        }
        for kind in ["", "script", "Row", "fringe"] {
            assert_eq!(checked_cell(a_cell(kind, "7")), None, "{kind}");
        }
        let long = "x".repeat(201);
        assert!(checked_cell(a_cell("row", &"x".repeat(200))).is_some());
        assert_eq!(checked_cell(a_cell("row", &long)), None);
        let mut c = a_cell("row", "7");
        c.parent = Some(long.clone());
        assert_eq!(checked_cell(c), None);
        let mut c = a_cell("row", "7");
        c.field = Some(long);
        assert_eq!(checked_cell(c), None);
        let mut c = a_cell("row", "7");
        c.parent = None;
        c.field = None;
        assert!(checked_cell(c).is_some());
    }

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
