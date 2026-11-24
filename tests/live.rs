mod common;
#[path = "common/provider.rs"]
mod provider;
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
    assert_eq!(parts[last_break - 1]["children"][0], "Last 120 tok/s");
    assert_eq!(parts[last_break - 1]["props"]["color"], "#FBCB97");
    assert_eq!(parts[last_break - 3]["children"][0], "API key");
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
            .filter(|c| {
                c["children"][0]
                    .as_str()
                    .is_some_and(|s| s.starts_with('['))
            })
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
fn credit_state(reset: i64) -> Value {
    json!({"thread":{"id":"a"},"account":{"rate_limits":[{"limit_id":"codex","primary_limit":{"window_minutes":10080,"used_percent":82,"resets_at":reset}}]}})
}
fn credit_event(t: &str, reset: i64, balance: &str) -> Value {
    json!({"timestamp":t,"type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","secondary":{"window_minutes":10080,"resets_at":reset},"credits":{"balance":balance,"unlimited":false}}}})
}
fn observations(path: &std::path::Path, events: &[Value]) {
    let mut file = fs::File::create(path).unwrap();
    for event in events {
        writeln!(file, "{event}").unwrap();
    }
}
fn account(home: &std::path::Path, id: &str) {
    fs::write(
        home.join("auth.json"),
        json!({"auth_mode":"chatgpt","tokens":{"account_id":id}}).to_string(),
    )
    .unwrap();
}
#[test]
fn credit_observations_are_persisted_once_for_the_authenticated_week() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let reset = 1736294400; // January 8, 2025 at midnight UTC.
    let a = credit_event("2025-01-01T00:00:00Z", reset, "60000");
    let b = credit_event("2025-01-02T09:00:00.050Z", reset, "62000");
    let c = credit_event("2025-01-06T20:30:30Z", reset, "58900.5");
    let mut unlimited = a.clone();
    unlimited["payload"]["rate_limits"]["credits"]["unlimited"] = json!(true);
    let mut other = a.clone();
    other["payload"]["rate_limits"]["limit_id"] = json!("separate");
    observations(
        &path,
        &[
            c.clone(),
            a.clone(),
            b.clone(),
            a.clone(),
            unlimited,
            other,
            credit_event("2024-12-30T23:59:59Z", reset, "90000"),
            credit_event("2025-01-08T00:00:00Z", reset, "40000"),
            credit_event("2025-01-05T00:00:00Z", reset, "NaN"),
        ],
    );
    let store = home.path().join("codex-statusline/credits.sqlite3");
    account(home.path(), "another-account");
    rendered(home.path(), credit_state(reset), "Tools");
    assert!(!store.exists());
    account(home.path(), "fixture-account");
    rendered(home.path(), credit_state(reset), "Tools");
    assert!(store.exists());
    observations(&path, &[b]);
    rendered(home.path(), credit_state(reset), "Tools");
    let db = Connection::open(store).unwrap();
    let mut query = db
        .prepare("SELECT balance FROM observations WHERE account=?1 AND reset=?2 ORDER BY at")
        .unwrap();
    let values = query
        .query_map(rusqlite::params!["fixture-account", reset], |r| {
            r.get::<_, f64>(0)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(values, vec![60000., 62000., 58900.5]);
}
#[test]
fn a_temporarily_unwritable_store_retries_only_valid_weekly_samples() {
    let home = tempfile::tempdir().unwrap();
    let gate = home.path().join("codex-statusline");
    fs::write(&gate, "reserved").unwrap();
    let mut ledger = codex_statusline::credits::Ledger::default();
    let mut event = credit_event("2025-01-02T00:00:00Z", 1736294400, "10");
    ledger.observe(&json!({"type":"other"}));
    let mut unknown = event.clone();
    unknown["payload"]["rate_limits"]["secondary"] = Value::Null;
    ledger.observe(&unknown);
    event["payload"]["rate_limits"]["credits"]["balance"] = json!(10);
    ledger.observe(&event);
    assert!(ledger.sync(home.path(), "a").is_err());
    fs::remove_file(gate).unwrap();
    ledger.sync(home.path(), "a").unwrap();
    ledger.sync(home.path(), "a").unwrap();
    let count: i64 = Connection::open(home.path().join("codex-statusline/credits.sqlite3"))
        .unwrap()
        .query_row("SELECT count(*) FROM observations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn spent_credits_replace_the_weekly_gauge_until_that_same_reset() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let now = codex_statusline::usage::unix_now();
    let reset = now + 259200;
    let db = Connection::open_in_memory().unwrap();
    let when = |t: i64| {
        db.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch')",
            [t],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    let at =
        |offset: i64, balance: &str| credit_event(&when(reset - 604800 + offset), reset, balance);
    let run = |state: Value| {
        let tree = rendered(home.path(), state, "Tools");
        tree["children"]
            .as_array()
            .unwrap()
            .iter()
            .skip_while(|v| *v != "\n")
            .skip(1)
            .take_while(|v| *v != "\n")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v["children"][0].as_str().unwrap().to_owned())
            })
            .collect::<String>()
    };
    observations(&path, &[at(0, "60000"), at(48000, "62000")]);
    let state = credit_state(reset);
    assert!(run(state.clone()).contains("WKLY 18% left"));
    // A balance observed in a separate launch arrives in a different order.
    observations(&path, &[at(100000, "31000"), at(60000, "46500")]);
    let usage = run(state.clone());
    assert!(
        usage.contains("CRDTS 31,000 / 62,000 50% left [██████████░░░░░░░░░░] · reset"),
        "{usage}"
    );
    assert!(!usage.contains("WKLY ") && !usage.contains('≈'));
    observations(&path, &[at(105000, "33000")]);
    assert!(run(state.clone()).contains("CRDTS 33,000 / 64,000"));
    observations(&path, &[at(106000, "0")]);
    assert!(run(state.clone()).contains("CRDTS 0 / 64,000 0% left"));
    assert!(run(credit_state(reset + 604800)).contains("WKLY 18% left"));
    assert!(run(credit_state(now - 1)).contains("WKLY 18% left"));
    account(home.path(), "second-account");
    assert!(!run(state).contains("CRDTS"));
}
#[test]
fn missing_chat_names_use_only_the_saved_name_of_the_displayed_thread() {
    let home = tempfile::tempdir().unwrap();
    fs::write(index(home.path()), "").unwrap();
    let db = Connection::open(home.path().join("state_1.sqlite")).unwrap();
    let first = |name: Value| {
        common::plain(
            &rendered(
                home.path(),
                json!({"thread":{"id":"a","name":name,"workspace":{"cwd":"Dir"}}}),
                "Tools",
            )["children"][0],
        )
    };
    assert_eq!(first(Value::Null), "Dir"); // The index title can contain the entire first prompt.
    db.execute("UPDATE threads SET name='Saved chat'", [])
        .unwrap();
    assert_eq!(first(Value::Null), "Dir · Saved chat");
    assert_eq!(first(json!("Live chat")), "Dir · Live chat");
    db.execute("UPDATE threads SET name='  '", []).unwrap();
    assert_eq!(first(Value::Null), "Dir");
}
#[test]
fn alternating_reset_seconds_keep_the_same_credit_balance_and_weekly_baseline() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let now = codex_statusline::usage::unix_now();
    let reset = now + 6500;
    let db = Connection::open_in_memory().unwrap();
    let date = |offset: i64| {
        db.query_row("SELECT datetime(?1,'unixepoch')", [now + offset], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
    };
    observations(
        &path,
        &[
            credit_event(&date(-3600), reset, "62000"),
            credit_event(&date(-900), reset + 1, "61400"),
            credit_event(&date(-60), reset, "60000"),
        ],
    );
    for live in [reset + 1, reset, reset + 1, reset - 1] {
        let mut state = credit_state(live);
        state["account"]["rate_limits"][0]["primary_limit"]["used_percent"] = json!(100);
        let row = rendered(home.path(), state, "Tools").to_string();
        assert!(row.contains("CRDTS 60,000 / 62,000"), "{row}");
        assert!(!row.contains("WKLY 0% left"));
    }
    assert!(
        !rendered(home.path(), credit_state(reset + 604800), "Tools")
            .to_string()
            .contains("CRDTS 60,000")
    );
}
#[test]
fn spent_credits_survive_cold_start_missing_limits_and_a_slow_thread_refresh() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let now = codex_statusline::usage::unix_now();
    let reset = now + 6500;
    let db = Connection::open_in_memory().unwrap();
    let at = |offset: i64| {
        db.query_row("SELECT datetime(?1,'unixepoch')", [now + offset], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
    };
    observations(
        &path,
        &[
            credit_event(&at(-10), reset, "500"),
            credit_event(&at(-1), reset, "450"),
        ],
    );
    rendered(home.path(), credit_state(reset), "Tools");
    for state in [credit_state(reset + 1), json!({"thread":{"id":"new"}})] {
        let first = rendered(home.path(), state, "").to_string();
        assert!(first.contains("CRDTS 450 / 500"), "{first}");
    }
    for state in [credit_state(reset + 604800), credit_state(now - 1)] {
        assert!(
            !rendered(home.path(), state, "")
                .to_string()
                .contains("CRDTS 450")
        );
    }
    account(home.path(), "different-account");
    assert!(
        !rendered(home.path(), credit_state(reset), "")
            .to_string()
            .contains("CRDTS 450")
    );
}
#[test]
fn idle_resume_restores_the_recorded_context_before_the_first_turn() {
    let home = tempfile::tempdir().unwrap();
    fs::write(index(home.path()), "").unwrap();
    let db = Connection::open(home.path().join("logs_1.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE logs(id INTEGER PRIMARY KEY,thread_id TEXT,target TEXT,feedback_log_body TEXT);
    INSERT INTO logs VALUES(1,'a','codex_core::session::turn','span model=new-model effort=xhigh}: post sampling token usage total_usage_tokens=135200 auto_compact_scope_tokens=135200 auto_compact_scope_limit=Some(244800) auto_compact_limit_scope=Total full_context_window_limit=Some(258400)');").unwrap();
    let read = |id: &str, model: &str, context: Value| {
        rendered(
            home.path(),
            json!({"thread":{"id":id,"model":{"id":model},"usage":{"context":context}}}),
            "",
        )
        .to_string()
    };
    let idle = read("a", "new-model", json!({}));
    assert!(
        idle.contains("CTX 50% used") && !idle.contains("258400"),
        "{idle}"
    );
    assert!(
        read(
            "a",
            "new-model",
            json!({"used_percentage":61,"context_window_size":258400})
        )
        .contains("CTX 61% used")
    );
    for (id, model) in [("b", "new-model"), ("a", "other")] {
        let row = read(id, model, json!({}));
        assert!(
            row.contains("CTX awaiting usage") && !row.contains("258400"),
            "{row}"
        );
    }
    assert!(!read("a", "new-model", json!({"context_window_size":800000})).contains("258400"));
}
#[test]
fn whole_credit_labels_preserve_fractional_bar_math() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let now = codex_statusline::usage::unix_now();
    let reset = now + 600;
    let db = Connection::open_in_memory().unwrap();
    let at = |offset: i64| {
        db.query_row("SELECT datetime(?1,'unixepoch')", [now + offset], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
    };
    observations(
        &path,
        &[
            credit_event(&at(-10), reset, "1.49"),
            credit_event(&at(-5), reset, "0.75"),
        ],
    );
    assert!(
        rendered(home.path(), credit_state(reset), "Tools")
            .to_string()
            .contains("CRDTS 1 / 1 50% left")
    );
}
#[test]
fn generation_speed_updates_between_model_responses_without_charging_tool_waits() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let event = |stamp: &str, p: Value| json!({"timestamp":stamp,"type":"event_msg","payload":p});
    let output = |turn: &str, start: i64| {
        event(
            "2025-01-01T00:00:01Z",
            json!({"type":"item_completed","turn_id":turn,"started_at_ms":start,"item":{"type":"Reasoning"}}),
        )
    };
    let count = |t: &str, total: u64, n: u64| {
        event(
            t,
            json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":total},"last_token_usage":{"output_tokens":n}}}),
        )
    };
    let mut events = vec![
        event(
            "2025-01-01T00:00:00Z",
            json!({"type":"task_started","turn_id":"one"}),
        ),
        output("one", 1735689600000),
        count("2025-01-01T00:00:02Z", 200, 200),
    ];
    let state = json!({"thread":{"id":"a"}});
    let read = |events: &[Value]| {
        observations(&path, events);
        rendered(home.path(), state.clone(), "Tools").to_string()
    };
    assert!(!read(&events).contains("tok/s"));
    events.push(event("2025-01-01T00:00:52Z",json!({"type":"item_completed","turn_id":"one","started_at_ms":1735689602000u64,"item":{"type":"CommandExecution","id":"cmd"}})));
    events.push(output("other", 1735689602000));
    events.push(output("one", 1735689652000));
    events.push(count("2025-01-01T00:00:52Z", 200, 200));
    events.push(count("2025-01-01T00:00:53.500Z", 425, 225));
    assert!(!read(&events).contains("tok/s"));
    events.push(event(
        "2025-01-01T00:01:00Z",
        json!({"type":"task_complete","turn_id":"one","duration_ms":60000}),
    ));
    let idle = read(&events);
    assert!(idle.contains("Last 150 tok/s") && !idle.contains("Latest "));
}
#[test]
fn a_resumed_chat_recovers_recorded_usage_after_diagnostic_logs_rotate() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let model = json!({"type":"turn_context","payload":{"model":"test-model"}});
    let usage = json!({"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":258400,"last_token_usage":{"total_tokens":135200},"total_token_usage":{"total_tokens":1000000}}}});
    let read = |model: &str, context: Value| {
        rendered(
            home.path(),
            json!({"thread":{"id":"a","model":{"id":model},"usage":{"context":context}}}),
            "",
        )
        .to_string()
    };
    observations(&path, &[model.clone(), usage.clone()]);
    assert!(read("test-model", json!({})).contains("CTX 50% used"));
    assert!(read("test-model", json!({"used_percentage":70})).contains("CTX 70% used"));
    assert!(!read("other", json!({})).contains("CTX 50% used"));
    assert!(!read("test-model", json!({"context_window_size":800000})).contains("CTX 50% used"));
    for end in [
        json!({"type":"compacted","payload":{}}),
        json!({"type":"turn_context","payload":{"model":"other"}}),
    ] {
        observations(&path, &[model.clone(), usage.clone(), end]);
        assert!(!read("test-model", json!({})).contains("CTX 50% used"));
    }
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "{}", "x".repeat(600_000)).unwrap();
    writeln!(f, "{model}\n{usage}").unwrap();
    assert!(read("test-model", json!({})).contains("CTX 50% used"));
}
#[test]
fn the_first_footer_read_restores_speed_from_a_recent_response_in_a_large_chat() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "{}", "x".repeat(600000)).unwrap();
    for p in [
        json!({"type":"item_completed","turn_id":"older","started_at_ms":1735689600000u64,"item":{"type":"Reasoning"}}),
        json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":100500},"last_token_usage":{"output_tokens":500}}}),
        json!({"type":"task_complete","turn_id":"older","duration_ms":210000}),
    ] {
        writeln!(
            f,
            "{}",
            json!({"type":"event_msg","timestamp":"2025-01-01T00:00:05Z","payload":p})
        )
        .unwrap();
    }
    let first = rendered(
        home.path(),
        json!({"thread":{"id":"a","usage":{"context":{"used_percentage":52}}}}),
        "",
    );
    assert!(first.to_string().contains("Last 100 tok/s"), "{first}");
    let absent = rendered(home.path(), json!({"thread":{"id":"different"}}), "");
    assert!(!absent.to_string().contains("Last 100"));
}

