//! The change log on disk (docs/FORMAT.md, "The change log";
//! docs/research/collaboration.md, section 4): `Feature.mthd-log/` beside
//! the budget, one folder per actor, sealed segment files of JSON lines
//! that only their actor ever writes, and the keeper's snapshot of the
//! merged state written as the `.mthd` itself.
//!
//! Nothing here watches or polls on its own: [`LogDir::scan`] and
//! [`absorb`] are the hooks an application or `method sync` drives.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use method_core::log::hash::sha256_hex;
use method_core::log::{
    ActorId, Change, ChangeId, Field, Frontier, Ids, Kind, ListKey, ListName, Log, Malformed,
    Member, Node, Op, Placement, Projection, Side, Stamp, Uid, Value,
};
use method_core::Budget;
use serde::{Deserialize, Serialize};

use crate::dto::{
    FileBaseline, FileBudget, FileCoding, FileDisplay, FileFringeRow, FileGroupRules, FileInfo,
    FileItemSource, FileLinePreset, FileNewLineDefaults, FileOtPattern, FilePosting,
    FileReportPreset, FileSetupOrder, FileSnapshot, FileTagPreset,
};

/// The value of `format` in `budget.json`.
pub const LOG_FORMAT_NAME: &str = "method-budget-log";
/// The log format version this build writes and reads.
pub const LOG_FORMAT_VERSION: u64 = 1;
/// The suffix after the budget's file name: `Feature.mthd` has
/// `Feature.mthd-log/`.
pub const LOG_DIR_SUFFIX: &str = "-log";
/// The extension of a segment file.
pub const SEGMENT_EXTENSION: &str = "chg";

// ---------------------------------------------------------------------------
// Errors

/// Why a log could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogFileError(pub String);

impl fmt::Display for LogFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LogFileError {}

impl From<String> for LogFileError {
    fn from(s: String) -> LogFileError {
        LogFileError(s)
    }
}

