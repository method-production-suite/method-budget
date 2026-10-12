//! File > Import Chart of Accounts… and the bundled chart templates
//! (src/coa_import.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error, FileIn};
use crate::coa_import::{self as coa, CoaSources, HostCharts, Imported, Request};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file}`: the chart comes in as `file` (the desktop shows an
        // Open dialog): a workbook, CSV, tab-delimited text or a budget.
        // What the dialog starts from, kept by the reply's token.
        "coa_import_pick" => (|| {
            let a: Pick = d.args()?;
            let sources = d.kept::<CoaSources>();
            json(coa::coa_import_pick(&sources, a.file.name, &a.file.bytes)?)
        })(),
        "coa_import_preview" => (|| {
            let a: Choices = d.args()?;
            let b = d.lock()?.budget().clone();
            let sources = d.kept::<CoaSources>();
            json(coa::coa_import_preview(&b, &sources, a.token, &a.request)?)
        })(),
        "coa_import" => (|| {
            let a: Choices = d.args()?;
            let sources = d.kept::<CoaSources>();
            let (id, message) = coa::coa_import(d.state(), &sources, d.doc(), a.token, &a.request)?;
            let view = d.state().lock_doc(Some(id))?.finish(None);
            json(Imported { view, message })
        })(),
        // The bundled charts, then any the host supplies itself
        // (`Host::set_charts`, `Host::add_chart`: the browser's).
        "chart_templates" => json(coa::chart_templates_with(&d.kept::<HostCharts>())),
        "chart_template_new" => (|| {
            let a: Id = d.args()?;
            let charts = d.kept::<HostCharts>();
            let id = coa::chart_template_new_with(d.state(), &charts, &a.id)?;
            json(d.state().lock_doc(Some(id))?.finish(None))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Pick {
    file: FileIn,
}

#[derive(Deserialize)]
struct Choices {
    token: u64,
    request: Request,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{call, file};
    use crate::Host;

    const CSV: &[u8] = b"Account,Description\n,ABOVE THE LINE\n1100,STORY\n1101,Rights\n,PRODUCTION\n2000,STAFF\n2001,Manager\n";

    #[test]
    fn a_picked_chart_opens_in_a_new_tab_and_bundled_charts_too() {
        let h = Host::new();
        let p = call(
            &h,
            "coa_import_pick",
            json!({"file": file("chart.csv", CSV)}),
        );
        assert_eq!(p["kind"], "csv");
        let request = json!({"detect_header": true, "guess": true, "default_levels": true});
        let pv = call(
            &h,
            "coa_import_preview",
            json!({"token": p["token"], "request": request}),
        );
        assert_eq!(pv["categories"], 2);
        let done = call(
            &h,
            "coa_import",
            json!({"token": p["token"], "request": request}),
        );
        assert!(done["message"].as_str().unwrap().contains("2 categories"));
        assert_eq!(done["view"]["budget"]["title"], "Untitled");
        assert_eq!(h.state().docs().len(), 2);

        #[cfg(feature = "bundled-charts")]
        {
            let charts = call(&h, "chart_templates", json!({}));
            let id = charts[0]["id"].clone();
            let v = call(&h, "chart_template_new", json!({"id": id}));
            assert_eq!(v["budget"]["title"], "Untitled");
            assert_eq!(h.state().docs().len(), 3);
        }
        assert!(h
            .dispatch("chart_template_new", json!({"id": "nothing"}))
            .is_err());
    }

    /// A browser build has no bundled charts: the host lists its own and
    /// hands one in when it is picked.
    #[test]
    fn a_host_lists_and_supplies_its_own_charts() {
        let h = Host::new();
        let before = call(&h, "chart_templates", json!({}))
            .as_array()
            .unwrap()
            .len();
        // As the browser's charts/index.json has them: extra fields are
        // ignored.
        let index = json!([{"id": "own", "name": "Own chart", "group": "Other",
            "description": "Ours", "categories": 4, "accounts": 9}]);
        h.set_charts(serde_json::from_value(index).unwrap());
        let charts = call(&h, "chart_templates", json!({}));
        assert_eq!(charts.as_array().unwrap().len(), before + 1);
        assert_eq!(
            charts.as_array().unwrap().last().unwrap(),
            &json!({"id": "own", "name": "Own chart", "group": "Other", "description": "Ours"})
        );
        let refused = h
            .dispatch("chart_template_new", json!({"id": "own"}))
            .unwrap_err();
        assert!(refused.message().contains("hasn't loaded yet"));
        let text = method_format::to_string(&method_core::starter::budget());
        assert!(h.add_chart("own", "{").is_err());
        h.add_chart("own", &text).unwrap();
        let v = call(&h, "chart_template_new", json!({"id": "own"}));
        assert_eq!(v["budget"]["title"], "Untitled");
    }
}