#[test]
fn first_read_groups_reset_clock_drift_before_showing_exhausted_weekly_usage() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let now = codex_statusline::usage::unix_now();
    let reset = now + 4500;
    let stamp = |at: i64| {
        Connection::open_in_memory()
            .unwrap()
            .query_row("SELECT datetime(?1,'unixepoch')", [at], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
    };
    observations(
        &path,
        &[
            credit_event(&stamp(now - 20000), reset - 8, "5000"),
            credit_event(&stamp(now - 900), reset + 3, "6500"),
            credit_event(&stamp(now - 10), reset, "5800"),
        ],
    );
    rendered(home.path(), credit_state(reset), "Tools");
    for clock in [reset, reset + 3, reset - 8] {
        let mut state = credit_state(clock);
        state["account"]["rate_limits"][0]["primary_limit"]["used_percent"] = json!(100);
        let first = rendered(home.path(), state, "").to_string();
        assert!(first.contains("CRDTS 5,800 / 6,500"), "{first}");
        assert!(!first.contains("WKLY"));
    }
    assert!(
        !rendered(home.path(), credit_state(reset + 604800), "")
            .to_string()
            .contains("CRDTS")
    );
}

#[test]
fn throughput_uses_the_response_usage_record_before_a_blocking_tool_returns() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let state = json!({"thread":{"id":"a"}});
    let start = |ms: u64| json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"t","started_at_ms":ms,"item":{"type":"Reasoning"}}});
    let usage = |turn: &str, t: &str, total: u64, n: u64| json!({"timestamp":t,"type":"token_usage_record","payload":{"turn_id":turn,"usage":{"output_tokens":n},"thread_token_usage":{"output_tokens":total}}});
    let first = usage("t", "2025-01-01T00:00:02Z", 400, 200);
    let mut events = vec![
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t"}}),
        start(1735689600000),
        usage("other", "2025-01-01T00:00:01Z", 900, 660),
        first.clone(),
    ];
    let check = |events: &[Value], expected: &str| {
        observations(&path, events);
        for wait in ["", "Tools"] {
            let tree = rendered(home.path(), state.clone(), wait);
            assert!(tree.to_string().contains(expected), "{tree}");
        }
    };
    let ended = |events: &[Value]| {
        let mut out = events.to_vec();
        out.push(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","duration_ms":60000}}));
        out
    };
    check(&ended(&events), "Last 100 tok/s");
    events.extend([
        start(1735689605000), first,
        json!({"timestamp":"2025-01-01T00:01:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":400},"last_token_usage":{"output_tokens":200}}}}),
        usage("t","2025-01-01T00:00:06Z",450,50),
    ]);
    check(&ended(&events), "Last 50 tok/s");
    events.push(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","duration_ms":60000}}));
    check(&events, "Last 50 tok/s");
}

