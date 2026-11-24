use codex_statusline::layout::fixed_line;
use unicode_linebreak::linebreaks;

fn visible(s: &str) -> String {
    s.replace('\u{2060}', "").replace('\u{a0}', " ")
}

#[test]
fn fixed_rows_have_no_internal_break_opportunities() {
    for label in [
        "",
        "one",
        "~/work/a project",
        "GPT-6.2 · High",
        "α/β/日本 語",
        "01234567-89ab-cdef-0123-456789abcdef",
        "👩‍💻 résumé e\u{301}",
    ] {
        let row = fixed_line(label);
        assert_eq!(visible(&row), label);
        assert!(
            linebreaks(&row).all(|(offset, _)| offset == row.len()),
            "{row:?}"
        );
        for pair in row.as_bytes().windows(2) {
            assert!(
                !(pair[0] == b'-' && pair[1].is_ascii_alphanumeric()),
                "Codex also splits ASCII hyphens"
            );
        }
    }
}

#[test]
fn field_controls_cannot_change_row_count_or_inject_terminal_sequences() {
    let row = fixed_line("one\nx\r\ty\x1bz");
    assert!(!row.chars().any(char::is_control));
    assert_eq!(visible(&row), "one x  y z");
}
#[test]
fn twenty_cell_bar_uses_bathory_colors_and_clamps_bounds() {
    use codex_statusline::layout::gauge;
    for (filled, pressure, blocks, color) in [
        (-3., 0., 0, "#5F8787"),
        (25., 49.9, 5, "#5F8787"),
        (50., 50., 10, "#FBCB97"),
        (75., 75., 15, "#E78A53"),
        (90., 89.99, 18, "#E78A53"),
        (100., 90., 20, "#E05252"),
        (110., 110., 20, "#E05252"),
    ] {
        let result = gauge(filled, pressure);
        assert_eq!(result["props"]["color"], color);
        let text = result["children"][0].as_str().unwrap();
        assert_eq!(
            text,
            format!("[{}{}]", "█".repeat(blocks), "░".repeat(20 - blocks))
        );
    }
}
#[test]
fn long_outer_rows_stay_within_the_resource_budget_at_a_grapheme_boundary() {
    let family = "👩‍👩‍👧‍👦";
    for input in [
        "a".repeat(30000),
        family.repeat(5000),
        format!("a{}", "\u{301}".repeat(9000)),
    ] {
        let result = fixed_line(&input);
        assert!(result.len() <= 8192);
        let text = visible(&result);
        let prefix = text.strip_suffix('…').unwrap();
        assert!(input.starts_with(prefix));
        if input.starts_with(family) {
            assert_eq!(prefix.len() % family.len(), 0);
        }
    }
    assert_eq!(fixed_line(&"a".repeat(8192)).len(), 8192);
}
#[test]
fn rows_that_fit_exactly_keep_their_last_grapheme() {
    for n in 8189..=8192 {
        let text = "x".repeat(n);
        assert_eq!(fixed_line(&text), text);
    }
    let text = format!("{}e\u{301}", "a".repeat(8189));
    assert_eq!(fixed_line(&text), text);
}
