use codex_statusline::statusline::render;
use serde_json::{Value, json};

mod common;
use common::plain;

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
            "CTX 48% used [",
            "]",
        ),
        (
            json!({"context_window_size":1000,"used_tokens":250}),
            "CTX 25% used [",
            "]",
        ),
        (
            json!({"context_window_size":800000}),
            "CTX awaiting usage",
            "usage",
        ),
        (json!({"used_percentage":12}), "CTX 12% used [", "]"),
        (json!({"context_window_size":0}), "CTX awaiting usage", ""),
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
        "WKLY 70% left [██████████████░░░░░░] · reset 2d 1h"
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
            .starts_with("WKLY 0% left")
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
        ("normal", "", "Standard"),
        ("normal", "Normal", "Standard"),
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

#[test]
fn healthy_usage_is_readable_and_an_empty_allowance_stands_out() {
    let state = json!({
        "thread": {"usage":{"context":{"used_percentage":25}}},
        "account": {"rate_limits":[{"limit_id":"codex","secondary_limit":{
            "window_minutes":10080,"used_percent":100,"resets_at":1
        }}]}
    });
    let footer = render(&state);
    let colored: Vec<_> = footer["children"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|part| {
            Some((
                part["children"].get(0)?.as_str()?,
                part["props"]["color"].as_str()?,
            ))
        })
        .filter(|(text, _)| !text.is_empty())
        .collect();
    assert_eq!(
        colored,
        [
            ("CTX 25% used ", "#C1C1C1"),
            ("[█████░░░░░░░░░░░░░░░]", "#5F8787"),
            ("WKLY 0% left ", "#E05252"),
            ("[░░░░░░░░░░░░░░░░░░░░]", "#E05252"),
            (" · reset now", "#969696"),
        ]
    );
}

#[test]
fn the_first_row_colors_each_field_by_its_role() {
    let state = json!({"thread":{"workspace":{"cwd":"Dir"},"name":"Design · review",
        "model":{"display_name":"Test model","reasoning_level":"xhigh","service_tier":"normal"},
        "permissions":{"display_name":"Full access"},"approval":{"policy":"never"}}});
    let tree = render(&state);
    fn colored(node: &Value, inherited: Option<&str>, out: &mut Vec<(String, String)>) {
        let color = node
            .pointer("/props/color")
            .and_then(Value::as_str)
            .or(inherited);
        if let Some(s) = node.as_str() {
            out.push((
                s.replace('\u{a0}', " ").replace('\u{2060}', ""),
                color.unwrap_or("").into(),
            ));
        } else if let Some(kids) = node["children"].as_array() {
            for child in kids {
                colored(child, color, out);
            }
        }
    }
    let mut leaves = Vec::new();
    colored(
        &tree["children"][0],
        tree.pointer("/props/color").and_then(Value::as_str),
        &mut leaves,
    );
    for (word, hue) in [
        ("Dir", "#FBCB97"),
        ("Design · review", "#C1C1C1"),
        ("Test model", "#E78A53"),
        ("xhigh", "#FBCB97"),
        ("Standard", "#5F8787"),
        ("Full access", "#C1C1C1"),
        ("Approvals never", "#969696"),
    ] {
        assert!(
            leaves
                .iter()
                .any(|(label, color)| label == word && color == hue),
            "{word}: {leaves:?}"
        );
    }
    for n in [8177, 8180, 8185, 8187, 8189, 8190, 8191, 8192, 8193] {
        let mut long = state.clone();
        long["thread"]["name"] = json!("界".repeat(n / 3) + "e\u{301}");
        let row = plain(&render(&long)["children"][0]);
        assert!(
            row.len() <= 8192 && row.ends_with('…'),
            "{} bytes",
            row.len()
        );
    }
}

#[test]
fn field_dividers_have_one_foreground_across_all_three_rows_and_account_modes() {
    use codex_statusline::{account::Identity, runtime::Extra, statusline::render_enriched};
    let state = json!({"app":{"cli_version":"v1"},"thread":{"id":"session","name":"Title","workspace":{"cwd":"Dir"},
        "model":{"id":"model","reasoning_level":"high"},"usage":{"context":{"used_percentage":30},
        "total":{"total_input_tokens":45,"output_tokens":12},"cost":{"total_cost_usd":0.05}}},
        "account":{"rate_limits":[{"limit_id":"codex","secondary_limit":{"window_minutes":10080,"used_percent":55,"resets_at":1}}]}});
    let mut extra = Extra::default();
    extra.history.speed = Some("Last 50 tok/s".into());
    fn check(node: &Value, inherited: &str, counts: &mut Vec<usize>) {
        let hue = node
            .pointer("/props/color")
            .and_then(Value::as_str)
            .unwrap_or(inherited);
        if let Some(s) = node.as_str() {
            if s == "\n" {
                counts.push(0);
            }
            for _ in s.matches('·') {
                assert_eq!(hue, "#969696", "{s}");
                *counts.last_mut().unwrap() += 1;
            }
        } else if let Some(children) = node["children"].as_array() {
            for child in children {
                check(child, hue, counts);
            }
        }
    }
    for account in [Identity::Api, Identity::ChatGpt(None)] {
        let tree = render_enriched(&state, &account, &extra);
        let mut counts = vec![0];
        check(&tree, "", &mut counts);
        assert_eq!(counts, [3, 4, 1]);
    }
}