#[test]
fn each_footer_poll_picks_up_a_new_rate_even_with_cached_tool_counts() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let mut log = fs::File::create(&path).unwrap();
    let mut write_event = |event: Value| writeln!(log, "{event}").unwrap();
    let started = codex_statusline::usage::unix_now() - 10;
    write_event(
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t","started_at":started}}),
    );
    let start = |ms: u64| json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":"t","started_at_ms":ms,"item":{"type":"AgentMessage"}}});
    let usage = |time: &str, n: u64, total: u64| json!({"type":"token_usage_record","timestamp":time,"payload":{"turn_id":"t","usage":{"output_tokens":n},"turn_token_usage":{"output_tokens":total},"thread_token_usage":{"output_tokens":total}}});
    write_event(start(1735689600000));
    write_event(usage("2025-01-01T00:00:02Z", 100, 100));
    let mut child = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
        .env("CODEX_HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut poll = |id: &str| {
        let request = json!({"id":1,"method":"resources/read","params":{"uri":"ui://codex-statusline/footer","_meta":{"com.openai/codex-statusline-state":{"thread":{"id":id,"agent":{"state":"working"}}}}}});
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut reply = String::new();
        output.read_line(&mut reply).unwrap();
        reply
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    let rate = |row: &str| {
        row.split("Turn est. ")
            .nth(1)
            .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
    };
    let previous = loop {
        let first = poll("a");
        if first.contains("Tools")
            && let Some(n) = rate(&first)
        {
            break n;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    };
    write_event(start(1735689602000));
    write_event(usage("2025-01-01T00:00:03Z", 125, 225));
    let next = poll("a");
    assert!(rate(&next).is_some_and(|n| n > previous * 1.9), "{next}");
    assert!(!poll("another").contains("tok/s"));
    write_event(
        json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","duration_ms":10000}}),
    );
    assert!(poll("a").contains("Last 22.5 tok/s"));
    drop(input);
    assert!(child.wait().unwrap().success());
}

#[test]
fn the_first_prompt_has_a_gauge_before_codex_reports_usage() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let read = |events: &[Value], model: &str, context: Value| {
        observations(&path, events);
        rendered(
            home.path(),
            json!({"thread":{"id":"a","model":{"id":model},"usage":{"context":context}}}),
            "",
        )
        .to_string()
    };
    let meta = json!({"type":"session_meta","payload":{"id":"a"}});
    assert!(read(std::slice::from_ref(&meta), "model", json!({})).contains("CTX 0% used"));
    let mut events = vec![
        meta,
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"one","model_context_window":112000}}),
        json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"a".repeat(60000)}]}}),
        json!({"type":"response_item","payload":{"type":"additional_tools","tools":[{"description":"b".repeat(8000)}]}}),
        json!({"type":"turn_context","payload":{"model":"model"}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"c".repeat(20000)}]}}),
        json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"text":"c".repeat(20000)}]}}}),
    ];
    let estimate = read(&events, "model", json!({}));
    assert!(
        estimate.contains("CTX est. 10% used") && estimate.contains("██░░"),
        "{estimate}"
    );
    assert!(!read(&events, "other", json!({})).contains("est."));
    assert!(!read(&events, "model", json!({"context_window_size":72000})).contains("est."));
    assert!(read(&events, "model", json!({"used_percentage":14})).contains("CTX 14% used"));
    let mut no_window = events.clone();
    no_window[1]["payload"]["model_context_window"] = Value::Null;
    assert!(
        read(&no_window, "model", json!({"context_window_size":112000})).contains("CTX est. 10%")
    );
    assert!(!read(&no_window, "model", json!({})).contains("est."));
    for stop in [
        json!({"type":"compacted","payload":{}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"two","model_context_window":112000}}),
    ] {
        let mut halted = events.clone();
        halted.push(stop);
        assert!(!read(&halted, "model", json!({})).contains("est."));
    }
    let mut fork = events.clone();
    fork[0]["payload"]["forked_from_id"] = json!("parent");
    assert!(!read(&fork, "model", json!({})).contains("est."));
    let mut unknown = events.clone();
    unknown.push(json!({"type":"response_item","payload":{"type":"message","content":[{"type":"input_image","image_url":"data:image/example;base64,abc"}]}}));
    assert!(!read(&unknown, "model", json!({})).contains("est."));
    events.push(json!({"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":112000,"last_token_usage":{"total_tokens":25000}}}}));
    let exact = read(&events, "model", json!({}));
    assert!(exact.contains("CTX 13% used") && !exact.contains("est."));
    events.push(json!({"type":"compacted","payload":{}}));
    assert!(!read(&events, "model", json!({})).contains("est."));

    let logs = Connection::open(home.path().join("logs_9.sqlite")).unwrap();
    logs.execute_batch("CREATE TABLE logs(id INTEGER PRIMARY KEY,thread_id TEXT,target TEXT,feedback_log_body TEXT); INSERT INTO logs VALUES(1,'a','codex_core::session::turn','span model=model effort=xhigh}: post sampling token usage total_usage_tokens=25000 auto_compact_scope_tokens=25000 auto_compact_scope_limit=Some(100000) auto_compact_limit_scope=Total full_context_window_limit=Some(112000)');").unwrap();
    let measured = read(&no_window, "model", json!({"context_window_size":112000}));
    assert!(
        measured.contains("CTX 13% used") && !measured.contains("est."),
        "{measured}"
    );
}

