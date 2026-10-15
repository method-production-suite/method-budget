//! The version-delta patcher (delta.rs `apply`) on any JSON document and
//! patch, and the differ's patches applied back.
//!
//! The input is a JSON array `[doc, other]`: `other` is applied to `doc`
//! as a patch, and the patch the differ writes from `doc` to `other` must
//! turn `doc` into `other`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::fuzzing::{apply_patch, diff_then_apply};
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(Value::Array(pair)) = serde_json::from_slice::<Value>(data) else {
        return;
    };
    let Ok([doc, other]) = <[Value; 2]>::try_from(pair) else {
        return;
    };
    let mut patched = doc.clone();
    let _ = apply_patch(&mut patched, other.clone());
    let rebuilt = diff_then_apply(&doc, &other).expect("the differ's own patch applies");
    assert_eq!(
        rebuilt, other,
        "the differ's patch did not rebuild its target"
    );
});
