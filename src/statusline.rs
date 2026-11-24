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
    footer(&first.join(" · "), Vec::new(), "")
}
