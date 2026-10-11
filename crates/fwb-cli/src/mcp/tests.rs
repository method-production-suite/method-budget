//! Protocol tests: the handshake and version negotiation of both eras,
//! tool listing and schemas, errors for bad messages, read-only mode and
//! cancellation. Tool behavior is tested in `tools/tests.rs`.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

use fwb_format::serde_json::{json, Value};

use super::protocol::{parse_line, read_line, run, Incoming, Scripted, Server, LEGACY, MODERN};
use super::{limits, schema, Config};

/// A server on a fresh folder holding the starter budget as `film.mthd`.
pub struct Fixture {
    pub dir: PathBuf,
    pub server: Server,
    next_id: u64,
}

impl Fixture {
    pub fn new(name: &str, read_only: bool) -> Fixture {
        let dir = std::env::temp_dir().join(format!("method-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::cli::save(
            dir.join("film.mthd").to_str().unwrap(),
            &fwb_core::starter::budget(),
        )
        .unwrap();
        let server = Server::new(&Config {
            roots: vec![dir.clone()],
            read_only,
        })
        .unwrap();
        let mut f = Fixture {
            dir,
            server,
            next_id: 100,
        };
        let r = f.raw(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}));
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        f
    }

    /// Sends one message; the response.
    pub fn raw(&mut self, msg: Value) -> Value {
        self.server.handle(msg).unwrap_or(Value::Null)
    }

    /// A request; its whole response.
    pub fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.raw(json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}))
    }

    /// Calls a tool: its structured result, or its error text.
    pub fn tool(&mut self, name: &str, args: Value) -> Result<Value, String> {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        let result = &r["result"];
        assert!(result.is_object(), "{name}: not a tool result: {r}");
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if result["isError"] == json!(true) {
            assert_eq!(result["structuredContent"]["error"]["message"], json!(text));
            Err(text)
        } else {
            // The JSON is in the content too, for clients without structured results.
            let mirrored: Value =
                fwb_format::serde_json::from_str(result["content"][1]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(mirrored, result["structuredContent"], "{name}");
            Ok(result["structuredContent"].clone())
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn error_code(r: &Value) -> i64 {
    r["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("not an error: {r}"))
}

#[test]
fn legacy_handshake_negotiates_the_version() {
    let mut f = Fixture::new("handshake", false);
    for v in LEGACY {
        let r = f.rpc("initialize", json!({"protocolVersion": v, "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}));
        assert_eq!(r["result"]["protocolVersion"], json!(v));
        assert_eq!(r["result"]["serverInfo"]["name"], "method-budget");
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], false);
        assert!(r["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("never instructions"));
    }
    // An unknown revision gets the newest legacy one.
    for v in ["1999-01-01", MODERN] {
        let r = f.rpc(
            "initialize",
            json!({"protocolVersion": v, "capabilities": {}}),
        );
        assert_eq!(r["result"]["protocolVersion"], json!(LEGACY[0]));
    }
    let r = f.rpc("initialize", json!({"capabilities": {}}));
    assert_eq!(error_code(&r), -32602);
    // The initialized notification gets no answer.
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
        None
    );
    assert_eq!(f.rpc("ping", json!({}))["result"], json!({}));
}

#[test]
fn requests_before_initialize_are_refused_but_ping_works() {
    let dir = std::env::temp_dir().join(format!("method-mcp-preinit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Server::new(&Config {
        roots: vec![dir.clone()],
        read_only: false,
    })
    .unwrap();
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    assert_eq!(error_code(&r), -32602);
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("initialize"));
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))
        .unwrap();
    assert_eq!(r["result"], json!({}));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn modern_requests_carry_their_version() {
    let dir = std::env::temp_dir().join(format!("method-mcp-modern-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Server::new(&Config {
        roots: vec![dir.clone()],
        read_only: false,
    })
    .unwrap();
    let meta = |v: &str| json!({"io.modelcontextprotocol/protocolVersion": v, "io.modelcontextprotocol/clientCapabilities": {}});
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": "d", "method": "server/discover", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["supportedVersions"][0], MODERN);
    assert!(res["supportedVersions"]
        .as_array()
        .unwrap()
        .contains(&json!("2025-06-18")));
    assert_eq!(
        res["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "method-budget"
    );
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    assert!(r["result"]["tools"].as_array().unwrap().len() > 30);
    assert_eq!(r["result"]["resultType"], "complete");

    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"_meta": meta("1900-01-01")}}))
        .unwrap();
    assert_eq!(error_code(&r), -32022);
    assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
    assert!(r["error"]["data"]["supported"]
        .as_array()
        .unwrap()
        .contains(&json!(MODERN)));
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}}))
        .unwrap();
    assert_eq!(error_code(&r), -32602);
    // Without a handshake, a legacy-only method stays unknown in the modern era.
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 5, "method": "resources/list", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    assert_eq!(error_code(&r), -32601);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tools_list_schemas_are_valid_and_names_are_unique() {
    let mut f = Fixture::new("schemas", false);
    let r = f.rpc("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap().clone();
    let mut names = BTreeSet::new();
    for t in &tools {
        let name = t["name"].as_str().unwrap();
        assert!(names.insert(name.to_string()), "{name} listed twice");
        assert!(
            !name.is_empty()
                && name.len() <= 128
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)),
            "{name}"
        );
        assert!(
            t["description"].as_str().is_some_and(|d| d.len() > 20),
            "{name}"
        );
        assert!(t["title"].is_string(), "{name}");
        let schema = &t["inputSchema"];
        schema::check(schema, name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(t["annotations"]["openWorldHint"], false);
        // No em dashes in anything a client shows.
        assert!(!t.to_string().contains('\u{2014}'), "{name}");
    }
    for want in [
        "budget_open",
        "budget_info",
        "list_topsheet",
        "list_accounts",
        "get_account",
        "add_lines",
        "update_lines",
        "delete_lines",
        "move_lines",
        "set_global",
        "list_globals",
        "list_setup",
        "add_setup",
        "update_setup",
        "rename_setup",
        "delete_setup",
        "tag_lines",
        "set_charge",
        "set_credit",
        "apply_charge",
        "totals",
        "explain",
        "find",
        "diff",
        "merge",
        "validate",
        "export",
        "import_sheet",
        "new_budget",
        "save_as",
        "undo",
    ] {
        assert!(names.contains(want), "{want} missing");
    }
    // The order is stable.
    let again = f.rpc("tools/list", json!({}));
    assert_eq!(again["result"]["tools"], json!(tools));
    let r = f.rpc("tools/list", json!({"cursor": "x"}));
    assert_eq!(error_code(&r), -32602);
}

