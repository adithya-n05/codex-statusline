use codex_statusline::history::History;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

#[test]
fn plan_tracks_complete_appended_records_and_resets_on_rotation() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("rollout.jsonl");
    let mut h = History::default();
    assert!(h.read(&path).is_none());
    fs::write(&path, "invalid\n").unwrap();
    let plan = json!({"plan":[{"step":"first","status":"completed"},{"step":"second","status":"pending"}]});
    let call = json!({"type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":plan.to_string()}});
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    write!(f, "{call}").unwrap();
    assert_eq!(h.read(&path).unwrap().plan, None);
    writeln!(f).unwrap();
    assert_eq!(h.read(&path).unwrap().plan.as_deref(), Some("Plan 1/2"));
    assert_eq!(h.read(&path).unwrap().plan.as_deref(), Some("Plan 1/2"));
    writeln!(
        f,
        "{}",
        json!({"type":"event_msg","payload":{"type":"plan_update","plan":[]}})
    )
    .unwrap();
    assert_eq!(h.read(&path).unwrap().plan, None);
    writeln!(f, "{call}").unwrap();
    assert!(h.read(&path).unwrap().plan.is_some());
    fs::rename(&path, root.path().join("old")).unwrap();
    fs::write(&path, "{}\n").unwrap();
    assert_eq!(h.read(&path).unwrap().plan, None);
}
#[test]
fn oversized_unfinished_events_never_become_a_plan_when_more_bytes_arrive() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.jsonl");
    fs::write(&path, vec![b'x'; 2 * 1024 * 1024 + 10]).unwrap();
    let mut history = History::default();
    assert!(history.read(&path).unwrap().plan.is_none());
    let event =
        json!({"type":"event_msg","payload":{"type":"plan_update","plan":[{"status":"pending"}]}});
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "{event}").unwrap();
    assert!(history.read(&path).unwrap().plan.is_none());
    writeln!(f, "{event}").unwrap();
    assert_eq!(
        history.read(&path).unwrap().plan.as_deref(),
        Some("Plan 0/1")
    );
}
#[test]
fn tool_activity_counts_completed_operations_once_and_keeps_failures_visible() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tools.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    for (id, kind, status) in [
        ("1", "CommandExecution", "Completed"),
        ("2", "FileChange", "Failed"),
        ("2", "FileChange", "Failed"),
        ("3", "McpToolCall", "failed"),
        ("4", "WebSearch", "Completed"),
        ("5", "FutureToolCall", "Completed"),
        ("6", "AgentMessage", "Completed"),
    ] {
        writeln!(f,"{}",json!({"type":"event_msg","payload":{"type":"item_completed","item":{"id":id,"type":kind,"status":status}}})).unwrap();
    }
    let mut h = History::default();
    assert_eq!(
        h.read(&path).unwrap().tools_label().as_deref(),
        Some("Tools 5 · Shell 1 · MCP 1 · Web 1 · Patch 1 · Other 1 · Failed 2")
    );
    assert_eq!(
        h.read(&path).unwrap().tools_label().as_deref(),
        Some("Tools 5 · Shell 1 · MCP 1 · Web 1 · Patch 1 · Other 1 · Failed 2")
    );
}
#[test]
fn tool_activity_includes_completed_codex_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extensions.jsonl");
    let mut f = fs::File::create(&path).unwrap();
    for (id, kind) in [
        ("1", "web.search"),
        ("2", "clock.sleep"),
        ("3", "future.tool"),
        ("1", "web.search"),
    ] {
        writeln!(f,"{}",json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"Extension","kind":kind,"id":id}}})).unwrap();
    }
    assert_eq!(
        History::default()
            .read(&path)
            .unwrap()
            .tools_label()
            .as_deref(),
        Some("Tools 3 · Web 1 · Other 2")
    );
}
#[test]
fn abort_preserves_the_latest_generation_rate_only_for_its_own_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("speed.jsonl");
    let mut f = fs::File::create(&path).unwrap();
    let mut history = History::default();
    for (payload, want) in [
        (json!({"type":"task_started","turn_id":"a"}), None),
        (
            json!({"type":"item_completed","turn_id":"a","item":{"type":"AgentMessage"},"started_at_ms":1735689600000u64}),
            None,
        ),
        (
            json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":42},"last_token_usage":{"output_tokens":42}}}),
            Some("Live 42 tok/s"),
        ),
        (
            json!({"type":"turn_aborted","turn_id":"stale"}),
            Some("Live 42 tok/s"),
        ),
        (
            json!({"type":"turn_aborted","turn_id":"a"}),
            Some("Last 42 tok/s"),
        ),
        (
            json!({"type":"turn_aborted","turn_id":"a"}),
            Some("Last 42 tok/s"),
        ),
    ] {
        writeln!(
            f,
            "{}",
            json!({"timestamp":"2025-01-01T00:00:01Z","type":"event_msg","payload":payload})
        )
        .unwrap();
        assert_eq!(history.read(&path).unwrap().speed.as_deref(), want);
    }
}
#[test]
fn distinct_shell_launches_and_terminal_interactions_are_counted_separately() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calls.jsonl");
    let mut f = fs::File::create(&path).unwrap();
    for (id, source, status) in [
        ("c1", "unified_exec_startup", "completed"),
        ("p1", "unified_exec_interaction", "completed"),
        ("p1", "unified_exec_interaction", "completed"),
        ("c2", "unified_exec_startup", "failed"),
        ("c2", "unified_exec_startup", "failed"),
    ] {
        writeln!(f,"{}",json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":id,"process_id":"42","source":source,"status":status}}})).unwrap();
    }
    writeln!(f,"{}",json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","id":"m1","status":"completed"}}})).unwrap();
    let mut history = History::default();
    for _ in 0..2 {
        assert_eq!(
            history.read(&path).unwrap().tools_label().as_deref(),
            Some("Tools 4 · Shell 2 · MCP 1 · Terminal I/O 1 · Failed 1")
        );
    }
}
#[test]
fn a_batch_counts_the_individual_requests_even_before_their_completions_arrive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("batch.jsonl");
    let mut f = fs::File::create(&path).unwrap();
    let mut history = History::default();
    let mut append = |value: serde_json::Value| writeln!(f, "{value}").unwrap();
    append(
        json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":"batch"}}),
    );
    let lifecycle = |when: &str, id: &str, status: &str| json!({"type":"event_msg","payload":{"type":when,"item":{"type":"CommandExecution","source":"unified_exec_startup","id":id,"process_id":"same","status":status}}});
    for id in ["c1", "c2", "c3"] {
        append(lifecycle("item_started", id, "in_progress"));
    }
    assert_eq!(
        history.read(&path).unwrap().tools_label().as_deref(),
        Some("Tools 3 · Shell 3")
    );
    for id in ["c3", "c1", "c2", "c2"] {
        append(lifecycle(
            "item_completed",
            id,
            if id == "c2" { "failed" } else { "completed" },
        ));
    }
    append(
        json!({"type":"response_item","payload":{"type":"function_call","namespace":"mcp__example","name":"read","call_id":"m1"}}),
    );
    append(
        json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","id":"m1","result":{"isError":true}}}}),
    );
    append(
        json!({"type":"response_item","payload":{"type":"function_call","name":"update_plan","call_id":"p1","arguments":"{}"}}),
    );
    append(
        json!({"type":"response_item","payload":{"type":"function_call","name":"wait","call_id":"parent-poll"}}),
    );
    assert_eq!(
        history.read(&path).unwrap().tools_label().as_deref(),
        Some("Tools 5 · Shell 3 · MCP 1 · Other 1 · Failed 2")
    );
}
