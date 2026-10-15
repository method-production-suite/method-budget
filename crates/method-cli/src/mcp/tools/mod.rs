//! The tools: each one a name, a description, an input schema, whether it
//! writes, and a function from checked arguments to a result.
//!
//! Results are a short summary for people and a JSON object for programs
//! (`structuredContent`). Money is always integer cents, in keys ending
//! `_cents`; rates and quantities are exact decimals written as strings;
//! formulas are the text as typed. Text from a budget (titles, descriptions,
//! notes, names) is returned as data, never as instructions.

use std::path::PathBuf;

use method_core::{recalc, Budget, Cents, RecalcOptions, Report};
use method_format::serde_json::{json, Map, Value};
use method_session::edit::EditError;

use super::docs::{Docs, Edited};
use super::paths::Roots;

pub mod files;
pub mod lines;
pub mod read;
pub mod setup;

#[cfg(test)]
mod tests;

/// What a tool returns when it works.
pub struct ToolOut {
    /// One or two sentences for a person.
    pub summary: String,
    /// The result for a program.
    pub data: Value,
}

/// Why a tool didn't do what it was asked. Returned to the client as a tool
/// result with `isError: true`, so a model can read it and try again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    /// `invalid_input`, `not_found`, `refused`, `conflict`, `path_refused`
    /// or `io`.
    pub kind: &'static str,
    pub message: String,
}

impl ToolError {
    pub fn invalid(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "invalid_input",
            message: m.into(),
        }
    }
    pub fn not_found(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "not_found",
            message: m.into(),
        }
    }
    pub fn refused(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "refused",
            message: m.into(),
        }
    }
    pub fn conflict(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "conflict",
            message: m.into(),
        }
    }
    pub fn path(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "path_refused",
            message: m.into(),
        }
    }
    pub fn io(m: impl Into<String>) -> ToolError {
        ToolError {
            kind: "io",
            message: m.into(),
        }
    }
}

impl From<String> for ToolError {
    fn from(m: String) -> ToolError {
        ToolError::invalid(m)
    }
}

/// A session edit's refusal, by its kind.
impl From<EditError> for ToolError {
    fn from(e: EditError) -> ToolError {
        match e {
            EditError::Invalid(m) => ToolError::invalid(m),
            EditError::NotFound(m) => ToolError::not_found(m),
            EditError::Refused(m) => ToolError::refused(m),
        }
    }
}

pub type ToolResult = Result<ToolOut, ToolError>;

/// What every tool works with.
pub struct Ctx {
    pub roots: Roots,
    pub docs: Docs,
}

/// A tool.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Writes a file. Hidden (and refused) when the server is read-only.
    pub mutating: bool,
    /// Removes or replaces something (a hint for clients).
    pub destructive: bool,
    pub schema: fn() -> Value,
    pub run: fn(&mut Ctx, &Args) -> ToolResult,
}

impl Tool {
    /// The tool as `tools/list` describes it.
    pub fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": (self.schema)(),
            "annotations": {
                "title": self.title,
                "readOnlyHint": !self.mutating,
                "destructiveHint": self.mutating && self.destructive,
                "idempotentHint": !self.mutating,
                "openWorldHint": false,
            },
        })
    }
}

/// Every tool, in the order `tools/list` gives them.
pub fn all() -> Vec<Tool> {
    let mut v = Vec::new();
    v.extend(read::tools());
    v.extend(lines::tools());
    v.extend(setup::tools());
    v.extend(files::tools());
    v
}

/// A tool call's arguments, already checked against the tool's schema.
pub struct Args<'a>(pub &'a Map<String, Value>);

