mod terminal;

use libtest_mimic::{Arguments, Trial};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--serve-rows") {
        let rows: [String; 3] = serde_json::from_str(&args[2]).unwrap();
        let tree =
            codex_statusline::layout::footer(&rows[0], vec![serde_json::json!(rows[1])], &rows[2]);
        codex_statusline::protocol::serve(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            |_| tree.clone(),
        )
        .unwrap();
        return;
    }
    libtest_mimic::run(
        &Arguments::from_args(),
        vec![
            Trial::test("native_footer_geometry", || {
                native_footer_geometry();
                Ok(())
            }),
            Trial::test("outer_rows_clip_after_resize", || {
                outer_rows_clip_after_resize();
                Ok(())
            }),
        ],
    )
    .exit();
}

fn native_footer_geometry() {
    let mut tty = terminal::Terminal::start(
        100,
        ["Directory · title", "CTX 50% · 7D 70%", "Tools · CLI"],
    );
    tty.wait(
        |lines| {
            lines.windows(3).any(|rows| {
                rows[0].trim() == "Directory · title"
                    && rows[1].trim() == "CTX 50% · 7D 70%"
                    && rows[2].trim() == "Tools · CLI"
            })
        },
        "three short footer rows",
    );
}
fn outer_rows_clip_after_resize() {
    let top = "Directory /wide-path · 日本 Hello 世界 · 👩‍💻 tab · name · plan 3/5";
    let bottom = "Tools · Shell 7 · MCP 3 · CLI 0.172.0 · 01234567-89ab-cdef-0123-456789abcdef";
    let mut tty = terminal::Terminal::start(120, [top, "CTX 50%", bottom]);
    tty.wait(
        |l| l.iter().any(|s| s.contains("456789abcdef")),
        "unclipped session ID",
    );
    tty.resize(34);
    tty.wait(
        |lines| {
            lines.windows(3).any(|s| {
                s[0].trim().starts_with("Directory")
                    && s[0].trim_end().ends_with('…')
                    && s[1].trim() == "CTX 50%"
                    && s[2].trim().starts_with("Tools")
                    && s[2].trim_end().ends_with('…')
            })
        },
        "a clipped first and third row with the middle row intact",
    );
    tty.resize(120);
    tty.wait(
        |l| {
            l.windows(3)
                .any(|s| s[0].contains("plan 3/5") && s[2].contains("456789abcdef"))
        },
        "restored footer on widening",
    );
}
