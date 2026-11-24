use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Provider {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Provider {
    pub fn open(home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codex-statusline"))
            .env("CODEX_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
        }
    }
    pub fn poll(&mut self, state: &Value) -> String {
        let request = json!({"id":1,"method":"resources/read","params":{"uri":"ui://codex-statusline/footer","_meta":{"com.openai/codex-statusline-state":state}}});
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut reply = String::new();
        self.output.read_line(&mut reply).unwrap();
        let msg: Value = serde_json::from_str(&reply).unwrap();
        let tree =
            serde_json::from_str(msg["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
        crate::common::plain(&tree)
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        drop(self.input.take());
        self.child.wait().unwrap();
    }
}