impl Args<'_> {
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k).filter(|v| !v.is_null())
    }
    /// Whether the key is there, `null` included (null clears a value).
    pub fn has(&self, k: &str) -> bool {
        self.0.contains_key(k)
    }
    pub fn str(&self, k: &str) -> Option<&str> {
        self.get(k).and_then(Value::as_str)
    }
    pub fn req(&self, k: &str) -> Result<&str, ToolError> {
        self.str(k)
            .ok_or_else(|| ToolError::invalid(format!("`{k}` is required")))
    }
    pub fn bool(&self, k: &str) -> bool {
        self.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn opt_bool(&self, k: &str) -> Option<bool> {
        self.get(k).and_then(Value::as_bool)
    }
    pub fn u64(&self, k: &str) -> Option<u64> {
        self.get(k).and_then(Value::as_u64)
    }
    pub fn list(&self, k: &str) -> &[Value] {
        self.get(k)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn strings(&self, k: &str) -> Vec<String> {
        self.list(k)
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }
}

/// A formula argument as text: a string as given, a number as written.
pub fn formula_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// An existing budget file named by the `path` argument (or `key`).
pub fn budget_path(ctx: &Ctx, a: &Args, key: &str) -> Result<(PathBuf, String), ToolError> {
    let typed = a.req(key)?;
    let real = ctx.roots.existing(typed, super::paths::BUDGET_EXTS)?;
    let shown = ctx.roots.show(&real);
    Ok((real, shown))
}

/// Recalculation options leaving out the `exclude_groups` argument's groups.
pub fn options(a: &Args) -> RecalcOptions {
    let mut o = RecalcOptions::default();
    for g in a.strings("exclude_groups") {
        o.exclude_groups.insert(g);
    }
    o
}

/// `1234567` cents as `12,345.67`.
pub fn money(c: Cents) -> String {
    crate::cli::money(c)
}

/// Budget text quoted for a summary line, cut short: it is data, so it is
/// shown, never followed.
pub fn quoted(s: &str) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cut: String = one_line.chars().take(60).collect();
    if cut.chars().count() < one_line.chars().count() {
        format!("\"{cut}...\"")
    } else {
        format!("\"{cut}\"")
    }
}

/// The headline totals of a report, in cents.
pub fn headline(r: &Report) -> Value {
    json!({
        "total_atl_cents": r.total_atl,
        "total_btl_cents": r.total_btl,
        "atl_btl_cents": r.atl_btl,
        "budget_fringes_cents": r.budget_fringes,
        "charges_cents": r.charges.iter().map(|c| c.amount).sum::<Cents>(),
        "grand_total_cents": r.grand_total,
        "credits_cents": r.credits_total,
        "net_total_cents": r.net_total,
    })
}

/// Recalculates `b` with the default options.
pub fn report_of(b: &Budget) -> Result<Report, String> {
    recalc(b, &RecalcOptions::default()).map_err(|e| e.to_string())
}

/// The result of a change, as every editing tool returns it: whether it
/// changed anything, the file's new revision, the totals before and after
/// and the categories whose totals moved, with `extra` merged in.
pub fn edited<T>(shown: &str, e: &Edited<T>, what: &str, mut extra: Map<String, Value>) -> ToolOut {
    extra.insert("path".into(), json!(shown));
    extra.insert("changed".into(), json!(e.changed));
    extra.insert("revision".into(), json!(e.revision));
    if !e.changed {
        return ToolOut {
            summary: format!("{what}: nothing changed in {shown}; nothing written."),
            data: Value::Object(extra),
        };
    }
    let mut summary = format!("{what} in {shown}.");
    match (&e.before, &e.after) {
        (Some(b), Ok(a)) => {
            let moved: Vec<Value> = a
                .categories
                .iter()
                .filter_map(|c| {
                    let old = b
                        .categories
                        .iter()
                        .find(|o| o.code == c.code)
                        .map_or(0, |o| o.total);
                    (old != c.total).then(|| {
                        json!({"code": c.code, "name": c.name, "before_cents": old, "after_cents": c.total})
                    })
                })
                .collect();
            extra.insert(
                "totals".into(),
                json!({"before": headline(b), "after": headline(a), "categories_moved": moved}),
            );
            let d = a.net_total.saturating_sub(b.net_total);
            if d != 0 {
                let sign = if d > 0 { "+" } else { "" };
                summary.push_str(&format!(
                    " Net total {} -> {} ({sign}{}).",
                    money(b.net_total),
                    money(a.net_total),
                    money(d)
                ));
            } else {
                summary.push_str(&format!(" Net total unchanged at {}.", money(a.net_total)));
            }
        }
        (_, Ok(a)) => {
            extra.insert("totals".into(), json!({"after": headline(a)}));
        }
        (_, Err(problem)) => {
            summary.push_str(&format!(" The budget still has a problem: {problem}."));
            extra.insert("problem".into(), json!(problem));
        }
    }
    ToolOut {
        summary,
        data: Value::Object(extra),
    }
}

/// A JSON object from key and value pairs.
pub fn obj(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// The schema property every budget tool takes.
pub fn path_prop() -> (&'static str, Value) {
    (
        "path",
        super::schema::path(
            "The budget file (.mthd): relative to the server's first root folder, or \
             absolute inside a root.",
        ),
    )
}

/// The schema property for leaving groups' lines out of totals.
pub fn exclude_prop() -> (&'static str, Value) {
    (
        "exclude_groups",
        super::schema::array(
            "Leave out lines tagged with any of these groups (as method totals --exclude does).",
            super::schema::name("A group name."),
            100,
        ),
    )
}