#[test]
fn first_prompt_estimate_yields_to_the_matching_response_usage_record() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let mut events = vec![
        json!({"type":"session_meta","payload":{"id":"a"}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"first","model_context_window":112000}}),
        json!({"type":"turn_context","payload":{"model":"test-model"}}),
        json!({"type":"response_item","payload":{"type":"message","content":[{"type":"input_text","text":"a".repeat(88000)}]}}),
    ];
    let state = json!({"thread":{"id":"a","model":{"id":"test-model"}}});
    let report = |id: &str, total: Value| json!({"type":"token_usage_record","payload":{"turn_id":id,"usage":{"total_tokens":total},"thread_token_usage":{"total_tokens":900000}}});
    let check = |events: &[Value], label: &str| {
        observations(&path, events);
        for stage in ["", "Tools"] {
            assert!(
                rendered(home.path(), state.clone(), stage)
                    .to_string()
                    .contains(label)
            );
        }
    };
    events.extend([
        report("previous", json!(99999)),
        report("first", Value::Null),
    ]);
    check(&events, "CTX est. 10% used");
    events.push(report("first", json!(26000)));
    check(&events, "CTX 14% used");
    events.push(json!({"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":112000,"last_token_usage":{"total_tokens":27000}}}}));
    check(&events, "CTX 15% used");
}

