use serde_json::Value;

/// The sampled request as saved in a rollout. Execution events can contain the same
/// command again, plus environment details and tool output that the model never wrote.
pub(crate) struct GeneratedCall<'a> {
    pub id: &'a str,
    pub turn: Option<&'a str>,
    pub bytes: u64,
    pub created_ms: Option<i64>,
}

impl<'a> GeneratedCall<'a> {
    pub fn parse(record: &'a Value) -> Option<Self> {
        if record["type"] != "response_item" || record["metadata"]["client_authored"] == true {
            return None;
        }
        let p = &record["payload"];
        let kind = p["type"].as_str()?;
        let id = [&p["call_id"], &p["id"]]
            .into_iter()
            .filter_map(Value::as_str)
            .find(|s| !s.is_empty())?;
        let bytes = match kind {
            "function_call" | "custom_tool_call" => {
                let name = p["name"].as_str()?;
                let field = if kind == "function_call" {
                    "arguments"
                } else {
                    "input"
                };
                let namespace = p["namespace"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map_or(0, |s| s.len() + 1);
                namespace + name.len() + p[field].as_str()?.len()
            }
            "tool_search_call" | "web_search_call" | "local_shell_call" => {
                let field = if kind == "tool_search_call" {
                    "arguments"
                } else {
                    "action"
                };
                let input = p.get(field).filter(|v| !v.is_null())?;
                kind.len() + input.to_string().len()
            }
            _ => return None,
        };
        let turn = p
            .pointer("/internal_chat_message_metadata_passthrough/turn_id")
            .and_then(Value::as_str)
            .or_else(|| p["turn_id"].as_str());
        Some(Self {
            id,
            turn,
            bytes: bytes as u64,
            created_ms: p
                .pointer("/internal_chat_message_metadata_passthrough/create_time")
                .and_then(Value::as_f64)
                .map(|seconds| (seconds * 1000.).round() as i64),
        })
    }
}
