//! Version deltas (docs/FORMAT.md, "Version deltas"): with the budget's
//! `compact_versions` on, a version is written as the changes that turn
//! another budget object into its own, instead of a full copy.
//!
//! The changes are a JSON Patch (RFC 6902, the `add`, `remove` and `replace`
//! operations, with RFC 6901 paths) applied to a **base**: the budget's own
//! content (`"budget"`) for the newest version, and the version after it
//! (`"next"`) for the others, so a version that differs a little from its
//! neighbor costs a little. A base is always the budget object exactly as
//! the reference writer writes it in canonical form: for `"budget"`, the
//! top-level budget without the fields a version can't have (versions,
//! presets, display settings, setup order and folders, provenance,
//! `compact_versions`, actuals); for `"next"`, the next version's budget.
//!
//! Deltas are only a way of writing: reading always rebuilds every version
//! as a full snapshot (the same `Arc<BudgetSnapshot>` a full copy gives),
//! and the patched object is read as strictly as a full version's budget.
//! The writer keeps a full copy wherever a delta would not be smaller.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use fwb_core::{Budget, BudgetSnapshot, Version};

use crate::dto::{FileBudget, FileSnapshot, FileVersion};

/// What a delta is applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeltaBase {
    /// The budget's own content.
    Budget,
    /// The next version in the list, as read.
    Next,
}

/// A version written as changes: `"delta": {"base": ..., "patch": [...]}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileDelta {
    pub base: DeltaBase,
    pub patch: Vec<PatchOp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpKind {
    Add,
    Remove,
    Replace,
}

/// One JSON Patch operation. `value` is required for `add` and `replace`
/// and not allowed for `remove`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PatchOp {
    pub op: OpKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// Arrays whose changed middles are bigger than this (old length times new
/// length) are not searched for a longest common run; the middle is then
/// patched element by element.
const LCS_LIMIT: usize = 4_000_000;

/// `token` escaped for a JSON Pointer (RFC 6901): `~` as `~0`, `/` as `~1`.
fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn child(path: &str, token: &str) -> String {
    format!("{path}/{}", escape_token(token))
}

fn op(op: OpKind, path: String, value: Option<Value>) -> PatchOp {
    PatchOp { op, path, value }
}

/// The patch that turns `from` into `to`: empty when they are equal.
pub(crate) fn diff(from: &Value, to: &Value) -> Vec<PatchOp> {
    let mut out = Vec::new();
    diff_into(from, to, "", &mut out);
    out
}

fn diff_into(from: &Value, to: &Value, path: &str, out: &mut Vec<PatchOp>) {
    if from == to {
        return;
    }
    match (from, to) {
        (Value::Object(a), Value::Object(b)) => diff_objects(a, b, path, out),
        (Value::Array(a), Value::Array(b)) => diff_arrays(a, b, path, out),
        _ => out.push(op(OpKind::Replace, path.to_string(), Some(to.clone()))),
    }
}

fn diff_objects(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    path: &str,
    out: &mut Vec<PatchOp>,
) {
    for key in a.keys() {
        if !b.contains_key(key) {
            out.push(op(OpKind::Remove, child(path, key), None));
        }
    }
    for (key, value) in b {
        match a.get(key) {
            Some(old) => diff_into(old, value, &child(path, key), out),
            None => out.push(op(OpKind::Add, child(path, key), Some(value.clone()))),
        }
    }
}

/// Serialized length, to choose the smaller of two ways to write a change.
fn size<T: Serialize>(v: &T) -> usize {
    serde_json::to_string(v).map_or(usize::MAX, |s| s.len())
}

/// Element `to` written over the element at `path`: patched inside when that
/// is smaller, else replaced whole.
fn edit_element(from: &Value, to: &Value, path: &str, out: &mut Vec<PatchOp>) {
    let mut inner = Vec::new();
    diff_into(from, to, path, &mut inner);
    let whole = op(OpKind::Replace, path.to_string(), Some(to.clone()));
    if size(&inner) <= size(&whole) {
        out.extend(inner);
    } else {
        out.push(whole);
    }
}

