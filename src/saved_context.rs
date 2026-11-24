use serde_json::Value;
#[derive(Default, Clone)]
pub struct SavedContext {
    model: String,
    reading: Option<(u64, u64)>,
    turn: Option<String>,
    output_at_reading: Option<u64>,
    response_context: Option<u64>,
    first_prompt: Option<crate::first_prompt::FirstPrompt>,
}
impl SavedContext {
    pub fn observe(&mut self, event: &Value) {
        let p = &event["payload"];
        if event["type"] == "session_meta" && p["forked_from_id"].is_null() {
            self.first_prompt = Some(Default::default());
        } else if self
            .first_prompt
            .as_mut()
            .is_some_and(|first| !first.observe(&event["type"], p))
        {
            self.first_prompt = None;
        }
        let model = match (event["type"].as_str(), p["type"].as_str()) {
            (Some("turn_context"), _) => p["model"].as_str(),
            (Some("event_msg"), Some("thread_settings_applied")) => {
                p.pointer("/thread_settings/model").and_then(Value::as_str)
            }
            (Some("event_msg"), Some("task_started")) => {
                self.turn = p["turn_id"].as_str().map(str::to_owned);
                self.response_context = None;
                if self.reading.is_some_and(|(window, _)| {
                    p["model_context_window"]
                        .as_u64()
                        .is_some_and(|n| n != window)
                }) {
                    self.reading = None;
                }
                return;
            }
            (Some("compacted"), _) | (Some("event_msg"), Some("context_compacted")) => {
                self.reading = None;
                self.turn = None;
                self.response_context = None;
                return;
            }
            (Some("token_usage_record"), _) => {
                if self.turn.is_some()
                    && p["turn_id"].as_str() == self.turn.as_deref()
                    && let Some(used) = p.pointer("/usage/total_tokens").and_then(Value::as_u64)
                {
                    self.response_context = Some(used);
                }
                if let Some(reading) = self
                    .first_prompt
                    .as_ref()
                    .and_then(|first| first.reported_usage(p))
                {
                    self.reading = Some(reading);
                }
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
                    self.first_prompt = None;
                    self.response_context = None;
                    self.output_at_reading = p
                        .pointer("/info/total_token_usage/output_tokens")
                        .and_then(Value::as_u64);
                }
                return;
            }
            _ => None,
        };
        if let Some(model) = model
            && self.model != model
        {
            if !self.model.is_empty() {
                self.first_prompt = None;
            }
            self.response_context = None;
            self.output_at_reading = None;
            self.model = model.to_owned();
            self.reading = None;
        }
    }
    /// Compare cumulative output from two Codex token_count snapshots.
    /// The thread total in token_usage_record uses a different accounting baseline.
    pub fn ahead_of_snapshot(&self, thread: &Value) -> Option<Value> {
        let (window, used) = self.reading?;
        let live = &thread["usage"]["context"];
        if self.model.is_empty()
            || thread["model"]["id"].as_str() != Some(&self.model)
            || live["context_window_size"].as_u64()? != window
        {
            return None;
        }
        let disk_output = self.output_at_reading?;
        let host_output = thread.pointer("/usage/total/output_tokens")?.as_u64()?;
        let fresh = match disk_output.cmp(&host_output) {
            std::cmp::Ordering::Greater => self.response_context.unwrap_or(used),
            std::cmp::Ordering::Equal if live["used_tokens"].as_u64() == Some(used) => {
                self.response_context?
            }
            _ => return None,
        };
        Some(crate::usage::restore(&Value::Null, window, Some(fresh)))
    }
    pub fn restore(&self, model: &Value, live: &Value) -> Option<Value> {
        let Some((window, used)) = self.reading else {
            return self.first_prompt.as_ref()?.display(
                !self.model.is_empty() && model.as_str() == Some(&self.model),
                live,
            );
        };
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
