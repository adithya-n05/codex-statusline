use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

const URI: &str = "ui://codex-statusline/footer";

pub fn serve(reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    for line in reader.lines() {
        let message = serde_json::from_str::<Value>(&line?);
        let reply = match message {
            Err(_) => error(Value::Null, -32700, "Invalid JSON"),
            Ok(request) => {
                let Some(method) = request["method"].as_str() else {
                    writeln!(writer, "{}", error(Value::Null, -32600, "Invalid request"))?;
                    writer.flush()?;
                    continue;
                };
                let Some(id) = request.get("id") else {
                    continue;
                };
                match method {
                    "initialize" => json!({"jsonrpc":"2.0","id":id,"result":{
                        "protocolVersion":"2025-06-18",
                        "capabilities":{"resources":{},"tools":{}},
                        "serverInfo":{"name":env!("CARGO_PKG_NAME"),"version":env!("CARGO_PKG_VERSION")}
                    }}),
                    "ping" => json!({"jsonrpc":"2.0","id":id,"result":{}}),
                    "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{
                        "name":"footer","title":"Codex Statusline","description":"Display the current thread in the Codex footer.",
                        "inputSchema":{"type":"object"},"_meta":{"openai/ui":{"entrypoints":[{"type":"statusline","resourceUri":URI}]}}
                    }]}}),
                    "resources/list" => json!({"jsonrpc":"2.0","id":id,"result":{"resources":[{
                        "name":"footer","uri":URI,"mimeType":"application/json"
                    }]}}),
                    "resources/templates/list" => {
                        json!({"jsonrpc":"2.0","id":id,"result":{"resourceTemplates":[]}})
                    }
                    "resources/read" if request["params"]["uri"] == URI => {
                        let state =
                            &request["params"]["_meta"]["com.openai/codex-statusline-state"];
                        let tree = json!(
                            state
                                .pointer("/thread/workspace/cwd")
                                .and_then(Value::as_str)
                        );
                        json!({"jsonrpc":"2.0","id":id,"result":{"contents":[{"uri":URI,"mimeType":"application/json","text":tree.to_string()}]}})
                    }
                    "resources/read" => error(id.clone(), -32602, "Unexpected resource URI"),
                    _ => error(id.clone(), -32601, "Unknown method"),
                }
            }
        };
        writeln!(writer, "{reply}")?;
        writer.flush()?;
    }
    Ok(())
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
