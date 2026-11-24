use serde_json::Value;
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
pub struct Metrics {
    pub plan: Option<String>,
    pub speed: Option<String>,
    available: bool,
    activity: crate::activity::Activity,
}

impl Metrics {
    fn observe(&mut self, event: &Value) {
        let p = &event["payload"];
        let args;
        let plan = match (event["type"].as_str(), p["type"].as_str()) {
            (Some("response_item"), Some("function_call"))
                if matches!(
                    p["name"].as_str(),
                    Some("update_plan" | "functions.update_plan")
                ) =>
            {
                args = serde_json::from_str::<Value>(p["arguments"].as_str().unwrap_or("")).ok();
                args.as_ref().and_then(|a| a["plan"].as_array())
            }
            (Some("event_msg"), Some("plan_update" | "turn_plan_updated")) => p["plan"].as_array(),
            _ => None,
        };
        if let Some(steps) = plan {
            self.plan = (!steps.is_empty()).then(|| {
                format!(
                    "Plan {}/{}",
                    steps.iter().filter(|s| s["status"] == "completed").count(),
                    steps.len()
                )
            });
        }
    }
}

#[derive(Default)]
struct Cursor {
    file_id: u64,
    offset: u64,
    metrics: Metrics,
    turns: crate::speed::Turns,
    seen: std::collections::HashMap<String, bool>,
}
#[derive(Default)]
pub struct History {
    files: VecDeque<(PathBuf, Cursor)>,
}

impl History {
    pub fn read(&mut self, path: &Path) -> Option<&Metrics> {
        self.read_with(path, |_| {})
    }
    pub fn read_with(&mut self, path: &Path, mut observe: impl FnMut(&Value)) -> Option<&Metrics> {
        let mut file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        #[cfg(unix)]
        let file_id = std::os::unix::fs::MetadataExt::ino(&metadata);
        #[cfg(not(unix))]
        let file_id = 0;
        let pos = self.files.iter().position(|(p, _)| p == path);
        let mut cursor = pos
            .and_then(|i| self.files.remove(i))
            .map(|(_, c)| c)
            .unwrap_or_default();
        if cursor.file_id != file_id || metadata.len() < cursor.offset {
            cursor = Cursor {
                file_id,
                ..Default::default()
            };
        }
        file.seek(SeekFrom::Start(cursor.offset)).ok()?;
        let mut reader = BufReader::new(file.take(metadata.len() - cursor.offset));
        let mut record = Vec::new();
        loop {
            record.clear();
            // Bound allocations even when a rollout contains a large inline image.
            let read = (&mut reader)
                .take(2 * 1024 * 1024)
                .read_until(b'\n', &mut record)
                .ok()?;
            if read == 0 {
                break;
            }
            if !record.ends_with(b"\n") {
                if read < 2 * 1024 * 1024 {
                    break;
                }
                let mut skipped = 0;
                loop {
                    let tail = reader.fill_buf().ok()?;
                    if tail.is_empty() {
                        break;
                    }
                    if let Some(end) = tail.iter().position(|b| *b == b'\n') {
                        skipped += end + 1;
                        reader.consume(end + 1);
                        cursor.offset += (read + skipped) as u64;
                        break;
                    }
                    let len = tail.len();
                    skipped += len;
                    reader.consume(len);
                }
                continue;
            }
            cursor.offset += read as u64;
            if let Ok(value) = serde_json::from_slice(&record) {
                let event: &Value = &value;
                cursor.metrics.activity.observe(event, &mut cursor.seen);
                if event["type"] == "event_msg" {
                    cursor.turns.observe(event, &mut cursor.metrics.speed);
                }
                observe(&value);
                cursor.metrics.observe(&value);
            }
        }
        cursor.metrics.available = true;
        self.files.push_front((path.into(), cursor));
        self.files.truncate(8);
        self.files.front().map(|(_, cursor)| &cursor.metrics)
    }
}
impl Metrics {
    pub fn tools_label(&self) -> Option<String> {
        self.available.then(|| self.activity.label())
    }
}
