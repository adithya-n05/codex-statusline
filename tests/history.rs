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
