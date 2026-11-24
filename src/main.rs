use std::{env, process::ExitCode};

fn main() -> ExitCode {
    if env::args_os().skip(1).eq(["--version"]) {
        println!("codex-statusline {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    eprintln!("Usage: codex-statusline --version");
    ExitCode::from(2)
}
