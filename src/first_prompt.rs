use serde_json::{Value, json};

/// Only estimate the first prompt while the complete start of its rollout is available.
#[derive(Clone, Default)]
pub(crate) struct FirstPrompt {
    turn_id: Option<String>,
    window: Option<u64>,
    bytes: u64,
}

impl FirstPrompt {
    pub fn observe(&mut self, kind: &Value, p: &Value) -> bool {
        match (kind.as_str(), p["type"].as_str()) {
            (Some("event_msg"), Some("task_started")) => {
                let id = p["turn_id"].as_str();
                if self.turn_id.as_deref().is_some_and(|prev| Some(prev) != id) {
                    return false;
                }
                self.turn_id = id.map(str::to_owned);
                self.window = p["model_context_window"].as_u64().filter(|w| *w > 0);
            }
            (Some("response_item"), Some("message")) => {
                if let Some(parts) = p["content"].as_array() {
                    for part in parts {
                        match (part["type"].as_str(), part["text"].as_str()) {
                            (Some("input_text" | "output_text"), Some(text)) => {
                                self.bytes = self.bytes.saturating_add(text.len() as u64);
                            }
                            // Media size on disk does not determine its token cost.
                            _ => return false,
                        }
                    }
                }
            }
            (Some("response_item"), Some("additional_tools")) if p["tools"].is_array() => {
                self.bytes = self
                    .bytes
                    .saturating_add(p["tools"].to_string().len() as u64);
            }
            (Some("compacted"), _) | (Some("event_msg"), Some("context_compacted")) => {
                return false;
            }
            _ => {}
        }
        true
    }

    pub fn display(&self, model_matches: bool, live: &Value) -> Option<Value> {
        if live["used_percentage"].is_number() || live["used_tokens"].is_number() {
            return None;
        }
        if self.bytes == 0 {
            return Some(json!({"used_percentage":0,"estimated":false}));
        }
        let window = self
            .window
            .or_else(|| live["context_window_size"].as_u64())?;
        if !model_matches
            || window <= 12000
            || live["context_window_size"]
                .as_u64()
                .is_some_and(|w| w > 0 && w != window)
        {
            return None;
        }
        let mut estimate = crate::usage::restore(live, window, Some(self.bytes.div_ceil(4)));
        estimate["estimated"] = true.into();
        Some(estimate)
    }
}