#[test]
fn active_turn_speed_uses_confirmed_turn_tokens_and_moves_between_reports() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let started = codex_statusline::usage::unix_now() - 10;
    let mut events = vec![
        json!({"type":"session_meta","payload":{"id":"a"}}),
        json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":9000}}}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"one","started_at":started}}),
    ];
    let report = |id: &str, output: u64, total: u64| json!({"type":"token_usage_record","payload":{"turn_id":id,"turn_token_usage":{"output_tokens":output},"thread_token_usage":{"output_tokens":total}}});
    let state = |id: &str, agent: &str, n: u64| json!({"thread":{"id":id,"agent":{"state":agent},"usage":{"total":{"output_tokens":n}}}});
    let rate = |state: Value| {
        let tree = rendered(home.path(), state, "");
        let text = common::plain(&tree);
        text.split("Turn est. ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
    };
    observations(&path, &events);
    assert_eq!(rate(state("a", "thinking", 9000)), Some(0.0));
    events.extend([
        report("one", 1000, 10000),
        report("other", 9000, 92000),
        report("one", 1000, 10000),
    ]);
    observations(&path, &events);
    let first = rate(state("a", "thinking", 10000)).expect("running turn rate");
    assert!((82.0..=100.1).contains(&first), "{first}");
    thread::sleep(Duration::from_millis(250));
    let next = rate(state("a", "working", 10000)).unwrap();
    assert!(next < first, "{first} then {next}");
    assert!(rate(state("a", "working", 10500)).unwrap() > first);
    assert_eq!(rate(state("a", "ready", 10500)), None);
    assert_eq!(rate(state("other", "thinking", 10500)), None);
    events.push(json!({"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"stale"}}));
    observations(&path, &events);
    assert!(rate(state("a", "waiting", 10000)).is_some());
    events.push(json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"one"}}));
    observations(&path, &events);
    assert_eq!(rate(state("a", "working", 10000)), None);
    events.push(json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"two","started_at":started+2}}));
    observations(&path, &events);
    assert_eq!(rate(state("a", "working", 10000)), Some(0.0));
    events.push(report("two", 50, 10050));
    observations(&path, &events);
    assert!(rate(state("a", "thinking", 10050)).is_some_and(|r| r > 0. && r < 9.));
}

