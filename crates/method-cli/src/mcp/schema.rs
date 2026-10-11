//! Tool input schemas: small builders for the JSON Schema (2020-12) subset
//! the tools use, a validator for that subset (every `tools/call` checks its
//! arguments with it before a tool runs), and a checker that a schema only
//! uses that subset correctly (the tests run it on every tool).
//!
//! The subset: `type` (a name or a list of names), `description`,
//! `properties`, `required`, `additionalProperties: false`, `items`, `enum`,
//! `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems`.

use method_format::serde_json::{json, Map, Value};

use super::limits;

/// An object schema with these properties, `required` naming some of them.
/// Unknown properties are refused (`additionalProperties: false`).
pub fn object(props: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut p = Map::new();
    for (k, v) in props {
        p.insert(k.to_string(), v);
    }
    let mut o = json!({
        "type": "object",
        "properties": Value::Object(p),
        "additionalProperties": false,
    });
    if !required.is_empty() {
        o["required"] = json!(required);
    }
    o
}

/// Free text up to [`limits::MAX_TEXT`] characters.
pub fn text(description: &str) -> Value {
    json!({"type": "string", "description": description, "maxLength": limits::MAX_TEXT})
}

/// A name or code up to [`limits::MAX_NAME`] characters.
pub fn name(description: &str) -> Value {
    json!({"type": "string", "description": description, "minLength": 1, "maxLength": limits::MAX_NAME})
}

/// A path up to [`limits::MAX_PATH`] bytes.
pub fn path(description: &str) -> Value {
    json!({"type": "string", "description": description, "minLength": 1, "maxLength": limits::MAX_PATH})
}

/// A formula: text, or a plain number.
pub fn formula(description: &str) -> Value {
    json!({"type": ["string", "number"], "description": description, "maxLength": limits::MAX_FORMULA})
}

pub fn boolean(description: &str) -> Value {
    json!({"type": "boolean", "description": description})
}

pub fn integer(description: &str, min: i64, max: i64) -> Value {
    json!({"type": "integer", "description": description, "minimum": min, "maximum": max})
}

/// One of `values`.
pub fn one_of(description: &str, values: &[&str]) -> Value {
    json!({"type": "string", "description": description, "enum": values})
}

/// A list of `items`, at most `max` long.
pub fn array(description: &str, items: Value, max: usize) -> Value {
    json!({"type": "array", "description": description, "items": items, "maxItems": max})
}

/// `schema`, or `null` (to clear a value).
pub fn nullable(mut schema: Value) -> Value {
    let types = match schema.get("type") {
        Some(Value::String(t)) => vec![Value::String(t.clone()), json!("null")],
        Some(Value::Array(ts)) => {
            let mut ts = ts.clone();
            ts.push(json!("null"));
            ts
        }
        _ => return schema,
    };
    schema["type"] = Value::Array(types);
    if let Some(Value::Array(values)) = schema.get_mut("enum") {
        values.push(Value::Null);
    }
    schema
}

/// The JSON type name of `v` as JSON Schema names it (`integer` for whole
/// numbers).
fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_matches(want: &str, v: &Value) -> bool {
    let got = type_of(v);
    got == want || (want == "number" && got == "integer")
}

fn at(path: &str) -> String {
    if path.is_empty() {
        "arguments".into()
    } else {
        path.to_string()
    }
}

