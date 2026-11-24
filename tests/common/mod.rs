use serde_json::Value;

pub fn plain(tree: &Value) -> String {
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
