//! Copying and pasting whole rows' commands (src/clipboard.rs), with the
//! host's one row clipboard, shared by every budget it has open.

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::clipboard::{self as c, ClipboardState, Pick, Resolution, Target};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    let clip = || d.kept::<ClipboardState>();
    Some(match d.command() {
        "clip_copy" => copy(d, &clip()),
        "clip_cut" => {
            d.view_with(|s, a: Copy| c::clip_cut(s, &clip(), &a.pick, &a.text, a.on_clipboard))
        }
        "clip_preview" => preview(d, &clip()),
        "clip_paste" => paste(d, &clip()),
        "clip_forget" => {
            c::clip_forget(&clip());
            json(())
        }
        _ => return None,
    })
}

fn copy(d: &Call<'_>, clip: &ClipboardState) -> Result<Value, Error> {
    let a: Copy = d.args()?;
    let s = d.lock()?;
    json(c::clip_copy(&s, clip, &a.pick, &a.text, a.on_clipboard)?)
}

fn preview(d: &Call<'_>, clip: &ClipboardState) -> Result<Value, Error> {
    let a: Preview = d.args()?;
    json(c::clip_preview(
        d.state(),
        clip,
        d.doc(),
        a.text.as_deref(),
    )?)
}

/// As the desktop replies: the view, noting the first pasted row.
fn paste(d: &Call<'_>, clip: &ClipboardState) -> Result<Value, Error> {
    let a: Paste = d.args()?;
    let mut s = d.lock()?;
    let first = c::clip_paste(&mut s, clip, a.token, &a.target, &a.resolutions)?;
    json(s.finish(first))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Copy {
    pick: Pick,
    text: String,
    on_clipboard: bool,
}

#[derive(Deserialize)]
struct Preview {
    text: Option<String>,
}

#[derive(Deserialize)]
struct Paste {
    token: u64,
    target: Target,
    resolutions: Vec<Resolution>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::Host;

    #[test]
    fn rows_copy_and_paste_through_json_as_the_page_sends_them() {
        let h = Host::new();
        let v = h.dispatch("get_view", json!({})).unwrap();
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let v = h
            .dispatch("add_detail", json!({"accountCode": code, "kind": "line"}))
            .unwrap();
        let row = v["created_row"].clone();
        let pick = json!({"what": "rows", "rowIds": [row]});
        let info = h
            .dispatch(
                "clip_copy",
                json!({"pick": pick, "text": "Copied\n", "onClipboard": true}),
            )
            .unwrap();
        assert_eq!(
            (info["what"].clone(), info["count"].clone()),
            (json!("rows"), json!(1))
        );

        // Other text on the clipboard: not the clip's, a cell paste.
        let other = h.dispatch("clip_preview", json!({"text": "else"})).unwrap();
        assert_eq!(other, json!(null));
        // That forgot the clip; copy again and paste it.
        let info = h
            .dispatch(
                "clip_copy",
                json!({"pick": pick, "text": "Copied", "onClipboard": true}),
            )
            .unwrap();
        let review = h
            .dispatch("clip_preview", json!({"text": "Copied\r\n"}))
            .unwrap();
        assert_eq!(review["count"], 1);
        let v = h
            .dispatch(
                "clip_paste",
                json!({"token": info["token"], "target": {"account": code}, "resolutions": []}),
            )
            .unwrap();
        assert!(v["created_row"].as_u64().unwrap() > row.as_u64().unwrap());
        let steps = h.dispatch("history_list", json!({})).unwrap();
        assert_eq!(steps["steps"][1]["label"], "Paste 1 row");

        let v = h
            .dispatch(
                "clip_cut",
                json!({"pick": pick, "text": "", "onClipboard": false}),
            )
            .unwrap();
        assert_eq!(v["can_undo"], true);
        let e = h
            .dispatch(
                "clip_paste",
                json!({"token": info["token"], "target": {"account": code}, "resolutions": []}),
            )
            .unwrap_err();
        assert_eq!(e.message(), "Something else was copied since. Paste again.");
        assert_eq!(h.dispatch("clip_forget", json!({})).unwrap(), json!(null));
        assert_eq!(
            h.dispatch("clip_preview", json!({"text": null})).unwrap(),
            json!(null)
        );
    }
}
