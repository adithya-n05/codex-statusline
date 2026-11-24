use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct Activity {
    counts: [usize; 6],
    failed: usize,
}

impl Activity {
    pub fn observe(&mut self, event: &Value, seen: &mut HashMap<String, bool>) {
        let p = &event["payload"];
        let (id, index, failed) = match (event["type"].as_str(), p["type"].as_str()) {
            (Some("event_msg"), Some("item_started" | "item_completed")) => {
                let item = &p["item"];
                let Some(index) = item_category(item) else {
                    return;
                };
                let failed = item["status"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("failed"))
                    || item["exit_code"].as_i64().is_some_and(|n| n != 0)
                    || item["error"].is_object()
                    || item.pointer("/result/isError") == Some(&Value::Bool(true));
                (item["id"].as_str(), index, failed)
            }
            (Some("response_item"), Some("function_call" | "custom_tool_call")) => {
                let Some(name) = p["name"].as_str() else {
                    return;
                };
                let index = if p["namespace"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("mcp__"))
                    || name.starts_with("mcp__")
                {
                    1
                } else {
                    match name {
                        "exec"
                        | "wait"
                        | "functions.exec"
                        | "functions.wait"
                        | "multi_tool_use.parallel" => return,
                        "exec_command" | "shell" | "shell_command" => 0,
                        "write_stdin" => 5,
                        "apply_patch" => 3,
                        "web.run" | "web_search" => 2,
                        _ => 4,
                    }
                };
                (p["call_id"].as_str(), index, false)
            }
            _ => return,
        };
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return;
        };
        use std::collections::hash_map::Entry;
        match seen.entry(id.to_owned()) {
            Entry::Vacant(slot) => {
                slot.insert(failed);
                self.counts[index] += 1;
                self.failed += usize::from(failed);
            }
            Entry::Occupied(mut slot) if failed && !*slot.get() => {
                slot.insert(true);
                self.failed += 1;
            }
            _ => {}
        }
    }
    pub fn label(&self) -> String {
        let mut fields = vec![format!("Tools {}", self.counts.iter().sum::<usize>())];
        for (name, n) in ["Shell", "MCP", "Web", "Patch", "Other", "Terminal I/O"]
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

fn item_category(item: &Value) -> Option<usize> {
    Some(match item["type"].as_str().unwrap_or("") {
        "CommandExecution" if item["source"] == "unified_exec_interaction" => 5,
        "CommandExecution" => 0,
        "McpToolCall" => 1,
        "WebSearch" | "WebSearchCall" => 2,
        "FileChange" => 3,
        "Extension" if item["kind"].as_str().is_some_and(|v| v.starts_with("web.")) => 2,
        "Extension" => 4,
        name if name.ends_with("ToolCall") || matches!(name, "ImageGeneration" | "ImageView") => 4,
        _ => return None,
    })
}
