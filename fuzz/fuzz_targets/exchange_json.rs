//! The budget exchange JSON reader (import/exchange/json.rs), then the
//! budget built from it and the reconciliation, as `method-budget import` does.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::exchange::{import, ExportFormat};

fuzz_target!(|data: &[u8]| {
    let _ = import(data, "fuzz.json", Some(ExportFormat::Json));
});
