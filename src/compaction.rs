use serde_json::Value;
use std::path::Path;

#[derive(Clone)]
pub struct Sample {
    limit: u64,
    window: u64,
    scoped: u64,
    total: Option<u64>,
    total_scope: bool,
}

impl Sample {
    pub fn load(home: &Path, thread: &Value) -> Option<Self> {
        let id = thread["id"].as_str()?;
        let model = thread["model"]["id"].as_str()?;
        let window = thread
            .pointer("/usage/context/context_window_size")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0);
        let db = crate::storage::open(home, "logs_")?;
        let body: String = db.query_row(
            "SELECT feedback_log_body FROM logs WHERE thread_id=?1 AND target='codex_core::session::turn' AND instr(feedback_log_body,'post sampling token usage')>0 ORDER BY id DESC LIMIT 1",
            [id], |r| r.get(0)).ok()?;
        let (span, metrics) = body.rsplit_once("post sampling token usage")?;
        let field = |key: &str| {
            metrics
                .split_ascii_whitespace()
                .find_map(|w| w.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
        };
        let number = |key: &str| {
            let s = field(key)?;
            s.strip_prefix("Some(")
                .and_then(|n| n.strip_suffix(')'))
                .unwrap_or(s)
                .parse::<u64>()
                .ok()
        };
        if !span.split_ascii_whitespace().any(|w| {
            w.strip_prefix("model=")
                .is_some_and(|v| v.trim_end_matches('}') == model)
        }) || window.is_some_and(|w| Some(w) != number("full_context_window_limit"))
        {
            return None;
        }
        let limit = number("auto_compact_scope_limit")?;
        if limit == 0 {
            return None;
        }
        Some(Self {
            limit,
            window: number("full_context_window_limit").filter(|n| *n > 0)?,
            scoped: number("auto_compact_scope_tokens")?,
            total: number("total_usage_tokens"),
            total_scope: field("auto_compact_limit_scope")? == "Total",
        })
    }
    pub fn restore(&self, live: &Value) -> Value {
        let mut value = if live.is_object() {
            live.clone()
        } else {
            serde_json::json!({})
        };
        if !value["context_window_size"].as_u64().is_some_and(|n| n > 0) {
            value["context_window_size"] = self.window.into();
        }
        if value["used_tokens"].is_null()
            && let Some(n) = self.total
        {
            value["used_tokens"] = n.into();
        }
        if value["used_percentage"].is_null()
            && let Some(n) = value["used_tokens"].as_u64()
        {
            // Codex excludes its 12,000 token baseline when displaying the remaining percentage.
            let capacity = self.window.saturating_sub(12000);
            let remaining = if capacity == 0 {
                0.
            } else {
                (100. * self.window.saturating_sub(n) as f64 / capacity as f64)
                    .clamp(0., 100.)
                    .round()
            };
            value["used_percentage"] = (100. - remaining).into();
        }
        value
    }
    pub fn pressure(&self, context: &Value) -> Option<f64> {
        let used = context["used_tokens"].as_u64();
        let n = if self.total_scope {
            used.unwrap_or(self.scoped)
        } else if used.is_none() || used == self.total {
            self.scoped
        } else {
            return None;
        };
        Some(n as f64 * 100. / self.limit as f64)
    }
}
