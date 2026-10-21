//! The change log on disk (docs/FORMAT.md, "The change log";
//! docs/research/collaboration.md, section 4): `Feature.mthd-log/` beside
//! the budget, one folder per actor, sealed segment files of JSON lines
//! that only their actor ever writes, and the keeper's snapshot of the
//! merged state written as the `.mthd` itself.
//!
//! Nothing here watches or polls on its own: [`LogDir::scan`] and
//! [`absorb`] are the hooks an application or `method-budget sync` drives.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use method_core::log::hash::sha256_hex;
use method_core::log::{
    ActorId, Binding, Change, ChangeId, Field, Frontier, Ids, Kind, ListKey, ListName, Log,
    LogError, Member, Node, Op, Placement, Projection, PublicKey, Role, Side, Signature, Signer,
    Stamp, Uid, Value,
};

pub mod sign;
use method_core::Budget;
use serde::{Deserialize, Serialize};

use crate::dto::{
    FileBaseline, FileBudget, FileCoding, FileDisplay, FileFringeRow, FileGroupRules, FileInfo,
    FileItemSource, FileLinePreset, FileNewLineDefaults, FileOtPattern, FilePosting,
    FileRateSource, FileRateTable, FileReportPreset, FileSetupOrder, FileSharedCap, FileSnapshot,
    FileTagPreset,
};
use crate::fsio::{read_capped, read_to_string_capped, MAX_READ_BYTES};

/// The value of `format` in `budget.json`.
pub const LOG_FORMAT_NAME: &str = "method-budget-log";
/// The highest log format version this build reads.
pub const LOG_FORMAT_VERSION: u64 = 4;
/// The version a log is written at until someone is given a role.
pub const LOG_FORMAT_BASE: u64 = 1;
/// The version from the first role on (decisions.md, entry 78): `budget.json`
/// and every segment holding a `grant` say 2, so a program that reads only 1
/// refuses the folder as newer than it reads rather than skipping the roles.
pub const LOG_FORMAT_ROLES: u64 = 2;
/// The version from the owner's key on (decisions.md, entry 89):
/// `budget.json` and every segment holding a `key`, a `revoke` or a signed
/// change say 3, so a program that can't check signatures refuses the
/// folder as newer than it reads rather than taking in unsigned changes.
pub const LOG_FORMAT_SIGNED: u64 = 3;
/// The version from the first invite on (decisions.md, entry 97):
/// `budget.json` and every segment holding an `invite`, a `claim` or a
/// `cancel_invite` say 4, so a program that reads only 3 (Method Budget
/// 0.7.0 and older) refuses the folder as newer than it reads rather than
/// failing on an op it doesn't know.
pub const LOG_FORMAT_INVITES: u64 = 4;

/// The format a segment holding `changes` is written at.
fn format_for(changes: &[Change]) -> u64 {
    if changes.iter().any(|c| {
        c.ops.iter().any(|op| {
            matches!(
                op,
                Op::Invite { .. } | Op::Claim { .. } | Op::CancelInvite { .. }
            )
        })
    }) {
        return LOG_FORMAT_INVITES;
    }
    if changes.iter().any(|c| {
        c.sig.is_some()
            || c.ops
                .iter()
                .any(|op| matches!(op, Op::Key { .. } | Op::Revoke { .. }))
    }) {
        return LOG_FORMAT_SIGNED;
    }
    if changes.iter().any(|c| {
        c.ops
            .iter()
            .any(|op| matches!(op, Op::Grant { .. } | Op::ActsFor { .. }))
    }) {
        LOG_FORMAT_ROLES
    } else {
        LOG_FORMAT_BASE
    }
}
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

/// Reads a whole text file from the folder, which collaborators write: at
/// most [`MAX_READ_BYTES`], so a huge file is refused, not read into memory.
fn read_text(path: impl AsRef<Path>) -> std::io::Result<String> {
    read_to_string_capped(path.as_ref(), MAX_READ_BYTES)
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
    /// A person's role (decisions.md, entry 78): `scope` lists a
    /// department editor's categories and accounts.
    Grant {
        actor: String,
        role: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scope: Vec<String>,
    },
    /// The change's actor writes for `person` (a command line or agent).
    #[serde(rename = "acts_for")]
    ActsFor {
        person: String,
    },
    /// The change's actor's public key (decisions.md, entry 89).
    Key {
        key: String,
    },
    /// A person removed, with a cutoff at the change's own `deps`.
    Revoke {
        actor: String,
    },
    /// An invite (decisions.md, entry 97): the role it gives (`scope` as
    /// a grant's), when it expires (RFC 3339), and the public key whose
    /// secret is in the link.
    Invite {
        id: String,
        role: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        scope: Vec<String>,
        expires: String,
        key: String,
    },
    /// An invite taken up: the invite key's signature over the invite and
    /// the change's actor.
    Claim {
        invite: String,
        proof: String,
    },
    /// An invite cancelled.
    #[serde(rename = "cancel_invite")]
    CancelInvite {
        invite: String,
    },
}

/// A role and its scope as a `grant` or an `invite` writes them.
fn role_scope(role: &Role) -> (String, Vec<String>) {
    (
        role.key().into(),
        role.scope()
            .map(|s| s.iter().map(Uid::hex).collect())
            .unwrap_or_default(),
    )
}

/// A role read back from a `grant` or an `invite`.
fn parse_role(role: &str, scope: &[String]) -> Result<Role, String> {
    let scope = scope
        .iter()
        .map(|u| parse_uid(u))
        .collect::<Result<_, String>>()?;
    Role::parse(role, scope).ok_or_else(|| {
        if Role::parse(role, Default::default()).is_some() {
            format!("a scope on role {role:?}, which takes none")
        } else {
            format!("unknown role {role:?}")
        }
    })
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
            Op::ActsFor { person } => FileOp::ActsFor {
                person: person.hex(),
            },
            Op::Key { key } => FileOp::Key { key: key.hex() },
            Op::Revoke { actor } => FileOp::Revoke { actor: actor.hex() },
            Op::Grant { actor, role } => {
                let (role, scope) = role_scope(role);
                FileOp::Grant {
                    actor: actor.hex(),
                    role,
                    scope,
                }
            }
            Op::Invite {
                id,
                role,
                expires,
                key,
            } => {
                let (role, scope) = role_scope(role);
                FileOp::Invite {
                    id: uid_text(id),
                    role,
                    scope,
                    expires: expires.clone(),
                    key: key.hex(),
                }
            }
            Op::Claim { invite, proof } => FileOp::Claim {
                invite: uid_text(invite),
                proof: proof.hex(),
            },
            Op::CancelInvite { invite } => FileOp::CancelInvite {
                invite: uid_text(invite),
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
            FileOp::ActsFor { person } => Op::ActsFor {
                person: ActorId::parse(&person).ok_or_else(|| format!("bad actor {person:?}"))?,
            },
            FileOp::Key { key } => Op::Key {
                key: PublicKey::parse(&key).ok_or_else(|| format!("bad key {key:?}"))?,
            },
            FileOp::Revoke { actor } => Op::Revoke {
                actor: ActorId::parse(&actor).ok_or_else(|| format!("bad actor {actor:?}"))?,
            },
            FileOp::Grant { actor, role, scope } => Op::Grant {
                actor: ActorId::parse(&actor).ok_or_else(|| format!("bad actor {actor:?}"))?,
                role: parse_role(&role, &scope)?,
            },
            FileOp::Invite {
                id,
                role,
                scope,
                expires,
                key,
            } => Op::Invite {
                id: parse_uid(&id)?,
                role: parse_role(&role, &scope)?,
                expires,
                key: PublicKey::parse(&key).ok_or_else(|| format!("bad key {key:?}"))?,
            },
            FileOp::Claim { invite, proof } => Op::Claim {
                invite: parse_uid(&invite)?,
                proof: Signature::parse(&proof).ok_or_else(|| format!("bad proof {proof:?}"))?,
            },
            FileOp::CancelInvite { invite } => Op::CancelInvite {
                invite: parse_uid(&invite)?,
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
    /// The author's Ed25519 signature over the line without this field
    /// (decisions.md, entry 89), 128 hex characters; last, so the signed
    /// bytes are the line up to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sig: Option<String>,
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
        Value::RateTables(t) => tagged(
            "rate_tables",
            t.iter().cloned().map(FileRateTable).collect::<Vec<_>>(),
        ),
        Value::RateSource(s) => tagged("rate_source", FileRateSource(s.clone())),
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
        Value::SharedCaps(m) => tagged(
            "shared_caps",
            m.iter()
                .map(|(n, a)| (n.clone(), FileSharedCap { amount: *a }))
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
                "rate_tables" => Value::RateTables(
                    from_json::<Vec<FileRateTable>>(&key, v)?
                        .into_iter()
                        .map(|t| t.0)
                        .collect(),
                ),
                "rate_source" => Value::RateSource(from_json::<FileRateSource>(&key, v)?.0),
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
                "shared_caps" => {
                    let m: BTreeMap<String, FileSharedCap> = from_json(&key, v)?;
                    Value::SharedCaps(m.into_iter().map(|(n, c)| (n, c.amount)).collect())
                }
                other => return Err(format!("unknown value type {other:?}")),
            })
        }
    }
}

