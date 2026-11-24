use std::process::Command;

#[test]
fn version_matches_the_package() {
    let out = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        out.stdout,
        format!("codex-statusline {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn unknown_options_exit_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
        .arg("--unknown")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8(out.stderr).unwrap().contains("Usage:"));
}
