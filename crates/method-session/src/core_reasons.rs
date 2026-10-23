//! method-core's own reasons, in the engine's catalog (decision 94, sixth
//! pass).
//!
//! method-core also builds for the browser and the command line, and reads
//! no table, so its errors keep writing their English as they always did
//! (`Display` on `EvalError`, `ParseDecError`, `CodingError`). The catalog
//! in [`crate::messages`] holds that English under `engine.core.*`, and the
//! tests here make each error and check it is recognized as its code, so a
//! change to method-core's words fails here instead of quietly showing
//! English. They reach a person inside another message, as its `{why}`
//! (a formula refused, a number that doesn't read, a numbering pattern that
//! can't be used), where the desktop translates them in turn.
//!
//! Not covered, on purpose: a formula's parse errors ("expected `)` at
//! position 4") and a calculation's errors ("account 1100 line 2: divide by
//! zero"). Both end or start with a parameter around a colon or a position,
//! and a template for them would win over the message they sit in
//! ("Amt can't be 1+: ..."), so they stay as method-core writes them.

/// The codes of method-core's reasons in the catalog.
pub const CORE_REASONS: &[&str] = &[
    "engine.core.bad_separator",
    "engine.core.category_width",
    "engine.core.divide_by_zero",
    "engine.core.empty_suffix",
    "engine.core.invalid_char",
    "engine.core.no_account_segment",
    "engine.core.not_a_number",
    "engine.core.number_too_large",
    "engine.core.segment_gap",
    "engine.core.too_many_segments",
    "engine.core.too_precise",
    "engine.core.too_precise_exact",
    "engine.core.unknown_global",
    "engine.core.width_too_large",
    "engine.what.sub_account",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{recognize, recognize_nested};
    use method_core::coding::{CodingError, Segment};
    use method_core::decimal::ParseDecError;
    use method_core::expr::EvalError;

    /// Each of method-core's errors, as it writes it, is recognized as its
    /// code; a segment's name inside it is a noun of the catalog.
    #[test]
    fn method_cores_reasons_are_recognized_as_their_codes() {
        let cases: Vec<(String, &str)> = vec![
            (
                EvalError::UnknownGlobal("RATE".into()).to_string(),
                "engine.core.unknown_global",
            ),
            (
                EvalError::DivideByZero.to_string(),
                "engine.core.divide_by_zero",
            ),
            (
                EvalError::Overflow.to_string(),
                "engine.core.number_too_large",
            ),
            (
                EvalError::TooPrecise.to_string(),
                "engine.core.too_precise_exact",
            ),
            (
                ParseDecError::Invalid.to_string(),
                "engine.core.not_a_number",
            ),
            (
                ParseDecError::TooPrecise.to_string(),
                "engine.core.too_precise",
            ),
            (
                ParseDecError::Overflow.to_string(),
                "engine.core.number_too_large",
            ),
            (
                CodingError::WidthTooLarge {
                    segment: Segment::Sub,
                    width: 9,
                }
                .to_string(),
                "engine.core.width_too_large",
            ),
            (
                CodingError::CategoryRequired.to_string(),
                "engine.core.category_width",
            ),
            (
                CodingError::SegmentGap {
                    segment: Segment::Account,
                }
                .to_string(),
                "engine.core.segment_gap",
            ),
            (
                CodingError::BadSeparator('#').to_string(),
                "engine.core.bad_separator",
            ),
            // The same words as the edits' own message, so the same code.
            (
                CodingError::Empty.to_string(),
                "engine.edit.account_number_empty",
            ),
            (
                CodingError::InvalidChar('#').to_string(),
                "engine.core.invalid_char",
            ),
            (
                CodingError::TooManySegments { max: 2, found: 3 }.to_string(),
                "engine.core.too_many_segments",
            ),
            (
                CodingError::NoAccountSegment.to_string(),
                "engine.core.no_account_segment",
            ),
            (
                CodingError::EmptySuffix.to_string(),
                "engine.core.empty_suffix",
            ),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for (text, code) in &cases {
            let r = recognize(text).unwrap_or_else(|| panic!("{text:?} isn't recognized"));
            assert_eq!(r.code, *code, "{text:?}");
            seen.insert(*code);
        }
        for s in [
            Segment::Category,
            Segment::Account,
            Segment::Sub,
            Segment::Set,
        ] {
            let r = recognize_nested(&s.to_string()).expect("a segment is a noun");
            assert_eq!(r.code.split('.').nth(1), Some("what"), "{s}");
            seen.insert(r.code);
        }
        for code in CORE_REASONS {
            assert!(seen.contains(code), "{code} isn't one of method-core's");
        }
        // Inside a refused edit, the reason is the nested `why`.
        let text = crate::messages::msg(
            "engine.edit.formula_refused",
            &[
                ("what", &"Amt"),
                ("value", &"1/0"),
                ("why", &EvalError::DivideByZero),
            ],
        );
        let r = recognize(&text).unwrap();
        assert_eq!(r.code, "engine.edit.formula_refused");
        assert_eq!(
            recognize(&r.params[2].1).unwrap().code,
            "engine.core.divide_by_zero"
        );
    }
}
