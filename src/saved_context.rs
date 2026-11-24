use serde_json::Value;
#[derive(Default, Clone)]
pub struct SavedContext {
    model: String,
    reading: Option<(u64, u64)>,
}
impl SavedContext {
    pub fn observe(&mut self, event: &Value) {
        let p = &event["payload"];
        let model = match (event["type"].as_str(), p["type"].as_str()) {
            (Some("turn_context"), _) => p["model"].as_str(),
            (Some("event_msg"), Some("thread_settings_applied")) => {
                p.pointer("/thread_settings/model").and_then(Value::as_str)
            }
            (Some("compacted"), _) | (Some("event_msg"), Some("context_compacted")) => {
                self.reading = None;
                return;
            }
            (Some("event_msg"), Some("token_count")) => {
                if let Some(reading) = p
                    .pointer("/info/model_context_window")
                    .and_then(Value::as_u64)
                    .filter(|w| *w > 0)
                    .zip(
                        p.pointer("/info/last_token_usage/total_tokens")
                            .and_then(Value::as_u64),
                    )
                {
                    self.reading = Some(reading);
                }
                return;
            }
            _ => None,
        };
        if let Some(model) = model
            && self.model != model
        {
            self.model = model.to_owned();
            self.reading = None;
        }
    }
    pub fn restore(&self, model: &Value, live: &Value) -> Option<Value> {
        let (window, used) = self.reading?;
        if self.model.is_empty()
            || model.as_str() != Some(&self.model)
            || live["context_window_size"]
                .as_u64()
                .is_some_and(|w| w > 0 && w != window)
        {
            return None;
        }
        Some(crate::usage::restore(live, window, Some(used)))
    }
}
