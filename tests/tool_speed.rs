mod common;
#[path = "common/provider.rs"]
mod provider;

use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs::File, io::Write};

struct Chat {
    _home: tempfile::TempDir,
    provider: provider::Provider,
    file: File,
    db: Connection,
    start: i64,
}

impl Chat {
    fn open() -> Self {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("rollout.jsonl");
        let db = Connection::open(home.path().join("state_1.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY, rollout_path TEXT);")
            .unwrap();
        db.execute(
            "INSERT INTO threads VALUES('chat', ?1)",
            [path.to_str().unwrap()],
        )
        .unwrap();
        let start = codex_statusline::usage::unix_now() - 100;
        let mut chat = Self {
            provider: provider::Provider::open(home.path()),
            file: File::create(path).unwrap(),
            db,
            start,
            _home: home,
        };
        chat.append(json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"turn","started_at":start}}));
        chat
    }
    fn append(&mut self, event: Value) {
        writeln!(self.file, "{event}").unwrap();
    }
    fn row(&mut self) -> String {
        self.provider
            .poll(&json!({"thread":{"id":"chat","agent":{"state":"working"}}}))
    }
    fn stamp(&self, offset: i64) -> String {
        self.db
            .query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')",
                [self.start + offset],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn call(&self, id: &str, mut payload: Value, offset: i64) -> Value {
        payload["call_id"] = json!(id);
        payload["internal_chat_message_metadata_passthrough"] = json!({"turn_id":"turn"});
        json!({"timestamp":self.stamp(offset),"type":"response_item","metadata":{"client_authored":false},"payload":payload})
    }
    fn report(&self, offset: i64, total: u64, response: u64) -> Value {
        json!({"timestamp":self.stamp(offset),"type":"token_usage_record","payload":{"turn_id":"turn","turn_token_usage":{"output_tokens":total},"usage":{"output_tokens":response},"thread_token_usage":{"output_tokens":10000+total}}})
    }
}

#[test]
fn generated_requests_affect_the_next_speed_read_before_usage_is_reported() {
    let mut chat = Chat::open();
    let function = chat.call(
        "a",
        json!({"type":"function_call","name":"shell","arguments":"é".repeat(1998)}),
        10,
    );
    let custom = chat.call(
        "b",
        json!({"type":"custom_tool_call","name":"exec","input":"x".repeat(3996)}),
        11,
    );
    let mut alien = custom.clone();
    alien["payload"]["internal_chat_message_metadata_passthrough"]["turn_id"] = json!("other");
    let mut client = custom.clone();
    client["metadata"]["client_authored"] = json!(true);
    for ignored in [
        alien,
        client,
        chat.call(
            "a",
            json!({"type":"function_call_output","name":"shell","output":"x".repeat(30000)}),
            10,
        ),
        chat.call(
            "b",
            json!({"type":"custom_tool_call_output","output":"y".repeat(30000)}),
            10,
        ),
    ] {
        chat.append(ignored);
    }
    assert!(!chat.row().contains("tok/s"));
    chat.append(function.clone());
    chat.append(function.clone());
    let estimate = |row: &str, low, high| {
        let n = row
            .split("Turn est. ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok());
        assert!(n.is_some_and(|n| (low..=high).contains(&n)), "{row}");
    };
    estimate(&chat.row(), 9.5, 10.1);
    chat.append(custom);
    chat.append(json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"turn","item":{"type":"CommandExecution","id":"nested","command":"x".repeat(9999),"aggregated_output":"y".repeat(9999)}}}));
    estimate(&chat.row(), 19.0, 20.1);
    chat.append(chat.report(12, 750, 750));
    let measured = chat.row();
    assert!(
        measured.contains("Turn ") && !measured.contains("Turn est."),
        "{measured}"
    );
    let mut replay = function;
    replay["timestamp"] = json!(chat.stamp(13));
    chat.append(replay);
    assert!(!chat.row().contains("Turn est."));
    chat.append(json!({"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"turn"}}));
    chat.append(chat.call(
        "late",
        json!({"type":"custom_tool_call","name":"exec","input":"late"}),
        15,
    ));
    assert!(!chat.row().contains("Turn est."));
}

#[test]
fn hosted_tool_estimates_use_only_the_recorded_request_parameters() {
    for (kind, field) in [
        ("web_search_call", "action"),
        ("local_shell_call", "action"),
        ("tool_search_call", "arguments"),
    ] {
        let mut chat = Chat::open();
        let mut record = chat.call("ignored", json!({"type":kind}), 10);
        let p = record["payload"].as_object_mut().unwrap();
        p.remove("internal_chat_message_metadata_passthrough"); // Older records inherit the open turn.
        p.remove("call_id");
        p.insert("id".into(), json!("hosted"));
        p.insert("result".into(), json!("x".repeat(20000)));
        chat.append(record.clone());
        assert!(!chat.row().contains("tok/s"), "{kind}");
        record["payload"][field] = json!({"query":"é".repeat(2000)});
        chat.append(record);
        let row = chat.row();
        let n = row
            .split("Turn est. ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok());
        assert!(
            n.is_some_and(|n| (9.5..=10.3).contains(&n)),
            "{kind}: {row}"
        );
    }
}
