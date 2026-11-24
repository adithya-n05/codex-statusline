use codex_statusline::storage::rollout;
use rusqlite::Connection;
use std::{fs, path::PathBuf};

#[test]
fn rollout_uses_the_latest_versioned_index_for_the_exact_thread() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    assert!(rollout(dir, "a").is_none());
    for (version, path) in [(9, "/stale"), (10, "/correct")] {
        let db = Connection::open(dir.join(format!("state_{version}.sqlite"))).unwrap();
        db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,rollout_path TEXT);")
            .unwrap();
        db.execute("INSERT INTO threads VALUES('a',?1)", [path])
            .unwrap();
    }
    fs::write(dir.join("state_100.sqlite-wal"), "").unwrap();
    assert_eq!(rollout(dir, "a"), Some(PathBuf::from("/correct")));
    assert!(rollout(dir, "a' OR 1=1").is_none());
    assert!(rollout(&dir.join("absent"), "a").is_none());
    fs::write(dir.join("state_11.sqlite"), "incompatible").unwrap();
    assert!(rollout(dir, "a").is_none());
}
