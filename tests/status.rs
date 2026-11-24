use codex_statusline::statusline::render;
use serde_json::{Value, json};

fn plain(tree: &Value) -> String {
    match tree {
        Value::String(s) => s.replace('\u{2060}', "").replace('\u{a0}', " "),
        Value::Object(_) => tree["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(plain)
            .collect(),
        _ => String::new(),
    }
}

fn screen(thread: Value) -> String {
    plain(&render(&json!({"thread":thread})))
}
#[test]
fn actual_chat_title_follows_the_directory_and_can_be_absent() {
    for (name, expected) in [
        (json!("Review README"), "~/work · Review README"),
        (Value::Null, "~/work"),
        (json!(" "), "~/work"),
    ] {
        let value = screen(json!({"workspace":{"cwd":"~/work"},"name":name}));
        assert_eq!(value.lines().next().unwrap(), expected);
    }
}
#[test]
fn new_models_and_effort_levels_are_read_from_the_snapshot() {
    for (model, expected) in [
        (
            json!({"display_name":"Future Model","id":"future-8","reasoning_level":"ultra"}),
            "Dir · Future Model · ultra",
        ),
        (json!({"id":"custom-123"}), "Dir · custom-123"),
        (json!({}), "Dir"),
    ] {
        assert_eq!(
            screen(json!({"workspace":{"cwd":"Dir"},"model":model}))
                .lines()
                .next()
                .unwrap(),
            expected
        );
    }
}
#[test]
fn permission_scope_and_approval_policy_remain_independent() {
    for (scope, policy) in [
        ("Full access", "never"),
        ("Read only", "never"),
        ("Workspace write", "on-request"),
    ] {
        let thread = json!({"workspace":{"cwd":"Dir"},"permissions":{"display_name":scope},"approval":{"policy":policy}});
        assert_eq!(
            screen(thread).lines().next().unwrap(),
            format!("Dir · {scope} · Approvals {policy}")
        );
    }
}
#[test]
fn metadata_closes_the_third_row_and_follows_the_displayed_session() {
    for id in ["019abc-001", "019abc-002"] {
        let state =
            json!({"app":{"cli_version":"1.2.3"},"thread":{"id":id,"workspace":{"cwd":"Dir"}}});
        let footer = plain(&render(&state));
        assert_eq!(
            footer.split('\n').nth(2),
            Some(format!("CLI 1.2.3 · {id}").as_str())
        );
    }
}
