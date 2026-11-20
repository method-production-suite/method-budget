//! A schedule days file (import/schedule.rs), as linking a schedule reads it.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::schedule::read;

fuzz_target!(|data: &[u8]| {
    let _ = read(data);
});