#[test]
fn visible_output_updates_turn_speed_before_the_first_usage_report() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let started = codex_statusline::usage::unix_now() - 20;
    let state = |output| json!({"thread":{"id":"a","agent":{"state":"thinking"},"usage":{"total":{"output_tokens":output}}}});
    let item = |id: &str, turn: &str, ms: i64, details: Value| {
        let mut item = details;
        item["id"] = json!(id);
        json!({"type":"event_msg","payload":{"type":"item_completed","turn_id":turn,"completed_at_ms":ms,"item":item}})
    };
    let message = |id: &str, turn: &str, ms: i64| {
        item(
            id,
            turn,
            ms,
            json!({"type":"AgentMessage","content":[{"type":"Text","text":"a".repeat(800)}]}),
        )
    };
    let at = (started + 5) * 1000;
    let talk = message("a1", "t", at);
    let mut events = vec![
        json!({"type":"session_meta","payload":{"id":"a"}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t","started_at":started}}),
        item(
            "c",
            "t",
            at,
            json!({"type":"CommandExecution","output":"a".repeat(8000)}),
        ),
        message("alien", "other", at),
    ];
    let read = |events: &[Value], n: u64| {
        observations(&path, events);
        common::plain(&rendered(home.path(), state(n), ""))
    };
    let rate = |row: &str| {
        row.split("Turn est. ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
    };
    assert_eq!(rate(&read(&events, 0)), Some(0.0));
    events.extend([talk.clone(), talk.clone()]);
    let a = read(&events, 0);
    assert!(rate(&a).is_some_and(|n| (8.5..=10.1).contains(&n)), "{a}");
    events.push(item("r","t",at+2,json!({"type":"Reasoning","summary_text":["b".repeat(800)],"raw_content":["ignore".repeat(1000)]})));
    let b = read(&events, 0);
    assert!(rate(&b).is_some_and(|n| (17.0..=20.1).contains(&n)), "{b}");
    let native = read(&events, 600);
    assert!(
        rate(&native).is_some_and(|r| (25.0..=30.1).contains(&r)),
        "{native}"
    );
    let stamp = Connection::open_in_memory()
        .unwrap()
        .query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch')",
            [started + 7],
            |r| r.get::<_, String>(0),
        )
        .unwrap();
    events.push(json!({"timestamp":stamp,"type":"token_usage_record","payload":{"turn_id":"t","turn_token_usage":{"output_tokens":800},"thread_token_usage":{"output_tokens":800}}}));
    events.push(talk);
    let exact = read(&events, 800);
    assert!(
        rate(&exact).is_some_and(|r| (35.0..=40.1).contains(&r)),
        "{exact}"
    );
    events.push(message("a2", "t", at + 4000));
    let later = read(&events, 800);
    assert!(
        rate(&later).is_some_and(|n| (44.0..=50.1).contains(&n)),
        "{later}"
    );
    events.push(json!({"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t"}}));
    assert_eq!(rate(&read(&events, 800)), None);
}

