use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::json;
use std::{
    env, fs,
    io::{Read, Write},
    sync::mpsc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const ROWS: u16 = 34;
const CAPABILITIES: &[(&[u8], &[u8])] = &[
    (b"\x1b[6n", b"\x1b[1;1R"),
    (b"\x1b[?u", b"\x1b[?0u"),
    (b"\x1b[c", b"\x1b[?1;2c"),
    (b"\x1b]10;?", b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\"),
    (b"\x1b]11;?", b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
];

pub struct Terminal {
    parser: vt100::Parser,
    input: Box<dyn Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    queries: Vec<u8>,
    remaining: Vec<(&'static [u8], &'static [u8])>,
    child: Box<dyn Child + Send + Sync>,
    pty: Box<dyn MasterPty + Send>,
    root: std::sync::Arc<TempDir>,
}

impl Terminal {
    pub fn start_provider(width: u16) -> Self {
        let provider = env::var_os("CODEX_STATUSLINE_TEST_PROVIDER")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_codex-statusline").into());
        Self::with_command(width, provider, Vec::new())
    }
    pub fn open_side_chat(&mut self) {
        self.persist();
        self.send(b"/side\r");
    }
    pub fn send(&mut self, bytes: &[u8]) {
        self.input.write_all(bytes).unwrap();
    }
    pub fn persist(&mut self) {
        // A local shell turn persists the parent thread without a model request.
        self.input
            .write_all(b"!printf '%s%s' fixture ready\r")
            .unwrap();
        self.wait(
            |rows| rows.iter().any(|s| s.contains("fixtureready")),
            "persisted parent thread",
        );
    }
    pub fn foreground(&self, word: &str) -> Option<vt100::Color> {
        let screen = self.parser.screen();
        for row in (0..screen.size().0).rev() {
            for col in 0..screen.size().1 {
                let matches = word.chars().enumerate().all(|(i, ch)| {
                    screen
                        .cell(row, col + i as u16)
                        .is_some_and(|c| c.contents().replace('\u{a0}', " ") == ch.to_string())
                });
                if matches {
                    return screen.cell(row, col).map(|c| c.fgcolor());
                }
            }
        }
        None
    }
    pub fn resize(&mut self, width: u16) {
        self.parser.screen_mut().set_size(ROWS, width);
        self.pty
            .resize(PtySize {
                rows: ROWS,
                cols: width,
                ..Default::default()
            })
            .unwrap();
    }
    pub fn start(width: u16, rows: [&str; 3]) -> Self {
        Self::with_command(
            width,
            env::current_exe().unwrap(),
            vec!["--serve-rows".into(), json!(rows).to_string()],
        )
    }
    fn with_command(width: u16, provider: std::path::PathBuf, arguments: Vec<String>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let home = base.join("codex");
        let cwd = base.join("work");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&cwd).unwrap();
        fs::write(
            home.join("auth.json"),
            r#"{"OPENAI_API_KEY":"local-test-only"}"#,
        )
        .unwrap();
        let config = format!(
            r#"
model = "gpt-5.6-terra"
openai_base_url = "http://127.0.0.1:1/v1"
features.daemon_auto_start = false
features.tui_extensions_statusline = true
check_for_update_on_startup = false
analytics.enabled = false
suppress_unstable_features_warning = true
notice.model_migrations."gpt-5.6-terra" = "gpt-6-sol"
[projects.{}]
trust_level = "trusted"
[tui]
screen_reader_detection_done = true
fullscreen_transcript = true
sidebar_width_percent = 31
disable_paste_burst = true
status_line_provider = {{ server = "geometry", tool = "footer" }}
[mcp_servers.geometry]
env_vars = ["CODEX_HOME"]
command = {}
args = {}
"#,
            json!(cwd),
            json!(provider),
            json!(arguments)
        );
        fs::write(home.join("config.toml"), config).unwrap();
        Self::launch(width, std::sync::Arc::new(root), false)
    }
    pub fn resume(self, width: u16) -> Self {
        let root = self.root.clone();
        drop(self);
        Self::launch(width, root, true)
    }
    fn launch(width: u16, root: std::sync::Arc<TempDir>, resume: bool) -> Self {
        let base = root.path().canonicalize().unwrap();
        let home = base.join("codex");
        let cwd = base.join("work");
        let mut cmd = CommandBuilder::new("codex");
        cmd.env_clear();
        for (key, value) in [("HOME", &base), ("TMPDIR", &base), ("CODEX_HOME", &home)] {
            cmd.env(key, value);
        }
        cmd.env("PATH", env::var_os("PATH").unwrap());
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("XDG_CACHE_HOME", base.join("cache"));
        cmd.args(["--no-daemon", "-C"]);
        cmd.arg(&cwd);
        if resume {
            cmd.args(["resume", "--last"]);
        }
        cmd.cwd(&cwd);
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: width,
                ..Default::default()
            })
            .unwrap();
        let child = pty.slave.spawn_command(cmd).unwrap();
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().unwrap();
        let input = pty.master.take_writer().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 32768];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 || tx.send(buffer[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            parser: vt100::Parser::new(ROWS, width, 0),
            input,
            output,
            queries: vec![],
            remaining: CAPABILITIES.to_vec(),
            child,
            pty: pty.master,
            root,
        }
    }

    pub fn wait(&mut self, predicate: impl Fn(&[String]) -> bool, description: &str) {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            let Ok(chunk) = self
                .output
                .recv_timeout(end.saturating_duration_since(Instant::now()))
            else {
                break;
            };
            self.parser.process(&chunk);
            self.queries.extend(&chunk);
            self.remaining.retain(|(query, reply)| {
                if self.queries.windows(query.len()).any(|s| s == *query) {
                    self.input.write_all(reply).unwrap();
                    false
                } else {
                    true
                }
            });
            self.queries.drain(..self.queries.len().saturating_sub(7));
            let s = self.parser.screen();
            let lines: Vec<_> = s
                .rows(0, s.size().1)
                .map(|s| s.replace('\u{a0}', " ").replace('\u{2060}', ""))
                .collect();
            if predicate(&lines) {
                return;
            }
        }
        panic!(
            "Missing {description}:\n{}",
            self.parser.screen().contents()
        );
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