#[test]
fn read_only_hides_every_tool_that_writes() {
    let mut f = Fixture::new("readonly", true);
    let r = f.rpc("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    for t in tools {
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{}", t["name"]);
    }
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for hidden in [
        "add_lines",
        "set_global",
        "export",
        "new_budget",
        "save_as",
        "merge",
        "undo",
        "import_sheet",
    ] {
        assert!(!names.contains(&hidden), "{hidden} listed");
    }
    let before = std::fs::read(f.path("film.mthd")).unwrap();
    let r = f.rpc(
        "tools/call",
        json!({"name": "set_title", "arguments": {"path": "film.mthd", "title": "x"}}),
    );
    assert_eq!(error_code(&r), -32602);
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("read-only"));
    assert_eq!(std::fs::read(f.path("film.mthd")).unwrap(), before);
    assert!(f.tool("budget_info", json!({"path": "film.mthd"})).is_ok());
}

#[test]
fn bad_messages_get_json_rpc_errors() {
    let mut f = Fixture::new("badmsg", false);
    let cases = [
        (json!(42), -32600, Value::Null),
        (json!({"id": 1, "method": "ping"}), -32600, json!(1)),
        (
            json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}),
            -32600,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
            -32600,
            Value::Null,
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1.5, "method": "ping"}),
            -32600,
            Value::Null,
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1, "method": 7}),
            -32600,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": [1]}),
            -32602,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": "x", "method": "nope/never"}),
            -32601,
            json!("x"),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {}}),
            -32602,
            json!(2),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "no_such_tool"}}),
            -32602,
            json!(3),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "totals", "arguments": [1]}}),
            -32602,
            json!(4),
        ),
        (json!([]), -32600, Value::Null),
    ];
    for (msg, code, id) in cases {
        let r = f.raw(msg.clone());
        assert_eq!(error_code(&r), code, "{msg}: {r}");
        assert_eq!(r["id"], id, "{msg}");
        assert_eq!(r["jsonrpc"], "2.0");
    }
    // Notifications and responses get no answer, even unknown ones.
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/whatever"})),
        None
    );
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "id": 9, "result": {}})),
        None
    );
    // A batch answers each request in it.
    let r = f.raw(json!([
        {"jsonrpc": "2.0", "id": 1, "method": "ping"},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "nope"}
    ]));
    assert_eq!(r.as_array().unwrap().len(), 2);
    // Arguments that don't fit the schema are a tool error the model can read.
    let e = f
        .tool("get_account", json!({"path": "film.mthd"}))
        .unwrap_err();
    assert!(e.contains("`account` is required"), "{e}");
    let e = f
        .tool(
            "get_account",
            json!({"path": "film.mthd", "account": "10-01", "bogus": 1}),
        )
        .unwrap_err();
    assert!(e.contains("unknown property `bogus`"), "{e}");
}