#[test]
fn a_large_active_chat_recovers_the_matching_turn_clock_on_the_first_read() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let now = codex_statusline::usage::unix_now();
    let state = json!({"thread":{"id":"a","agent":{"state":"working"}}});
    // The start of this turn is outside the bounded first read.
    observations(
        &path,
        &[
            json!({"type":"ignored","padding":".".repeat(600_000)}),
            json!({"type":"token_usage_record","payload":{"turn_id":"running","turn_token_usage":{"output_tokens":1000},"thread_token_usage":{"output_tokens":9000}}}),
        ],
    );
    let read = || {
        let row = common::plain(&rendered(home.path(), state.clone(), ""));
        row.split("Turn est. ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
    };
    assert_eq!(read(), None);
    let db = Connection::open(home.path().join("thread_history_1.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE thread_turns(thread_id TEXT, turn_id TEXT, status TEXT, started_at INTEGER, PRIMARY KEY(thread_id,turn_id))").unwrap();
    for (thread, turn, status) in [
        ("other", "running", "inProgress"),
        ("a", "unrelated", "inProgress"),
        ("a", "running", "completed"),
    ] {
        db.execute(
            "INSERT INTO thread_turns VALUES (?1,?2,?3,?4)",
            rusqlite::params![thread, turn, status, now - 60],
        )
        .unwrap();
    }
    assert_eq!(read(), None);
    db.execute("UPDATE thread_turns SET status='inProgress', started_at=?1 WHERE thread_id='a' AND turn_id='running'", [now-10]).unwrap();
    let rate = read().expect("rate on the first read of a large active chat");
    assert!((80.0..=100.1).contains(&rate), "{rate}");
}

#[test]
fn each_usage_label_matches_its_bar_across_all_pressure_bands() {
    fn assert_gauge(tree: &Value, label: &str, color: &str) {
        let cells = tree["children"].as_array().unwrap();
        let index = cells
            .iter()
            .position(|cell| cell["children"][0] == label)
            .unwrap_or_else(|| panic!("missing {label} in {}", common::plain(tree)));
        assert!(common::plain(&cells[index + 1]).starts_with('['));
        assert_eq!(cells[index + 1]["props"]["color"], color, "bar for {label}");
        assert_eq!(cells[index]["props"]["color"], color, "label {label}");
    }
    let now = codex_statusline::usage::unix_now();
    let reset = now + 3600;
    for (spent, color) in [
        (0, "#5F8787"),
        (49, "#5F8787"),
        (50, "#FBCB97"),
        (74, "#FBCB97"),
        (75, "#E78A53"),
        (89, "#E78A53"),
        (90, "#E05252"),
        (100, "#E05252"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let path = index(home.path());
        account(home.path(), "fixture-account");
        let db = Connection::open(home.path().join("logs_1.sqlite")).unwrap();
        let body = format!(
            "span model=test-model effort=xhigh}}: post sampling token usage total_usage_tokens={spent} auto_compact_scope_tokens={spent} auto_compact_scope_limit=Some(100) auto_compact_limit_scope=Total full_context_window_limit=Some(1000)"
        );
        db.execute_batch("CREATE TABLE logs(id INTEGER PRIMARY KEY,thread_id TEXT,target TEXT,feedback_log_body TEXT);").unwrap();
        db.execute(
            "INSERT INTO logs VALUES(1,'a','codex_core::session::turn',?1)",
            [body],
        )
        .unwrap();
        fs::write(&path, "").unwrap();
        let mut state = credit_state(reset);
        state["account"]["rate_limits"][0]["primary_limit"]["used_percent"] = json!(spent);
        state["thread"]["model"]["id"] = json!("test-model");
        state["thread"]["usage"]["context"] =
            json!({"used_percentage":spent,"used_tokens":spent,"context_window_size":1000});
        let tree = rendered(home.path(), state.clone(), "Tools");
        let left = 100 - spent;
        assert_gauge(&tree, &format!("CTX {spent}% used "), color);
        assert_gauge(&tree, &format!("WKLY {left}% left "), color);
        // No spending has occurred at 0%, so the weekly gauge remains selected.
        if spent > 0 {
            let timestamp = |offset| {
                db.query_row("SELECT datetime(?1,'unixepoch')", [now - offset], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap()
            };
            observations(
                &path,
                &[
                    credit_event(&timestamp(6), reset, "100"),
                    credit_event(&timestamp(3), reset, &left.to_string()),
                ],
            );
            let tree = rendered(home.path(), state, "CRDTS");
            assert_gauge(&tree, &format!("CRDTS {left} / 100 {left}% left "), color);
        }
    }
}

#[test]
fn new_account_balances_reach_open_footers_on_the_very_next_read() {
    let home = tempfile::tempdir().unwrap();
    account(home.path(), "fixture-account");
    let path = index(home.path());
    let now = codex_statusline::usage::unix_now();
    let reset = now + 3600;
    let db = Connection::open_in_memory().unwrap();
    let sample = |t, balance| {
        let stamp = db
            .query_row("SELECT datetime(?1,'unixepoch')", [now - t], |r| {
                r.get::<_, String>(0)
            })
            .unwrap();
        credit_event(&stamp, reset, balance)
    };
    let state = credit_state(reset);
    let mut session = provider::Provider::open(home.path());
    let mut other = provider::Provider::open(home.path());
    let other_thread = json!({"thread":{"id":"other"}});
    assert!(!other.poll(&other_thread).contains("CRDTS"));
    let mut log = fs::File::create(path).unwrap();
    writeln!(log, "{}", sample(10, "1000")).unwrap();
    writeln!(log, "{}", sample(9, "950")).unwrap();
    assert!(session.poll(&state).contains("CRDTS 950 / 1,000"));
    for (t, amount, label) in [
        (8, "925", "CRDTS 925 / 1,000"),
        (7, "1025", "CRDTS 1,025 / 1,100"),
        (6, "924", "CRDTS 924 / 1,100"),
    ] {
        writeln!(log, "{}", sample(t, amount)).unwrap();
        assert!(session.poll(&state).contains(label), "same chat: {label}");
        assert!(
            other.poll(&other_thread).contains(label),
            "other open chat: {label}"
        );
    }
}

#[test]
fn context_uses_newer_session_measurements_while_the_host_snapshot_catches_up() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let mut session = provider::Provider::open(home.path());
    let state = |used: u64, pct: u64, output: u64, model: &str| {
        json!({"thread":{"id":"a","model":{"id":model},
        "usage":{"context":{"context_window_size":112000,"used_tokens":used,"used_percentage":pct},"total":{"output_tokens":output}}}})
    };
    let count = |used, total| json!({"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":112000,"last_token_usage":{"total_tokens":used},"total_token_usage":{"output_tokens":total}}}});
    let report = |id: &str, used| json!({"type":"token_usage_record","payload":{"thread_id":"a","turn_id":id,"usage":{"total_tokens":used},"thread_token_usage":{"output_tokens":999999}}});
    let mut f = fs::File::create(&path).unwrap();
    for e in [
        json!({"type":"turn_context","payload":{"model":"m"}}),
        count(24000, 300),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"two","model_context_window":112000}}),
    ] {
        writeln!(f, "{e}").unwrap();
    }
    let native = state(24000, 12, 300, "m");
    assert!(session.poll(&native).contains("CTX 12% used"));
    writeln!(f, "{}", report("other", 92000)).unwrap();
    assert!(session.poll(&native).contains("CTX 12% used"));
    writeln!(f, "{}", report("two", 77000)).unwrap();
    assert!(session.poll(&native).contains("CTX 65% used"));
    assert!(
        session
            .poll(&state(83000, 71, 700, "m"))
            .contains("CTX 71% used")
    );
    writeln!(f, "{}", count(77000, 500)).unwrap();
    assert!(session.poll(&native).contains("CTX 65% used"));
    assert!(
        session
            .poll(&state(24000, 12, 300, "new-model"))
            .contains("CTX 12% used")
    );
    let mut switched = native.clone();
    switched["thread"]["usage"]["context"]["context_window_size"] = json!(224000);
    assert!(session.poll(&switched).contains("CTX 12% used"));
    writeln!(
        f,
        "{}",
        json!({"type":"event_msg","payload":{"type":"context_compacted"}})
    )
    .unwrap();
    assert!(session.poll(&native).contains("CTX 12% used"));
}

