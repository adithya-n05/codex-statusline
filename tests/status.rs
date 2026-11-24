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
#[test]
fn context_uses_the_displayed_models_window_and_native_percentage() {
    for (context, start, end) in [
        (
            json!({"context_window_size":258400,"used_tokens":120000,"used_percentage":48}),
            "CTX 48% [",
            " / 258400",
        ),
        (
            json!({"context_window_size":1000,"used_tokens":250}),
            "CTX 25% [",
            " / 1000",
        ),
        (json!({"context_window_size":800000}), "CTX —", " / 800000"),
        (json!({"used_percentage":12}), "CTX 12% [", "]"),
        (
            json!({"context_window_size":0}),
            "Context awaiting usage",
            "",
        ),
    ] {
        let text = screen(json!({"usage":{"context":context}}));
        let row = text.split('\n').nth(1).unwrap();
        assert!(row.starts_with(start) && row.ends_with(end), "{row}");
        if start.is_empty() {
            assert!(row.is_empty());
        }
    }
}
#[test]
fn weekly_allowance_decreases_and_counts_down_to_the_reported_reset() {
    use codex_statusline::usage::weekly;
    let account = json!({"rate_limits":[
        {"limit_id":"other","secondary_limit":{"window_minutes":10080,"used_percent":99}},
        {"limit_id":"codex","primary_limit":{"window_minutes":300,"used_percent":90},
         "secondary_limit":{"window_minutes":10080,"used_percent":30,"resets_at":186500}}]});
    let tree = weekly(&account, 10000);
    assert_eq!(
        tree.iter().map(plain).collect::<String>(),
        "7D 70% left [██████████████░░░░░░] · reset 2d 1h"
    );
    assert!(weekly(&json!({}), 0).is_empty());
    assert!(
        weekly(
            &json!({"rate_limits":[{"primary_limit":{"window_minutes":300,"used_percent":0}}]}),
            0
        )
        .is_empty()
    );
    for (delta, expected) in [(3601, "1h 0m"), (61, "1m"), (0, "now")] {
        let x = weekly(&account, 186500 - delta);
        assert!(x.iter().map(plain).collect::<String>().ends_with(expected));
    }
    let x = weekly(
        &json!({"rate_limits":[{"primary_limit":{"window_minutes":10080,"used_percent":105}}]}),
        0,
    );
    assert!(
        x.iter()
            .map(plain)
            .collect::<String>()
            .starts_with("7D 0% left")
    );
}
#[test]
fn api_spend_uses_cli_thread_totals_without_a_model_price_table() {
    use codex_statusline::{account::Identity, statusline::render_for};
    let state = json!({"thread":{"model":{"id":"future"},"usage":{
        "total":{"total_input_tokens":13000,"output_tokens":420},"cost":{"total_cost_usd":0.12432}}}});
    let row = plain(&render_for(&state, &Identity::Api));
    assert!(
        row.contains("Thread 13000 in / 420 out · est. $0.1243 · API key"),
        "{row}"
    );
    assert!(!plain(&render_for(&json!({"thread":{}}), &Identity::Api)).contains('$'));
    assert!(!plain(&render_for(&state, &Identity::ChatGpt(None))).contains('$'));
}
#[test]
fn home_paths_abbreviate_only_the_actual_home_directory_component() {
    let h = std::env::var("HOME").unwrap();
    for (input, expected) in [
        (h.clone(), "~".into()),
        (format!("{h}/work/app"), "~/work/app".into()),
        (format!("{h}-copy/repo"), format!("{h}-copy/repo")),
        ("/".into(), "/".into()),
    ] {
        assert_eq!(
            screen(json!({"workspace":{"cwd":input}}))
                .lines()
                .next()
                .unwrap(),
            expected
        );
    }
}
#[test]
fn the_compaction_warning_requires_a_known_threshold() {
    use codex_statusline::usage::context;
    let sample = json!({"used_percentage":96,"used_tokens":960,"context_window_size":1000});
    assert_eq!(context(&sample, None)[1]["props"]["color"], "#E78A53");
    assert_eq!(context(&sample, Some(90.))[1]["props"]["color"], "#E05252");
    assert_eq!(
        context(&sample, Some(89.99))[1]["props"]["color"],
        "#E78A53"
    );
}
#[test]
fn active_service_tier_follows_effort_and_precedes_permissions() {
    for (raw, display, want) in [
        ("normal", "", "Normal"),
        ("priority", "", "Fast"),
        ("fast", "Fast", "Fast"),
        ("ultrafast", "Ultrafast", "Ultrafast"),
        ("custom", "Burst", "Burst"),
        ("new-tier", "", "new-tier"),
    ] {
        let row = screen(
            json!({"workspace":{"cwd":"Dir"},"model":{"id":"Test","reasoning_level":"xhigh","service_tier":raw,"service_tier_display_name":display},"permissions":{"display_name":"Full Access"}}),
        );
        assert_eq!(
            row.lines().next().unwrap(),
            format!("Dir · Test · xhigh · {want} · Full Access")
        );
    }
}
