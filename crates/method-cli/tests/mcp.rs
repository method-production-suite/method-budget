//! `method mcp` end to end: the binary spawned as an MCP client would spawn
//! it, driven over stdin and stdout one JSON-RPC message per line, building
//! a small budget from nothing and checking its totals to the cent
//! (docs/MCP.md).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use method_format::serde_json::{self, json, Value};

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Session {
    fn start(root: &Path, extra: &[&str]) -> Session {
        let mut child = Command::new(env!("CARGO_BIN_EXE_method"))
            .arg("mcp")
            .arg("--root")
            .arg(root)
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("method mcp starts");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Session {
            child,
            stdin,
            stdout,
            id: 0,
        }
    }

    fn send(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).unwrap();
        line.push('\n');
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(line.ends_with('\n'), "one message per line: {line:?}");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {line}"))
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let id = self.id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let r = self.read();
        assert_eq!(r["id"], json!(id), "{r}");
        r
    }

    fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        let res = &r["result"];
        if res["isError"] == json!(true) {
            Err(res["content"][0]["text"].as_str().unwrap().to_string())
        } else {
            Ok(res["structuredContent"].clone())
        }
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        self.call(name, args)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// Closes stdin; the server must exit cleanly.
    fn finish(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{status}");
    }
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("method-mcp-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_agent_builds_a_budget_over_stdio() {
    let dir = temp("build");
    let mut s = Session::start(&dir, &[]);
    let init = s.rpc(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "e2e", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "method-budget");
    s.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let list = s.rpc("tools/list", json!({}));
    assert!(list["result"]["tools"].as_array().unwrap().len() >= 40);

    // An empty budget, its setup, two categories under each production total.
    let p = "short.mthd";
    s.ok(
        "new_budget",
        json!({"path": p, "from": "blank", "title": "Short film"}),
    );
    s.ok(
        "set_global",
        json!({"path": p, "name": "SHOOT_DAYS", "formula": 10}),
    );
    s.ok("add_setup", json!({"path": p, "type": "fringe", "name": "FICA", "fields": {"rate": "7.65", "description": "Social security and Medicare"}}));
    s.ok(
        "add_category",
        json!({"path": p, "code": "10", "name": "Story"}),
    );
    s.ok(
        "add_category",
        json!({"path": p, "code": "11", "name": "Cast"}),
    );
    s.ok(
        "add_production_total",
        json!({"path": p, "name": "Total above-the-line"}),
    );
    s.ok(
        "add_category",
        json!({"path": p, "code": "20", "name": "Camera"}),
    );
    s.ok(
        "add_production_total",
        json!({"path": p, "name": "Total below-the-line"}),
    );
    s.ok(
        "add_account",
        json!({"path": p, "category": "10", "code": "10-01", "name": "Writer"}),
    );
    s.ok(
        "add_account",
        json!({"path": p, "category": "11", "code": "11-01", "name": "Lead"}),
    );
    s.ok(
        "add_account",
        json!({"path": p, "category": "20", "code": "20-01", "name": "Camera crew"}),
    );
    // Units are in a blank budget's table; a unit that isn't is refused.
    let e = s
        .call("add_lines", json!({"path": p, "account": "10-01", "lines": [{"description": "Script", "amount": 1, "unit": "Draft", "rate": 10000}]}))
        .unwrap_err();
    assert!(e.contains("no unit Draft"), "{e}");
    s.ok("add_lines", json!({"path": p, "account": "10-01", "lines": [{"description": "Script", "amount": 1, "unit": "Flat", "rate": 10000}]}));
    s.ok("add_lines", json!({"path": p, "account": "11-01", "lines": [{"description": "Lead actor", "amount": "SHOOT_DAYS", "unit": "Day", "rate": 800, "fringes": ["FICA"]}]}));
    let added = s.ok(
        "add_lines",
        json!({"path": p, "account": "20-01", "lines": [
            {"description": "Director of photography", "amount": "SHOOT_DAYS", "unit": "Day", "rate": 600, "fringes": ["FICA"]},
            {"description": "Camera package", "amount": 1, "unit": "Flat", "rate": "5000"}
        ]}),
    );
    let package = added["added"][1]["id"].as_u64().unwrap();
    // The package copied FICA from the line above (the budget's new-line
    // default); it is gear, so take it off.
    let cam = s.ok("get_account", json!({"path": p, "account": "20-01"}));
    assert_eq!(cam["rows"][1]["fringes"], json!(["FICA"]));
    s.ok(
        "tag_lines",
        json!({"path": p, "rows": [package], "remove_fringes": ["FICA"]}),
    );
    s.ok(
        "set_charge",
        json!({"path": p, "name": "Contingency", "charge_kind": "percent", "percent": 10}),
    );

    // Writer 10,000.00. Lead 10 days x 800.00 = 8,000.00, FICA 612.00.
    // ATL 18,000.00. DP 6,000.00, FICA 459.00, package 5,000.00: BTL
    // 11,000.00. A blank budget posts fringes by budget (decisions.md,
    // entry 60): the fringe row is 1,071.00. 30,071.00 plus 10%
    // contingency: 33,078.10.
    let t = s.ok("totals", json!({"path": p}));
    assert_eq!(t["total_atl_cents"], 1_800_000, "{t}");
    assert_eq!(t["total_btl_cents"], 1_100_000);
    assert_eq!(t["atl_btl_cents"], 2_900_000);
    assert_eq!(t["budget_fringes_cents"], 107_100);
    assert_eq!(t["charges"][0]["amount_cents"], 300_710);
    assert_eq!(t["grand_total_cents"], 3_307_810);
    assert_eq!(t["net_total_cents"], 3_307_810);
    let ex = s.ok("explain", json!({"path": p, "total": "atl"}));
    assert_eq!(ex["reported_cents"], 1_800_000);
    let ex = s.ok("explain", json!({"path": p, "total": "fringe:FICA"}));
    assert_eq!(ex["reported_cents"], 107_100);
    assert_eq!(s.ok("validate", json!({"path": p}))["ok"], true);

    // The file is a valid budget the command line reads the same way.
    let file = dir.join(p);
    let b = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(b.title, "Short film");
    let r = method_core::recalc(&b, &method_core::RecalcOptions::default()).unwrap();
    assert_eq!(r.net_total, 3_307_810);

    // Undo takes back the last change (the charge).
    let u = s.ok("undo", json!({"path": p}));
    assert_eq!(u["undone"], "Set charge Contingency");
    assert_eq!(
        s.ok("totals", json!({"path": p}))["grand_total_cents"],
        3_007_100
    );

    // Someone else saves the file: the next change is refused until the
    // file is opened again.
    let mut theirs = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    theirs.title = "Short film (producer's notes)".into();
    std::fs::write(&file, method_format::to_string(&theirs)).unwrap();
    let e = s
        .call(
            "set_global",
            json!({"path": p, "name": "SHOOT_DAYS", "formula": 12}),
        )
        .unwrap_err();
    assert!(e.contains("changed on disk"), "{e}");
    let reopened = s.ok("budget_open", json!({"path": p}));
    assert_eq!(reopened["reloaded"], true);
    assert_eq!(reopened["title"], "Short film (producer's notes)");
    let r = s.ok(
        "set_global",
        json!({"path": p, "name": "SHOOT_DAYS", "formula": 12}),
    );
    // Two more days of the lead and the DP, with FICA: 2 x 1,400.00 x 1.0765.
    let before = r["totals"]["before"]["net_total_cents"].as_i64().unwrap();
    let after = r["totals"]["after"]["net_total_cents"].as_i64().unwrap();
    assert_eq!(after - before, 301_420);

    // Paths outside the root are refused.
    let e = s
        .call("budget_info", json!({"path": "../elsewhere.mthd"}))
        .unwrap_err();
    assert!(e.contains("outside") || e.contains("no such file"), "{e}");

    // Garbage gets a parse error and the session goes on.
    s.stdin
        .as_mut()
        .unwrap()
        .write_all(b"this is not json\n")
        .unwrap();
    let r = s.read();
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(s.rpc("ping", json!({}))["result"], json!({}));
    s.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_only_and_modern_requests_over_stdio() {
    let dir = temp("readonly");
    std::fs::write(
        dir.join("film.mthd"),
        method_format::to_string(&method_core::starter::budget()),
    )
    .unwrap();
    let before = std::fs::read(dir.join("film.mthd")).unwrap();
    let mut s = Session::start(&dir, &["--read-only"]);
    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}});
    let d = s.rpc("server/discover", json!({"_meta": meta}));
    assert_eq!(d["result"]["resultType"], "complete");
    let list = s.rpc("tools/list", json!({"_meta": meta}));
    let tools = list["result"]["tools"].as_array().unwrap();
    assert!(tools
        .iter()
        .all(|t| t["annotations"]["readOnlyHint"] == json!(true)));
    let r = s.rpc(
        "tools/call",
        json!({"_meta": meta, "name": "totals", "arguments": {"path": "film.mthd"}}),
    );
    assert_eq!(
        r["result"]["structuredContent"]["net_total_cents"],
        57_860_000
    );
    let r = s.rpc("tools/call", json!({"_meta": meta, "name": "set_title", "arguments": {"path": "film.mthd", "title": "x"}}));
    assert_eq!(r["error"]["code"], -32602);
    s.finish();
    assert_eq!(std::fs::read(dir.join("film.mthd")).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}
