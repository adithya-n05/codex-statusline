use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Codex increments the numeric filename when it migrates an index.
pub fn open(home: &Path, prefix: &str) -> Option<Connection> {
    let path = home
        .read_dir()
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let version = name
                .to_str()?
                .strip_prefix(prefix)?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            Some((version, entry.path()))
        })
        .max_by_key(|(version, _)| *version)?
        .1;
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    db.busy_timeout(std::time::Duration::from_millis(30)).ok()?;
    Some(db)
}

pub fn rollout(home: &Path, id: &str) -> Option<PathBuf> {
    open(home, "state_")?
        .query_row("SELECT rollout_path FROM threads WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .map(PathBuf::from)
}
pub fn account_matches(home: &Path, thread: &str, account: &str) -> bool {
    open(home, "state_")
        .and_then(|db| {
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM threads WHERE id=?1 AND creator_account_id=?2)",
                [thread, account],
                |r| r.get(0),
            )
            .ok()
        })
        .unwrap_or(false)
}
pub fn saved_name(home: &Path, id: &str) -> Option<String> {
    open(home, "state_")?
        .query_row("SELECT name FROM threads WHERE id=?1", [id], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .filter(|s| !s.trim().is_empty())
}
