//! Delimited text (import/csvread.rs): decoding, delimiter sniffing and
//! parsing. The first byte picks the delimiter hint.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::csvread::{read_csv, Delimiter};

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let hint = match pick % 4 {
        0 => None,
        1 => Some(Delimiter::Comma),
        2 => Some(Delimiter::Tab),
        _ => Some(Delimiter::Semicolon),
    };
    let _ = read_csv(rest, "fuzz", hint);
});
