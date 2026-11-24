use codex_statusline::account::Identity;
use serde_json::{Value, json};

#[test]
fn authenticates_from_the_local_mode_without_disclosing_secrets() {
    for (record, expected) in [
        (
            json!({"auth_mode":"chatgpt","tokens":{"account_id":"workspace-A","access_token":"secret"}}),
            Some("ChatGPT account"),
        ),
        (
            json!({"auth_mode":"apikey","OPENAI_API_KEY":"secret"}),
            Some("API key"),
        ),
        (json!({"OPENAI_API_KEY":"legacy"}), Some("API key")),
        (
            json!({"auth_mode":"future-provider","OPENAI_API_KEY":"secret"}),
            None,
        ),
        (Value::Null, None),
    ] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("auth.json");
        std::fs::write(&path, record.to_string()).unwrap();
        let id = Identity::load(home.path());
        assert_eq!(id.label(), expected);
        let state = json!({"thread":{"workspace":{"cwd":"work"}},"account":{"rate_limits":[{"limit_id":"codex","secondary_limit":{"window_minutes":10080,"used_percent":30}}]}});
        let body = codex_statusline::statusline::render_for(&state, &id).to_string();
        if let Some(label) = expected {
            assert!(body.contains(label));
        }
        if expected == Some("API key") {
            assert!(!body.contains("WKLY "));
        }
        assert!(!body.contains("secret"));
        std::fs::remove_file(path).unwrap();
        assert_eq!(Identity::load(home.path()).label(), None);
    }
}