/// Matches elements of `a` and `b` (a longest common subsequence of equal
/// elements, after the common prefix and suffix), then writes what lies
/// between matches as edits of paired elements, removals and additions.
/// Operations apply in order, so each index is where the element sits after
/// the operations before it.
fn diff_arrays(a: &[Value], b: &[Value], path: &str, out: &mut Vec<PatchOp>) {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let am = &a[prefix..a.len() - suffix];
    let bm = &b[prefix..b.len() - suffix];

    // Matched pairs (index in am, index in bm), in order.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let (n, m) = (am.len(), bm.len());
    if n > 0 && m > 0 && n.saturating_mul(m) <= LCS_LIMIT {
        // lcs[i][j]: length of the longest common subsequence of am[i..], bm[j..].
        let w = m + 1;
        let mut lcs = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * w + j] = if am[i] == bm[j] {
                    lcs[(i + 1) * w + j + 1] + 1
                } else {
                    lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if am[i] == bm[j] {
                pairs.push((i, j));
                i += 1;
                j += 1;
            } else if lcs[(i + 1) * w + j] >= lcs[i * w + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    pairs.push((n, m)); // a sentinel that closes the last gap

    let mut idx = prefix;
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs {
        // The gap am[i..pi] (old) against bm[j..pj] (new).
        let (old, new) = (pi - i, pj - j);
        let paired = old.min(new);
        for k in 0..paired {
            edit_element(&am[i + k], &bm[j + k], &format!("{path}/{idx}"), out);
            idx += 1;
        }
        for _ in paired..old {
            out.push(op(OpKind::Remove, format!("{path}/{idx}"), None));
        }
        for v in &bm[j + paired..pj] {
            out.push(op(OpKind::Add, format!("{path}/{idx}"), Some(v.clone())));
            idx += 1;
        }
        // The match itself.
        idx += 1;
        i = pi + 1;
        j = pj + 1;
    }
}

/// The tokens of a JSON Pointer.
fn tokens(path: &str) -> Result<Vec<String>, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let rest = path
        .strip_prefix('/')
        .ok_or_else(|| format!("the path `{path}` must start with /"))?;
    rest.split('/')
        .map(|t| {
            let mut out = String::new();
            let mut chars = t.chars();
            while let Some(c) = chars.next() {
                if c == '~' {
                    match chars.next() {
                        Some('0') => out.push('~'),
                        Some('1') => out.push('/'),
                        _ => return Err(format!("the path `{path}` has a bad ~ escape")),
                    }
                } else {
                    out.push(c);
                }
            }
            Ok(out)
        })
        .collect()
}

/// An array index token: digits, no leading zero (RFC 6901).
fn index(token: &str, path: &str) -> Result<usize, String> {
    let digits = !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit());
    if !digits || (token.len() > 1 && token.starts_with('0')) {
        return Err(format!("`{token}` in `{path}` is not an array index"));
    }
    token
        .parse()
        .map_err(|_| format!("`{token}` in `{path}` is not an array index"))
}

/// Applies `patch` to `doc` in order. On an error `doc` may be partly
/// patched.
pub(crate) fn apply(doc: &mut Value, patch: &[PatchOp]) -> Result<(), String> {
    for (n, p) in patch.iter().enumerate() {
        apply_one(doc, p).map_err(|e| format!("patch operation {n}: {e}"))?;
    }
    Ok(())
}

