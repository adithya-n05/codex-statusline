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
                    |rows| {
                        ["~/work", "API key", "CLI "]
                            .iter()
                            .all(|word| rows.iter().any(|row| row.contains(word)))
                    },
                    "all three colored footer rows",
                );
                for (word, rgb) in [
                    ("~/work", (251, 203, 151)),
                    ("API key", (231, 138, 83)),
                    ("CLI ", (150, 150, 150)),
                ] {
                    assert_eq!(
                        tty.foreground(word),
                        Some(vt100::Color::Rgb(rgb.0, rgb.1, rgb.2)),
                        "{word}"
                    );
                }
                Ok(())
            }),
            Trial::test("standard_shortcuts_work_on_start_and_resume", || {
                standard_shortcuts_work_on_start_and_resume();
                Ok(())
            }),
            Trial::test(
                "exact_fit_usage_has_no_ellipsis_and_real_overflow_does",
                || {
                    exact_fit_usage_has_no_ellipsis_and_real_overflow_does();
                    Ok(())
                },
            ),
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
        ["Directory · title", "CTX 50% · WKLY 70%", "Tools · CLI"],
    );
    tty.wait(
        |lines| {
            lines.windows(3).any(|rows| {
                rows[0].trim() == "Directory · title"
                    && rows[1].trim() == "CTX 50% · WKLY 70%"
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
    let middle = "CTX [██████████░░░░░░░░░░] 50% / 258k · WKLY 70% left · ChatGPT · Last 81 tok/s";
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
                    && r[2].trim_start().starts_with("Tools 0 · CLI ")
            })
        },
        "real provider rendering in a fresh Codex chat",
    );
}
fn standard_shortcuts_work_on_start_and_resume() {
    fn exercise(t: &mut terminal::Terminal) {
        let hints = |lines: &[String]| {
            lines
                .iter()
                .any(|r| r.contains("← for agents") && r.contains("? for shortcuts"))
        };
        t.wait(hints, "idle shortcut hints");
        t.send(b"?");
        t.wait(
            |rows| rows.iter().any(|r| r.contains("Keyboard shortcuts")),
            "shortcut help opens",
        );
        t.send(b"\x1b");
        t.wait(hints, "idle hints after closing help");
        t.send(b"\x1b[D");
        t.wait(
            |rows| rows.iter().any(|r| r.contains("Agent command center")),
            "agent picker opens",
        );
    }
    let mut tty = terminal::Terminal::start_provider(180);
    exercise(&mut tty);
    drop(tty);
    let mut original = terminal::Terminal::start_provider(180);
    original.wait(
        |r| r.iter().any(|s| s.contains("API key")),
        "provider ready before saving the fixture",
    );
    original.persist();
    let mut resumed = original.resume(180);
    exercise(&mut resumed);
}
fn exact_fit_usage_has_no_ellipsis_and_real_overflow_does() {
    let width = 60;
    let content = std::cell::Cell::new(0);
    {
        let mut calibration =
            terminal::Terminal::start(width, [&"q".repeat(120), "usage", "tools"]);
        calibration.wait(
            |rows| {
                rows.iter().any(|r| {
                    let row = r.trim();
                    if row.starts_with("qq") && row.ends_with('…') {
                        content.set(row.chars().count());
                        true
                    } else {
                        false
                    }
                })
            },
            "usable pane width",
        );
    }
    let span = content.get();
    let first = "A".repeat(span);
    let second = format!("{} END", "B".repeat(span - 4));
    let usage = format!("{first} {second}");
    let mut tty = terminal::Terminal::start(width, ["Dir", &usage, ""]);
    tty.wait(
        |r| {
            r.windows(3)
                .any(|w| w[0].trim() == "Dir" && w[1].trim() == first && w[2].trim() == second)
        },
        "fully visible final field at exact fit",
    );
    tty.resize(width - 1);
    tty.wait(
        |r| {
            r.windows(3).any(|w| {
                w[0].trim() == "Dir"
                    && w[2].contains("BBB")
                    && w[2].trim_end().ends_with('…')
                    && !w[2].contains("END")
            })
        },
        "ellipsis only when the final field falls outside the viewport",
    );
    tty.resize(width);
    tty.wait(
        |r| {
            r.windows(3)
                .any(|w| w[0].trim() == "Dir" && w[2].trim() == second)
        },
        "restored final field after widening",
    );
}
