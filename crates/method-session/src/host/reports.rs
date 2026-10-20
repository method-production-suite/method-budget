//! Report presets (src/reports.rs). Printing a report is the host's own
//! (`print_report` stays the desktop's: a browser prints the page itself).

use serde::Deserialize;
use serde_json::Value;

use super::{Call, Error};
use crate::reports as r;

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "save_report_preset" => {
            d.view_with(|s, a: SavePreset| r::save_report_preset(s, a.index, a.preset))
        }
        "delete_report_preset" => d.view_with(|s, a: Index| r::delete_report_preset(s, a.index)),
        "bid_sheet" => d.args::<Layout>().and_then(|a| {
            let sheet = r::bid_sheet(&mut *d.lock()?, a.layout)?;
            Ok(serde_json::to_value(sheet).expect("a bid sheet always serializes"))
        }),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct SavePreset {
    index: Option<usize>,
    preset: Value,
}

#[derive(Deserialize)]
struct Layout {
    #[serde(default)]
    layout: Option<Value>,
}

#[derive(Deserialize)]
struct Index {
    index: usize,
}
