use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn exchange(lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
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
