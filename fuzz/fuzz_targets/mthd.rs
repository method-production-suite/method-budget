//! A native `.mthd` file (`method_format::from_str`, version deltas
//! included). Whatever reads must write and read back to the same budget.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::{from_str, to_string};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(budget) = from_str(text) else {
        return;
    };
    let written = to_string(&budget);
    let again = from_str(&written).expect("a budget written back reads again");
    assert_eq!(again, budget, "a budget changed through a write and a read");
});
