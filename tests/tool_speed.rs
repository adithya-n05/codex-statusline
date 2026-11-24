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
    assert!(chat.row().contains("Turn est. 0.0 tok/s"));
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
    estimate(&chat.row(), 7.0, 7.6);
    let mut replay = function;
    replay["timestamp"] = json!(chat.stamp(13));
    chat.append(replay);
    estimate(&chat.row(), 7.0, 7.6);
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
        assert!(chat.row().contains("Turn est. 0.0 tok/s"), "{kind}");
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

#[test]
fn latest_response_speed_includes_generation_of_tool_only_replies() {
    let mut chat = Chat::open();
    // Native cumulative output and the response ledger can start from different counts.
    chat.append(json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":70000}}}}));
    let idle_view = json!({"thread":{"id":"chat","agent":{"state":"ready"}}});
    let mut call = chat.call(
        "first",
        json!({"type":"custom_tool_call","name":"exec","input":"command"}),
        14,
    );
    call["payload"]["internal_chat_message_metadata_passthrough"]["create_time"] =
        json!(chat.start as f64 + 10.25);
    chat.append(call.clone());
    chat.append(chat.report(15, 190, 190)); // 190 tokens in 4.75 seconds.
    assert!(!chat.provider.poll(&idle_view).contains("tok/s"));
    chat.append(call);
    chat.append(chat.call(
        "first",
        json!({"type":"custom_tool_call_output","output":"slow result"}),
        40,
    ));
    chat.append(json!({"timestamp":chat.stamp(40),"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"output_tokens":190},"total_token_usage":{"output_tokens":190}}}}));
    assert!(!chat.provider.poll(&idle_view).contains("tok/s"));
    let reasoning = json!({"timestamp":chat.stamp(47),"type":"event_msg","payload":{"type":"item_completed","turn_id":"turn","started_at_ms":(chat.start+44)*1000,"item":{"type":"Reasoning","id":"reason"}}});
    chat.append(reasoning);
    for (id, offset) in [("second", 47), ("third", 48)] {
        let mut call = chat.call(
            id,
            json!({"type":"function_call","name":"lookup","arguments":"{}"}),
            49,
        );
        call["payload"]["internal_chat_message_metadata_passthrough"]["create_time"] =
            json!(chat.start + offset);
        chat.append(call);
    }
    chat.append(chat.report(50, 550, 360)); // One response, including reasoning and both calls, in six seconds.
    assert!(!chat.provider.poll(&idle_view).contains("tok/s"));
    chat.append(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","duration_ms":90000}}));
    assert!(chat.provider.poll(&idle_view).contains("Last 6.1 tok/s"));
}

#[test]
fn invalid_or_missing_tool_creation_times_cannot_supply_a_generation_rate() {
    for creation in [
        Value::Null,
        json!("yesterday"),
        json!(-30),
        json!(1e40),
        json!(codex_statusline::usage::unix_now() + 100),
    ] {
        let mut chat = Chat::open();
        let mut call = chat.call(
            "c",
            json!({"type":"function_call","name":"run","arguments":"{}"}),
            10,
        );
        call["payload"]["internal_chat_message_metadata_passthrough"]["create_time"] = creation;
        chat.append(call);
        chat.append(chat.report(11, 500, 500));
        assert!(
            !chat
                .provider
                .poll(&json!({"thread":{"id":"chat"}}))
                .contains("Latest")
        );
    }
}

#[test]
fn the_live_turn_rate_changes_on_quiet_polls_and_counts_newly_written_output() {
    let mut chat = Chat::open();
    fn rate(row: &str) -> f64 {
        assert!(!row.contains("Latest") && !row.contains("Last "), "{row}");
        row.split("Turn est. ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }
    let mut call = chat.call(
        "code",
        json!({"type":"custom_tool_call","name":"exec","input":"abcd"}),
        25,
    );
    call["payload"]["internal_chat_message_metadata_passthrough"]["create_time"] =
        json!(chat.start + 20);
    chat.append(call);
    chat.append(chat.report(26, 30000, 30000));
    let first = rate(&chat.row());
    assert!((285.0..=300.1).contains(&first), "{first}");
    std::thread::sleep(std::time::Duration::from_millis(200));
    chat.append(chat.call(
        "code",
        json!({"type":"custom_tool_call_output","output":"x".repeat(40000)}),
        70,
    ));
    let quiet = chat
        .provider
        .poll(&json!({"thread":{"id":"chat","agent":{"state":"waiting"}}}));
    assert!(rate(&quiet) < first, "{quiet}");
    chat.append(chat.call(
        "next",
        json!({"type":"function_call","name":"lookup","arguments":"b".repeat(40000)}),
        74,
    ));
    let generated = rate(&chat.row());
    assert!(generated > first + 80.0);
    chat.append(chat.report(75, 36000, 6000));
    assert!(rate(&chat.row()) < generated);
    chat.append(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn","duration_ms":80000}}));
    assert!(chat.row().contains("Last 450.0 tok/s"));
}

#[test]
fn a_new_usage_report_updates_the_live_turn_in_a_large_chat() {
    let mut chat = Chat::open();
    chat.append(json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"turn","started_at_ms":(chat.start+10)*1000,"item":{"type":"Reasoning","id":"r"}}}));
    let mut call = chat.call(
        "before-tail",
        json!({"type":"function_call","name":"run","arguments":"{}"}),
        18,
    );
    call["payload"]["internal_chat_message_metadata_passthrough"]["create_time"] =
        json!(chat.start + 17);
    chat.append(call);
    chat.append(json!({"type":"event_msg","payload":{"type":"plan_update","plan":[{"status":"completed"}]}}));
    chat.append(json!({"padding":"x".repeat(600000)}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while !chat.row().contains("Plan 1/1") {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    chat.append(chat.report(20, 400, 400));
    let row = chat.row();
    assert!(
        row.contains("Turn est. 4.0 tok/s") || row.contains("Turn est. 3.9 tok/s"),
        "{row}"
    );
}

#[test]
fn ending_a_turn_preserves_its_estimate_including_unreported_generated_input() {
    for end in ["task_complete", "turn_aborted"] {
        let mut chat = Chat::open();
        chat.append(chat.report(10, 600, 600));
        let pending = chat.call(
            "pending",
            json!({"type":"function_call","name":"tool","arguments":"x".repeat(396)}),
            18,
        );
        chat.append(pending.clone());
        chat.append(pending);
        chat.append(json!({"timestamp":chat.stamp(19),"type":"event_msg","payload":{"type":end,"turn_id":"other"}}));
        assert!(chat.row().contains("Turn est."));
        chat.append(json!({"timestamp":chat.stamp(20),"type":"event_msg","payload":{"type":end,"turn_id":"turn"}}));
        for _ in 0..2 {
            let row = chat.row();
            assert!(
                row.contains("Last 35.0 tok/s") && !row.contains("Turn est."),
                "{row}"
            );
        }
        chat.append(json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"new","started_at":chat.start+21}}));
        let row = chat.row();
        assert!(
            row.contains("Turn est. 0.0 tok/s") && !row.contains("Last "),
            "{row}"
        );
    }
}

#[test]
fn streamed_text_changes_the_first_response_rate_before_rollout_usage_arrives() {
    let mut chat = Chat::open();
    let db = Connection::open(chat._home.path().join("logs_2.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE logs(id INTEGER PRIMARY KEY,ts INTEGER,ts_nanos INTEGER,thread_id TEXT,process_uuid TEXT,target TEXT,feedback_log_body TEXT)").unwrap();
    let start = chat.start;
    let add = |process: &str, owner: Option<&str>, offset: i64, target: &str, body: &str| {
        db.execute("INSERT INTO logs(ts,ts_nanos,thread_id,process_uuid,target,feedback_log_body) VALUES(?1,0,?2,?3,?4,?5)",
            rusqlite::params![start+offset,owner,process,target,body]).unwrap();
    };
    let marker = "codex_core::stream_events_utils";
    let notify = "codex_app_server::outgoing_message";
    let markdown = "codex_tui::markdown_stream";
    let mark = |turn: &str| {
        format!(
            "run{{turn_id={turn} model=test}}: Output item item_type=\"message\" item_id=\"msg1\""
        )
    };
    let chunk = |proc: &str, offset: i64, body: &str| {
        add(
            proc,
            None,
            offset,
            notify,
            "app-server event: item/agentMessage/delta targeted_connections=1",
        );
        add(
            proc,
            None,
            offset,
            markdown,
            &format!("push_delta: {}", json!(body)),
        );
    };
    fn check(row: String, low: f64, high: f64) {
        let n = row
            .split("Turn est. ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((low..=high).contains(&n), "{row}");
    }
    add("p", Some("chat"), 1, marker, &mark("other"));
    chunk("p", 2, &"y".repeat(8000));
    check(chat.row(), 0.0, 0.0);
    add("p", Some("chat"), 4, marker, &mark("turn"));
    chunk("other-process", 5, &"y".repeat(8000));
    check(chat.row(), 0.0, 0.0);
    chunk("p", 6, &"\n".repeat(2000));
    check(chat.row(), 4.7, 5.1);
    chunk("p", 7, &"é".repeat(1000));
    check(chat.row(), 9.5, 10.1);
    chat.append(json!({"timestamp":chat.stamp(8),"type":"event_msg","payload":{"type":"item_completed","turn_id":"turn","item":{"type":"AgentMessage","id":"msg1","content":[{"type":"Text","text":"a".repeat(4000)}]}}}));
    check(chat.row(), 9.5, 10.1);
    chat.append(chat.report(9, 600, 600));
    check(chat.row(), 5.7, 6.1);
    add(
        "p",
        Some("chat"),
        10,
        marker,
        &mark("turn").replace("msg1", "msg2"),
    );
    chunk("p", 11, &"x".repeat(4000));
    check(chat.row(), 15.0, 16.1);
    add("p", Some("side"), 12, marker, &mark("side"));
    chunk("p", 13, &"y".repeat(8000));
    check(chat.row(), 5.7, 6.1);
}