/// One change as one line of JSON (no newline), in canonical form: the
/// same change always gives the same bytes.
pub fn change_to_json(c: &Change) -> String {
    let f = file_change(c, c.sig.map(|s| s.hex()));
    serde_json::to_string(&f).expect("a change always serializes")
}

/// The line a signature is made over: the change's line without its
/// `sig` (decisions.md, entry 89).
pub fn unsigned_line(c: &Change) -> String {
    serde_json::to_string(&file_change(c, None)).expect("a change always serializes")
}

fn file_change(c: &Change, sig: Option<String>) -> FileChange {
    FileChange {
        actor: c.actor.hex(),
        seq: c.seq,
        deps: c.deps.0.iter().map(|(a, s)| (a.hex(), *s)).collect(),
        lamport: c.lamport,
        time: c.time.clone(),
        label: c.label.clone(),
        ops: c.ops.iter().map(FileOp::from).collect(),
        sig,
    }
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
        sig: f
            .sig
            .map(|s| Signature::parse(&s).ok_or_else(|| format!("bad signature {s:?}")))
            .transpose()?,
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
            format: format_for(changes),
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
/// (named with the `.~` prefix sync services skip) and a rename
/// ([`crate::fsio::write_atomically`]).
pub fn write_sealed(path: &Path, bytes: &[u8]) -> Result<(), LogFileError> {
    crate::fsio::write_atomically_with_prefix(path, bytes, crate::fsio::Access::Document, ".~")
        .map_err(|e| io_err(path, e))
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

    /// Whether the folder exists with a `budget.json`, and wasn't replaced
    /// by a budget started again (a folder holding [`REPLACED_FILE`] is
    /// ignored; docs/research/owner-key.md, part 1).
    pub fn exists(&self) -> bool {
        self.root.join("budget.json").is_file() && !self.root.join(REPLACED_FILE).is_file()
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
            version: LOG_FORMAT_BASE,
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

    /// Raises `budget.json`'s version to `version` (never lowers it),
    /// through a temporary file and a rename. Every writer writes the same
    /// bytes, so two raising it at once agree.
    pub fn raise_version(&self, version: u64) -> Result<(), LogFileError> {
        let path = self.root.join("budget.json");
        let text = read_text(&path).map_err(|e| io_err(&path, e))?;
        let mut f: FileBudgetInfo = serde_json::from_str(&text)
            .map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
        if f.version >= version {
            return Ok(());
        }
        f.version = version;
        let mut text = serde_json::to_string_pretty(&f)
            .map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
        text.push('\n');
        write_sealed(&path, text.as_bytes())
    }

    /// Reads `budget.json`.
    pub fn info(&self) -> Result<BudgetInfo, LogFileError> {
        let path = self.root.join("budget.json");
        let text = read_text(&path).map_err(|e| io_err(&path, e))?;
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
        let bytes =
            read_capped(&segment.path, MAX_READ_BYTES).map_err(|e| io_err(&segment.path, e))?;
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
    // The first role raises the folder's format before the role is
    // written, so no program too old to read it ever sees it half.
    let format = format_for(&own);
    if format > LOG_FORMAT_BASE {
        dir.raise_version(format)?;
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
    let text = read_text(path).map_err(|e| io_err(path, e))?;
    let budget =
        crate::from_str(&text).map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("budget.mthd")
        .to_string();
    let mut log = Log::adopt(actor, ids, &budget, &format!("Adopted {file}"), time);
    log.set_verifier(Arc::new(sign::Ed25519));
    let genesis = log
        .changes()
        .next()
        .expect("adopt makes one change")
        .clone();
    let info = dir.create(&genesis, &file)?;
    dir.note_written(actor, text.as_bytes())?;
    Ok(Shared {
        log,
        dir,
        path: path.to_path_buf(),
        info,
    })
}

/// Opens the shared budget at `path`: reads every actor's segments, with
/// the owner key remembered in the usual place ([`owner_keys_dir`]).
pub fn open(path: &Path, actor: ActorId, ids: Ids) -> Result<(Shared, LoadReport), LogFileError> {
    open_in(path, actor, ids, owner_keys_dir().as_deref())
}

/// [`open`], with the owner keys this computer remembers in `owner_keys`
/// (`None`: none is checked).
pub fn open_in(
    path: &Path,
    actor: ActorId,
    ids: Ids,
    owner_keys: Option<&Path>,
) -> Result<(Shared, LoadReport), LogFileError> {
    finish_replacement(path)?;
    let dir = LogDir::for_budget(path);
    if !dir.exists() {
        return Err(LogFileError(format!(
            "{}: no change log here",
            dir.root().display()
        )));
    }
    let info = dir.info()?;
    let mut log = Log::new(actor, ids);
    // Every signature is checked (decisions.md, entry 89).
    log.set_verifier(Arc::new(sign::Ed25519));
    // The owner's key as this computer first saw it, kept outside the
    // folder (docs/research/owner-key.md, part 2), carried over when the
    // folder now holds another budget than the one this computer knew here.
    if let Some(d) = owner_keys {
        if let Some(key) = carried_owner_key(d, path, &info)? {
            log.pin_owner_key(key);
        }
    }
    // The owner is whoever started sharing, as `budget.json` records,
    // whatever order the segments are read in.
    log.set_genesis(info.genesis);
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

/// The file in an actor's folder listing the hashes of the `.mthd` files
/// it wrote from the log (or started sharing from), newest last.
pub const WRITTEN_FILE: &str = "written.json";
/// How many of those hashes are kept.
const WRITTEN_KEPT: usize = 20;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Written {
    hashes: Vec<String>,
}

impl LogDir {
    /// Notes that `actor` wrote (or started from) a `.mthd` holding `bytes`.
    pub fn note_written(&self, actor: ActorId, bytes: &[u8]) -> Result<(), LogFileError> {
        let dir = self.actor_dir(actor);
        fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        // One program writes each actor folder (the app as the person, every
        // other program as a tool actor it holds alone), so this read,
        // change and write never races another.
        let path = dir.join(WRITTEN_FILE);
        let mut w: Written = read_text(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let hash = sha256_hex(bytes);
        w.hashes.retain(|h| *h != hash);
        w.hashes.push(hash);
        if w.hashes.len() > WRITTEN_KEPT {
            let cut = w.hashes.len() - WRITTEN_KEPT;
            w.hashes.drain(..cut);
        }
        let text = serde_json::to_string(&w)
            .map_err(|e| LogFileError(format!("{}: {e}", path.display())))?;
        write_sealed(&path, text.as_bytes())
    }

    /// Whether some actor wrote (or started from) a `.mthd` holding exactly
    /// `bytes`: a file that is behind the log, not edited outside it.
    pub fn is_written(&self, bytes: &[u8]) -> bool {
        let hash = sha256_hex(bytes);
        self.actors().into_iter().any(|a| {
            read_text(self.actor_dir(a).join(WRITTEN_FILE))
                .ok()
                .and_then(|t| serde_json::from_str::<Written>(&t).ok())
                .is_some_and(|w| w.hashes.contains(&hash))
        })
    }
}

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
    /// What they ask the owner, a co-owner or an admin for, while they ask
    /// (docs/APP.md, "Asking for access"). Left out of the file when they
    /// ask for nothing, so a heartbeat without one reads exactly as before;
    /// an older app ignores the field (the struct reads with defaults and
    /// never refuses a field it doesn't know).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<AccessRequest>,
}

/// An access request in a heartbeat: what role a person would like, with
/// the categories or accounts it is about. Never in the change log: the
/// role itself, if given, is a signed `grant` by whoever allows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessRequest {
    /// The role asked for, as `grant` names it ("editor", "department",
    /// "actuals", "comment").
    pub role: String,
    /// Category or account numbers it is about (a department editor's
    /// scope to add, or where they were when they asked). Left out when
    /// none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<String>,
    /// A short note to whoever decides, as typed. Left out when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// When they asked, ISO 8601 UTC: with the actor, what names the
    /// request (a decline is remembered against it).
    pub at: String,
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
        let text = read_text(self.actor_dir(actor).join(PRESENCE_FILE)).ok()?;
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
    if let Ok(text) = read_text(path) {
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
    let text = read_text(&shared.path).map_err(|e| io_err(&shared.path, e))?;
    // Only behind the log, not edited: taking it in would undo changes.
    if shared.dir.is_written(text.as_bytes()) {
        return Ok(None);
    }
    let on_disk = crate::from_str(&text)
        .map_err(|e| LogFileError(format!("{}: {e}", shared.path.display())))?;
    let proj = shared.log.project();
    let label = format!("Outside edit to {}", shared.info.file);
    shared
        .log
        .record(&proj, &on_disk, &[], &label, time)
        .map_err(|m: LogError| LogFileError(format!("could not record the outside edit: {m}")))
}

/// The `.mthd` text the keeper would write for `proj` now: the merged
/// budget with the file's provenance.
pub fn snapshot_text(shared: &Shared, proj: &Projection) -> String {
    crate::to_string(&with_file_provenance(proj.budget.clone(), &shared.path))
}

/// Writes the merged state as the `.mthd` (the keeper's job) and returns
/// the projection it wrote. The file's provenance, which the log doesn't
/// carry, is kept.
pub fn write_snapshot(shared: &Shared) -> Result<Projection, LogFileError> {
    let proj = shared.log.project();
    let bytes = snapshot_text(shared, &proj);
    // The hash is noted before the file is written, so no reader ever sees
    // this file without knowing it came from the log.
    shared
        .dir
        .note_written(shared.log.actor(), bytes.as_bytes())?;
    write_sealed(&shared.path, bytes.as_bytes())?;
    Ok(proj)
}

// ---------------------------------------------------------------------------
// Changing a shared budget from a program without a window (the command
// line, the MCP server; decisions.md, entry 78). Such a program reads the
// budget as everyone's changes make it and records its edit as one change
// by this computer's person, checked against their role like any other,
// instead of writing the `.mthd` (which only the keeper writes).

/// Whether the budget at `path` is shared through a folder: a change log
/// is beside it.
pub fn is_shared(path: &Path) -> bool {
    LogDir::for_budget(path).exists() || staging_dir(path).exists()
}

/// `budget` with the provenance of the file at `path` (the log doesn't
/// carry it).
fn with_file_provenance(mut budget: Budget, path: &Path) -> Budget {
    if let Ok(text) = read_text(path) {
        if let Ok(on_disk) = crate::from_str(&text) {
            budget.provenance = on_disk.provenance;
        }
    }
    budget
}

/// The shared budget at `path` as everyone's changes make it, for reading.
pub fn read_shared(path: &Path) -> Result<Budget, LogFileError> {
    let (shared, _) = open(path, ActorId::NONE, Ids::seeded(0))?;
    Ok(with_file_provenance(shared.log.project().budget, path))
}

/// What a program without a window is told when it has no person in a
/// shared budget whose owner has set roles.
pub const NO_PERSON: &str = "This computer has no person in this shared budget, and its owner has set who may change what, so nothing was written. Open the budget in Method Budget to join it; the owner can then give you a role.";

/// What a program is told when changes in the budget are signed and this
/// computer's person hasn't published their key yet (their app does when it
/// opens the budget).
pub const NO_KEY_YET: &str = "Changes in this shared budget are signed, and this computer's key isn't in it yet, so nothing was written. Open the budget in Method Budget once; it adds the key, and then this works.";

/// What a program is told when the key published in this computer's
/// person's name (or its own) isn't the key on this computer: someone else
/// may have put it in (decisions.md, entry 89).
pub const KEY_NOT_YOURS: &str = "A key in this computer's name is in this shared budget, and it isn't this computer's key, so nothing was written: someone else may have put it there. Tell the budget's owner before going on.";

/// What a program is told when this computer's person was removed from a
/// shared budget.
pub const REMOVED: &str = "This computer's person was removed from this shared budget, so nothing was written. The owner can give you a role again in the Share dialog.";

/// How many tool actors one computer keeps (one per program changing a
/// shared budget at the same moment).
const TOOL_ACTORS: usize = 32;

/// Takes a tool actor from `dir` (`tool-0`, `tool-1`...): the first whose
/// file this process can lock, made when new. The lock is held until the
/// returned file closes and is let go by the system if the program stops,
/// so no two programs ever write as one tool actor.
fn take_tool_actor(dir: &Path) -> Result<(ActorId, fs::File), LogFileError> {
    use std::io::{Read, Seek, Write};
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    for k in 0..TOOL_ACTORS {
        let path = dir.join(format!("tool-{k}"));
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io_err(&path, e))?;
        if file.try_lock().is_err() {
            continue;
        }
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|e| io_err(&path, e))?;
        if let Some(actor) = ActorId::parse(&text) {
            return Ok((actor, file));
        }
        let actor = ActorId::random(&mut Ids::from_os());
        file.set_len(0).map_err(|e| io_err(&path, e))?;
        file.rewind().map_err(|e| io_err(&path, e))?;
        file.write_all(format!("{}\n", actor.hex()).as_bytes())
            .map_err(|e| io_err(&path, e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        return Ok((actor, file));
    }
    Err(LogFileError(
        "Too many programs are changing shared budgets on this computer at once. Try again in a moment."
            .into(),
    ))
}

/// What a program is told when this computer's person has no role in a
/// shared budget whose owner has set roles.
pub const NO_ROLE: &str = "This computer's person has no role in this shared budget, and its owner has set who may change what, so nothing was written. The owner can give you a role in the Share dialog.";

/// Where the tool actor pools sit: `tools/` beside the person's actor file,
/// one folder per person inside.
pub fn tools_root(person_file: &Path) -> PathBuf {
    person_file
        .parent()
        .map_or_else(|| PathBuf::from("tools"), |p| p.join("tools"))
}

/// The tool actors of `person` on this computer (read without taking
/// them): what lets the app show this person's own programs as theirs
/// before they say so in the log.
pub fn tool_actors(tools_root: &Path, person: ActorId) -> Vec<ActorId> {
    let Ok(entries) = fs::read_dir(tools_root.join(person.hex())) else {
        return Vec::new();
    };
    let mut out: Vec<ActorId> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("tool-"))
        .filter_map(|e| read_text(e.path()).ok())
        .filter_map(|t| ActorId::parse(&t))
        .collect();
    out.sort();
    out
}

/// A program's hold on its tool actor: let go when dropped.
pub struct ToolLock(fs::File);

/// Opens the shared budget at `path` as a tool actor of `person` (whose
/// actor id is kept in `person_file`): one this program holds alone, from
/// the person's pool beside that file. What `method-budget sync` and
/// [`SharedEdit`] write with, so they never share a sequence of changes
/// with the app (decisions.md, entry 78).
pub fn open_as_tool(
    path: &Path,
    person_file: &Path,
    person: ActorId,
) -> Result<(Shared, LoadReport, ToolLock), LogFileError> {
    let tools = tools_root(person_file).join(person.hex());
    let (tool, lock) = take_tool_actor(&tools)?;
    let (mut shared, report) = open(path, tool, Ids::from_os())?;
    // A program signs with its person's key (decisions.md, entry 89): the
    // same key file, beside the person's actor id.
    let key = sign::load_or_make_key(&sign::key_path(person_file))?;
    shared.log.set_signer(Arc::new(key));
    // The owner's key, remembered the first time it is seen (docs/
    // research/owner-key.md, part 2). The time is only a note for people.
    remember_owner_key(&mut shared, &owner_keys_beside(person_file), person, "")?;
    Ok((shared, report, ToolLock(lock)))
}

/// A shared budget opened to change it as this computer's person, through
/// an actor of the program's own (a tool actor) that writes for that
/// person, so it never shares a sequence of changes with the app or with
/// another program on the same computer (decisions.md, entry 78).
pub struct SharedEdit {
    shared: Shared,
    proj: Projection,
    person: ActorId,
    /// Holds the tool actor for this program while it is open.
    _lock: fs::File,
}

impl SharedEdit {
    /// Opens the shared budget at `path` as the person whose actor id is
    /// kept in `actor_file` (by default [`actor_id_path`]). When there is
    /// none yet, one is made, unless the owner has set roles: then a new
    /// person could only read, and the edit is refused ([`NO_PERSON`]).
    pub fn open(path: &Path, actor_file: Option<&Path>) -> Result<SharedEdit, LogFileError> {
        let file = match actor_file {
            Some(f) => Some(f.to_path_buf()),
            None => actor_id_path(),
        };
        let (person, file) = match file {
            Some(f) if f.is_file() => (load_or_make_actor(&f, &mut Ids::from_os())?, f),
            _ => {
                let (probe, _) = open(path, ActorId::NONE, Ids::seeded(0))?;
                if probe.log.roles_started() {
                    return Err(LogFileError(NO_PERSON.into()));
                }
                let f = file.ok_or_else(|| LogFileError(NO_PERSON.into()))?;
                (load_or_make_actor(&f, &mut Ids::from_os())?, f)
            }
        };
        let (shared, _, lock) = open_as_tool(path, &file, person)?;
        let proj = shared.log.project();
        Ok(SharedEdit {
            shared,
            proj,
            person,
            _lock: lock.0,
        })
    }

    /// Whether a key published for this computer's person, or for the
    /// program's own actor, isn't the key on this computer.
    pub fn key_not_ours(&self) -> bool {
        let Some(mine) = self.shared.log.signer_key() else {
            return false;
        };
        self.shared.log.key_conflict().is_some()
            || self
                .shared
                .log
                .key_of(self.person)
                .is_some_and(|k| k != mine)
    }

    /// The program's own actor (the tool actor).
    pub fn actor(&self) -> ActorId {
        self.shared.log.actor()
    }

    /// The person it writes for: this computer's actor.
    pub fn person(&self) -> ActorId {
        self.person
    }

    /// What this person may change.
    pub fn role(&self) -> Role {
        self.shared.log.role_of(self.person)
    }

    /// The budget as everyone's changes make it, with the file's provenance.
    pub fn budget(&self) -> Budget {
        with_file_provenance(self.proj.budget.clone(), &self.shared.path)
    }

    /// Records the edit that made `after` from [`SharedEdit::budget`] as one
    /// change by this person, checked against their role, and writes it to
    /// this person's own folder; the keeper (who started sharing) also
    /// rewrites the `.mthd`. `Ok(false)` when nothing changed. Refused,
    /// with nothing written, when the role doesn't allow it.
    pub fn record(
        &mut self,
        after: &Budget,
        label: &str,
        time: &str,
    ) -> Result<bool, LogFileError> {
        if self.key_not_ours() {
            return Err(LogFileError(KEY_NOT_YOURS.into()));
        }
        if self.shared.log.owner_key_changed().is_some() {
            return Err(LogFileError(OWNER_KEY_CHANGED.into()));
        }
        if self.shared.log.removed(self.person) {
            return Err(LogFileError(REMOVED.into()));
        }
        // Once roles are set, the tool actor says whose it is first, so it
        // has its person's role; the two go into the folder together, or
        // neither does.
        if self.shared.log.roles_started()
            && self.shared.log.person_of(self.actor()) == self.actor()
        {
            let member = self.shared.log.owner() == Some(self.person)
                || self
                    .shared
                    .log
                    .members()
                    .iter()
                    .any(|(a, _)| *a == self.person);
            if !member {
                return Err(LogFileError(NO_ROLE.into()));
            }
            self.shared
                .log
                .acts_for(self.person, "Writes for this computer's person", time)
                .map_err(|e| match e {
                    LogError::Unsigned { reason, .. }
                        if reason == method_core::log::sign::why::PERSON_HAS_NO_KEY =>
                    {
                        LogFileError(NO_KEY_YET.into())
                    }
                    LogError::Removed { .. } => LogFileError(REMOVED.into()),
                    e => LogFileError(format!("This change couldn't be shared: {e}")),
                })?;
        }
        let recorded = self
            .shared
            .log
            .record(&self.proj, after, &[], label, time)
            .map_err(|e| match e {
                LogError::NotAllowed { refusal, .. } => LogFileError(format!(
                    "Your role in this shared budget doesn't allow this change ({}), so nothing was written. The owner sets who may change what.",
                    refusal.reason
                )),
                LogError::Removed { .. } => LogFileError(REMOVED.into()),
                LogError::Unsigned { reason, .. }
                    if reason == method_core::log::sign::why::PERSON_HAS_NO_KEY =>
                {
                    LogFileError(NO_KEY_YET.into())
                }
                other => LogFileError(format!("This change couldn't be shared: {other}")),
            })?;
        if recorded.is_none() {
            return Ok(false);
        }
        flush(&self.shared.log, &self.shared.dir)?;
        // The owner's own program keeps the file, as the owner's app does.
        if self.person == self.shared.info.genesis.actor {
            write_snapshot(&self.shared)?;
        }
        self.proj = self.shared.log.project();
        Ok(true)
    }
}

/// Whether the `.mthd` holds something the log doesn't (an edit by a
/// program that doesn't write the log). Nothing is recorded.
pub fn has_outside_edit(shared: &Shared, time: &str) -> Result<bool, LogFileError> {
    let text = read_text(&shared.path).map_err(|e| io_err(&shared.path, e))?;
    if shared.dir.is_written(text.as_bytes()) {
        return Ok(false);
    }
    let on_disk = crate::from_str(&text)
        .map_err(|e| LogFileError(format!("{}: {e}", shared.path.display())))?;
    let mut probe = shared.log.clone();
    let proj = probe.project();
    // Nothing to record is `Ok(None)` before any role is checked, so a
    // refusal still means the file differs.
    Ok(!matches!(
        probe.record(&proj, &on_disk, &[], "probe", time),
        Ok(None)
    ))
}

/// When someone changed the `.mthd` with a program that doesn't write the
/// log, and roles are set, nobody can say whose change it was, so it isn't
/// taken in as the keeper's: the file as edited is kept beside it as
/// "Feature (outside edit 2026-10-18 140322).mthd" (never over another
/// file) and the keeper writes the budget again from everyone's changes.
/// Returns the kept copy's path, or `None` when the file matches the log.
pub fn set_aside(shared: &Shared, time: &str) -> Result<Option<PathBuf>, LogFileError> {
    if !has_outside_edit(shared, time)? {
        return Ok(None);
    }
    let text = read_text(&shared.path).map_err(|e| io_err(&shared.path, e))?;
    let stem = shared
        .path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("budget");
    let when: String = time
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-' || *c == 'T')
        .collect::<String>()
        .replacen('T', " ", 1);
    let when = when.chars().take(17).collect::<String>();
    let mut n = 1;
    let kept = loop {
        let name = if n == 1 {
            format!("{stem} (outside edit {when}).mthd")
        } else {
            format!("{stem} (outside edit {when} {n}).mthd")
        };
        let candidate = shared.path.with_file_name(name);
        if !candidate.exists() {
            break candidate;
        }
        n += 1;
    };
    write_sealed(&kept, text.as_bytes())?;
    write_snapshot(shared)?;
    Ok(Some(kept))
}

// ---------------------------------------------------------------------------
// The owner's key (docs/research/owner-key.md): each computer remembers the
// owner key it first saw for a budget (part 2), and the owner's computer can
// start a budget again when a key in its name isn't its own (part 1).

/// The file in a log folder that a budget started again replaced: the new
/// budget's id. Every program ignores a folder holding it.
pub const REPLACED_FILE: &str = "REPLACED";

/// What a computer is told when the owner's key in a shared budget isn't
/// the one it saw first.
pub const OWNER_KEY_CHANGED: &str = "The owner\u{2019}s key in this shared budget isn\u{2019}t the one this computer saw before, so nothing was written. Ask the owner before you go on.";

/// What a computer is told when the log beside a budget it knew now names
/// another owner: someone else made it, whatever its folder says.
pub const NOT_SAME_OWNER: &str = "The change log beside this budget was started by someone other than its owner, so it wasn\u{2019}t opened as shared: someone with the shared folder may have replaced it. Ask the owner before you go on.";

/// What this computer knew of the shared budget at one path: which budget,
/// its owner, and the owner's key when it had seen one.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileKnownBudget {
    budget_id: String,
    owner: String,
    key: Option<String>,
    seen: String,
}

fn known_file(dir: &Path, path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = sha256_hex(path.to_string_lossy().as_bytes());
    dir.join("paths").join(format!("{name}.json"))
}

fn known_budget(dir: &Path, path: &Path) -> Option<FileKnownBudget> {
    let text = read_text(known_file(dir, path)).ok()?;
    serde_json::from_str(&text).ok()
}

fn note_budget(
    dir: &Path,
    path: &Path,
    info: &BudgetInfo,
    key: Option<PublicKey>,
    time: &str,
) -> Result<(), LogFileError> {
    let file = known_file(dir, path);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let f = FileKnownBudget {
        budget_id: info.budget_id.clone(),
        owner: info.genesis.actor.hex(),
        key: key.map(|k| k.hex()),
        seen: time.to_string(),
    };
    let mut text = serde_json::to_string_pretty(&f).expect("a known budget serializes");
    text.push('\n');
    write_sealed(&file, text.as_bytes())
}

/// The owner key to check the budget `info` describes against, as kept in
/// `dir`. When the log beside `path` now holds another budget than the one
/// this computer knew there (the owner started it again, or someone
/// replaced it), what it knew is carried over (MReview on pull request #73):
/// another owner is refused ([`NOT_SAME_OWNER`]); the owner's key it had
/// seen is remembered for the new budget too, so the same key joins without
/// a word and a different one is kept out until the person trusts it,
/// whether or not the old folder is still there.
fn carried_owner_key(
    dir: &Path,
    path: &Path,
    info: &BudgetInfo,
) -> Result<Option<PublicKey>, LogFileError> {
    let pin = remembered_owner_key(dir, info);
    let Some(known) = known_budget(dir, path) else {
        return Ok(pin);
    };
    if known.budget_id == info.budget_id {
        return Ok(pin);
    }
    if ActorId::parse(&known.owner) != Some(info.genesis.actor) {
        return Err(LogFileError(NOT_SAME_OWNER.into()));
    }
    let key = pin.or_else(|| known.key.as_deref().and_then(PublicKey::parse));
    if pin.is_none() {
        if let Some(k) = key {
            write_owner_key(dir, info, k, &known.seen)?;
        }
    }
    note_budget(dir, path, info, key, &known.seen)?;
    Ok(key)
}

/// One remembered owner key, as kept on this computer.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOwnerKey {
    owner: String,
    key: String,
    seen: String,
}

/// Where this computer keeps the owner keys it has seen:
/// `$METHOD_OWNER_KEYS`, or `owner-keys/` beside the actor id. Never in the
/// shared folder, which anyone in the budget can write.
pub fn owner_keys_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("METHOD_OWNER_KEYS") {
        return Some(PathBuf::from(p));
    }
    actor_id_path().map(|p| owner_keys_beside(&p))
}

/// `owner-keys/` beside the actor id kept in `actor_file` (or
/// `$METHOD_OWNER_KEYS`): where a program given its own actor file keeps
/// them.
pub fn owner_keys_beside(actor_file: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("METHOD_OWNER_KEYS") {
        return PathBuf::from(p);
    }
    actor_file
        .parent()
        .map_or_else(|| PathBuf::from("owner-keys"), |d| d.join("owner-keys"))
}

fn owner_key_file(dir: &Path, budget_id: &str) -> PathBuf {
    dir.join(format!("{budget_id}.json"))
}

/// The owner key remembered in `dir` for the budget `info` describes, when
/// one is remembered for that owner.
pub fn remembered_owner_key(dir: &Path, info: &BudgetInfo) -> Option<PublicKey> {
    let text = read_text(owner_key_file(dir, &info.budget_id)).ok()?;
    let f: FileOwnerKey = serde_json::from_str(&text).ok()?;
    if ActorId::parse(&f.owner)? != info.genesis.actor {
        return None;
    }
    PublicKey::parse(&f.key)
}

fn write_owner_key(
    dir: &Path,
    info: &BudgetInfo,
    key: PublicKey,
    time: &str,
) -> Result<(), LogFileError> {
    let path = owner_key_file(dir, &info.budget_id);
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let f = FileOwnerKey {
        owner: info.genesis.actor.hex(),
        key: key.hex(),
        seen: time.to_string(),
    };
    let mut text = serde_json::to_string_pretty(&f).expect("an owner key serializes");
    text.push('\n');
    write_sealed(&path, text.as_bytes())
}

/// Remembers in `dir` the owner's key the first time this computer sees
/// one in the log (docs/research/owner-key.md, part 2), and checks against
/// it from then on. On the owner's own computer (`person` is the owner)
/// only its own key is remembered, so a key planted before it looked is
/// never taken for its own. True when it remembered one now.
pub fn remember_owner_key(
    shared: &mut Shared,
    dir: &Path,
    person: ActorId,
    time: &str,
) -> Result<bool, LogFileError> {
    if shared.log.owner_key_pin().is_some() {
        return Ok(false);
    }
    // Which budget is here and whose it is, so a replacement is told from
    // a fresh budget (see `carried_owner_key`). An entry for another budget
    // is moved on only by opening this one through it.
    let known = known_budget(dir, &shared.path);
    let ours = known
        .as_ref()
        .is_none_or(|k| k.budget_id == shared.info.budget_id);
    if let Some(key) = remembered_owner_key(dir, &shared.info) {
        shared.log.pin_owner_key(key);
        if ours && known.as_ref().is_none_or(|k| k.key.is_none()) {
            note_budget(dir, &shared.path, &shared.info, Some(key), time)?;
        }
        return Ok(false);
    }
    let owner = shared.info.genesis.actor;
    let key = shared
        .log
        .key_of(owner)
        .filter(|k| person != owner || shared.log.signer_key() == Some(*k));
    if ours && (known.is_none() || key.is_some()) {
        note_budget(dir, &shared.path, &shared.info, key, time)?;
    }
    let Some(key) = key else {
        return Ok(false);
    };
    write_owner_key(dir, &shared.info, key, time)?;
    shared.log.pin_owner_key(key);
    Ok(true)
}

/// Trust the New Key (docs/research/owner-key.md, part 2): remembers `key`
/// as the owner's for this budget in place of the one seen before, for an
/// owner who moved to a new computer. The budget must be opened again to
/// take in what the old key kept aside.
pub fn trust_owner_key(
    dir: &Path,
    path: &Path,
    info: &BudgetInfo,
    key: PublicKey,
    time: &str,
) -> Result<(), LogFileError> {
    write_owner_key(dir, info, key, time)?;
    note_budget(dir, path, info, Some(key), time)
}

/// Whether the log folder beside this budget now belongs to another budget
/// (the owner started it again): what a member's app notices.
pub fn replaced(shared: &Shared) -> bool {
    shared
        .dir
        .info()
        .is_ok_and(|i| i.budget_id != shared.info.budget_id)
}

/// Where a budget started again is written before it takes the log
/// folder's place: `.~Feature.mthd-log-new` beside it (sync services skip
/// the `.~` prefix until the rename).
fn staging_dir(path: &Path) -> LogDir {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("budget.mthd");
    LogDir {
        root: path.with_file_name(format!(".~{name}{LOG_DIR_SUFFIX}-new")),
    }
}

/// Finishes a start-again that stopped after the old folder was marked
/// [`REPLACED_FILE`] (it is moved aside) or after it was moved aside: the
/// new one is moved into its place.
fn finish_replacement(path: &Path) -> Result<(), LogFileError> {
    let dir = LogDir::for_budget(path);
    let staging = staging_dir(path);
    if !staging.root.join("budget.json").is_file() {
        return Ok(());
    }
    if dir.root().join(REPLACED_FILE).is_file() {
        let aside = replaced_root(path, "unfinished");
        fs::rename(dir.root(), &aside).map_err(|e| io_err(dir.root(), e))?;
    }
    if !dir.root().exists() {
        fs::rename(&staging.root, dir.root()).map_err(|e| io_err(dir.root(), e))?;
    }
    Ok(())
}

/// Where the replaced folder is kept: `Feature.mthd-log-replaced-<date>`
/// (the first ten characters of `time`), never over another folder.
fn replaced_root(path: &Path, time: &str) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("budget.mthd");
    let date: String = time.chars().take(10).collect();
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            path.with_file_name(format!("{name}{LOG_DIR_SUFFIX}-replaced-{date}"))
        } else {
            path.with_file_name(format!("{name}{LOG_DIR_SUFFIX}-replaced-{date}-{n}"))
        };
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// A department's categories and accounts in another projection of the
/// same budget, matched by code.
fn rebind(scope: &BTreeSet<Uid>, old: &Binding, new: &Binding) -> BTreeSet<Uid> {
    let mut out = BTreeSet::new();
    for (code, uid) in &old.categories {
        if scope.contains(uid) {
            out.extend(new.categories.get(code).copied());
        }
    }
    for (code, uid) in &old.accounts {
        if scope.contains(uid) {
            out.extend(new.accounts.get(code).copied());
        }
    }
    out
}

