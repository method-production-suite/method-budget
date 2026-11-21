//! The shared folder's files, as another computer could write them: a
//! segment (taken into a log, so the change rules run on it), change lines,
//! a heartbeat and `budget.json` (log.rs). The first byte picks which.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::fuzzing::read_shared_folder_file;

fuzz_target!(|data: &[u8]| {
    read_shared_folder_file(data);
});