/// Checks `v` against `schema` (the subset above). The error names the
/// value, like `lines[2].rate: expected string or number, got boolean`.
pub fn validate(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
    if let Some(t) = schema.get("type") {
        let wanted: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !wanted.is_empty() && !wanted.iter().any(|w| type_matches(w, v)) {
            return Err(format!(
                "{}: expected {}, got {}",
                at(path),
                wanted.join(" or "),
                type_of(v)
            ));
        }
    }
    if let Some(Value::Array(values)) = schema.get("enum") {
        if !values.contains(v) {
            let names: Vec<String> = values.iter().map(Value::to_string).collect();
            return Err(format!("{}: must be one of {}", at(path), names.join(", ")));
        }
    }
    match v {
        Value::String(s) => {
            let n = s.chars().count();
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if n as u64 > max {
                    return Err(format!("{}: longer than {max} characters ({n})", at(path)));
                }
            }
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if (n as u64) < min {
                    return Err(format!("{}: can't be empty", at(path)));
                }
            }
        }
        Value::Number(num) => {
            if let (Some(min), Some(x)) =
                (schema.get("minimum").and_then(Value::as_f64), num.as_f64())
            {
                if x < min {
                    return Err(format!("{}: must be at least {min}", at(path)));
                }
            }
            if let (Some(max), Some(x)) =
                (schema.get("maximum").and_then(Value::as_f64), num.as_f64())
            {
                if x > max {
                    return Err(format!("{}: must be at most {max}", at(path)));
                }
            }
        }
        Value::Array(items) => {
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                if items.len() as u64 > max {
                    return Err(format!(
                        "{}: more than {max} items ({})",
                        at(path),
                        items.len()
                    ));
                }
            }
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                if (items.len() as u64) < min {
                    return Err(format!("{}: needs at least {min} item(s)", at(path)));
                }
            }
            if let Some(item) = schema.get("items") {
                for (i, x) in items.iter().enumerate() {
                    validate(item, x, &format!("{}[{i}]", at(path)))?;
                }
            }
        }
        Value::Object(map) => {
            let props = schema.get("properties").and_then(Value::as_object);
            if let Some(Value::Array(req)) = schema.get("required") {
                for r in req.iter().filter_map(Value::as_str) {
                    if !map.contains_key(r) {
                        return Err(format!("{}: `{r}` is required", at(path)));
                    }
                }
            }
            for (k, x) in map {
                let sub = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match props.and_then(|p| p.get(k)) {
                    Some(s) => validate(s, x, &sub)?,
                    None => {
                        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                            let known: Vec<&str> = props
                                .map(|p| p.keys().map(String::as_str).collect())
                                .unwrap_or_default();
                            return Err(format!(
                                "{}: unknown property `{k}` (known: {})",
                                at(path),
                                known.join(", ")
                            ));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Checks that `schema` is well formed in the subset: known keywords with
/// values of the right type, `required` naming declared properties, and
/// every object closed. For the tests.
#[cfg(test)]
pub fn check(schema: &Value, path: &str) -> Result<(), String> {
    const TYPES: &[&str] = &[
        "object", "array", "string", "integer", "number", "boolean", "null",
    ];
    const KEYWORDS: &[&str] = &[
        "type",
        "description",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ];
    let o = schema
        .as_object()
        .ok_or_else(|| format!("{path}: a schema must be an object"))?;
    for k in o.keys() {
        if !KEYWORDS.contains(&k.as_str()) {
            return Err(format!("{path}: unknown keyword `{k}`"));
        }
    }
    let types: Vec<&str> = match o.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) if !ts.is_empty() => ts
            .iter()
            .map(|t| {
                t.as_str()
                    .ok_or_else(|| format!("{path}: type names are strings"))
            })
            .collect::<Result<_, _>>()?,
        _ => return Err(format!("{path}: every schema here names its type")),
    };
    for t in &types {
        if !TYPES.contains(t) {
            return Err(format!("{path}: unknown type `{t}`"));
        }
    }
    if let Some(d) = o.get("description") {
        if !d.is_string() {
            return Err(format!("{path}: description must be a string"));
        }
    }
    for k in ["minimum", "maximum"] {
        if o.get(k).is_some_and(|v| !v.is_number()) {
            return Err(format!("{path}: {k} must be a number"));
        }
    }
    for k in ["minLength", "maxLength", "minItems", "maxItems"] {
        if o.get(k).is_some_and(|v| v.as_u64().is_none()) {
            return Err(format!("{path}: {k} must be a whole number"));
        }
    }
    if let Some(e) = o.get("enum") {
        let values = e
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| format!("{path}: enum must be a list"))?;
        for v in values {
            if !types.iter().any(|t| type_matches(t, v)) {
                return Err(format!(
                    "{path}: enum value {v} is not of the schema's type"
                ));
            }
        }
    }
    if types.contains(&"object") {
        let props = o
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("{path}: an object schema lists its properties"))?;
        if o.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(format!(
                "{path}: objects here are closed (additionalProperties: false)"
            ));
        }
        for (k, s) in props {
            check(s, &format!("{path}.{k}"))?;
        }
        if let Some(r) = o.get("required") {
            let r = r
                .as_array()
                .ok_or_else(|| format!("{path}: required must be a list"))?;
            for name in r {
                let name = name
                    .as_str()
                    .ok_or_else(|| format!("{path}: required names are strings"))?;
                if !props.contains_key(name) {
                    return Err(format!("{path}: `{name}` is required but not a property"));
                }
            }
        }
    } else if o.contains_key("properties") || o.contains_key("required") {
        return Err(format!("{path}: properties belong to object schemas"));
    }
    if types.contains(&"array") {
        let items = o
            .get("items")
            .ok_or_else(|| format!("{path}: an array schema says what its items are"))?;
        check(items, &format!("{path}[]"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        object(
            vec![
                ("path", path("A file.")),
                ("count", integer("How many.", 1, 10)),
                ("rate", formula("A formula.")),
                ("kind", nullable(one_of("Kind.", &["a", "b"]))),
                (
                    "rows",
                    array(
                        "Rows.",
                        object(vec![("id", integer("Row id.", 1, i64::MAX))], &["id"]),
                        2,
                    ),
                ),
            ],
            &["path"],
        )
    }

    #[test]
    fn builders_make_checked_schemas() {
        check(&sample(), "sample").unwrap();
        assert!(check(&json!({"type": "object"}), "x").is_err());
        assert!(check(&json!({"type": "string", "format": "uri"}), "x").is_err());
        assert!(check(&json!({"type": "array"}), "x").is_err());
        assert!(check(
            &json!({"type": "object", "properties": {}, "required": ["a"], "additionalProperties": false}),
            "x"
        )
        .is_err());
        assert!(check(&json!({"type": "string", "enum": [1]}), "x").is_err());
    }

    #[test]
    fn validates_arguments() {
        let s = sample();
        let ok =
            json!({"path": "a.mthd", "count": 3, "rate": 12.5, "kind": null, "rows": [{"id": 4}]});
        validate(&s, &ok, "").unwrap();
        let cases = [
            (json!({}), "`path` is required"),
            (json!({"path": ""}), "path: can't be empty"),
            (
                json!({"path": "a", "count": 0}),
                "count: must be at least 1",
            ),
            (
                json!({"path": "a", "count": 2.5}),
                "count: expected integer, got number",
            ),
            (
                json!({"path": "a", "rate": true}),
                "rate: expected string or number",
            ),
            (json!({"path": "a", "kind": "c"}), "kind: must be one of"),
            (
                json!({"path": "a", "rows": [{}, {}, {}]}),
                "rows: more than 2 items",
            ),
            (
                json!({"path": "a", "rows": [{"id": "x"}]}),
                "rows[0].id: expected integer",
            ),
            (json!({"path": "a", "nope": 1}), "unknown property `nope`"),
            (json!([1]), "arguments: expected object, got array"),
        ];
        for (v, want) in cases {
            let e = validate(&s, &v, "").unwrap_err();
            assert!(e.contains(want), "{v}: {e}");
        }
    }
}
