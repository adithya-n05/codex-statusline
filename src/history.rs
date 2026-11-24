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
    pub context: crate::saved_context::SavedContext,
    pub speed: Option<String>,
    pub(crate) responses: crate::speed::Turns,
    pub(crate) turn_rate: crate::turn_rate::TurnRate,
    tool_position: Option<(u64, u64)>,
    activity: crate::activity::Activity,
}

impl Metrics {
    fn observe_speed(&mut self, event: &Value) {
        let call = self.turn_rate.observe(event);
        self.responses.observe(event, call, &mut self.speed);
    }
    fn observe(&mut self, event: &Value) {
        self.context.observe(event);
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
}
#[derive(Default)]
pub struct History {
    files: VecDeque<(PathBuf, Cursor)>,
}

fn file_id(metadata: &std::fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        std::os::unix::fs::MetadataExt::ino(metadata)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

impl History {
    pub fn read(&mut self, path: &Path) -> Option<&Metrics> {
        self.read_with(path, |_| {})
    }
    pub fn read_with(&mut self, path: &Path, mut observe: impl FnMut(&Value)) -> Option<&Metrics> {
        let mut file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let file_id = file_id(&metadata);
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
                if let Some(call) = crate::activity::Call::parse(event) {
                    cursor.metrics.activity.record(&call);
                }
                cursor.metrics.observe_speed(event);
                observe(&value);
                cursor.metrics.observe(&value);
            }
        }
        cursor.metrics.tool_position = Some((file_id, cursor.offset));
        self.files.push_front((path.into(), cursor));
        self.files.truncate(8);
        self.files.front().map(|(_, cursor)| &cursor.metrics)
    }
}
impl Metrics {
    pub fn merge_recent_speed(&mut self, recent: &Self, home: &Path, thread: &str) {
        self.turn_rate.merge_recent(&recent.turn_rate);
        self.turn_rate.restore_start(home, thread);
        if let Some(speed) = &recent.speed {
            self.speed = Some(speed.clone());
        }
    }
    pub fn tools_label(&self) -> Option<String> {
        self.tool_position.map(|_| self.activity.label())
    }
}
/// Read the latest measurements independently of the background Git and activity scan.
#[derive(Default)]
pub struct RecentHistory {
    baseline_offset: Option<u64>,
    snapshot: Option<(PathBuf, u64, Option<std::time::SystemTime>, Metrics)>,
    tool_tail: ToolTail,
}
impl RecentHistory {
    pub fn update_tools(&self, history: &mut Metrics) {
        let tail = &self.tool_tail;
        let offset = match history.tool_position {
            Some((id, end)) if id == tail.file_id && end <= tail.end => end,
            _ => {
                history.activity = Default::default();
                history.tool_position = None;
                0
            }
        };
        if offset < tail.start {
            return;
        }
        for (_, call) in tail.calls.iter().filter(|(end, _)| *end > offset) {
            history.activity.record(call);
        }
        history.tool_position = Some((tail.file_id, tail.end));
    }
    pub fn read(&mut self, path: &Path) -> Option<&Metrics> {
        self.read_with(path, |_| {})
    }
    pub fn read_with(&mut self, path: &Path, observe: impl FnMut(&Value)) -> Option<&Metrics> {
        self.read_after(path, None, observe)
    }
    pub fn read_after(
        &mut self,
        path: &Path,
        baseline: Option<&Metrics>,
        observe: impl FnMut(&Value),
    ) -> Option<&Metrics> {
        let file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let length = metadata.len();
        let modified = metadata.modified().ok();
        let id = file_id(&metadata);
        let base = baseline.and_then(|m| {
            let (original, end) = m.tool_position?;
            (length > TAIL_BYTES && original == id && (length - TAIL_BYTES..=length).contains(&end))
                .then_some((end, m))
        });
        let baseline_offset = base.map(|(end, _)| end);
        if !self.snapshot.as_ref().is_some_and(|(prev, size, time, _)| {
            prev == path
                && self.tool_tail.file_id == id
                && self.baseline_offset == baseline_offset
                && *size == length
                && modified.is_some()
                && *time == modified
        }) {
            let (data, tail) = recent(file, id, length, base, observe)?;
            self.baseline_offset = baseline_offset;
            self.tool_tail = tail;
            self.snapshot = Some((path.to_owned(), length, modified, data));
        }
        self.snapshot.as_ref().map(|(_, _, _, data)| data)
    }
}
#[derive(Default)]
struct ToolTail {
    file_id: u64,
    start: u64,
    end: u64,
    calls: Vec<(u64, crate::activity::Call)>,
}

const TAIL_BYTES: u64 = 512 * 1024;

/// A large chat can still supply its latest measurements in one native footer read.
fn recent(
    mut file: File,
    id: u64,
    length: u64,
    base: Option<(u64, &Metrics)>,
    mut observe: impl FnMut(&Value),
) -> Option<(Metrics, ToolTail)> {
    let offset = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut reader = BufReader::new(file.take(length - offset));
    let start = if offset == 0 {
        0
    } else {
        offset + reader.skip_until(b'\n').ok()? as u64
    };
    let mut tail = ToolTail {
        file_id: id,
        start,
        end: start,
        calls: vec![],
    };
    let (mut result, observed_end) = match base.filter(|(end, _)| *end >= start) {
        Some((end, previous)) => (
            Metrics {
                context: previous.context.clone(),
                turn_rate: previous.turn_rate.clone(),
                responses: previous.responses.clone(),
                speed: previous.speed.clone(),
                ..Default::default()
            },
            end,
        ),
        None => (
            Metrics {
                responses: crate::speed::Turns::recent(offset > 0),
                ..Default::default()
            },
            start,
        ),
    };
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = reader.read_until(b'\n', &mut bytes).ok()?;
        if read == 0 || !bytes.ends_with(b"\n") {
            break;
        }
        tail.end += read as u64;
        if let Ok(event) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(call) = crate::activity::Call::parse(&event) {
                tail.calls.push((tail.end, call));
            }
            observe(&event);
            if tail.end > observed_end {
                result.context.observe(&event);
                result.observe_speed(&event);
            }
        }
    }
    Some((result, tail))
}
