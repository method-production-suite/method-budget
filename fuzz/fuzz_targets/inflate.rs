//! The DEFLATE decoder (import/inflate.rs) on raw bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::inflate::inflate;

/// Output cap: big enough for every seed, small enough to stay fast.
const LIMIT: usize = 4 << 20;

fuzz_target!(|data: &[u8]| {
    if let Ok(out) = inflate(data, LIMIT) {
        assert!(out.len() <= LIMIT, "inflate passed its output cap");
    }
});
