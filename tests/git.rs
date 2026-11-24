use codex_statusline::git::summary;
use std::process::Command;

#[test]
fn git_summary_counts_dirty_files_and_tracks_divergence() {
    let dir = tempfile::tempdir().unwrap();
    let run = |cmd: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", dir.path())
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(cmd)
            .output()
            .unwrap();
        assert!(output.status.success(), "{cmd:?}");
    };
    assert!(summary(dir.path()).is_none());
    run(&["init", "-b", "main"]);
    for f in ["edit", "remove"] {
        std::fs::write(dir.path().join(f), "before").unwrap();
    }
    run(&["add", "edit", "remove"]);
    run(&["commit", "-m", "test: seed fixture"]);
    run(&["branch", "upstream"]);
    run(&["branch", "--set-upstream-to=upstream"]);
    run(&[
        "commit",
        "--allow-empty",
        "-m",
        "test: advance local branch",
    ]);
    std::fs::write(dir.path().join("edit"), "after").unwrap();
    std::fs::remove_file(dir.path().join("remove")).unwrap();
    std::fs::write(dir.path().join("new name\nwith newline"), "a").unwrap();
    let text = summary(dir.path()).unwrap();
    assert_eq!(text, "main* ↑1 ↓0 A1 M1 D1");
    run(&["checkout", "--detach", "-q"]);
    assert!(summary(dir.path()).unwrap().starts_with("detached@"));
}

#[test]
fn porcelain_rename_paths_and_conflicts_are_counted_once() {
    let data=b"# branch.head main\x00# branch.ab +0 -4\x002 R. original\x00? fake-name\x00u UU conflict\x001 .D gone\x002 C. copy\x00original\x00";
    assert_eq!(
        codex_statusline::git::parse(data).as_deref(),
        Some("main* ↑0 ↓4 A1 M2 D1")
    );
    assert!(codex_statusline::git::parse(b"").is_none());
}
