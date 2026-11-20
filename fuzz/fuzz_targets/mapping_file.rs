//! A saved column mapping file (import/mapping_file.rs), as
//! `method-budget import --mapping` reads it. Only UTF-8 text reaches it.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::mapping_file::parse;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = parse(text);
});
