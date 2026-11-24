use serde_json::Value;

#[derive(Clone, Default)]
pub struct Turns {
    cumulative: u64,
    response_cumulative: u64,
    awaiting_native_mirror: Option<u64>,
    partial: bool,
    active: Option<(String, u64)>,
    observed: bool,
    output_started_ms: Option<i64>,
    last_response_rate: Option<f64>,
}

impl Turns {
    pub fn recent(partial: bool) -> Self {
        Self {
            partial,
            ..Default::default()
        }
    }
    pub fn observe(
        &mut self,
        event: &Value,
        call: Option<crate::generated_call::GeneratedCall<'_>>,
        last: &mut Option<String>,
    ) {
        let timestamp = &event["timestamp"];
        let payload = &event["payload"];
        if let Some(call) = call {
            if self.partial && self.active.is_none() {
                self.active = call.turn.map(|id| (id.to_owned(), self.cumulative));
            }
            if let Some(start) = call.created_ms {
                self.include_start(start);
            }
            return;
        }
        if event["type"] == "token_usage_record" {
            if self
                .active
                .as_ref()
                .is_some_and(|(id, _)| payload["turn_id"].as_str() == Some(id))
                && let Some(total) = payload
                    .pointer("/thread_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                && total > self.response_cumulative
            {
                self.response_cumulative = total;
                self.awaiting_native_mirror = payload["usage"]["output_tokens"].as_u64();
                self.record_rate(timestamp, &payload["usage"]["output_tokens"]);
            }
            return;
        }
        if event["type"] != "event_msg" {
            return;
        }
        let event = payload;
        if self.partial && self.active.is_none() && event["type"] == "item_completed" {
            self.active = event["turn_id"]
                .as_str()
                .map(|id| (id.to_owned(), self.cumulative));
        }
        match event["type"].as_str() {
            Some("task_started") => {
                self.active = event["turn_id"]
                    .as_str()
                    .map(|id| (id.into(), self.cumulative));
                self.response_cumulative = 0;
                self.awaiting_native_mirror = None;
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
                    self.include_start(start);
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
                    let response_output =
                        event["info"]["last_token_usage"]["output_tokens"].as_u64();
                    let mirror =
                        response_output.is_some() && self.awaiting_native_mirror == response_output;
                    if mirror {
                        self.awaiting_native_mirror = None;
                    }
                    if n > self.cumulative && !mirror {
                        self.record_rate(
                            timestamp,
                            &event["info"]["last_token_usage"]["output_tokens"],
                        );
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
                    .filter(|(_, ms)| *ms > 0 && self.observed && !self.partial)
                    .map(|(n, ms)| format!("Last {:.0} tok/s", n as f64 * 1000. / ms as f64));
            }
            _ => {}
        }
    }
    fn include_start(&mut self, start: i64) {
        self.output_started_ms = Some(self.output_started_ms.map_or(start, |old| old.min(start)));
    }
    fn record_rate(&mut self, timestamp: &Value, output: &Value) {
        let rate = self
            .output_started_ms
            .and_then(|start| response_rate(start, timestamp, output));
        if let Some(rate) = rate {
            self.output_started_ms = None;
            self.last_response_rate = Some(rate);
        }
    }
}
fn response_rate(start: i64, timestamp: &Value, output: &Value) -> Option<f64> {
    let end = timestamp_ms(timestamp)?;
    let elapsed = i128::from(end) - i128::from(start);
    let output = output.as_u64()?;
    (elapsed > 0 && output > 0).then(|| output as f64 * 1000. / elapsed as f64)
}
pub(crate) fn timestamp_ms(timestamp: &Value) -> Option<i64> {
    let ns = time::OffsetDateTime::parse(
        timestamp.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?
    .unix_timestamp_nanos();
    i64::try_from(ns / 1_000_000).ok()
}
