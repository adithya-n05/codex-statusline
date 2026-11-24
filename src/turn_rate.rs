use serde_json::Value;

#[derive(Clone, Default)]
pub(crate) struct TurnRate {
    id: Option<String>,
    started_ms: Option<i64>,
    thread_total: Option<u64>,
    baseline: Option<u64>,
    confirmed: u64,
    finished: bool,
}

impl TurnRate {
    pub fn observe(&mut self, record: &Value) {
        let p = &record["payload"];
        let turn = p["turn_id"].as_str();
        match (record["type"].as_str(), p["type"].as_str()) {
            (Some("session_meta"), _) if p["forked_from_id"].is_null() => {
                self.thread_total = Some(0)
            }
            (Some("event_msg"), Some("task_started"))
                if turn.is_some() && self.id.as_deref() != turn =>
            {
                *self = Self {
                    id: turn.map(str::to_owned),
                    started_ms: crate::speed::timestamp_ms(&record["timestamp"])
                        .or_else(|| p["started_at"].as_i64()?.checked_mul(1000)),
                    baseline: self.thread_total,
                    thread_total: self.thread_total,
                    ..Default::default()
                };
            }
            (Some("token_usage_record"), _)
                if turn.is_some() && (self.id.is_none() || self.id.as_deref() == turn) =>
            {
                self.id = turn.map(str::to_owned);
                if let Some(n) = p
                    .pointer("/turn_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.confirmed = self.confirmed.max(n);
                    if let Some(total) = p
                        .pointer("/thread_token_usage/output_tokens")
                        .and_then(Value::as_u64)
                    {
                        self.baseline = total.checked_sub(n).or(self.baseline);
                        self.thread_total = Some(self.thread_total.unwrap_or(0).max(total));
                    }
                }
            }
            (Some("event_msg"), Some("token_count")) => {
                if let Some(n) = p
                    .pointer("/info/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.thread_total = Some(n);
                    if let Some(base) = self.baseline {
                        self.confirmed = self.confirmed.max(n.saturating_sub(base));
                    }
                }
            }
            (Some("event_msg"), Some("task_complete" | "turn_aborted"))
                if turn.is_none() || self.id.is_none() || self.id.as_deref() == turn =>
            {
                if self.id.is_none() {
                    self.id = turn.map(str::to_owned);
                }
                self.finished = true;
            }
            _ => {}
        }
    }
    pub fn merge_recent(&mut self, recent: &Self) {
        if recent.id.is_none() {
            return;
        }
        if self.id == recent.id && recent.started_ms.is_none() {
            self.confirmed = self.confirmed.max(recent.confirmed);
            self.baseline = recent.baseline.or(self.baseline);
            self.finished |= recent.finished;
        } else {
            *self = recent.clone();
        }
    }
    pub fn label(&self, thread: &Value, now_ms: i64) -> Option<String> {
        if self.finished
            || !matches!(
                thread.pointer("/agent/state")?.as_str()?,
                "working" | "thinking" | "waiting"
            )
        {
            return None;
        }
        let elapsed = now_ms.checked_sub(self.started_ms?)?;
        let native = thread
            .pointer("/usage/total/output_tokens")
            .and_then(Value::as_u64)
            .zip(self.baseline)
            .map_or(0, |(n, base)| n.saturating_sub(base));
        let tokens = self.confirmed.max(native);
        (tokens > 0 && elapsed > 0)
            .then(|| format!("Turn {:.1} tok/s", tokens as f64 * 1000. / elapsed as f64))
    }
}
