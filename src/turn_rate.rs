use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct TurnRate {
    id: Option<String>,
    started_ms: Option<i64>,
    thread_total: Option<u64>,
    baseline: Option<u64>,
    confirmed: u64,
    finished: bool,
    report_ms: Option<i64>,
    visible: HashMap<String, (i64, u64)>,
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
            (Some("event_msg"), Some("item_completed"))
                if !self.finished
                    && turn.is_some()
                    && (self.id.is_none() || self.id.as_deref() == turn) =>
            {
                self.visible_item(record)
            }
            (Some("token_usage_record"), _)
                if turn.is_some() && (self.id.is_none() || self.id.as_deref() == turn) =>
            {
                self.id = turn.map(str::to_owned);
                if let Some(n) = p
                    .pointer("/turn_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.confirm(n, record);
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
                        self.confirm(n.saturating_sub(base), record);
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
    fn visible_item(&mut self, record: &Value) {
        let p = &record["payload"];
        let item = &p["item"];
        let Some(id) = item["id"].as_str() else {
            return;
        };
        let Some(at) = p["completed_at_ms"]
            .as_i64()
            .or_else(|| crate::speed::timestamp_ms(&record["timestamp"]))
        else {
            return;
        };
        if self.report_ms.is_some_and(|report| at <= report) {
            return;
        }
        let bytes: usize = match item["type"].as_str() {
            Some("AgentMessage") => item["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|part| part["type"] == "Text")
                .filter_map(|part| part["text"].as_str())
                .map(str::len)
                .sum(),
            Some("Reasoning") => item["summary_text"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::len)
                .sum(),
            _ => return,
        };
        if bytes > 0 {
            self.id = p["turn_id"].as_str().map(str::to_owned);
            self.visible.insert(id.to_owned(), (at, bytes as u64));
        }
    }
    fn confirm(&mut self, tokens: u64, record: &Value) {
        if tokens <= self.confirmed {
            return;
        }
        self.confirmed = tokens;
        if let Some(at) = crate::speed::timestamp_ms(&record["timestamp"]) {
            self.report_ms = Some(self.report_ms.map_or(at, |old| old.max(at)));
            self.visible.retain(|_, (ms, _)| *ms > at);
        } else {
            self.visible.clear();
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
            self.report_ms = self.report_ms.max(recent.report_ms);
            self.visible.extend(recent.visible.clone());
            if let Some(at) = self.report_ms {
                self.visible.retain(|_, (ms, _)| *ms > at);
            }
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
        let estimated = if native > self.confirmed {
            0
        } else {
            self.visible
                .values()
                .map(|(_, bytes)| *bytes)
                .sum::<u64>()
                .div_ceil(4)
        };
        let tokens = self.confirmed.max(native).saturating_add(estimated);
        let label = if estimated > 0 { "Turn est." } else { "Turn" };
        (tokens > 0 && elapsed > 0).then(|| {
            format!(
                "{label} {:.1} tok/s",
                tokens as f64 * 1000. / elapsed as f64
            )
        })
    }
}
