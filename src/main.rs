use codex_statusline::protocol;
use serde_json::{Value, json};

use std::{env, io, process::ExitCode};

fn main() -> io::Result<ExitCode> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, None) => {
            protocol::serve(io::stdin().lock(), io::stdout().lock(), |state| {
                json!(
                    state
                        .pointer("/thread/workspace/cwd")
                        .and_then(Value::as_str)
                )
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
