use codex_statusline::layout::footer;
use serde_json::json;

#[test]
fn only_the_middle_row_has_breakable_text() {
    let styled = json!({"type":"text","props":{"color":"#5F8787"},"children":["half full"]});
    let tree = footer(
        "one two",
        vec![styled.clone(), json!(" and more")],
        "three four",
    );
    assert_eq!(
        tree,
        json!({"type":"text","props":{"color":"#FBCB97"},"children":["one\u{a0}two","\n",styled," and more","\n",{"type":"text","props":{"color":"#969696"},"children":["three\u{a0}four"]}]})
    );
}

#[test]
fn absent_middle_fields_leave_the_bottom_on_its_own_logical_row() {
    assert_eq!(
        footer("dir", vec![], "tools")["children"],
        json!(["dir", "\n", "\n", {"type":"text","props":{"color":"#969696"},"children":["tools"]}])
    );
}

#[test]
fn activity_distinguishes_totals_categories_failures_and_session_details() {
    let tree = footer(
        "Dir",
        vec![],
        "Tools 31 · Shell 20 · MCP 5 · Patch 4 · Terminal I/O 2 · Failed 3 · CLI 0.172.0 · session",
    );
    let row = tree["children"].as_array().unwrap().last().unwrap();
    let colors = |word: &str| -> Option<&str> {
        row["children"].as_array()?.iter().find_map(|part| {
            let text = part
                .get("children")?
                .get(0)?
                .as_str()?
                .replace('\u{a0}', " ")
                .replace('\u{2060}', "");
            (text == word)
                .then(|| part["props"]["color"].as_str())
                .flatten()
        })
    };
    assert_eq!(row["props"]["color"], "#969696");
    assert_eq!(colors("Tools 31"), Some("#E78A53"));
    assert_eq!(colors("MCP"), Some("#E78A53"));
    assert_eq!(colors("5"), Some("#FBCB97"));
    assert_eq!(colors("Shell"), Some("#C1C1C1"));
    assert_eq!(colors("20"), Some("#FBCB97"));
    assert_eq!(colors("Terminal I/O"), Some("#C1C1C1"));
    assert_eq!(colors("Failed 3"), Some("#CC7373"));
}
