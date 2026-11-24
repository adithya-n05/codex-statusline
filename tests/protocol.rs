mod common;
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn exchange(lines: &[&str]) -> Vec<Value> {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
        .env("CODEX_HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for line in lines {
        writeln!(input, "{line}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

#[test]
fn stdio_handshake_survives_bad_input_and_ignores_notifications() {
    let replies = exchange(&[
        "{broken",
        "null",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":"setup","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"missing"}"#,
    ]);
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["error"]["code"], -32600);
    let hello = &replies[2];
    assert_eq!(hello["id"], "setup");
    assert_eq!(hello["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(hello["result"]["serverInfo"]["name"], "codex-statusline");
    assert_eq!(replies[3], json!({"jsonrpc":"2.0","id":2,"result":{}}));
    assert_eq!(replies[4]["error"]["code"], -32601);
}
#[test]
fn inventory_advertises_one_matching_statusline_resource() {
    let replies = exchange(&[
        r#"{"id":1,"method":"tools/list"}"#,
        r#"{"id":2,"method":"resources/list"}"#,
        r#"{"id":3,"method":"resources/templates/list"}"#,
    ]);
    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    let entry = &tools[0]["_meta"]["openai/ui"]["entrypoints"][0];
    assert_eq!(entry["type"], "statusline");
    let resource = &replies[1]["result"]["resources"][0];
    assert_eq!(entry["resourceUri"], "ui://codex-statusline/footer");
    assert_eq!(entry["resourceUri"], resource["uri"]);
    assert_eq!(resource["mimeType"], "application/json");
    assert_eq!(replies[2]["result"]["resourceTemplates"], json!([]));
}
fn read_request(id: i32, uri: &str, state: Value) -> String {
    json!({"id":id,"method":"resources/read","params":{"uri":uri,"_meta":{"com.openai/codex-statusline-state":state}}}).to_string()
}

#[test]
fn reads_the_displayed_workspace_on_every_resource_request() {
    let uri = "ui://codex-statusline/footer";
    let paths = ["/a/alpha", "/b/日本", "/a/alpha"];
    let mut requests: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            read_request(
                index as i32,
                uri,
                json!({"thread":{"workspace":{"cwd":path}}}),
            )
        })
        .collect();
    requests.push(read_request(3, uri, json!({})));
    requests.push(read_request(4, "ui://other/resource", json!({})));
    let replies = exchange(&requests.iter().map(String::as_str).collect::<Vec<_>>());
    for (message, path) in replies.iter().zip(paths) {
        let resource = &message["result"]["contents"][0];
        assert_eq!(resource["uri"], uri);
        assert_eq!(resource["mimeType"], "application/json");
        let tree: Value = serde_json::from_str(resource["text"].as_str().unwrap()).unwrap();
        assert_eq!(tree["type"], "text");
        let first = common::plain(&tree["children"][0]);
        assert_eq!(first, path);
    }
    assert_eq!(replies[3]["result"]["contents"][0]["text"], "null");
    assert_eq!(replies[4]["error"]["code"], -32602);
}
