use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Extra {
    pub git: Option<String>,
    pub saved_name: Option<String>,
    pub credits: Option<crate::credits::Standing>,
    pub measured_context: Option<Value>,
    pub compaction: Option<crate::compaction::Sample>,
    pub history: crate::history::Metrics,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    thread: String,
    cwd: String,
    model: String,
    window: Option<u64>,
}

impl Key {
    fn from(state: &Value) -> Self {
        let t = &state["thread"];
        Self {
            thread: t["id"].as_str().unwrap_or("").into(),
            cwd: t["workspace"]["cwd"].as_str().unwrap_or("").into(),
            model: t["model"]["id"].as_str().unwrap_or("").into(),
            window: t["usage"]["context"]["context_window_size"].as_u64(),
        }
    }
}

pub struct Worker {
    requests: SyncSender<(Key, Value)>,
    results: Receiver<(Key, Instant, Extra)>,
    pending: HashSet<Key>,
    cache: VecDeque<(Key, Instant, Extra)>,
}

impl Worker {
    pub fn new(mut collect: impl FnMut(&Value) -> Extra + Send + 'static) -> Self {
        let (requests, jobs) = mpsc::sync_channel::<(Key, Value)>(2);
        let (tx, results) = mpsc::channel();
        std::thread::spawn(move || {
            for (key, state) in jobs {
                let started = Instant::now();
                let data = collect(&state);
                if tx.send((key, started, data)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            pending: HashSet::new(),
            cache: VecDeque::new(),
        }
    }

    pub fn latest(&mut self, state: &Value) -> Extra {
        for (key, started, data) in self.results.try_iter() {
            self.pending.remove(&key);
            self.cache.retain(|(k, _, _)| k != &key);
            self.cache.push_front((key, started, data));
            self.cache.truncate(8);
        }
        let key = Key::from(state);
        let cached = self.cache.iter().find(|(k, _, _)| k == &key);
        let refresh = match state.pointer("/thread/agent/state").and_then(Value::as_str) {
            Some("working" | "thinking" | "waiting") => Duration::from_millis(900),
            _ => Duration::from_secs(2),
        };
        if cached.is_none_or(|(_, when, _)| when.elapsed() >= refresh)
            && !self.pending.contains(&key)
            && self.requests.try_send((key.clone(), state.clone())).is_ok()
        {
            self.pending.insert(key);
        }
        cached.map(|(_, _, d)| d.clone()).unwrap_or_default()
    }
}