/// Starts the shared budget again (docs/research/owner-key.md, part 1), on
/// the owner's computer, when the key in the owner's name isn't `signer`'s:
/// a new change log from the budget as it stands, its genesis by this
/// computer followed by this computer's key and everyone's roles, takes the
/// old folder's place, and the old folder is kept aside with a
/// [`REPLACED_FILE`] in it. Every program ignores the old folder from then
/// on; members reopen the budget to join the new one. A stop at any point
/// leaves the old budget or the new one, never a mix.
pub fn start_again(
    shared: &Shared,
    signer: Arc<dyn Signer>,
    owner_keys: Option<&Path>,
    ids: Ids,
    time: &str,
) -> Result<Shared, LogFileError> {
    let owner = shared.info.genesis.actor;
    let me = shared.log.actor();
    if me != owner {
        return Err(LogFileError(
            "Only the owner\u{2019}s computer can start a shared budget again.".into(),
        ));
    }
    if shared
        .log
        .key_of(owner)
        .is_none_or(|k| k == signer.public())
    {
        return Err(LogFileError(
            "The key in your name is this computer\u{2019}s, so there is nothing to start again."
                .into(),
        ));
    }
    let old = shared.log.project();
    let budget = with_file_provenance(old.budget.clone(), &shared.path);
    let file = shared.info.file.clone();

    // The new log, in memory first.
    let mut log = Log::adopt(me, ids, &budget, &format!("Started {file} again"), time);
    log.set_verifier(Arc::new(sign::Ed25519));
    log.set_signer(signer);
    log.publish_key_now("Added this computer\u{2019}s key", time)
        .map_err(|e| {
            LogFileError(format!(
                "This computer\u{2019}s key couldn\u{2019}t be added: {e}"
            ))
        })?;
    let new = log.project();
    for (actor, role) in shared.log.members() {
        if actor == owner {
            continue;
        }
        let role = match role {
            Role::Department(scope) => Role::Department(rebind(&scope, &old.binding, &new.binding)),
            r => r,
        };
        log.grant(actor, role, "Carried over a role", time)
            .map_err(|e| LogFileError(format!("A role couldn\u{2019}t be carried over: {e}")))?;
    }
    let genesis = log
        .changes()
        .next()
        .expect("adopt makes one change")
        .clone();

    // Written beside the old folder, then moved into its place.
    let staging = staging_dir(&shared.path);
    if staging.root.exists() {
        // Left by a start-again that stopped before the old folder moved.
        fs::remove_dir_all(&staging.root).map_err(|e| io_err(&staging.root, e))?;
    }
    staging.create(&genesis, &file)?;
    flush(&log, &staging)?;
    let final_dir = LogDir::for_budget(&shared.path);
    let marker = format!("{}\n", budget_id(&genesis));
    write_sealed(&final_dir.root.join(REPLACED_FILE), marker.as_bytes())?;
    let aside = replaced_root(&shared.path, time);
    fs::rename(&final_dir.root, &aside).map_err(|e| io_err(&final_dir.root, e))?;
    fs::rename(&staging.root, &final_dir.root).map_err(|e| io_err(&final_dir.root, e))?;
    let info = final_dir.info()?;
    let mut new_shared = Shared {
        log,
        dir: final_dir,
        path: shared.path.clone(),
        info,
    };
    write_snapshot(&new_shared)?;
    if let Some(dir) = owner_keys {
        remember_owner_key(&mut new_shared, dir, me, time)?;
    }
    Ok(new_shared)
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

    /// A row's `person` (decisions.md, entry 79) is written to the log only
    /// when set, as in the `.mthd`, so a shared budget that never uses it
    /// writes logs an app without the field still reads. Setting and
    /// clearing it still travel through the log.
    #[test]
    fn an_unset_person_writes_no_key_in_the_file_or_the_log() {
        let b = method_core::sample::budget(&method_core::sample::SampleSpec {
            rows: 40,
            accounts: 4,
            categories: 2,
            fringes: 3,
            groups: 2,
            ..Default::default()
        });
        assert!(!crate::to_string(&b).contains("\"person\""));
        let t = "2026-10-18T00:00:00Z";
        let mut log = Log::adopt(actor(1), Ids::seeded(1), &b, "Adopted", t);
        // A new row without a person.
        let p = log.project();
        let mut after = p.budget.clone();
        let next = after.highest_row_id().unwrap_or(0) + 1;
        if let Some(a) = after.categories_mut().flat_map(|c| &mut c.accounts).next() {
            a.details
                .push(method_core::Detail::line(next, "New", "1", "", "10"));
        }
        log.record(&p, &after, &[], "Insert", t).unwrap();
        for c in log.changes() {
            assert!(
                !change_to_json(c).contains("\"person\""),
                "{}",
                change_to_json(c)
            );
        }
        // Set, then clear: both are logged and project back.
        let set_person = |b: &mut method_core::Budget, v: Option<&str>| {
            for d in b
                .categories_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
                .filter(|d| d.id == next)
            {
                d.person = v.map(String::from);
            }
        };
        let p = log.project();
        let mut after = p.budget.clone();
        set_person(&mut after, Some("Mara Lee"));
        log.record(&p, &after, &[], "Person", t).unwrap();
        assert_eq!(log.project().budget, after);
        let p = log.project();
        let mut after = p.budget.clone();
        set_person(&mut after, None);
        log.record(&p, &after, &[], "No person", t).unwrap();
        assert_eq!(log.project().budget, after);
        // Each change still reads back.
        for c in log.changes() {
            let line = change_to_json(c);
            assert_eq!(&change_from_json(&line).unwrap(), c);
        }
    }

    /// Invites, claims and cancels (decisions.md, entry 97) round-trip, and
    /// only a segment holding one is written at format 4.
    #[test]
    fn invite_ops_round_trip_and_raise_only_their_segment_to_4() {
        let ops = vec![
            Op::Invite {
                id: Uid(0xabc),
                role: Role::Department([Uid(7)].into_iter().collect()),
                expires: "2026-10-28T09:00:00Z".into(),
                key: PublicKey([3; 32]),
            },
            Op::Invite {
                id: Uid(0xabd),
                role: Role::Comment,
                expires: "2026-10-22T09:00:00Z".into(),
                key: PublicKey([4; 32]),
            },
            Op::Claim {
                invite: Uid(0xabc),
                proof: Signature([5; 64]),
            },
            Op::CancelInvite { invite: Uid(0xabd) },
        ];
        let change = Change {
            actor: actor(1),
            seq: 1,
            deps: Frontier::new(),
            lamport: 1,
            time: "2026-10-21T00:00:00Z".into(),
            label: "Invites".into(),
            ops,
            sig: None,
        };
        let line = change_to_json(&change);
        for op in [
            r#""op":"invite""#,
            r#""op":"claim""#,
            r#""op":"cancel_invite""#,
            r#""expires":"2026-10-28T09:00:00Z""#,
            r#""scope":["#,
        ] {
            assert!(line.contains(op), "{op}: {line}");
        }
        let back = change_from_json(&line).unwrap();
        assert_eq!(back, change);
        assert_eq!(change_to_json(&back), line);
        // A scope on a role that takes none is refused, as in a grant.
        let bad = line.replacen(r#""role":"department""#, r#""role":"editor""#, 1);
        assert!(change_from_json(&bad).unwrap_err().contains("scope"));
        assert_eq!(
            format_for(std::slice::from_ref(&change)),
            LOG_FORMAT_INVITES
        );
        // Each op alone raises it; a signed change without one stays at 3.
        for op in &change.ops {
            let one = Change {
                ops: vec![op.clone()],
                ..change.clone()
            };
            assert_eq!(format_for(&[one]), LOG_FORMAT_INVITES, "{op:?}");
        }
        let signed = Change {
            ops: vec![Op::Key {
                key: PublicKey([9; 32]),
            }],
            ..change.clone()
        };
        assert_eq!(format_for(&[signed]), LOG_FORMAT_SIGNED);
        let header = String::from_utf8(segment_bytes(&[change]).unwrap()).unwrap();
        assert!(header.starts_with(r#"{"segment":{"format":4,"#), "{header}");
    }

    #[test]
    fn roles_round_trip_through_json() {
        let roles = [
            Role::CoOwner,
            Role::Admin,
            Role::Editor,
            Role::Department([Uid(7), Uid(9)].into_iter().collect()),
            Role::Department(Default::default()),
            Role::Actuals,
            Role::Comment,
            Role::ReadOnly,
        ];
        let change = Change {
            actor: actor(1),
            seq: 2,
            deps: Frontier::new(),
            lamport: 3,
            time: "2026-10-18T00:00:00Z".into(),
            label: "Roles".into(),
            ops: roles
                .iter()
                .map(|role| Op::Grant {
                    actor: actor(2),
                    role: role.clone(),
                })
                .chain([Op::ActsFor { person: actor(3) }])
                .collect(),
            sig: None,
        };
        let line = change_to_json(&change);
        assert!(line.contains(r#""op":"grant""#), "{line}");
        assert!(line.contains(r#""role":"read_only""#), "{line}");
        assert!(line.contains(r#""op":"acts_for""#), "{line}");
        let back = change_from_json(&line).unwrap();
        assert_eq!(back, change);
        assert_eq!(change_to_json(&back), line);
        // An unknown role is a malformed line, never a guess.
        let bad = line.replace(r#""role":"comment""#, r#""role":"producer""#);
        assert!(change_from_json(&bad).is_err());
        // So is a scope on a role that has none: refused, not dropped.
        let scoped = line.replace(
            r#""role":"comment"}"#,
            &format!(r#""role":"comment","scope":["{}"]}}"#, Uid(7).hex()),
        );
        assert_ne!(scoped, line);
        let err = change_from_json(&scoped).unwrap_err();
        assert!(err.contains("scope"), "{err}");
    }

    /// Rate tables and a row's rate source (decisions.md, entry 90) are
    /// written to the log only when used, as in the `.mthd`, so a shared
    /// budget that never uses them writes logs an app without them reads.
    /// Adding, filling, changing and removing still travel through the log.
    #[test]
    fn unused_rate_tables_write_no_key_in_the_log() {
        let b = method_core::sample::budget(&method_core::sample::SampleSpec {
            rows: 20,
            accounts: 2,
            categories: 1,
            ..Default::default()
        });
        let t = "2030-01-01T00:00:00Z";
        let mut log = Log::adopt(actor(1), Ids::seeded(1), &b, "Adopted", t);
        let p = log.project();
        let mut after = p.budget.clone();
        after.title = "Renamed".into();
        log.record(&p, &after, &[], "Title", t).unwrap();
        for c in log.changes() {
            let j = change_to_json(c);
            assert!(
                !j.contains("rate_tables") && !j.contains("rate_source"),
                "{j}"
            );
        }
        // A table, then a line filled from it, then the table gone.
        let p = log.project();
        let mut after = p.budget.clone();
        after.rate_tables.push(method_core::rates::RateTable {
            name: "Invented Crew".into(),
            notes: String::new(),
            rows: vec![method_core::rates::RateRow {
                classification: "Made Up".into(),
                rates: [("Day".to_string(), "410.5".parse().unwrap())].into(),
                from: None,
                to: None,
                notes: String::new(),
            }],
        });
        log.record(&p, &after, &[], "Table", t).unwrap();
        assert_eq!(log.project().budget, after);
        let p = log.project();
        let mut after = p.budget.clone();
        let row = after
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.kind == method_core::RowKind::Line)
            .unwrap()
            .id;
        method_core::rates::fill(
            &mut after,
            row,
            "Invented Crew",
            "Made Up",
            Some("Day"),
            None,
        )
        .unwrap();
        log.record(&p, &after, &[], "Fill", t).unwrap();
        assert_eq!(log.project().budget, after);
        let p = log.project();
        let mut after = p.budget.clone();
        after.rate_tables.clear();
        log.record(&p, &after, &[], "No table", t).unwrap();
        assert_eq!(log.project().budget, after);
        for c in log.changes() {
            let line = change_to_json(c);
            assert_eq!(&change_from_json(&line).unwrap(), c);
        }
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
            Value::RateTables(vec![method_core::rates::RateTable {
                name: "Invented".into(),
                notes: "n".into(),
                rows: vec![method_core::rates::RateRow {
                    classification: "Made Up".into(),
                    rates: [("Day".to_string(), "1.5".parse().unwrap())].into(),
                    from: Some("2030-01-01".into()),
                    to: None,
                    notes: String::new(),
                }],
            }]),
            Value::RateSource(method_core::rates::RateSource {
                table: "Invented".into(),
                classification: "Made Up".into(),
                unit: "Day".into(),
                as_of: Some("2030-01-01".into()),
                rate: "1.5".parse().unwrap(),
            }),
            Value::SetupOrder(b.setup_order.clone()),
            Value::SetupFolders(b.setup_folders.clone()),
            Value::SetupSources(b.setup_sources.clone()),
            Value::SharedCaps([("State".to_string(), 50_000)].into()),
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
    fn a_heartbeat_with_and_without_an_access_request_round_trips() {
        let dir = std::env::temp_dir().join(format!("method-presence-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let log = LogDir::for_budget(&dir.join("Feature.mthd"));
        let plain = Presence {
            name: "Jordan Lee".into(),
            color: "blue".into(),
            category: Some("2000".into()),
            account: Some("2400".into()),
            at: "2026-10-21T10:00:00Z".into(),
            frontier: "abc".into(),
            request: None,
        };
        log.write_presence(actor(2), &plain).unwrap();
        let text = fs::read_to_string(log.actor_dir(actor(2)).join(PRESENCE_FILE)).unwrap();
        assert!(!text.contains("request"), "no request, no field: {text}");
        assert_eq!(log.read_presence(actor(2)), Some(plain.clone()));

        let asking = Presence {
            request: Some(AccessRequest {
                role: "editor".into(),
                codes: vec!["2400".into()],
                note: "For the grip rentals".into(),
                at: "2026-10-21T10:01:00Z".into(),
            }),
            ..plain.clone()
        };
        log.write_presence(actor(2), &asking).unwrap();
        assert_eq!(log.read_presence(actor(2)), Some(asking.clone()));
        // Codes and a note are left out when there are none.
        let bare = AccessRequest {
            role: "editor".into(),
            at: "t".into(),
            ..AccessRequest::default()
        };
        let j = serde_json::to_string(&bare).unwrap();
        assert_eq!(j, r#"{"role":"editor","at":"t"}"#);

        // A heartbeat written by an older app (no request field) still reads,
        // and a field this version doesn't know is passed over, as an older
        // app passes over `request`.
        let old = r#"{"name":"Alex Kim","color":"teal","category":null,"account":null,"at":"2026-10-20T09:00:00Z","frontier":"f"}"#;
        let p: Presence = serde_json::from_str(old).unwrap();
        assert_eq!(p.name, "Alex Kim");
        assert_eq!(p.request, None);
        let newer = r#"{"name":"Alex Kim","color":"teal","at":"t","frontier":"f","something_later":{"x":1}}"#;
        assert_eq!(
            serde_json::from_str::<Presence>(newer).unwrap().name,
            "Alex Kim"
        );
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct OlderPresence {
            name: String,
            color: String,
            category: Option<String>,
            account: Option<String>,
            at: String,
            frontier: String,
        }
        let read_by_older: OlderPresence =
            serde_json::from_str(&serde_json::to_string(&asking).unwrap()).unwrap();
        assert_eq!(read_by_older.name, "Jordan Lee");
        let _ = fs::remove_dir_all(&dir);
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
