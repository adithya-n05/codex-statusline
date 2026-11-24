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
