use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn rendered(home: &std::path::Path, state: Value, wait_for: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
        .env("CODEX_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let req = json!({"id":1,"method":"resources/read","params":{"uri":"ui://codex-statusline/footer","_meta":{"com.openai/codex-statusline-state":state}}});
    let deadline = Instant::now() + Duration::from_secs(4);
    let tree = loop {
        writeln!(input, "{req}").unwrap();
        input.flush().unwrap();
        let mut s = String::new();
        output.read_line(&mut s).unwrap();
        let msg: Value = serde_json::from_str(&s).unwrap();
        let tree: Value =
            serde_json::from_str(msg["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
        if tree.to_string().contains(wait_for) || Instant::now() >= deadline {
            break tree;
        }
        thread::sleep(Duration::from_millis(15));
    };
    drop(input);
    assert!(child.wait().unwrap().success());
    tree
}
fn write_events(path: &std::path::Path, events: &[Value]) {
    let mut f = fs::File::create(path).unwrap();
    for e in events {
        writeln!(f, "{}", json!({"type":"event_msg","payload":e})).unwrap();
    }
}
fn index(home: &std::path::Path) -> std::path::PathBuf {
    let path = home.join("session.jsonl");
    let db = Connection::open(home.join("state_1.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY, rollout_path TEXT, creator_account_id TEXT, title TEXT,name TEXT);").unwrap();
    db.execute(
        "INSERT INTO threads VALUES('a',?1,'fixture-account','Fixture title',NULL)",
        [path.to_str().unwrap()],
    )
    .unwrap();
    path
}
#[test]
fn last_turn_speed_uses_completed_output_and_closes_the_usage_row() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    fs::write(home.path().join("auth.json"), r#"{"auth_mode":"apikey"}"#).unwrap();
    let state = json!({"thread":{"id":"a","workspace":{"cwd":home.path()}}});
    let count = |n: u64| json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":8000,"output_tokens":n}}});
    let start = |id: &str| json!({"type":"task_started","turn_id":id});
    let end = |id: &str, ms: u64| json!({"type":"task_complete","turn_id":id,"duration_ms":ms});
    let mut events = vec![
        count(100),
        start("first"),
        count(330),
        count(340),
        end("stray", 100),
        end("first", 2000),
        start("second"),
        count(540),
    ];
    write_events(&path, &events);
    let tree = rendered(home.path(), state.clone(), "tok/s");
    let parts = tree["children"].as_array().unwrap();
    let last_break = parts.iter().rposition(|v| v == "\n").unwrap();
    assert_eq!(parts[last_break - 1], "Last 120 tok/s");
    assert_eq!(parts[last_break - 3], "API key");
    for (output, ms) in [(540, 0), (300, 2000)] {
        events.push(count(output));
        events.push(end("second", ms));
        write_events(&path, &events);
        assert!(
            !rendered(home.path(), state.clone(), "Tools")
                .to_string()
                .contains("tok/s")
        );
        events.truncate(8);
    }
}
#[test]
fn compaction_red_tracks_the_displayed_models_actual_scope_at_the_exact_boundary() {
    let home = tempfile::tempdir().unwrap();
    fs::write(index(home.path()), "").unwrap();
    let db = Connection::open(home.path().join("logs_9.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE logs(id INTEGER PRIMARY KEY,thread_id TEXT,target TEXT,feedback_log_body TEXT);").unwrap();
    let insert = |id: &str, model: &str, scope: &str, pressure: u64, limit: u64, window: u64| {
        let body = format!(
            "span model={model} effort=xhigh}}: post sampling token usage total_usage_tokens=66 auto_compact_scope_tokens={pressure} auto_compact_scope_limit=Some({limit}) auto_compact_limit_scope={scope} full_context_window_limit=Some({window})"
        );
        db.execute("INSERT INTO logs(thread_id,target,feedback_log_body) VALUES(?1,'codex_core::session::turn',?2)",[id,&body]).unwrap();
    };
    insert("a", "future-model", "Total", 0, 500, 1000);
    insert("other", "future-model", "Total", 80, 100, 1000);
    let color = |model: &str, n: u64, window: u64| {
        let state = json!({"thread":{"id":"a","workspace":{"cwd":home.path()},"model":{"id":model},"usage":{"context":{"used_tokens":n,"context_window_size":window}}}});
        rendered(home.path(), state, "Tools")["children"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|c| c.pointer("/props/color").and_then(Value::as_str))
            .unwrap()
            .to_owned()
    };
    assert_eq!(color("future-model", 449, 1000), "#E78A53");
    assert_eq!(color("future-model", 450, 1000), "#E05252");
    assert_eq!(color("different-model", 450, 1000), "#5F8787");
    assert_eq!(color("future-model", 450, 2000), "#5F8787");
    insert("a", "future-model", "Window", 90, 100, 1000);
    assert_eq!(color("future-model", 66, 1000), "#E05252");
    insert("a", "future-model", "Window", 89, 100, 1000);
    assert_eq!(color("future-model", 66, 1000), "#E78A53");
    insert("a", "future-model", "Window", 100, 0, 1000);
    assert_eq!(color("future-model", 66, 1000), "#5F8787");
}
