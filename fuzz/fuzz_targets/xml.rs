//! The XML tokenizer (import/xml.rs): every event read until the end or an
//! error, with attribute lookups and entity expansion along the way.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::xml::{attr, decode, unescape, Event, Reader};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = decode(data) else {
        return;
    };
    let _ = unescape(&text);
    let mut r = Reader::new(&text);
    // Every call consumes input or ends, so this is bounded; the cap only
    // turns a reader that stops advancing into a failure instead of a hang.
    for _ in 0..=data.len() * 2 + 2 {
        match r.next() {
            Ok(Some(Event::Start { attrs, .. })) => {
                let _ = attr(&attrs, "r");
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return,
        }
    }
    panic!("the XML reader kept giving events without reaching the end");
});
