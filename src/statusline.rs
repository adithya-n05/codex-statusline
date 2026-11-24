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
    footer(cwd, Vec::new(), "")
}
