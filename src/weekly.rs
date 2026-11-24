use serde_json::{Value, json};

#[derive(Clone)]
struct Window {
    reset: i64,
    used: f64,
}

impl Window {
    fn parse(value: &Value) -> Option<Self> {
        if value["window_minutes"] != 10080 {
            return None;
        }
        Some(Self {
            reset: value["resets_at"].as_i64()?,
            used: value["used_percent"].as_f64()?,
        })
    }
    fn contains(&self, seconds: i64) -> bool {
        (self.reset.saturating_sub(604800)..self.reset).contains(&seconds)
    }
    fn account(&self) -> Value {
        json!({"rate_limits":[{"limit_id":"codex","secondary_limit":{"window_minutes":10080,"used_percent":self.used,"resets_at":self.reset}}]})
    }
}

/// Native weekly snapshots have no observation timestamp. A changed native reading
/// takes precedence until a later local observation arrives for that same week.
#[derive(Default)]
pub struct Weekly {
    disk: Option<(i64, Window)>,
    host: Option<Window>,
    disk_at_host_update: Option<i64>,
}
impl Weekly {
    pub fn observe(&mut self, event: &Value) {
        let p = &event["payload"];
        let r = &p["rate_limits"];
        if event["type"] != "event_msg" || p["type"] != "token_count" || r["limit_id"] != "codex" {
            return;
        }
        let Some(at) = crate::speed::timestamp_ms(&event["timestamp"]) else {
            return;
        };
        let now = crate::usage::unix_now();
        if at / 1000 > now || self.disk.as_ref().is_some_and(|(prev, _)| at <= *prev) {
            return;
        }
        if let Some(w) = [&r["secondary"], &r["primary"]]
            .into_iter()
            .find_map(Window::parse)
            && w.contains(at / 1000)
            && w.contains(now)
        {
            self.disk = Some((at, w));
        }
    }
    pub fn account(&mut self, native: &Value, now: i64) -> Option<Value> {
        let supplied = crate::usage::weekly_window(native);
        if let Some(w) = supplied {
            let latest = Window::parse(w)?;
            if self.host.as_ref().is_none_or(|h| {
                h.used != latest.used || !crate::credits::same_week(h.reset, latest.reset)
            }) {
                self.disk_at_host_update = self.disk.as_ref().map(|(at, _)| *at);
            }
            self.host = Some(latest);
        }
        let host = self.host.as_ref().filter(|w| w.contains(now));
        if let Some((at, disk)) = &self.disk
            && disk.contains(now)
            && host.is_none_or(|h| {
                crate::credits::same_week(h.reset, disk.reset)
                    && self
                        .disk_at_host_update
                        .is_none_or(|previous| *at > previous)
            })
            && !(supplied.is_some() && host.is_none())
        {
            return Some(disk.account());
        }
        if supplied.is_none() {
            return host.map(Window::account);
        }
        None
    }
}
