mod terminal;

use libtest_mimic::{Arguments, Trial};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--serve-rows") {
        let rows: [String; 3] = serde_json::from_str(&args[2]).unwrap();
        let tree =
            codex_statusline::layout::footer(&rows[0], vec![serde_json::json!(rows[1])], &rows[2]);
        codex_statusline::protocol::serve(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            |_| tree.clone(),
        )
        .unwrap();
        return;
    }
    libtest_mimic::run(
        &Arguments::from_args(),
        vec![Trial::test("native_footer_geometry", || {
            native_footer_geometry();
            Ok(())
        })],
    )
    .exit();
}

fn native_footer_geometry() {
    let mut tty = terminal::Terminal::start(
        100,
        ["Directory · title", "CTX 50% · 7D 70%", "Tools · CLI"],
    );
    tty.wait(
        |lines| {
            lines.windows(3).any(|rows| {
                rows[0].trim() == "Directory · title"
                    && rows[1].trim() == "CTX 50% · 7D 70%"
                    && rows[2].trim() == "Tools · CLI"
            })
        },
        "three short footer rows",
    );
}