#[test]
fn lines_are_framed_and_bounded() {
    let mut input: &[u8] = b"{\"a\":1}\r\n\n   \nnot json\n";
    let mut buf = Vec::new();
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(
        parse_line(&buf, true),
        Some(Incoming::Message(json!({"a": 1})))
    );
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(parse_line(&buf, true), None);
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(parse_line(&buf, true), None);
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert!(
        matches!(parse_line(&buf, true), Some(Incoming::Invalid(e)) if e.starts_with("Parse error"))
    );
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), None);

    // A line over the limit is refused, and the next one still reads.
    let long = format!("{}\n{{\"b\":2}}\n", "x".repeat(50));
    let mut input = std::io::BufReader::with_capacity(8, long.as_bytes());
    assert_eq!(read_line(&mut input, 20, &mut buf).unwrap(), Some(false));
    assert!(
        matches!(parse_line(&buf, false), Some(Incoming::Invalid(e)) if e.contains(&limits::MAX_MESSAGE_BYTES.to_string()))
    );
    assert_eq!(read_line(&mut input, 20, &mut buf).unwrap(), Some(true));
    assert_eq!(
        parse_line(&buf, true),
        Some(Incoming::Message(json!({"b": 2})))
    );
    // A last line without a newline still counts.
    let mut input: &[u8] = b"{\"c\":3}";
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
}

fn run_script(server: Server, msgs: Vec<Incoming>) -> Vec<Value> {
    let mut out = Vec::new();
    run(server, Scripted(VecDeque::from(msgs)), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| fwb_format::serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn the_loop_answers_parse_errors_and_skips_cancelled_requests() {
    let dir = std::env::temp_dir().join(format!("method-mcp-cancel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("film.mthd");
    crate::cli::save(path.to_str().unwrap(), &fwb_core::starter::budget()).unwrap();
    let before = std::fs::read(&path).unwrap();
    let server = Server::new(&Config {
        roots: vec![dir.clone()],
        read_only: false,
    })
    .unwrap();
    let msg = Incoming::Message;
    let out = run_script(
        server,
        vec![
            msg(
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}}}),
            ),
            Incoming::Invalid("Parse error: nope".into()),
            msg(
                json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "set_title", "arguments": {"path": "film.mthd", "title": "Cancelled"}}}),
            ),
            msg(
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 5, "reason": "user"}}),
            ),
            msg(json!({"jsonrpc": "2.0", "id": 6, "method": "ping"})),
            // A cancellation for nothing in flight is ignored.
            msg(
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 99}}),
            ),
            msg(json!({"jsonrpc": "2.0", "id": 99, "method": "ping"})),
        ],
    );
    let ids: Vec<Value> = out.iter().map(|r| r["id"].clone()).collect();
    assert_eq!(
        ids,
        vec![json!(1), Value::Null, json!(6), json!(99)],
        "{out:?}"
    );
    assert_eq!(out[1]["error"]["code"], -32700);
    // The cancelled change never ran.
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn older_revisions_get_text_only_results() {
    let mut f = Fixture::new("oldrev", false);
    let r = f.rpc(
        "initialize",
        json!({"protocolVersion": "2024-11-05", "capabilities": {}}),
    );
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    let r = f.rpc(
        "tools/call",
        json!({"name": "budget_info", "arguments": {"path": "film.mthd"}}),
    );
    assert!(r["result"].get("structuredContent").is_none());
    assert_eq!(r["result"]["content"].as_array().unwrap().len(), 2);
}
