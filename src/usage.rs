use crate::layout::gauge;
use serde_json::{Value, json};

pub fn context(value: &Value, compaction_pressure: Option<f64>) -> Vec<Value> {
    let window = value["context_window_size"].as_u64().filter(|n| *n > 0);
    let used = value["used_tokens"].as_u64();
    let percent = value["used_percentage"]
        .as_f64()
        .or_else(|| Some(100. * used? as f64 / window? as f64));
    let mut line = Vec::new();
    if let Some(pct) = percent {
        let pct = pct.clamp(0., 100.);
        line.push(json!(format!("CTX {pct:.0}% ")));
        // Reserve red for a measured compaction limit; the host percentage can use a different denominator.
        line.push(gauge(pct, compaction_pressure.unwrap_or(pct.min(89.))));
    } else if window.is_some() {
        line.push(json!("CTX —"));
    }
    if let Some(n) = window {
        line.push(json!(format!(" / {n}")));
    }
    line
}
pub fn weekly_window(account: &Value) -> Option<&Value> {
    let limits = account["rate_limits"].as_array()?;
    limits
        .iter()
        .filter_map(|r| {
            let window = [&r["secondary_limit"], &r["primary_limit"]]
                .into_iter()
                .find(|w| {
                    w["window_minutes"].as_i64() == Some(10080) && w["used_percent"].is_number()
                })?;
            Some((r["limit_id"] == "codex", window))
        })
        .max_by_key(|(is_codex, _)| *is_codex)
        .map(|(_, w)| w)
}

pub fn countdown(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    if seconds <= 0 {
        "now".into()
    } else if minutes >= 1440 {
        format!("{}d {}h", minutes / 1440, minutes % 1440 / 60)
    } else if minutes >= 60 {
        format!("{}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("{}m", minutes.max(1))
    }
}

pub fn weekly(account: &Value, now: i64) -> Vec<Value> {
    let Some(w) = weekly_window(account) else {
        return vec![];
    };
    let used = w["used_percent"]
        .as_f64()
        .unwrap_or_default()
        .clamp(0., 100.);
    let left = 100. - used;
    let mut line = vec![json!(format!("7D {left:.0}% left ")), gauge(left, used)];
    if let Some(reset) = w["resets_at"].as_i64() {
        line.push(json!(format!(
            " · reset {}",
            countdown(reset.saturating_sub(now))
        )));
    }
    line
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn api_thread(usage: &Value) -> Vec<Value> {
    let mut fields = Vec::new();
    if let (Some(input), Some(output)) = (
        usage["total"]["total_input_tokens"].as_u64(),
        usage["total"]["output_tokens"].as_u64(),
    ) {
        fields.push(format!("Thread {input} in / {output} out"));
    }
    if let Some(cost) = usage["cost"]["total_cost_usd"]
        .as_f64()
        .filter(|c| *c >= 0.)
    {
        fields.push(format!("est. ${cost:.4}"));
    }
    if fields.is_empty() {
        vec![]
    } else {
        vec![json!(fields.join(" · "))]
    }
}