fn io_err(path: &Path, e: std::io::Error) -> LogFileError {
    LogFileError(format!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// JSON encoding of changes

fn stamp_text(s: &Stamp) -> String {
    format!("{}/{}/{}/{}", s.lamport, s.actor.hex(), s.seq, s.index)
}

fn parse_stamp(s: &str) -> Result<Stamp, String> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 4 {
        return Err(format!("bad stamp {s:?}"));
    }
    Ok(Stamp {
        lamport: parts[0].parse().map_err(|_| format!("bad stamp {s:?}"))?,
        actor: ActorId::parse(parts[1]).ok_or_else(|| format!("bad stamp {s:?}"))?,
        seq: parts[2].parse().map_err(|_| format!("bad stamp {s:?}"))?,
        index: parts[3].parse().map_err(|_| format!("bad stamp {s:?}"))?,
    })
}

fn parse_uid(s: &str) -> Result<Uid, String> {
    if s == "root" {
        return Ok(Uid::ROOT);
    }
    Uid::parse(s).ok_or_else(|| format!("bad entity id {s:?}"))
}

fn uid_text(u: &Uid) -> String {
    if *u == Uid::ROOT {
        "root".into()
    } else {
        u.hex()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
enum FileMember {
    Ref(String),
    Text(String),
}

impl From<&Member> for FileMember {
    fn from(m: &Member) -> FileMember {
        match m {
            Member::Ref(u) => FileMember::Ref(u.hex()),
            Member::Text(s) => FileMember::Text(s.clone()),
        }
    }
}

impl TryFrom<FileMember> for Member {
    type Error = String;

    fn try_from(f: FileMember) -> Result<Member, String> {
        Ok(match f {
            FileMember::Ref(s) => Member::Ref(parse_uid(&s)?),
            FileMember::Text(s) => Member::Text(s),
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    side: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePlacement {
    parent: String,
    list: String,
    pos: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node: Option<FileNode>,
}

impl From<&Placement> for FilePlacement {
    fn from(p: &Placement) -> FilePlacement {
        FilePlacement {
            parent: uid_text(&p.list.parent),
            list: p.list.name.name().into(),
            pos: stamp_text(&p.pos),
            node: p.node.map(|n| FileNode {
                parent: n.parent.as_ref().map(stamp_text),
                side: n.side.name().into(),
            }),
        }
    }
}

impl TryFrom<FilePlacement> for Placement {
    type Error = String;

    fn try_from(f: FilePlacement) -> Result<Placement, String> {
        Ok(Placement {
            list: ListKey::new(
                parse_uid(&f.parent)?,
                ListName::parse(&f.list).ok_or_else(|| format!("bad list {:?}", f.list))?,
            ),
            pos: parse_stamp(&f.pos)?,
            node: match f.node {
                None => None,
                Some(n) => Some(Node {
                    parent: n.parent.as_deref().map(parse_stamp).transpose()?,
                    side: Side::parse(&n.side).ok_or_else(|| format!("bad side {:?}", n.side))?,
                }),
            },
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum FileOp {
    Create {
        entity: String,
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        place: Option<FilePlacement>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fields: BTreeMap<String, serde_json::Value>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        sets: BTreeMap<String, Vec<FileMember>>,
    },
    Set {
        entity: String,
        field: String,
        value: serde_json::Value,
    },
    Move {
        entity: String,
        place: FilePlacement,
    },
    Delete {
        entity: String,
    },
    Restore {
        entity: String,
    },
    Add {
        entity: String,
        field: String,
        member: FileMember,
    },
    Remove {
        entity: String,
        field: String,
        member: FileMember,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        seen: Vec<String>,
    },
}

fn parse_field(s: &str) -> Result<Field, String> {
    Field::parse(s).ok_or_else(|| format!("unknown field {s:?}"))
}

impl From<&Op> for FileOp {
    fn from(op: &Op) -> FileOp {
        match op {
            Op::Create {
                entity,
                kind,
                place,
                fields,
                sets,
            } => FileOp::Create {
                entity: uid_text(entity),
                kind: kind.name().into(),
                place: place.as_ref().map(FilePlacement::from),
                fields: fields
                    .iter()
                    .map(|(f, v)| (f.name().to_string(), value_to_json(v)))
                    .collect(),
                sets: sets
                    .iter()
                    .map(|(f, m)| {
                        (
                            f.name().to_string(),
                            m.iter().map(FileMember::from).collect(),
                        )
                    })
                    .collect(),
            },
            Op::Set {
                entity,
                field,
                value,
            } => FileOp::Set {
                entity: uid_text(entity),
                field: field.name().into(),
                value: value_to_json(value),
            },
            Op::Move { entity, place } => FileOp::Move {
                entity: uid_text(entity),
                place: FilePlacement::from(place),
            },
            Op::Delete { entity } => FileOp::Delete {
                entity: uid_text(entity),
            },
            Op::Restore { entity } => FileOp::Restore {
                entity: uid_text(entity),
            },
            Op::Add {
                entity,
                field,
                member,
            } => FileOp::Add {
                entity: uid_text(entity),
                field: field.name().into(),
                member: FileMember::from(member),
            },
            Op::Remove {
                entity,
                field,
                member,
                seen,
            } => FileOp::Remove {
                entity: uid_text(entity),
                field: field.name().into(),
                member: FileMember::from(member),
                seen: seen.iter().map(stamp_text).collect(),
            },
        }
    }
}

impl TryFrom<FileOp> for Op {
    type Error = String;

    fn try_from(f: FileOp) -> Result<Op, String> {
        Ok(match f {
            FileOp::Create {
                entity,
                kind,
                place,
                fields,
                sets,
            } => Op::Create {
                entity: parse_uid(&entity)?,
                kind: Kind::parse(&kind).ok_or_else(|| format!("unknown kind {kind:?}"))?,
                place: place.map(Placement::try_from).transpose()?,
                fields: fields
                    .into_iter()
                    .map(|(f, v)| Ok((parse_field(&f)?, value_from_json(v)?)))
                    .collect::<Result<_, String>>()?,
                sets: sets
                    .into_iter()
                    .map(|(f, m)| {
                        Ok((
                            parse_field(&f)?,
                            m.into_iter()
                                .map(Member::try_from)
                                .collect::<Result<_, String>>()?,
                        ))
                    })
                    .collect::<Result<_, String>>()?,
            },
            FileOp::Set {
                entity,
                field,
                value,
            } => Op::Set {
                entity: parse_uid(&entity)?,
                field: parse_field(&field)?,
                value: value_from_json(value)?,
            },
            FileOp::Move { entity, place } => Op::Move {
                entity: parse_uid(&entity)?,
                place: Placement::try_from(place)?,
            },
            FileOp::Delete { entity } => Op::Delete {
                entity: parse_uid(&entity)?,
            },
            FileOp::Restore { entity } => Op::Restore {
                entity: parse_uid(&entity)?,
            },
            FileOp::Add {
                entity,
                field,
                member,
            } => Op::Add {
                entity: parse_uid(&entity)?,
                field: parse_field(&field)?,
                member: Member::try_from(member)?,
            },
            FileOp::Remove {
                entity,
                field,
                member,
                seen,
            } => Op::Remove {
                entity: parse_uid(&entity)?,
                field: parse_field(&field)?,
                member: Member::try_from(member)?,
                seen: seen
                    .iter()
                    .map(|s| parse_stamp(s))
                    .collect::<Result<_, String>>()?,
            },
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileChange {
    actor: String,
    seq: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    deps: BTreeMap<String, u64>,
    lamport: u64,
    time: String,
    label: String,
    ops: Vec<FileOp>,
}

/// One compound value as the `.mthd` writes it, under a key naming its
/// type (`{"coding": {...}}`), so a reader never has to guess.
fn tagged<T: Serialize>(key: &str, v: T) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert(
        key.into(),
        serde_json::to_value(v).expect("file types always serialize"),
    );
    serde_json::Value::Object(m)
}

/// A register's value as JSON: scalars as JSON scalars, everything else
/// as a one-key object naming its type.
pub fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(n) => J::from(*n),
        Value::Text(s) => J::String(s.clone()),
        Value::Dec(d) => tagged("dec", d.to_string()),
        Value::Ref(u) => tagged("ref", u.hex()),
        Value::Formula(f) => tagged("formula", f),
        Value::Info(i) => tagged("info", FileInfo::from(i)),
        Value::Coding(c) => tagged("coding", FileCoding::from(c)),
        Value::Posting(p) => tagged("posting", FilePosting::from(p)),
        Value::FringeRow(r) => tagged("fringe_row", FileFringeRow::from(r)),
        Value::NewLineDefaults(n) => tagged("new_line_defaults", FileNewLineDefaults::from(n)),
        Value::GroupRules(g) => tagged("group_rules", FileGroupRules::from(*g)),
        Value::Display(d) => tagged("display", FileDisplay::from(d)),
        Value::Baseline(b) => tagged("baseline", FileBaseline::from(b)),
        Value::Snapshot(s) => tagged("snapshot", FileSnapshot(FileBudget::from(&s.to_budget()))),
        Value::ReportPresets(p) => tagged(
            "report_presets",
            p.iter().map(FileReportPreset::from).collect::<Vec<_>>(),
        ),
        Value::TagPresets(p) => tagged(
            "tag_presets",
            p.iter().map(FileTagPreset::from).collect::<Vec<_>>(),
        ),
        Value::LinePresets(p) => tagged(
            "line_presets",
            p.iter().map(FileLinePreset::from).collect::<Vec<_>>(),
        ),
        Value::OtPatterns(p) => tagged(
            "ot_patterns",
            p.iter().cloned().map(FileOtPattern).collect::<Vec<_>>(),
        ),
        Value::SetupOrder(o) => tagged("setup_order", FileSetupOrder::from(o.clone())),
        Value::SetupFolders(f) => tagged("setup_folders", &f.0),
        Value::SetupSources(s) => tagged(
            "setup_sources",
            s.0.iter()
                .map(|(k, m)| {
                    (
                        k.clone(),
                        m.iter()
                            .map(|(n, src)| (n.clone(), FileItemSource::from(src)))
                            .collect::<BTreeMap<_, _>>(),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

fn from_json<T: serde::de::DeserializeOwned>(key: &str, v: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("bad {key} value: {e}"))
}

/// The register value a JSON value stands for.
pub fn value_from_json(v: serde_json::Value) -> Result<Value, String> {
    use serde_json::Value as J;
    match v {
        J::Null => Ok(Value::Null),
        J::Bool(b) => Ok(Value::Bool(b)),
        J::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .ok_or_else(|| format!("bad integer {n}")),
        J::String(s) => Ok(Value::Text(s)),
        J::Array(_) => Err("a value cannot be an array".into()),
        J::Object(m) => {
            if m.len() != 1 {
                return Err("a compound value has exactly one key".into());
            }
            let (key, v) = m.into_iter().next().expect("one entry");
            Ok(match key.as_str() {
                "dec" => Value::Dec(
                    from_json::<String>(&key, v)?
                        .parse()
                        .map_err(|e| format!("bad decimal: {e}"))?,
                ),
                "ref" => Value::Ref(parse_uid(&from_json::<String>(&key, v)?)?),
                "formula" => Value::Formula(from_json(&key, v)?),
                "info" => Value::Info(from_json::<FileInfo>(&key, v)?.into()),
                "coding" => Value::Coding(from_json::<FileCoding>(&key, v)?.into()),
                "posting" => Value::Posting(from_json::<FilePosting>(&key, v)?.into()),
                "fringe_row" => Value::FringeRow(from_json::<FileFringeRow>(&key, v)?.into()),
                "new_line_defaults" => {
                    Value::NewLineDefaults(from_json::<FileNewLineDefaults>(&key, v)?.into())
                }
                "group_rules" => Value::GroupRules(from_json::<FileGroupRules>(&key, v)?.into()),
                "display" => Value::Display(from_json::<FileDisplay>(&key, v)?.into()),
                "baseline" => Value::Baseline(from_json::<FileBaseline>(&key, v)?.into()),
                "snapshot" => {
                    let b: Budget = from_json::<FileSnapshot>(&key, v)?.0.into();
                    Value::Snapshot(Arc::new(b.snapshot()))
                }
                "report_presets" => Value::ReportPresets(
                    from_json::<Vec<FileReportPreset>>(&key, v)?
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ),
                "tag_presets" => Value::TagPresets(
                    from_json::<Vec<FileTagPreset>>(&key, v)?
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ),
                "line_presets" => Value::LinePresets(
                    from_json::<Vec<FileLinePreset>>(&key, v)?
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ),
                "ot_patterns" => Value::OtPatterns(
                    from_json::<Vec<FileOtPattern>>(&key, v)?
                        .into_iter()
                        .map(|p| p.0)
                        .collect(),
                ),
                "setup_order" => Value::SetupOrder(from_json::<FileSetupOrder>(&key, v)?.into()),
                "setup_folders" => {
                    Value::SetupFolders(method_core::folders::SetupFolders(from_json(&key, v)?))
                }
                "setup_sources" => {
                    let m: BTreeMap<String, BTreeMap<String, FileItemSource>> = from_json(&key, v)?;
                    Value::SetupSources(method_core::pack::SetupSources(
                        m.into_iter()
                            .map(|(k, inner)| {
                                (k, inner.into_iter().map(|(n, s)| (n, s.into())).collect())
                            })
                            .collect(),
                    ))
                }
                other => return Err(format!("unknown value type {other:?}")),
            })
        }
    }
}

/// One change as one line of JSON (no newline), in canonical form: the
/// same change always gives the same bytes.
pub fn change_to_json(c: &Change) -> String {
    let f = FileChange {
        actor: c.actor.hex(),
        seq: c.seq,
        deps: c.deps.0.iter().map(|(a, s)| (a.hex(), *s)).collect(),
        lamport: c.lamport,
        time: c.time.clone(),
        label: c.label.clone(),
        ops: c.ops.iter().map(FileOp::from).collect(),
    };
    serde_json::to_string(&f).expect("a change always serializes")
}

/// Reads one change from its line of JSON.
pub fn change_from_json(line: &str) -> Result<Change, String> {
    let f: FileChange = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let mut deps = Frontier::new();
    for (a, s) in f.deps {
        deps.advance(
            ActorId::parse(&a).ok_or_else(|| format!("bad actor {a:?}"))?,
            s,
        );
    }
    Ok(Change {
        actor: ActorId::parse(&f.actor).ok_or_else(|| format!("bad actor {:?}", f.actor))?,
        seq: f.seq,
        deps,
        lamport: f.lamport,
        time: f.time,
        label: f.label,
        ops: f
            .ops
            .into_iter()
            .map(Op::try_from)
            .collect::<Result<_, String>>()?,
    })
}

/// The budget id: the hash of the genesis change's canonical line.
pub fn budget_id(genesis: &Change) -> String {
    sha256_hex(change_to_json(genesis).as_bytes())
}

// ---------------------------------------------------------------------------
// Segment files

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentHeader {
    format: u64,
    actor: String,
    from: u64,
    to: u64,
    hash: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentLine {
    segment: SegmentHeader,
}

/// A segment file's bytes: a header line, then one change per line. The
/// header's hash covers every byte after it, so a half-copied file is
/// told from a whole one.
pub fn segment_bytes(changes: &[Change]) -> Result<Vec<u8>, String> {
    let first = changes
        .first()
        .ok_or("a segment holds at least one change")?;
    let actor = first.actor;
    let mut body = String::new();
    for (i, c) in changes.iter().enumerate() {
        if c.actor != actor {
            return Err("a segment holds one actor's changes".into());
        }
        if c.seq != first.seq + i as u64 {
            return Err("a segment's changes run in sequence".into());
        }
        body.push_str(&change_to_json(c));
        body.push('\n');
    }
    let header = SegmentLine {
        segment: SegmentHeader {
            format: LOG_FORMAT_VERSION,
            actor: actor.hex(),
            from: first.seq,
            to: first.seq + changes.len() as u64 - 1,
            hash: sha256_hex(body.as_bytes()),
        },
    };
    let mut out = serde_json::to_string(&header).expect("a header always serializes");
    out.push('\n');
    out.push_str(&body);
    Ok(out.into_bytes())
}

/// A segment's changes, after checking its hash and sequence. A
/// truncated or altered file is an error (the caller leaves it until it
/// is whole).
pub fn read_segment(bytes: &[u8]) -> Result<Vec<Change>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?;
    let (head, body) = text
        .split_once('\n')
        .ok_or_else(|| "no header line".to_string())?;
    let header: SegmentLine = serde_json::from_str(head).map_err(|e| format!("header: {e}"))?;
    let h = header.segment;
    if h.format > LOG_FORMAT_VERSION {
        return Err(format!(
            "log format {} is newer than this program reads ({LOG_FORMAT_VERSION})",
            h.format
        ));
    }
    if sha256_hex(body.as_bytes()) != h.hash {
        return Err("hash mismatch: the file is incomplete or altered".into());
    }
    let actor = ActorId::parse(&h.actor).ok_or_else(|| format!("bad actor {:?}", h.actor))?;
    let mut out = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let c = change_from_json(line).map_err(|e| format!("line {}: {e}", i + 2))?;
        if c.actor != actor || c.seq != h.from + out.len() as u64 {
            return Err(format!("line {}: out of sequence", i + 2));
        }
        out.push(c);
    }
    if out.len() as u64 != h.to.saturating_sub(h.from) + 1 {
        return Err("the header's range doesn't match the changes".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The log folder

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGenesis {
    actor: String,
    seq: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileBudgetInfo {
    format: String,
    version: u64,
    budget_id: String,
    file: String,
    genesis: FileGenesis,
}

/// What `budget.json` says: which budget the folder belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetInfo {
    /// The hash of the genesis change.
    pub budget_id: String,
    /// The `.mthd` file's name (never a path).
    pub file: String,
    pub genesis: ChangeId,
}

/// One segment file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub path: PathBuf,
    pub actor: ActorId,
    pub from: u64,
    pub to: u64,
}

/// A snapshot of the folder's segment files (path, size, modification
/// time), for noticing new ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan(BTreeMap<PathBuf, (u64, Option<std::time::SystemTime>)>);

/// What loading or absorbing did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub applied: usize,
    pub duplicates: usize,
    /// Changes still waiting for their dependencies.
    pub pending: usize,
    /// Changes refused, with why.
    pub rejected: Vec<(ChangeId, String)>,
    /// Files skipped (not whole yet, or malformed), with why.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Writes `bytes` to `path` through a temporary file in the same folder
/// (named with the `.~` prefix sync services skip) and a rename.
pub fn write_sealed(path: &Path, bytes: &[u8]) -> Result<(), LogFileError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| LogFileError(format!("{}: not a file name", path.display())))?;
    let tmp = dir.join(format!(".~{name}.{}", std::process::id()));
    let mut f = fs::File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
    f.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
    f.sync_all().map_err(|e| io_err(&tmp, e))?;
    drop(f);
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(path, e)
    })
}

/// The `Feature.mthd-log/` folder beside a budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogDir {
    root: PathBuf,
}

impl LogDir {
    /// The log folder of the budget at `mthd`.
    pub fn for_budget(mthd: &Path) -> LogDir {
        let name = mthd
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("budget.mthd");
        LogDir {
            root: mthd.with_file_name(format!("{name}{LOG_DIR_SUFFIX}")),
        }
    }

    /// A log folder at `root`.
    pub fn at(root: &Path) -> LogDir {
        LogDir {
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the folder exists with a `budget.json`.
    pub fn exists(&self) -> bool {
        self.root.join("budget.json").is_file()
    }

    fn actors_dir(&self) -> PathBuf {
        self.root.join("actors")
    }

    fn actor_dir(&self, actor: ActorId) -> PathBuf {
        self.actors_dir().join(actor.hex())
    }

    /// Creates the folder for a log whose first change is `genesis`,
    /// writing `budget.json` and the first segment.
    pub fn create(&self, genesis: &Change, file: &str) -> Result<BudgetInfo, LogFileError> {
        if self.exists() {
            return Err(LogFileError(format!(
                "{} already exists",
                self.root.display()
            )));
        }
        fs::create_dir_all(self.actors_dir()).map_err(|e| io_err(&self.root, e))?;
        let info = FileBudgetInfo {
            format: LOG_FORMAT_NAME.into(),
            version: LOG_FORMAT_VERSION,
            budget_id: budget_id(genesis),
            file: file.into(),
            genesis: FileGenesis {
                actor: genesis.actor.hex(),
                seq: genesis.seq,
            },
        };
        let mut text = serde_json::to_string_pretty(&info).expect("info serializes");
        text.push('\n');
        write_sealed(&self.root.join("budget.json"), text.as_bytes())?;
        self.write_segment(std::slice::from_ref(genesis))?;
        self.info()
    }

    /// Reads `budget.json`.
    pub fn info(&self) -> Result<BudgetInfo, LogFileError> {
        let path = self.root.join("budget.json");
        let text = fs::read_to_string(&path).map_err(|e| io_err(&path, e))?;
        let f: FileBudgetInfo = serde_json::from_str(&text)
            .map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
        if f.format != LOG_FORMAT_NAME {
            return Err(LogFileError(format!(
                "{}: not a change log (format {:?})",
                path.display(),
                f.format
            )));
        }
        if f.version > LOG_FORMAT_VERSION {
            return Err(LogFileError(format!(
                "{}: log format {} is newer than this program reads ({LOG_FORMAT_VERSION})",
                path.display(),
                f.version
            )));
        }
        Ok(BudgetInfo {
            budget_id: f.budget_id,
            file: f.file,
            genesis: ChangeId {
                actor: ActorId::parse(&f.genesis.actor).ok_or_else(|| {
                    LogFileError(format!("{}: bad genesis actor", path.display()))
                })?,
                seq: f.genesis.seq,
            },
        })
    }

    /// Every actor with a folder.
    pub fn actors(&self) -> Vec<ActorId> {
        let Ok(entries) = fs::read_dir(self.actors_dir()) else {
            return Vec::new();
        };
        let mut out: Vec<ActorId> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(ActorId::parse))
            .collect();
        out.sort();
        out
    }

    /// The segment files of one actor, in sequence order.
    pub fn segments(&self, actor: ActorId) -> Vec<Segment> {
        let Ok(entries) = fs::read_dir(self.actor_dir(actor)) else {
            return Vec::new();
        };
        let mut out: Vec<Segment> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let stem = path.file_stem()?.to_str()?;
                if path.extension()?.to_str()? != SEGMENT_EXTENSION {
                    return None;
                }
                let (from, to) = stem.split_once('-')?;
                Some(Segment {
                    actor,
                    from: from.parse().ok()?,
                    to: to.parse().ok()?,
                    path,
                })
            })
            .collect();
        out.sort_by_key(|s| (s.from, s.to));
        out
    }

    /// Every segment of every actor.
    pub fn all_segments(&self) -> Vec<Segment> {
        self.actors()
            .into_iter()
            .flat_map(|a| self.segments(a))
            .collect()
    }

    /// The highest seq of `actor` written to a segment (0 for none).
    pub fn highest_written(&self, actor: ActorId) -> u64 {
        self.segments(actor).iter().map(|s| s.to).max().unwrap_or(0)
    }

    /// Writes one sealed segment of `changes` (one actor, in sequence)
    /// into that actor's folder and returns its path.
    pub fn write_segment(&self, changes: &[Change]) -> Result<PathBuf, LogFileError> {
        let bytes = segment_bytes(changes)?;
        let first = &changes[0];
        let dir = self.actor_dir(first.actor);
        fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let path = dir.join(format!(
            "{:06}-{:06}.{SEGMENT_EXTENSION}",
            first.seq,
            first.seq + changes.len() as u64 - 1
        ));
        write_sealed(&path, &bytes)?;
        Ok(path)
    }

    /// Reads one segment file.
    pub fn read(&self, segment: &Segment) -> Result<Vec<Change>, LogFileError> {
        let bytes = fs::read(&segment.path).map_err(|e| io_err(&segment.path, e))?;
        read_segment(&bytes).map_err(|e| LogFileError(format!("{}: {e}", segment.path.display())))
    }

    /// Rolls `actor`'s segments into one file (written before the old
    /// ones are removed). Returns the new path, or `None` when there was
    /// nothing to roll.
    pub fn roll(&self, actor: ActorId) -> Result<Option<PathBuf>, LogFileError> {
        let segments = self.segments(actor);
        if segments.len() < 2 {
            return Ok(None);
        }
        let mut changes = Vec::new();
        for s in &segments {
            changes.extend(self.read(s)?);
        }
        // Only a gap-free run can be one segment.
        if changes.windows(2).any(|w| w[1].seq != w[0].seq + 1) {
            return Err(LogFileError(format!(
                "actor {} has a gap in its segments",
                actor.short()
            )));
        }
        let path = self.write_segment(&changes)?;
        for s in segments {
            if s.path != path {
                fs::remove_file(&s.path).map_err(|e| io_err(&s.path, e))?;
            }
        }
        Ok(Some(path))
    }

    /// What the folder's segment files look like now.
    pub fn scan(&self) -> Scan {
        let mut out = BTreeMap::new();
        for s in self.all_segments() {
            let meta = fs::metadata(&s.path).ok();
            out.insert(
                s.path,
                (
                    meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    meta.and_then(|m| m.modified().ok()),
                ),
            );
        }
        Scan(out)
    }

    /// Segments that are new or changed since `since`.
    pub fn changed_since(&self, since: &Scan) -> Vec<Segment> {
        let now = self.scan();
        self.all_segments()
            .into_iter()
            .filter(|s| now.0.get(&s.path) != since.0.get(&s.path))
            .collect()
    }
}

/// Applies the changes of `segments` to `log`, in any order (the log
/// holds out-of-order ones until their dependencies arrive).
fn apply_segments(log: &mut Log, dir: &LogDir, segments: &[Segment]) -> LoadReport {
    let mut report = LoadReport::default();
    let mut changes = Vec::new();
    for s in segments {
        match dir.read(s) {
            Ok(cs) => changes.extend(cs),
            Err(e) => report.skipped.push((s.path.clone(), e.0)),
        }
    }
    changes.sort_by_key(|c| (c.actor, c.seq));
    for c in changes {
        let id = c.id();
        match log.apply(c) {
            Ok(method_core::log::Applied::Applied { unblocked }) => report.applied += 1 + unblocked,
            Ok(method_core::log::Applied::Duplicate) => report.duplicates += 1,
            Ok(method_core::log::Applied::Pending) => {}
            Err(e) => report.rejected.push((id, e.to_string())),
        }
    }
    report.pending = log.pending().len();
    report
}

/// Reads every whole segment in the folder into `log`.
pub fn load(log: &mut Log, dir: &LogDir) -> LoadReport {
    apply_segments(log, dir, &dir.all_segments())
}

/// Reads the segments that are new since `scan` into `log` and moves
/// `scan` forward: the hook a poller or a file watcher calls.
pub fn absorb(log: &mut Log, dir: &LogDir, scan: &mut Scan) -> LoadReport {
    let changed = dir.changed_since(scan);
    let report = apply_segments(log, dir, &changed);
    // A skipped file (half synced) is left out of the scan, so it is
    // read again next time.
    let mut now = dir.scan();
    for (path, _) in &report.skipped {
        now.0.remove(path);
    }
    *scan = now;
    report
}

/// Writes this actor's changes that aren't in a segment yet. Returns the
/// new segment's path, or `None` when everything was already written.
pub fn flush(log: &Log, dir: &LogDir) -> Result<Option<PathBuf>, LogFileError> {
    let written = dir.highest_written(log.actor());
    let own: Vec<Change> = log.own_after(written).into_iter().cloned().collect();
    if own.is_empty() {
        return Ok(None);
    }
    dir.write_segment(&own).map(Some)
}

/// A budget opened for sharing: its log, its folder and the path of its
/// `.mthd`.
pub struct Shared {
    pub log: Log,
    pub dir: LogDir,
    pub path: PathBuf,
    pub info: BudgetInfo,
}

/// Starts sharing the budget at `path`: adopts it as `actor` (the genesis
/// change) and creates the log folder beside it.
pub fn init(path: &Path, actor: ActorId, ids: Ids, time: &str) -> Result<Shared, LogFileError> {
    let dir = LogDir::for_budget(path);
    if dir.exists() {
        return Err(LogFileError(format!(
            "{} already has a change log",
            path.display()
        )));
    }
    let text = fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    let budget =
        crate::from_str(&text).map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("budget.mthd")
        .to_string();
    let log = Log::adopt(actor, ids, &budget, &format!("Adopted {file}"), time);
    let genesis = log
        .changes()
        .next()
        .expect("adopt makes one change")
        .clone();
    let info = dir.create(&genesis, &file)?;
    Ok(Shared {
        log,
        dir,
        path: path.to_path_buf(),
        info,
    })
}

/// Opens the shared budget at `path`: reads every actor's segments.
pub fn open(path: &Path, actor: ActorId, ids: Ids) -> Result<(Shared, LoadReport), LogFileError> {
    let dir = LogDir::for_budget(path);
    let info = dir.info()?;
    let mut log = Log::new(actor, ids);
    let report = load(&mut log, &dir);
    if log.is_empty() {
        return Err(LogFileError(format!(
            "{}: no whole segment could be read",
            dir.root().display()
        )));
    }
    Ok((
        Shared {
            log,
            dir,
            path: path.to_path_buf(),
            info,
        },
        report,
    ))
}

// ---------------------------------------------------------------------------
// Presence (docs/research/collaboration.md, section 5): each actor rewrites
// its own `presence.json` every few seconds while the budget is open.

/// The name of the heartbeat file in an actor's folder.
pub const PRESENCE_FILE: &str = "presence.json";

/// One actor's heartbeat: who they are, where they are in the budget and
/// when they last wrote it. Never stored in the log.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presence {
    /// The person's display name, as they typed it.
    pub name: String,
    /// A color name from the application's palette ("teal").
    pub color: String,
    /// The category they are in (its code), if any.
    pub category: Option<String>,
    /// The account they are in (its code), if any.
    pub account: Option<String>,
    /// When this was written, ISO 8601 UTC.
    pub at: String,
    /// The frontier digest of their log at that moment.
    pub frontier: String,
}

impl LogDir {
    /// Writes `actor`'s heartbeat (only that actor's own application
    /// does), through a temporary file and a rename like a segment.
    pub fn write_presence(&self, actor: ActorId, p: &Presence) -> Result<(), LogFileError> {
        let dir = self.actor_dir(actor);
        fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let mut text = serde_json::to_string_pretty(p).expect("presence serializes");
        text.push('\n');
        write_sealed(&dir.join(PRESENCE_FILE), text.as_bytes())
    }

    /// `actor`'s heartbeat, if it has written one that reads.
    pub fn read_presence(&self, actor: ActorId) -> Option<Presence> {
        let text = fs::read_to_string(self.actor_dir(actor).join(PRESENCE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Every actor's heartbeat that reads.
    pub fn presences(&self) -> BTreeMap<ActorId, Presence> {
        self.actors()
            .into_iter()
            .filter_map(|a| self.read_presence(a).map(|p| (a, p)))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// This computer's actor id

/// Where this computer's actor id lives: `$METHOD_ACTOR_FILE`, or
/// `~/.method/actor`. `None` without a home folder.
pub fn actor_id_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("METHOD_ACTOR_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(Path::new(&home).join(".method").join("actor"))
}

/// The actor id kept in `path` (64 hex characters), made with `ids` the
/// first time and kept readable by its owner only. The command line and
/// the application share it, so one computer is one actor.
pub fn load_or_make_actor(path: &Path, ids: &mut Ids) -> Result<ActorId, LogFileError> {
    if let Ok(text) = fs::read_to_string(path) {
        return ActorId::parse(&text)
            .ok_or_else(|| LogFileError(format!("{}: not an actor id", path.display())));
    }
    let actor = ActorId::random(ids);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    }
    fs::write(path, format!("{}\n", actor.hex())).map_err(|e| io_err(path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(actor)
}

/// Records an outside edit to the `.mthd` (someone saved it with a
/// program that doesn't write the log; docs/research/collaboration.md,
/// 4.4): the difference between the file and the merged state becomes a
/// change by this actor. Returns the change, or `None` when the file
/// matches.
pub fn ingest(shared: &mut Shared, time: &str) -> Result<Option<ChangeId>, LogFileError> {
    let text = fs::read_to_string(&shared.path).map_err(|e| io_err(&shared.path, e))?;
    let on_disk = crate::from_str(&text)
        .map_err(|e| LogFileError(format!("{}: {e}", shared.path.display())))?;
    let proj = shared.log.project();
    let label = format!("Outside edit to {}", shared.info.file);
    shared
        .log
        .record(&proj, &on_disk, &[], &label, time)
        .map_err(|m: Malformed| LogFileError(format!("could not record the outside edit: {m}")))
}

/// Writes the merged state as the `.mthd` (the keeper's job) and returns
/// the projection it wrote. The file's provenance, which the log doesn't
/// carry, is kept.
pub fn write_snapshot(shared: &Shared) -> Result<Projection, LogFileError> {
    let proj = shared.log.project();
    let mut budget = proj.budget.clone();
    if let Ok(text) = fs::read_to_string(&shared.path) {
        if let Ok(on_disk) = crate::from_str(&text) {
            budget.provenance = on_disk.provenance;
        }
    }
    write_sealed(&shared.path, crate::to_string(&budget).as_bytes())?;
    Ok(proj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(n: u8) -> ActorId {
        let mut a = [0u8; 32];
        a[0] = n;
        ActorId(a)
    }

    #[test]
    fn changes_round_trip_through_json() {
        let b = method_core::sample::budget(&method_core::sample::SampleSpec {
            rows: 60,
            accounts: 6,
            categories: 2,
            fringes: 4,
            groups: 3,
            ..Default::default()
        });
        let log = Log::adopt(
            actor(1),
            Ids::seeded(1),
            &b,
            "Adopted",
            "2026-10-12T00:00:00Z",
        );
        let genesis = log.changes().next().unwrap();
        let line = change_to_json(genesis);
        assert!(!line.contains('\n'));
        let back = change_from_json(&line).unwrap();
        assert_eq!(&back, genesis);
        assert_eq!(change_to_json(&back), line);
        // The budget id is stable.
        assert_eq!(budget_id(genesis), budget_id(&back));
        assert_eq!(budget_id(genesis).len(), 64);
    }

    #[test]
    fn every_value_type_round_trips() {
        let b = crate::from_str(&crate::to_string(&method_core::sample::budget(
            &method_core::sample::SampleSpec {
                rows: 20,
                accounts: 4,
                categories: 2,
                fringes: 3,
                groups: 2,
                ..Default::default()
            },
        )))
        .unwrap();
        let snapshot = Arc::new(b.snapshot());
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(-5),
            Value::text("x"),
            Value::Dec("1.5".parse().unwrap()),
            Value::Ref(Uid(7)),
            Value::Formula("{g:00000000000000000000000000000007} * 2".into()),
            Value::Info(method_core::InfoValue::Date("2026-01-01".into())),
            Value::Coding(b.account_coding),
            Value::Posting(b.fringe_posting.clone()),
            Value::FringeRow(b.fringe_row.clone()),
            Value::NewLineDefaults(b.new_line_defaults),
            Value::GroupRules(b.group_rules),
            Value::Display(b.display.clone()),
            Value::Baseline(b.baseline.clone().unwrap()),
            Value::Snapshot(snapshot),
            Value::ReportPresets(b.report_presets.clone()),
            Value::TagPresets(b.tag_presets.clone()),
            Value::LinePresets(b.line_presets.clone()),
            Value::OtPatterns(b.ot_patterns.clone()),
            Value::SetupOrder(b.setup_order.clone()),
            Value::SetupFolders(b.setup_folders.clone()),
            Value::SetupSources(b.setup_sources.clone()),
        ];
        for v in values {
            let j = value_to_json(&v);
            let back = value_from_json(j.clone()).unwrap_or_else(|e| panic!("{j}: {e}"));
            assert_eq!(back, v, "{j}");
        }
        assert!(value_from_json(serde_json::json!({"nope": 1})).is_err());
        assert!(value_from_json(serde_json::json!([1])).is_err());
        assert!(value_from_json(serde_json::json!(1.5)).is_err());
    }

    #[test]
    fn segments_check_their_hash() {
        let b = method_core::Budget::new("T", "USD");
        let mut log = Log::adopt(actor(1), Ids::seeded(1), &b, "Adopted", "t");
        for i in 0..3 {
            let p = log.project();
            let mut after = p.budget.clone();
            after.title = format!("T{i}");
            log.record(&p, &after, &[], "Title", "t").unwrap();
        }
        let changes: Vec<Change> = log.changes().cloned().collect();
        let bytes = segment_bytes(&changes).unwrap();
        assert_eq!(read_segment(&bytes).unwrap(), changes);
        // Truncated: refused.
        assert!(read_segment(&bytes[..bytes.len() - 10]).is_err());
        // Altered: refused.
        let mut altered = bytes.clone();
        let at = altered.len() - 5;
        altered[at] ^= 1;
        assert!(read_segment(&altered).is_err());
        // Not in sequence: refused at write.
        assert!(segment_bytes(&[changes[0].clone(), changes[2].clone()]).is_err());
    }
}
