use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub enum Identity {
    ChatGpt(Option<String>),
    Api,
    #[default]
    Unknown,
}

impl Identity {
    pub fn load(home: &Path) -> Self {
        let Some(auth) = std::fs::read(home.join("auth.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            return Self::Unknown;
        };
        match auth["auth_mode"].as_str() {
            Some("chatgpt" | "chatgptAuthTokens") => {
                Self::ChatGpt(auth["tokens"]["account_id"].as_str().map(str::to_owned))
            }
            Some("apikey" | "api_key") => Self::Api,
            None if auth["OPENAI_API_KEY"]
                .as_str()
                .is_some_and(|s| !s.is_empty()) =>
            {
                Self::Api
            }
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> Option<&'static str> {
        match self {
            Self::ChatGpt(_) => Some("ChatGPT account"),
            Self::Api => Some("API key"),
            Self::Unknown => None,
        }
    }
}

pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".codex")))
}