fn apply_one(doc: &mut Value, p: &PatchOp) -> Result<(), String> {
    let path = p.path.as_str();
    let value = match (p.op, &p.value) {
        (OpKind::Remove, Some(_)) => return Err("a remove takes no value".into()),
        (OpKind::Remove, None) => None,
        (_, None) => return Err("an add or replace needs a value".into()),
        (_, Some(v)) => Some(v.clone()),
    };
    let mut toks = tokens(path)?;
    let Some(last) = toks.pop() else {
        // The whole document.
        return match value {
            Some(v) => {
                *doc = v;
                Ok(())
            }
            None => Err("the whole budget can't be removed".into()),
        };
    };
    let mut target = doc;
    for t in &toks {
        target = match target {
            Value::Object(m) => m
                .get_mut(t)
                .ok_or_else(|| format!("`{path}`: there is no `{t}`"))?,
            Value::Array(a) => {
                let i = index(t, path)?;
                let len = a.len();
                a.get_mut(i)
                    .ok_or_else(|| format!("`{path}`: index {i} is past the end ({len})"))?
            }
            _ => return Err(format!("`{path}` goes inside a value that has no parts")),
        };
    }
    match (target, p.op) {
        (Value::Object(m), OpKind::Add) => {
            m.insert(last, value.unwrap_or(Value::Null));
        }
        (Value::Object(m), OpKind::Replace) => match m.get_mut(&last) {
            Some(slot) => *slot = value.unwrap_or(Value::Null),
            None => return Err(format!("`{path}`: there is no `{last}` to replace")),
        },
        (Value::Object(m), OpKind::Remove) => {
            if m.remove(&last).is_none() {
                return Err(format!("`{path}`: there is no `{last}` to remove"));
            }
        }
        (Value::Array(a), kind) => {
            let len = a.len();
            let i = if kind == OpKind::Add && last == "-" {
                len
            } else {
                index(&last, path)?
            };
            match kind {
                OpKind::Add if i <= len => a.insert(i, value.unwrap_or(Value::Null)),
                OpKind::Replace if i < len => a[i] = value.unwrap_or(Value::Null),
                OpKind::Remove if i < len => {
                    a.remove(i);
                }
                _ => return Err(format!("`{path}`: index {i} is past the end ({len})")),
            }
        }
        _ => return Err(format!("`{path}` goes inside a value that has no parts")),
    }
    Ok(())
}

/// A version's content as the canonical budget object.
fn canonical(content: &BudgetSnapshot) -> Value {
    serde_json::to_value(FileBudget::from(&content.to_budget()))
        .expect("a budget always serializes")
}

/// The budget's own content as the canonical budget object (the `"budget"`
/// base).
fn budget_base(b: &Budget) -> Value {
    canonical(&b.snapshot())
}

/// Rewrites `versions` (built from `b`, every one a full copy) as deltas,
/// newest first, wherever a delta is smaller than the full copy.
pub(crate) fn compact(b: &Budget, versions: &mut [FileVersion]) {
    let mut base = budget_base(b);
    let mut base_kind = DeltaBase::Budget;
    for v in versions.iter_mut().rev() {
        let Some(full) = v.budget.as_ref() else {
            continue;
        };
        let full = serde_json::to_value(full).expect("a budget always serializes");
        let patch = diff(&base, &full);
        let delta = FileDelta {
            base: base_kind,
            patch,
        };
        if size(&delta) < size(&full) {
            v.budget = None;
            v.delta = Some(delta);
        }
        base = full;
        base_kind = DeltaBase::Next;
    }
}

