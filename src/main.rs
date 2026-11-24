mod protocol;

use std::{env, io, process::ExitCode};

fn main() -> io::Result<ExitCode> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, None) => {
            protocol::serve(io::stdin().lock(), io::stdout().lock())?;
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
