//! The budget exchange readers other than JSON (import/exchange: xmlread.rs
//! and sheets.rs), then the budget built from it and the reconciliation, as
//! `method-budget import` does. The first byte picks the format.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::exchange::{import, ExportFormat};

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let format = match pick % 4 {
        0 => ExportFormat::XmlBasic,
        1 => ExportFormat::XmlAdvanced,
        2 => ExportFormat::Excel,
        _ => ExportFormat::Tab,
    };
    let _ = import(rest, "fuzz", Some(format));
});
