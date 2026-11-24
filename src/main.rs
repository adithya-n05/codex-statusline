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
            let mut worker = codex_statusline::runtime::Worker::new(|state| {
                let cwd = state
                    .pointer("/thread/workspace/cwd")
                    .and_then(serde_json::Value::as_str);
                codex_statusline::runtime::Extra {
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
