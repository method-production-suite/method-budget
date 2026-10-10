//! Files that must be rejected, and the errors they produce.

use fwb_format::{from_str, FormatError, FORMAT_VERSION};

const SMALL: &str = include_str!("golden/small.json");

/// The golden file with one exact substring replaced (which must exist).
fn edit(from: &str, to: &str) -> String {
    assert!(SMALL.contains(from), "fixture lacks {from:?}");
    SMALL.replacen(from, to, 1)
}

/// The error for `text`, which must be `Invalid`; returns (line, path, message).
fn invalid(text: &str) -> (usize, String, String) {
    match from_str(text) {
        Err(FormatError::Invalid {
            line,
            path,
            message,
            ..
        }) => (line, path, message),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

fn line_of(text: &str, needle: &str) -> usize {
    text.lines().position(|l| l.contains(needle)).unwrap() + 1
}

#[test]
fn golden_file_reads() {
    assert!(from_str(SMALL).is_ok());
}

#[test]
fn newer_version_is_rejected() {
    let text = edit("\"version\": 1", "\"version\": 2");
    let err = from_str(&text).unwrap_err();
    assert_eq!(
        err,
        FormatError::NewerVersion {
            found: 2,
            supported: FORMAT_VERSION
        }
    );
    assert!(err.to_string().contains("version 2"));
}

#[test]
fn newer_version_wins_over_unknown_fields() {
    // A future file may add fields; the version error is the useful one.
    let text = edit("\"version\": 1", "\"version\": 7").replace("\"title\"", "\"name_v7\"");
    assert!(matches!(
        from_str(&text),
        Err(FormatError::NewerVersion { found: 7, .. })
    ));
}

#[test]
fn bad_versions_are_rejected() {
    for v in ["0", "-1", "1.0", "\"1\""] {
        let text = edit("\"version\": 1", &format!("\"version\": {v}"));
        assert_eq!(
            from_str(&text),
            Err(FormatError::BadVersion {
                found: Some(v.into())
            }),
            "{v}"
        );
    }
    let text = edit("  \"version\": 1,\n", "");
    let err = from_str(&text).unwrap_err();
    assert_eq!(err, FormatError::BadVersion { found: None });
    assert!(err.to_string().contains("missing \"version\""));
    // JSON null counts as missing.
    let text = edit("\"version\": 1", "\"version\": null");
    assert_eq!(
        from_str(&text),
        Err(FormatError::BadVersion { found: None })
    );
}

#[test]
fn wrong_or_missing_format_is_rejected() {
    let text = edit("\"method-budget\"", "\"some-other-app\"");
    let err = from_str(&text).unwrap_err();
    assert_eq!(
        err,
        FormatError::NotABudget {
            found: Some("\"some-other-app\"".into())
        }
    );
    assert!(err.to_string().contains("not a Method Budget file"));

    assert_eq!(
        from_str("{\"title\": \"x\"}"),
        Err(FormatError::NotABudget { found: None })
    );
}

#[test]
fn both_format_identifiers_are_read_and_the_new_one_is_written() {
    // Files from before the rename carry the legacy identifier; they still
    // read, and saving writes the new one.
    let original = edit("\"method-budget\"", "\"method-budget\"");
    let legacy = edit("\"method-budget\"", "\"film-wizard-budget\"");
    let b = from_str(&legacy).expect("the legacy identifier reads");
    assert_eq!(b, from_str(&original).unwrap());
    assert!(fwb_format::to_string(&b).contains("\"format\": \"method-budget\""));
    assert_eq!(
        fwb_format::FORMAT_NAMES,
        ["method-budget", "film-wizard-budget"]
    );
}

#[test]
fn not_json_reports_line_and_column() {
    let text = edit("\"name\": \"Camera\",", "\"name\": \"Camera\",,");
    let (line, _, message) = invalid(&text);
    assert_eq!(line, line_of(&text, "\"Camera\",,"));
    assert!(
        !message.contains(" at line "),
        "position not repeated: {message}"
    );

    let (line, path, _) = invalid("[1, 2]");
    assert_eq!((line, path.as_str()), (1, ""));
    assert!(matches!(from_str(""), Err(FormatError::Invalid { .. })));
}

#[test]
fn unknown_field_is_rejected_with_its_path() {
    let text = edit("\"amount\": \"DAYS\"", "\"amout\": \"DAYS\"");
    let (line, path, message) = invalid(&text);
    assert_eq!(line, line_of(&text, "\"amout\""));
    assert_eq!(path, "budget.topsheet[2].accounts[0].details[0].amout");
    assert!(message.contains("unknown field `amout`"), "{message}");

    let err = from_str(&text).unwrap_err().to_string();
    assert!(err.starts_with(&format!("line {line}, column ")), "{err}");
    assert!(
        err.contains("(budget.topsheet[2].accounts[0].details[0].amout)"),
        "{err}"
    );
}

#[test]
fn unknown_top_level_and_budget_fields_are_rejected() {
    let text = edit("\"version\": 1,", "\"version\": 1,\n  \"extra\": true,");
    let (_, path, message) = invalid(&text);
    assert_eq!(path, "extra");
    assert!(message.contains("unknown field `extra`"));

    let text = edit("\"title\":", "\"titel\":");
    let (_, path, _) = invalid(&text);
    assert_eq!(path, "budget.titel");
}

#[test]
fn unknown_field_inside_tagged_object_is_rejected() {
    let text = edit(
        "\"cutoff\": 15000000",
        "\"cutoff\": 15000000,\n        \"unit\": \"Day\"",
    );
    let (_, path, message) = invalid(&text);
    assert!(message.contains("unknown field `unit`"), "{message}");
    assert_eq!(path, "budget.fringes.Payroll");
}

#[test]
fn float_where_decimal_string_expected_is_rejected() {
    let text = edit("\"percent\": \"10\"", "\"percent\": 10.5");
    let (line, path, message) = invalid(&text);
    assert!(message.contains("floating point `10.5`"), "{message}");
    assert!(
        message.contains("decimal number written as a string"),
        "{message}"
    );
    // Charges are small tagged objects, located by the object.
    assert_eq!(path, "budget.charges[0]");
    assert!(line >= line_of(&text, "10.5"));

    // In a plain (untagged) object the exact field is reported.
    let text = edit("\"hours\": \"10\"", "\"hours\": 10");
    let (line, path, message) = invalid(&text);
    assert_eq!(line, line_of(&text, "\"hours\": 10"));
    assert_eq!(path, "budget.units.Day.hours");
    assert!(message.contains("integer `10`"), "{message}");
}

#[test]
fn bad_decimal_string_is_rejected() {
    for (bad, why) in [
        ("1.2.3", "not a number"),
        ("1e5", "not a number"),
        ("", "not a number"),
        ("0.0000000001", "more than 9 decimal places"),
        ("99999999999999999999999999999999999999", "number too large"),
    ] {
        let text = edit("\"hours\": \"10\"", &format!("\"hours\": \"{bad}\""));
        let (_, path, message) = invalid(&text);
        assert_eq!(path, "budget.units.Day.hours", "{bad}");
        assert!(
            message.contains(&format!("invalid decimal {bad:?}")),
            "{message}"
        );
        assert!(message.contains(why), "{message}");
    }
}

#[test]
fn money_must_be_an_integer() {
    let text = edit("\"cutoff\": 15000000", "\"cutoff\": 150000.00");
    let (_, _, message) = invalid(&text);
    assert!(message.contains("floating point"), "{message}");
    let text = edit("\"cutoff\": 15000000", "\"cutoff\": \"15000000\"");
    assert!(matches!(from_str(&text), Err(FormatError::Invalid { .. })));
}

#[test]
fn unknown_kinds_are_rejected() {
    let text = edit("\"kind\": \"adjustment\"", "\"kind\": \"adjustmnet\"");
    let (_, path, message) = invalid(&text);
    assert_eq!(path, "budget.topsheet[2].accounts[0].details[1].kind");
    assert!(
        message.contains("unknown variant `adjustmnet`"),
        "{message}"
    );

    let text = edit(
        "\"kind\": \"percent\",\n        \"name\"",
        "\"kind\": \"percentage\",\n        \"name\"",
    );
    let (_, _, message) = invalid(&text);
    assert!(
        message.contains("unknown variant `percentage`"),
        "{message}"
    );
}

#[test]
fn topsheet_row_fields_must_match_kind() {
    let text = edit(
        "\"kind\": \"production_total\",\n        \"name\": \"Total Above-the-Line\"",
        "\"kind\": \"production_total\",\n        \"code\": \"99\",\n        \"name\": \"Total Above-the-Line\"",
    );
    let (_, path, message) = invalid(&text);
    assert_eq!(path, "budget.topsheet[1]");
    assert!(
        message.contains("production_total row has no `code`"),
        "{message}"
    );

    let text = edit("\"code\": \"10\",\n", "");
    let (_, path, message) = invalid(&text);
    assert_eq!(path, "budget.topsheet[0]");
    assert!(message.contains("missing field `code`"), "{message}");
}

#[test]
fn required_fields_are_required() {
    let text = edit("\"id\": 1,\n", "");
    let (_, path, message) = invalid(&text);
    assert!(message.contains("missing field `id`"), "{message}");
    assert_eq!(path, "budget.topsheet[0].accounts[0].details[0]");
}

#[test]
fn bad_separator_is_rejected() {
    let text = edit(
        "\"base_currency\": \"USD\",",
        "\"base_currency\": \"USD\",\n    \"account_coding\": {\"category_width\": 2, \"account_width\": 2, \"sub_width\": 0, \"set_width\": 0, \"separator\": \"--\"},",
    );
    let (_, path, message) = invalid(&text);
    assert_eq!(path, "budget.account_coding.separator");
    assert!(message.contains("invalid separator"), "{message}");
}

#[test]
fn trailing_content_is_rejected() {
    let text = format!("{SMALL}{{}}");
    assert!(matches!(from_str(&text), Err(FormatError::Invalid { .. })));
}

#[test]
fn row_style_fields_are_checked() {
    let at = "\"description\": \"Kit rental\",";
    let with = |extra: &str| edit(at, &format!("{at}\n                \"{extra},"));

    for bad in [
        "indent\": 256",
        "indent\": -1",
        "indent\": 1.0",
        "indent\": \"2\"",
        "bold\": \"yes\"",
        "italic\": 1",
        "color\": 7",
        "note\": [\"a\"]",
    ] {
        let (_, path, _) = invalid(&with(bad));
        let field = bad.split('"').next().unwrap();
        assert_eq!(
            path,
            format!("budget.topsheet[2].accounts[0].details[1].{field}"),
            "{bad}"
        );
    }
    // The file stores what it is given: range and color rules are the app's.
    for ok in ["indent\": 255", "color\": \"not a color\"", "note\": null"] {
        assert!(from_str(&with(ok)).is_ok(), "{ok}");
    }
    let text = edit("\"kind\": \"adjustment\"", "\"kind\": \"separator\"");
    assert!(from_str(&text).is_ok());
}

/// The golden file with one version (a copy of its own budget) appended.
fn with_version(version_fields: &str, inner_extra: &str) -> String {
    let budget_start = SMALL.find("\"budget\": {").unwrap() + "\"budget\": ".len();
    let budget = &SMALL[budget_start..SMALL.rfind('}').unwrap()].trim_end();
    let inner = format!("{}{inner_extra}}}", budget[..budget.len() - 1].trim_end());
    let version = format!("{{{version_fields}\"budget\": {inner}}}");
    format!(
        "{}, \"versions\": [{version}]}}}}\n",
        budget[..budget.len() - 1].trim_end()
    )
    .replacen("{", &format!("{}{{", &SMALL[..budget_start]), 1)
}

#[test]
fn versions_read_and_are_checked() {
    let good = with_version(
        "\"id\": 1, \"name\": \"v\", \"created\": \"2026-10-09\", ",
        "",
    );
    let b = from_str(&good).unwrap_or_else(|e| panic!("{e}\n{good}"));
    assert_eq!(b.versions.len(), 1);
    assert_eq!(b.versions[0].budget.to_budget().versions, vec![]);
    assert_eq!(
        b.versions[0].budget.title, b.title,
        "the version holds the budget's content"
    );

    // A version's budget may not have versions of its own.
    let nested = with_version(
        "\"id\": 1, \"name\": \"v\", \"created\": \"t\", ",
        ", \"versions\": [{\"id\": 2, \"name\": \"w\", \"created\": \"t\", \
         \"budget\": {\"title\": \"x\", \"base_currency\": \"USD\"}}]",
    );
    let (_, path, message) = invalid(&nested);
    assert!(
        message.contains("a version's budget cannot have versions of its own"),
        "{message}"
    );
    assert!(path.starts_with("budget.versions[0]"), "{path}");

    // Unknown fields and missing required fields are errors.
    let unknown = with_version(
        "\"id\": 1, \"name\": \"v\", \"created\": \"t\", \"author\": \"me\", ",
        "",
    );
    let (_, path, message) = invalid(&unknown);
    assert_eq!(path, "budget.versions[0].author");
    assert!(message.contains("unknown field `author`"), "{message}");
    let missing = with_version("\"id\": 1, \"name\": \"v\", ", "");
    let (_, _, message) = invalid(&missing);
    assert!(message.contains("missing field `created`"), "{message}");
    let bad_id = with_version("\"id\": -1, \"name\": \"v\", \"created\": \"t\", ", "");
    invalid(&bad_id);
}
