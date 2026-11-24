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