#[test]
fn weekly_allowance_uses_the_next_report_for_the_signed_in_accounts_week() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    account(home.path(), "fixture-account");
    let mut session = provider::Provider::open(home.path());
    let now = codex_statusline::usage::unix_now();
    let reset = now + 3600;
    let mut f = fs::File::create(&path).unwrap();
    let db = Connection::open_in_memory().unwrap();
    let sample = |seconds, used: u64, resets, limit: &str| {
        let ts = db
            .query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch')",
                [now - seconds],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        json!({"timestamp":ts,"type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":limit,
            "secondary":{"window_minutes":10080,"used_percent":used,"resets_at":resets}}}})
    };
    let mut host = credit_state(reset);
    host["account"]["rate_limits"][0]["primary_limit"]["used_percent"] = json!(10);
    writeln!(f, "{}", sample(10, 10, reset, "codex")).unwrap();
    assert!(session.poll(&host).contains("WKLY 90% left"));
    writeln!(f, "{}", sample(8, 16, reset + 1, "codex")).unwrap();
    assert!(session.poll(&host).contains("WKLY 84% left"));
    assert!(session.poll(&host).contains("WKLY 84% left"));
    for event in [
        sample(12, 60, reset, "codex"),
        sample(5, 80, reset, "other"),
    ] {
        writeln!(f, "{event}").unwrap();
        assert!(session.poll(&host).contains("WKLY 84% left"));
    }
    host["account"]["rate_limits"][0]["primary_limit"]["used_percent"] = json!(20);
    assert!(session.poll(&host).contains("WKLY 80% left"));
    let empty = json!({"thread":{"id":"a"}});
    assert!(session.poll(&empty).contains("WKLY 80% left"));
    writeln!(f, "{}", sample(4, 25, reset, "codex")).unwrap();
    assert!(session.poll(&empty).contains("WKLY 75% left"));
    assert!(
        session
            .poll(&credit_state(reset + 604800))
            .contains("WKLY 18% left")
    );
    account(home.path(), "different-account");
    let mut foreign = provider::Provider::open(home.path());
    assert!(!foreign.poll(&empty).contains("WKLY"));
}

#[test]
fn tool_starts_and_failures_reach_the_next_read_once_even_in_a_large_chat() {
    for large in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let path = index(home.path());
        let mut session = provider::Provider::open(home.path());
        let state = json!({"thread":{"id":"a","agent":{"state":"working"}}});
        let call = |when: &str, id: &str, kind: &str, status: &str| json!({"type":"event_msg","payload":{"type":when,"item":{"id":id,"type":kind,"status":status}}});
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            "{}",
            call("item_started", "original", "CommandExecution", "running")
        )
        .unwrap();
        if large {
            writeln!(f, "{}", json!({"padding":"x".repeat(600_000)})).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        while !session.poll(&state).contains("Tools 1 · Shell 1") {
            assert!(
                large && Instant::now() < deadline,
                "small files must be ready on the first read"
            );
            thread::sleep(Duration::from_millis(5));
        }
        for id in ["one", "two", "three"] {
            writeln!(
                f,
                "{}",
                call("item_started", id, "CommandExecution", "running")
            )
            .unwrap();
        }
        assert!(session.poll(&state).contains("Tools 4 · Shell 4"));
        for id in ["original", "one", "original"] {
            writeln!(
                f,
                "{}",
                call("item_completed", id, "CommandExecution", "failed")
            )
            .unwrap();
        }
        let web = call("item_started", "four", "WebSearch", "running");
        write!(f, "{web}").unwrap();
        assert!(
            session
                .poll(&state)
                .contains("Tools 4 · Shell 4 · Failed 2")
        );
        writeln!(f).unwrap();
        for _ in 0..2 {
            assert!(
                session
                    .poll(&state)
                    .contains("Tools 5 · Shell 4 · Web 1 · Failed 2")
            );
        }
        assert!(
            !session
                .poll(&json!({"thread":{"id":"other"}}))
                .contains("Shell 4")
        );
        fs::rename(&path, home.path().join("previous.jsonl")).unwrap();
        fs::write(&path, "{}\n").unwrap();
        assert!(session.poll(&state).contains("Tools 0"));
    }
}

#[test]
fn context_updates_in_a_long_turn_whose_model_and_window_are_outside_the_tail() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let mut f = fs::File::create(&path).unwrap();
    for record in [
        json!({"type":"turn_context","payload":{"model":"m"}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"long","model_context_window":112000}}),
        json!({"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":112000,"total_token_usage":{"output_tokens":200},"last_token_usage":{"total_tokens":24000}}}}),
        json!({"type":"event_msg","payload":{"type":"plan_update","plan":[{"status":"completed"}]}}),
        json!({"padding":"a".repeat(600000)}),
    ] {
        writeln!(f, "{record}").unwrap();
    }
    let state = json!({"thread":{"id":"a","model":{"id":"m"},"usage":{"context":{"context_window_size":112000,"used_tokens":24000,"used_percentage":12},"total":{"output_tokens":200}}}});
    let mut session = provider::Provider::open(home.path());
    let deadline = Instant::now() + Duration::from_secs(4);
    while !session.poll(&state).contains("Plan 1/1") {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    for (turn, tokens, expected) in [
        ("stale", 100000, 12),
        ("long", 75000, 63),
        ("long", 78000, 66),
    ] {
        writeln!(f,"{}",json!({"type":"token_usage_record","payload":{"thread_id":"a","turn_id":turn,"usage":{"total_tokens":tokens}}})).unwrap();
        assert!(
            session
                .poll(&state)
                .contains(&format!("CTX {expected}% used"))
        );
    }
    fs::rename(&path, home.path().join("old.jsonl")).unwrap();
    fs::write(&path, "{}\n").unwrap();
    assert!(session.poll(&state).contains("CTX 12% used"));
}

#[test]
fn turn_speed_only_subtracts_totals_from_the_same_counter() {
    let home = tempfile::tempdir().unwrap();
    let path = index(home.path());
    let start = codex_statusline::usage::unix_now() - 10;
    let state = json!({"thread":{"id":"a","agent":{"state":"working"},"usage":{"total":{"output_tokens":11000}}}});
    for response_total in [2100, 20100] {
        observations(
            &path,
            &[
                json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"output_tokens":10000}}}}),
                json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"t","started_at":start}}),
                json!({"type":"token_usage_record","payload":{"turn_id":"t","turn_token_usage":{"output_tokens":100},"thread_token_usage":{"output_tokens":response_total}}}),
            ],
        );
        let row = common::plain(&rendered(home.path(), state.clone(), ""));
        let rate = row
            .split("Turn est. ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((80.0..=100.1).contains(&rate), "{response_total}: {row}");
    }
}
