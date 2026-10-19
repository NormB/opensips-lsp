//! Request cost over real LSP stdio must grow with the size of the
//! document, not with its square or cube.
//!
//! Handlers that produce one item per route, call or diagnostic used
//! to redo a whole-document scan for EACH item: the code lens counted
//! references by rescanning every file once per route, inlay hints
//! re-read the loaded modules once per call, and converting a line
//! number to text walked the document from the top every time.  On a
//! 3000-line config the code lens took about a minute and a half.
//!
//! Each request below must answer within a budget the linear handlers
//! meet with a wide margin on a loaded host.

mod common;
use common::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const N: usize = 3000;
const BUDGET: Duration = Duration::from_secs(5);

fn big_cfg() -> String {
    let mut s = String::from("loadmodule \"tm.so\"\n");
    for i in 0..N {
        s.push_str(&format!(
            "route[r{i}] {{\n    t_relay(\"udp\", 1);\n    xlog(\"$ru {i}\");\n    route(r{});\n}}\n",
            i + 1
        ));
    }
    // r{N} is never defined, so there is exactly one analyzer warning
    s
}

#[test]
fn per_item_requests_scale_with_the_document() {
    let base_scratch = common::Scratch::new("oslsp-scale");
    let base = base_scratch.path().to_path_buf();
    let tree = base.join("tree");
    std::fs::create_dir_all(tree.join("modules/tm")).unwrap();
    std::fs::write(
        tree.join("modules/tm/README.md"),
        "# tm\n\n## Admin Guide\n\n### Exported Functions\n\n\
         #### t_relay([flags],[outbound_proxy])\n\nRelay it.\n",
    )
    .unwrap();
    let cfg_text = big_cfg();
    let cfg = base.join("big.cfg");
    std::fs::write(&cfg, &cfg_text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_SRC", tree.display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &base,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{}, "initializationOptions": {}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "harvest-ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":cfg_text}}}),
    );
    let last = (5 * N) as u32;
    let doc = serde_json::json!({"uri": uri});
    let requests: Vec<(&str, serde_json::Value)> = vec![
        (
            "textDocument/codeLens",
            serde_json::json!({"textDocument": doc}),
        ),
        (
            "textDocument/inlayHint",
            serde_json::json!({"textDocument": doc,
                "range":{"start":{"line":0,"character":0},"end":{"line":last,"character":0}}}),
        ),
        (
            "textDocument/documentSymbol",
            serde_json::json!({"textDocument": doc}),
        ),
        (
            "textDocument/semanticTokens/full",
            serde_json::json!({"textDocument": doc}),
        ),
        (
            "textDocument/foldingRange",
            serde_json::json!({"textDocument": doc}),
        ),
        (
            "textDocument/diagnostic",
            serde_json::json!({"textDocument": doc}),
        ),
        (
            // `route(r1)` on the line of route r0's body
            "textDocument/references",
            serde_json::json!({"textDocument": doc,
                "position":{"line":4,"character":11},
                "context":{"includeDeclaration":true}}),
        ),
    ];
    for (i, (method, params)) in requests.into_iter().enumerate() {
        let id = 100 + i as i64;
        let t = Instant::now();
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        );
        let r = wait_for(&rx, |v| v["id"] == id, method);
        let took = t.elapsed();
        assert!(r.get("error").is_none(), "{method} failed: {r}");
        assert!(
            took < BUDGET,
            "{method} took {took:?} on a {N}-route document (budget {BUDGET:?})"
        );
        // the answers must still be the full answers
        match method {
            "textDocument/codeLens" => {
                let lenses = r["result"].as_array().unwrap();
                assert_eq!(lenses.len(), N);
                assert_eq!(lenses[1]["command"]["title"], "1 reference");
            }
            "textDocument/inlayHint" => {
                // two for t_relay's arguments, one for the core xlog's
                assert_eq!(r["result"].as_array().unwrap().len(), 3 * N);
            }
            "textDocument/references" => {
                assert_eq!(r["result"].as_array().unwrap().len(), 2);
            }
            _ => {}
        }
    }
    let _ = child.kill();
}
