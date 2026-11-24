use codex_statusline::statusline::render;
use serde_json::{Value, json};

fn plain(tree: &Value) -> String {
    match tree {
        Value::String(s) => s.replace('\u{2060}', "").replace('\u{a0}', " "),
        Value::Object(_) => tree["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(plain)
            .collect(),
        _ => String::new(),
    }
}

fn screen(thread: Value) -> String {
    plain(&render(&json!({"thread":thread})))
}
#[test]
fn actual_chat_title_follows_the_directory_and_can_be_absent() {
    for (name, expected) in [
        (json!("Review README"), "~/work · Review README"),
        (Value::Null, "~/work"),
        (json!(" "), "~/work"),
    ] {
        let value = screen(json!({"workspace":{"cwd":"~/work"},"name":name}));
        assert_eq!(value.lines().next().unwrap(), expected);
    }
}
