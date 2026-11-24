use crate::layout::{assemble_footer, header};
use serde_json::Value;

pub fn render(state: &Value) -> Value {
    render_for(state, &crate::account::Identity::Unknown)
}

pub fn render_for(state: &Value, identity: &crate::account::Identity) -> Value {
    render_enriched(state, identity, &crate::runtime::Extra::default())
}
pub fn render_enriched(
    state: &Value,
    identity: &crate::account::Identity,
    extra: &crate::runtime::Extra,
) -> Value {
    let Some(thread) = state["thread"].as_object() else {
        return Value::Null;
    };
    let cwd = thread
        .get("workspace")
        .and_then(|w| w["cwd"].as_str())
        .unwrap_or("");
    let abbreviated = std::env::var_os("HOME").and_then(|home| {
        let suffix = std::path::Path::new(cwd).strip_prefix(&home).ok()?;
        Some(
            std::path::Path::new("~")
                .join(suffix)
                .display()
                .to_string()
                .trim_end_matches('/')
                .to_owned(),
        )
    });
    let mut first = vec![(abbreviated.as_deref().unwrap_or(cwd), "#FBCB97")];
    if let Some(name) = thread
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or(extra.saved_name.as_deref())
    {
        first.push((name, "#C1C1C1"));
    }
    if let Some(model) = thread.get("model") {
        first.extend(
            model["display_name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| model["id"].as_str())
                .map(|name| (name, "#E78A53")),
        );
        first.extend(
            model["reasoning_level"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| (s, "#FBCB97")),
        );
        let tier = model["service_tier_display_name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                Some(match model["service_tier"].as_str()? {
                    "default" | "normal" => "Normal",
                    "priority" | "fast" => "Fast",
                    "ultrafast" => "Ultrafast",
                    "" => return None,
                    other => other,
                })
            });
        first.extend(tier.map(|name| {
            let label = if name.eq_ignore_ascii_case("normal") {
                "Standard"
            } else {
                name
            };
            (label, "#5F8787")
        }));
    }
    first.extend(
        thread
            .get("permissions")
            .and_then(|p| p["display_name"].as_str())
            .map(|s| (s, "#969696")),
    );
    let approval = thread
        .get("approval")
        .and_then(|a| a["policy"].as_str())
        .map(|p| format!("Approvals {p}"));
    first.extend(approval.as_deref().map(|s| (s, "#969696")));
    first.extend(extra.git.as_deref().map(|s| (s, "#5F8787")));
    first.extend(extra.history.plan.as_deref().map(|s| (s, "#FBCB97")));
    let version = state["app"]["cli_version"]
        .as_str()
        .map(|v| format!("CLI {v}"));
    let tools = extra.history.tools_label();
    let last = [
        tools.as_deref(),
        version.as_deref(),
        thread.get("id").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let live = &state["thread"]["usage"]["context"];
    let recorded = extra
        .history
        .context
        .restore(&state["thread"]["model"]["id"], live);
    let context = extra
        .measured_context
        .as_ref()
        .or(recorded.as_ref())
        .unwrap_or(live);
    let recovered = extra.compaction.as_ref().map(|sample| {
        if context["estimated"].is_boolean() {
            let measured = sample.restore(live);
            if measured["used_percentage"].is_number() {
                return measured;
            }
        }
        sample.restore(context)
    });
    let context = recovered.as_ref().unwrap_or(context);
    let pressure = extra.compaction.as_ref().and_then(|s| s.pressure(context));
    let mut middle = crate::usage::context(context, pressure);
    let now_ms = crate::usage::unix_now_ms();
    let now = now_ms / 1000;
    let account = extra.weekly_account.as_ref().unwrap_or(&state["account"]);
    let live_reset = crate::usage::weekly_window(account).and_then(|w| w["resets_at"].as_i64());
    crate::layout::paint_gauge_labels(&mut middle);
    let mut weekly =
        if matches!(identity, crate::account::Identity::Api) {
            crate::usage::api_thread(&state["thread"]["usage"])
        } else {
            if let Some(credit) = extra.credits.as_ref().filter(|c| {
                c.reset > now && live_reset.is_none_or(|r| r > now && c.matches_reset(r))
            }) {
                credit.render(now)
            } else {
                crate::usage::weekly(account, now)
            }
        };
    if !middle.is_empty() && !weekly.is_empty() {
        middle.push(serde_json::json!(" · "));
    }
    crate::layout::paint_gauge_labels(&mut weekly);
    middle.extend(weekly);
    let turn_rate =
        extra
            .history
            .turn_rate
            .label(&state["thread"], now_ms, extra.history.speed.as_deref());
    for (label, color) in [
        (identity.label(), "#E78A53"),
        (turn_rate.as_deref(), "#FBCB97"),
    ] {
        let Some(label) = label else { continue };
        if !middle.is_empty() {
            middle.push(serde_json::json!(" · "));
        }
        middle.push(crate::layout::painted(label, color));
    }
    assemble_footer(header(&first), middle, &last)
}
