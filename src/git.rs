use std::{path::Path, process::Command};

pub fn summary(cwd: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ])
        .output()
        .ok()?;
    out.status.success().then(|| parse(&out.stdout)).flatten()
}

pub fn parse(bytes: &[u8]) -> Option<String> {
    let mut head = None;
    let mut oid = "";
    let mut tracking = None;
    let (mut added, mut modified, mut deleted) = (0, 0, 0);
    let data = String::from_utf8_lossy(bytes);
    let mut rows = data.split('\0');
    while let Some(row) = rows.next() {
        if let Some(h) = row.strip_prefix("# branch.head ") {
            head = Some(h);
        } else if let Some(h) = row.strip_prefix("# branch.oid ") {
            oid = h;
        } else if let Some(ab) = row.strip_prefix("# branch.ab ") {
            tracking = ab.split_once(' ');
        } else if row.starts_with("? ") {
            added += 1;
        } else if row.starts_with("u ") {
            modified += 1;
        } else if row.starts_with("1 ") || row.starts_with("2 ") {
            if row.starts_with("2 ") {
                rows.next();
            }
            let status = row.split_ascii_whitespace().nth(1).unwrap_or("");
            if status.contains('D') {
                deleted += 1;
            } else if status.contains(['A', 'C']) {
                added += 1;
            } else {
                modified += 1;
            }
        }
    }
    let head = head?;
    let mut label = if head == "(detached)" {
        format!("detached@{}", oid.chars().take(7).collect::<String>())
    } else {
        head.to_owned()
    };
    if added + modified + deleted > 0 {
        label.push('*');
    }
    if let Some((a, b)) = tracking {
        label.push_str(&format!(
            " ↑{} ↓{}",
            a.trim_start_matches('+'),
            b.trim_start_matches('-')
        ));
    }
    label.push_str(&format!(" A{added} M{modified} D{deleted}"));
    Some(label)
}
