//! The ZIP reader (import/zipread.rs): the central directory, then every
//! entry read out.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::zipread::ZipArchive;

/// Decompressed bytes allowed per input, below the reader's default so a
/// run stays fast (the cap itself is what's being tested).
const LIMIT: usize = 16 << 20;

fuzz_target!(|data: &[u8]| {
    // The default limit for the directory (nothing is decompressed yet)...
    if ZipArchive::new(data).is_err() {
        return;
    }
    // ...and a smaller one for reading the entries.
    let Ok(mut zip) = ZipArchive::with_limit(data, LIMIT) else {
        panic!("a directory read with one limit failed with another");
    };
    let names: Vec<String> = zip.names().map(str::to_string).collect();
    let mut total = 0usize;
    for name in &names {
        assert!(zip.contains(name));
        if let Ok(Some(bytes)) = zip.read(name) {
            total += bytes.len();
        }
    }
    assert!(
        total <= LIMIT,
        "the ZIP reader gave out more than its limit"
    );
});
