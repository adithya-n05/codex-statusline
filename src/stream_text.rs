use rusqlite::params;
use std::path::Path;

/// Codex tags output item boundaries with the chat and turn, but tags terminal
/// deltas only with the process. A process used by multiple chats is ambiguous.
pub(crate) fn pending_text(
    home: &Path,
    thread: &str,
    turn: &str,
    after_ms: i64,
) -> Option<(String, i64, u64)> {
    let mut db = crate::storage::open(home, "logs_")?;
    let read = db.transaction().ok()?;
    let (id, at, process, marker): (i64, i64, String, String) = read
        .query_row(
            "SELECT id, ts*1000 + ts_nanos/1000000, process_uuid, feedback_log_body
         FROM logs WHERE thread_id=?1 AND ts>=?2 AND target='codex_core::stream_events_utils'
         ORDER BY ts DESC, ts_nanos DESC, id DESC LIMIT 1",
            params![thread, after_ms.div_euclid(1000)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok()?;
    if at < after_ms || process.is_empty() {
        return None;
    }
    let (span, item) = marker.rsplit_once("Output item item_type=\"message\" item_id=")?;
    if !span.split("turn_id=").skip(1).any(|field| {
        field
            .trim_start_matches('"')
            .split([' ', '}', '"', ','])
            .next()
            == Some(turn)
    }) {
        return None;
    }
    let item: String = serde_json::from_str(item.trim()).ok()?;
    let bytes = read.query_row(
        "WITH events AS (
           SELECT target, feedback_log_body AS body FROM logs
           WHERE process_uuid=?1 AND thread_id IS NULL AND ts>=?2 AND id>?3
             AND target IN ('codex_tui::markdown_stream', 'codex_app_server::outgoing_message')
         ), chunks AS (
           SELECT substr(body,13) AS value FROM events WHERE target='codex_tui::markdown_stream'
             AND body LIKE 'push_delta: \"%'
         )
         SELECT sum(CASE WHEN json_valid(value) THEN length(CAST(json_extract(value,'$') AS BLOB)) END)
         FROM chunks HAVING count(*) <= (
           SELECT count(*) FROM events WHERE target='codex_app_server::outgoing_message'
             AND body LIKE 'app-server event: item/agentMessage/delta targeted_connections=%'
         ) AND NOT EXISTS (
           SELECT 1 FROM logs WHERE process_uuid=?1 AND thread_id IS NOT NULL AND thread_id<>?4
         )",
        params![process, at.div_euclid(1000), id, thread],
        |r| r.get::<_, u64>(0),
    ).ok()?;
    (bytes > 0).then_some((item, at, bytes))
}
