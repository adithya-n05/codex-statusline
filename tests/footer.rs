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
        json!({"type":"text","children":["one\u{a0}two","\n",styled," and more","\n","three\u{a0}four"]})
    );
}

#[test]
fn absent_middle_fields_leave_the_bottom_on_its_own_logical_row() {
    assert_eq!(
        footer("dir", vec![], "tools")["children"],
        json!(["dir", "\n", "\n", "tools"])
    );
}
