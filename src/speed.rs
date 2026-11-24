use serde_json::Value;

#[derive(Default)]
pub struct Turns {
    cumulative: u64,
    active: Option<(String, u64)>,
    observed: bool,
}

impl Turns {
    pub fn observe(&mut self, event: &Value, last: &mut Option<String>) {
        match event["type"].as_str() {
            Some("task_started") => {
                self.active = event["turn_id"]
                    .as_str()
                    .map(|id| (id.into(), self.cumulative));
                self.observed = false;
            }
            Some("token_count") => {
                if let Some(n) = event
                    .pointer("/info/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.cumulative = n;
                    self.observed = true;
                }
            }
            Some("task_complete")
                if self
                    .active
                    .as_ref()
                    .is_some_and(|(id, _)| event["turn_id"].as_str() == Some(id)) =>
            {
                let (_, start) = self.active.take().unwrap();
                *last = self
                    .cumulative
                    .checked_sub(start)
                    .zip(event["duration_ms"].as_u64())
                    .filter(|(_, ms)| *ms > 0 && self.observed)
                    .map(|(n, ms)| format!("Last {:.0} tok/s", n as f64 * 1000. / ms as f64));
            }
            _ => {}
        }
    }
}