/// Rebuilds full versions from the file's, oldest first. `b` is the budget
/// read so far (its content is the `"budget"` base). Errors name the version
/// by its place in the list and its name.
pub(crate) fn read_versions(
    b: &Budget,
    versions: Vec<FileVersion>,
) -> Result<Vec<Version>, crate::FormatError> {
    let mut out: Vec<Version> = Vec::with_capacity(versions.len());
    let mut budget_value: Option<Value> = None;
    // The version after the one being read, already rebuilt.
    let mut next: Option<std::sync::Arc<BudgetSnapshot>> = None;
    for (i, fv) in versions.into_iter().enumerate().rev() {
        let fail = |message: String| crate::FormatError::VersionDelta {
            index: i,
            name: fv.name.clone(),
            message,
        };
        let content = match (fv.budget, fv.delta) {
            (Some(snapshot), None) => Budget::from(snapshot.0).into_parts().0,
            (None, Some(delta)) => {
                let mut doc = match delta.base {
                    DeltaBase::Budget => budget_value.get_or_insert_with(|| budget_base(b)).clone(),
                    DeltaBase::Next => match &next {
                        Some(n) => canonical(n),
                        None => {
                            return Err(fail(
                                "the last version's delta must have the budget as its base".into(),
                            ))
                        }
                    },
                };
                apply(&mut doc, &delta.patch).map_err(&fail)?;
                let snapshot = FileSnapshot::deserialize(doc)
                    .map_err(|e| fail(format!("the patched budget is not valid: {e}")))?;
                Budget::from(snapshot.0).into_parts().0
            }
            // The reader requires exactly one of the two.
            _ => return Err(fail("a version needs a budget or a delta".into())),
        };
        let content = std::sync::Arc::new(content);
        next = Some(content.clone());
        out.push(Version {
            id: fv.id,
            name: fv.name,
            created: fv.created,
            note: fv.note,
            budget: content,
        });
    }
    out.reverse();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round(from: Value, to: Value) -> usize {
        let patch = diff(&from, &to);
        let mut doc = from;
        apply(&mut doc, &patch).unwrap();
        assert_eq!(doc, to);
        patch.len()
    }

    #[test]
    fn diffs_apply_back() {
        assert_eq!(round(json!({"a": 1}), json!({"a": 1})), 0);
        assert_eq!(round(json!({"a": 1, "b": 2}), json!({"a": 2, "c": [1]})), 3);
        assert_eq!(round(json!([1, 2, 3, 4]), json!([1, 3, 4])), 1);
        assert_eq!(round(json!([1, 2, 3]), json!([0, 1, 2, 3, 9])), 2);
        round(json!([1, 2, 3]), json!([3, 2, 1]));
        round(json!([]), json!([1, 2]));
        round(json!([1, 2]), json!([]));
        round(json!({"a/b": {"~x": 1}}), json!({"a/b": {"~x": 2}}));
        round(
            json!([{"id": 1, "r": "5"}, {"id": 2}]),
            json!([{"id": 1, "r": "6"}, {"id": 2}]),
        );
        round(json!("x"), json!({"a": 1}));
    }

    #[test]
    fn edited_element_patches_inside() {
        let from = json!([{"id": 1, "description": "a long description here", "rate": "5"}]);
        let to = json!([{"id": 1, "description": "a long description here", "rate": "6"}]);
        let patch = diff(&from, &to);
        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].path, "/0/rate");
    }

    #[test]
    fn pointers() {
        assert_eq!(tokens("").unwrap(), Vec::<String>::new());
        assert_eq!(tokens("/a~1b/~0c/0").unwrap(), ["a/b", "~c", "0"]);
        assert!(tokens("a").is_err());
        assert!(tokens("/a~2").is_err());
        assert!(index("01", "p").is_err());
        assert!(index("-1", "p").is_err());
        assert_eq!(index("10", "p"), Ok(10));
    }

    #[test]
    fn bad_patches_fail_cleanly() {
        let mut doc = json!({"a": [1, 2]});
        for p in [
            op(OpKind::Remove, "/b".into(), None),
            op(OpKind::Replace, "/a/5".into(), Some(json!(1))),
            op(OpKind::Add, "/a/3".into(), Some(json!(1))),
            op(OpKind::Add, "/a/0/x".into(), Some(json!(1))),
            op(OpKind::Remove, "".into(), None),
            op(OpKind::Remove, "/a".into(), Some(json!(1))),
            op(OpKind::Add, "/a/x".into(), Some(json!(1))),
            op(OpKind::Replace, "/c".into(), None),
        ] {
            assert!(apply(&mut doc, std::slice::from_ref(&p)).is_err(), "{p:?}");
        }
        apply(&mut doc, &[op(OpKind::Add, "/a/-".into(), Some(json!(3)))]).unwrap();
        assert_eq!(doc, json!({"a": [1, 2, 3]}));
    }
}
