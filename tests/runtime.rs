use codex_statusline::{
    account::Identity,
    runtime::{Extra, Worker},
    statusline::render_enriched,
};
use serde_json::json;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[test]
fn a_slow_repository_never_blocks_a_read_or_leaks_into_a_different_chat() {
    let (release, permit) = mpsc::channel();
    let mut worker = Worker::new(move |s| {
        permit.recv_timeout(Duration::from_secs(3)).unwrap();
        Extra {
            git: Some(format!("branch-{}", s["thread"]["id"].as_str().unwrap())),
            ..Default::default()
        }
    });
    let a = json!({"thread":{"id":"a","workspace":{"cwd":"/one"}}});
    let b = json!({"thread":{"id":"b","workspace":{"cwd":"/two"}}});
    assert!(worker.latest(&a).git.is_none());
    assert!(worker.latest(&b).git.is_none());
    release.send(()).unwrap();
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let (Some(first), Some(second)) = (worker.latest(&a).git, worker.latest(&b).git) {
            assert_eq!(first, "branch-a");
            assert_eq!(second, "branch-b");
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let tree = render_enriched(&a, &Identity::Unknown, &worker.latest(&a)).to_string();
    assert!(tree.contains("branch"));
    assert!(!tree.contains("branch-b"));
}
