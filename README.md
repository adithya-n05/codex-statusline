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

Use Rust stable to build the project. Terminal tests also need Codex CLI on your path. CI runs on macOS with Codex CLI 0.172.0. From the repository root, open a development shell that keeps installed tools and dependency downloads in the ignored `.tools/` directory.

```sh
env CARGO_HOME="$PWD/.tools/cargo" RUSTUP_HOME="$PWD/.tools/rustup" \
  PATH="$PWD/.tools/cargo/bin:$PATH" "$SHELL"
```

In that shell, run `rustup toolchain install stable --profile minimal --component rustfmt,clippy --no-self-update` on first use. Run the Cargo commands below from the same shell. Use `exit` when finished.

| Command | What it does |
| --- | --- |
| `cargo test --workspace --locked --no-fail-fast` | Run the full workspace suite and report every failing target. |
| `cargo fmt --all --check` | Check formatting. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lint the workspace and its tests. |
| `cargo build --release --locked` | Build `target/release/codex-statusline`. |

Develop one behavior per cycle. Begin with a failing test, run the full suite before and after the change, then commit and push.

## Measuring coverage

Contributors can run the same coverage check as CI with `cargo coverage`. It measures which Rust source lines the tests execute and requires 90% coverage overall and in each measured source file. Building and running the footer requires neither coverage tool.

To measure coverage locally, run these commands once inside the isolated development shell.

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.2 --locked
```

## Releases and license

The project uses the MIT License. The changelog tracks work awaiting a release. [Changelog](CHANGELOG.md) · [Release process](docs/releases.md) · [License](LICENSE)
