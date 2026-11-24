use serde_json::Value;

#[derive(Default)]
pub struct Turns {
    cumulative: u64,
    active: Option<(String, u64)>,
    observed: bool,
    output_started_ms: Option<i64>,
    last_response_rate: Option<f64>,
}

impl Turns {
    pub fn observe(&mut self, event: &Value, last: &mut Option<String>) {
        let timestamp = &event["timestamp"];
        let event = &event["payload"];
        match event["type"].as_str() {
            Some("task_started") => {
                self.active = event["turn_id"]
                    .as_str()
                    .map(|id| (id.into(), self.cumulative));
                self.observed = false;
                self.output_started_ms = None;
                self.last_response_rate = None;
            }
            Some("item_completed")
                if matches!(
                    event["item"]["type"].as_str(),
                    Some("Reasoning" | "AgentMessage" | "Plan")
                ) && self
                    .active
                    .as_ref()
                    .is_some_and(|(id, _)| event["turn_id"].as_str() == Some(id)) =>
            {
                if let Some(start) = event["started_at_ms"].as_i64() {
                    self.output_started_ms =
                        Some(self.output_started_ms.map_or(start, |old| old.min(start)));
                }
            }
            Some("turn_aborted")
                if self
                    .active
                    .as_ref()
                    .is_some_and(|(id, _)| event["turn_id"].as_str().is_none_or(|v| v == id)) =>
            {
                self.active = None;
                self.output_started_ms = None;
                if let Some(rate) = self.last_response_rate.take() {
                    *last = Some(format!("Last {rate:.0} tok/s"));
                }
            }
            Some("token_count") => {
                if let Some(n) = event
                    .pointer("/info/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    if n > self.cumulative
                        && let Some(start) = self.output_started_ms.take()
                        && let Some(rate) = response_rate(start, timestamp, event)
                    {
                        self.last_response_rate = Some(rate);
                        *last = Some(format!("Live {rate:.0} tok/s"));
                    }
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
                if let Some(rate) = self.last_response_rate.take() {
                    *last = Some(format!("Last {rate:.0} tok/s"));
                    return;
                }
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
fn response_rate(start: i64, timestamp: &Value, event: &Value) -> Option<f64> {
    let end = time::OffsetDateTime::parse(
        timestamp.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?
    .unix_timestamp_nanos()
        / 1_000_000;
    let elapsed = end - i128::from(start);
    let output = event
        .pointer("/info/last_token_usage/output_tokens")?
        .as_u64()?;
    (elapsed > 0 && output > 0).then(|| output as f64 * 1000. / elapsed as f64)
}
