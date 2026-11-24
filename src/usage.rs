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
        line.push(gauge(pct, compaction_pressure.unwrap_or(pct)));
    } else if window.is_some() {
        line.push(json!("CTX —"));
    }
    if let Some(n) = window {
        line.push(json!(format!(" / {n}")));
    }
    line
}
