# Codex Statusline

Codex Statusline is a Rust project for the custom footer in Codex CLI. The design fits three screen rows with the standard keyboard shortcuts enabled. The provider is in development.

## What goes where

| Row | Contents | When space runs out |
| --- | --- | --- |
| 1 | Directory, chat title, model, reasoning effort, permissions, Git changes, then plan progress | Truncate at the right edge. |
| 2 | Context usage and window size, weekly allowance (`7D`) or credits, account, then the last turn's token speed | Continue onto row three. |
| 3 | Tool activity, CLI version, and session ID | Truncate at the right edge. When usage wraps, its continuation takes this row. |

Codex CLI 0.172.0 gives the footer three rows in a tall window when shortcut hints are visible. A side chat gets its own, narrower column. The terminal tests use both panes to check what fits.

## Development

Use Rust stable, Make, and Codex CLI on your path. CI runs on macOS with Codex CLI 0.172.0. Install the coverage tools once with these commands.

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.2 --locked
```

| Command | What it does |
| --- | --- |
| `make test` | Run the full workspace suite and report every failing target. |
| `make check` | Check formatting, run Clippy and the tests, then measure line coverage. |
| `make build` | Build the release binary at `target/release/codex-statusline` using the lockfile. |

Develop one behavior per cycle. Begin with a failing test, run the full suite before and after the change, then commit and push. Coverage must reach 90% across the workspace and in each measured Rust source file.

## Releases and license

The project uses the MIT License. The changelog tracks work awaiting a release. [Changelog](CHANGELOG.md) · [Release process](docs/releases.md) · [License](LICENSE)
