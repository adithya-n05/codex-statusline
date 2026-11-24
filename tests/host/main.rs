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
            Trial::test("provider_paints_text_on_all_three_terminal_rows", || {
                let mut tty = terminal::Terminal::start_provider(180);
                tty.wait(
                    |l| l.iter().any(|r| r.contains("API key")),
                    "colored footer",
                );
                for (word, rgb) in [
                    ("~/work", (251, 203, 151)),
                    ("API key", (231, 138, 83)),
                    ("CLI ", (95, 135, 135)),
                ] {
                    assert_eq!(
                        tty.foreground(word),
                        Some(vt100::Color::Rgb(rgb.0, rgb.1, rgb.2)),
                        "{word}"
                    );
                }
                Ok(())
            }),
            Trial::test("installed_provider_populates_a_fresh_chat", || {
                installed_provider_populates_a_fresh_chat();
                Ok(())
            }),
            Trial::test("native_footer_geometry", || {
                native_footer_geometry();
                Ok(())
            }),
            Trial::test("outer_rows_clip_after_resize", || {
                outer_rows_clip_after_resize();
                Ok(())
            }),
            Trial::test("usage_wraps_only_in_the_narrow_pane", || {
                usage_wraps_only_in_the_narrow_pane();
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
fn usage_wraps_only_in_the_narrow_pane() {
    let middle = "CTX [██████████░░░░░░░░░░] 50% / 258k · 7D 70% left · ChatGPT · Last 81 tok/s";
    let mut tty = terminal::Terminal::start(160, ["Project dir", middle, "Tools · CLI"]);
    tty.wait(
        |l| l.iter().any(|s| s.contains(middle)),
        "complete usage in wide main chat",
    );
    tty.open_side_chat();
    tty.wait(
        |lines| {
            lines.windows(3).any(|rows| {
                let panes: Vec<_> = rows.iter().filter_map(|s| s.split_once('│')).collect();
                panes.len() == 3
                    && panes[0].0.contains("Project dir")
                    && panes[0].1.contains("Project dir")
                    && panes[1].0.contains(middle)
                    && panes[1].1.trim_start().starts_with("CTX")
                    && panes[2].0.contains("Tools · CLI")
                    && panes[2].1.contains("81 tok/s")
                    && !panes[2].1.contains("Tools")
            })
        },
        "side chat usage continues on row three while main chat keeps tools",
    );
}
fn installed_provider_populates_a_fresh_chat() {
    let mut terminal = terminal::Terminal::start_provider(180);
    terminal.wait(
        |lines| {
            lines.windows(3).any(|r| {
                r[0].contains("work")
                    && r[0].contains("Approvals")
                    && r[1].contains("API key")
                    && r[2].contains("CLI ")
            })
        },
        "real provider rendering in a fresh Codex chat",
    );
}
