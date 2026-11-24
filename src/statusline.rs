use crate::layout::footer;
use serde_json::Value;

pub fn render(state: &Value) -> Value {
    let Some(thread) = state["thread"].as_object() else {
        return Value::Null;
    };
    let cwd = thread
        .get("workspace")
        .and_then(|w| w["cwd"].as_str())
        .unwrap_or("");
    let mut first = vec![cwd];
    if let Some(name) = thread
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        first.push(name);
    }
    if let Some(model) = thread.get("model") {
        first.extend(
            model["display_name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| model["id"].as_str()),
        );
        first.extend(model["reasoning_level"].as_str().filter(|s| !s.is_empty()));
    }
    first.extend(
        thread
            .get("permissions")
            .and_then(|p| p["display_name"].as_str()),
    );
    let approval = thread
        .get("approval")
        .and_then(|a| a["policy"].as_str())
        .map(|p| format!("Approvals {p}"));
    first.extend(approval.as_deref());
    let version = state["app"]["cli_version"]
        .as_str()
        .map(|v| format!("CLI {v}"));
    let last = [version.as_deref(), thread.get("id").and_then(Value::as_str)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let mut middle = crate::usage::context(&state["thread"]["usage"]["context"], None);
    let weekly = crate::usage::weekly(&state["account"], crate::usage::unix_now());
    if !middle.is_empty() && !weekly.is_empty() {
        middle.push(serde_json::json!(" · "));
    }
    middle.extend(weekly);
    footer(&first.join(" · "), middle, &last)
}
