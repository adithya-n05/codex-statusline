use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct TurnRate {
    id: Option<String>,
    started_ms: Option<i64>,
    host_total: Option<u64>,
    host_baseline: Option<u64>,
    confirmed: u64,
    finished: bool,
    report_ms: Option<i64>,
    visible: HashMap<String, (i64, u64)>,
    calls: HashMap<String, (i64, u64)>,
}

impl TurnRate {
    /// Return the creation time of a newly observed model request for response timing.
    pub fn observe(&mut self, record: &Value) -> Option<i64> {
        let p = &record["payload"];
        let turn = p["turn_id"].as_str();
        match (record["type"].as_str(), p["type"].as_str()) {
            (Some("session_meta"), _) if p["forked_from_id"].is_null() => self.host_total = Some(0),
            (Some("event_msg"), Some("task_started"))
                if turn.is_some() && self.id.as_deref() != turn =>
            {
                *self = Self {
                    id: turn.map(str::to_owned),
                    started_ms: crate::speed::timestamp_ms(&record["timestamp"])
                        .or_else(|| p["started_at"].as_i64()?.checked_mul(1000)),
                    host_baseline: self.host_total,
                    host_total: self.host_total,
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
            (Some("response_item"), _) if !self.finished => {
                if let Some(call) = crate::generated_call::GeneratedCall::parse(record)
                    && let Some(turn) = call.turn.or(self.id.as_deref())
                    && self.id.as_deref().is_none_or(|active| active == turn)
                    && let Some(at) = crate::speed::timestamp_ms(&record["timestamp"])
                    && self.started_ms.is_none_or(|start| at >= start)
                {
                    if self.id.is_none() {
                        self.id = Some(turn.to_owned());
                    }
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        self.calls.entry(call.id.to_owned())
                    {
                        entry.insert((at, call.bytes));
                        let seconds = p
                            .pointer("/internal_chat_message_metadata_passthrough/create_time")?
                            .as_f64()?;
                        let created = (seconds * 1000.).round() as i64;
                        return (created > 0
                            && created <= at
                            && self.started_ms.is_none_or(|start| created >= start)
                            && self.report_ms.is_none_or(|report| created >= report))
                        .then_some(created);
                    }
                }
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
                    // Advance the native baseline by the measured turn output only.
                    if let Some(end) = self.host_baseline.and_then(|base| base.checked_add(n)) {
                        self.host_total = self.host_total.max(Some(end));
                    }
                }
            }
            (Some("event_msg"), Some("token_count")) => {
                if let Some(n) = p
                    .pointer("/info/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.host_total = Some(n);
                    if let Some(base) = self.host_baseline {
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
        None
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
            for (_, bytes) in self.calls.values_mut() {
                *bytes = 0;
            }
        }
    }
    pub fn merge_recent(&mut self, recent: &Self) {
        if recent.id.is_none() {
            return;
        }
        if self.id == recent.id && recent.started_ms.is_none() {
            self.confirmed = self.confirmed.max(recent.confirmed);
            self.host_baseline = recent.host_baseline.or(self.host_baseline);
            self.finished |= recent.finished;
            self.report_ms = self.report_ms.max(recent.report_ms);
            self.visible.extend(recent.visible.clone());
            for (id, sample) in &recent.calls {
                self.calls.entry(id.clone()).or_insert(*sample);
            }
            if let Some(at) = self.report_ms {
                self.visible.retain(|_, (ms, _)| *ms > at);
            }
        } else {
            *self = recent.clone();
        }
    }
    pub fn restore_start(&mut self, home: &std::path::Path, thread: &str) {
        if self.started_ms.is_some() || self.finished {
            return;
        }
        let Some(turn) = self.id.as_deref() else {
            return;
        };
        self.started_ms = crate::storage::open(home, "thread_history_").and_then(|db| {
            let seconds: i64 = db.query_row(
                "SELECT started_at FROM thread_turns WHERE thread_id=?1 AND turn_id=?2 AND status='inProgress'",
                [thread, turn], |row| row.get(0),
            ).ok()?;
            seconds.checked_mul(1000)
        });
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
            .zip(self.host_baseline)
            .map_or(0, |(n, base)| n.saturating_sub(base));
        let estimated = if native > self.confirmed {
            0
        } else {
            self.visible
                .values()
                .chain(
                    self.calls
                        .values()
                        .filter(|(at, _)| self.report_ms.is_none_or(|report| *at > report)),
                )
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
