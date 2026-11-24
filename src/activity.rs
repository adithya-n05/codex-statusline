use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Default)]
pub struct Activity {
    counts: [usize; 5],
    failed: usize,
}

impl Activity {
    pub fn record(&mut self, item: &Value, seen: &mut HashSet<String>) {
        let index = match item["type"].as_str().unwrap_or("") {
            "CommandExecution" => 0,
            "McpToolCall" => 1,
            "WebSearch" | "WebSearchCall" => 2,
            "FileChange" => 3,
            name if name.ends_with("ToolCall")
                || matches!(name, "ImageGeneration" | "ImageView" | "CodeMode") =>
            {
                4
            }
            _ => return,
        };
        if let Some(id) = item["id"].as_str()
            && !seen.insert(id.to_owned())
        {
            return;
        }
        self.counts[index] += 1;
        if item["status"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("failed"))
            || item["exit_code"].as_i64().is_some_and(|n| n != 0)
            || item["error"].is_object()
        {
            self.failed += 1;
        }
    }
    pub fn label(&self) -> String {
        let mut fields = vec![format!("Tools {}", self.counts.iter().sum::<usize>())];
        for (name, n) in ["Shell", "MCP", "Web", "Patch", "Other"]
            .into_iter()
            .zip(self.counts)
        {
            if n > 0 {
                fields.push(format!("{name} {n}"));
            }
        }
        if self.failed > 0 {
            fields.push(format!("Failed {}", self.failed));
        }
        fields.join(" · ")
    }
}
