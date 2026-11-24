use codex_statusline::protocol;
use codex_statusline::statusline;

use std::{env, io, process::ExitCode};

fn main() -> io::Result<ExitCode> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, None) => {
            let identity = codex_statusline::account::codex_home()
                .map(|home| codex_statusline::account::Identity::load(&home))
                .unwrap_or_default();
            let home = codex_statusline::account::codex_home();
            let mut history = codex_statusline::history::History::default();
            let mut worker = codex_statusline::runtime::Worker::new(move |state| {
                let cwd = state
                    .pointer("/thread/workspace/cwd")
                    .and_then(serde_json::Value::as_str);
                let metrics = home
                    .as_deref()
                    .zip(
                        state
                            .pointer("/thread/id")
                            .and_then(serde_json::Value::as_str),
                    )
                    .and_then(|(h, id)| codex_statusline::storage::rollout(h, id))
                    .and_then(|p| history.read(&p))
                    .cloned()
                    .unwrap_or_default();
                codex_statusline::runtime::Extra {
                    history: metrics,
                    git: cwd.and_then(|p| codex_statusline::git::summary(std::path::Path::new(p))),
                }
            });
            protocol::serve(io::stdin().lock(), io::stdout().lock(), |s| {
                statusline::render_enriched(s, &identity, &worker.latest(s))
            })?;
            Ok(ExitCode::SUCCESS)
        }
        (Some(flag), None) if flag == "--version" => {
            println!("codex-statusline {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            eprintln!("Usage: codex-statusline [--version]");
            Ok(ExitCode::from(2))
        }
    }
}
