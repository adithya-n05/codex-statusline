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
    _pty: Box<dyn MasterPty + Send>,
    _root: TempDir,
}

impl Terminal {
    pub fn start(width: u16, rows: [&str; 3]) -> Self {
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
command = {}
args = {}
"#,
            json!(cwd),
            json!(env::current_exe().unwrap()),
            json!(["--serve-rows".to_string(), json!(rows).to_string()])
        );
        fs::write(home.join("config.toml"), config).unwrap();
        let mut cmd = CommandBuilder::new("codex");
        cmd.env_clear();
        for (key, value) in [("HOME", &base), ("TMPDIR", &base), ("CODEX_HOME", &home)] {
            cmd.env(key, value);
        }
        cmd.env("PATH", env::var_os("PATH").unwrap());
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("TERM", "xterm-256color");
        cmd.env("XDG_CACHE_HOME", base.join("cache"));
        cmd.args(["--no-daemon", "-C"]);
        cmd.arg(&cwd);
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
            _pty: pty.master,
            _root: root,
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
