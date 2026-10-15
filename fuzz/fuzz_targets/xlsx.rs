//! A whole Excel workbook (import/xlsxread.rs), through the ZIP, inflate
//! and XML readers under it.
#![no_main]

use libfuzzer_sys::fuzz_target;
use method_format::import::xlsxread::read_xlsx;

fuzz_target!(|data: &[u8]| {
    let _ = read_xlsx(data);
});
