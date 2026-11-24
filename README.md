# Codex Statusline

[![CI](https://github.com/adithya-n05/codex-statusline/actions/workflows/check.yml/badge.svg)](https://github.com/adithya-n05/codex-statusline/actions/workflows/check.yml) [![Contributors](https://img.shields.io/github/contributors/adithya-n05/codex-statusline)](https://github.com/adithya-n05/codex-statusline/graphs/contributors) [![Last commit](https://img.shields.io/github/last-commit/adithya-n05/codex-statusline/main)](https://github.com/adithya-n05/codex-statusline/commits/main/) [![MIT license](https://img.shields.io/github/license/adithya-n05/codex-statusline)](LICENSE)

Codex Statusline shows the current chat, Git changes, and usage in three rows beneath the Codex CLI prompt. The standard keyboard shortcuts stay available in the main chat and in side chats.

## What goes where

| Row | Contents | When space runs out |
| --- | --- | --- |
| 1 | Directory, chat title, model, reasoning effort, service tier (`Standard`, `Fast`, or `Ultrafast`), permissions, Git changes, then plan progress | Truncate at the right edge. |
| 2 | Context used, weekly allowance (`7D`) or credits, account, then output speed | Continue onto row three. |
| 3 | Tool activity, CLI version, and session ID | Truncate at the right edge. When usage wraps, its continuation takes this row. |

Codex CLI 0.172.0 gives the footer three rows in a tall window when shortcut hints are visible. A side chat gets its own, narrower column. Text that fits exactly stays visible. Codex adds an ellipsis when text extends past the edge or a wrapped usage row hides the tools row below it. The CLI version and session ID come last on row three. Codex sets their position in each pane.

## Install in Codex CLI

Build the binary from the [isolated development shell](#development). If you use a custom Cargo target directory, substitute its release path in the install command.

```sh
cargo build --release --locked
install -d "$HOME/.local/bin"
install -m 755 target/release/codex-statusline "$HOME/.local/bin/codex-statusline"
codex mcp add codex-statusline --env "CODEX_HOME=${CODEX_HOME:-$HOME/.codex}" -- "$HOME/.local/bin/codex-statusline"
```

Set these entries in your Codex `config.toml`, using the existing sections if present. Open a new CLI session to load the provider.

```toml
[features]
tui_extensions_statusline = true

[tui]
status_line_provider = { server = "codex-statusline", tool = "footer" }
```

Codex starts the Rust executable over standard input and output. The provider reads the current pane’s snapshot, Git status, and local Codex records. To switch back, set `status_line_provider = "default"`. You can then run `codex mcp remove codex-statusline` and remove the installed executable.

## Reading the gauges

**Context.** `Context 56% used` means the chat has used 56% of its effective context window. On resume, the footer can recover the last measurement for the same thread and model before the next turn. A fresh chat needs its first usage measurement. The bar changes from Bathory blue to peach at 50% of the measured compaction limit, orange at 75%, and red at 90%. Its fill still shows the CLI’s context usage. When Codex has yet to report a matching limit for the current thread, model, and window, the colors follow context usage up to orange. A CLI update that changes the diagnostic format can also make the threshold unavailable.

**Weekly usage and credits.** `7D` shows the weekly allowance remaining and the time until Codex’s reported reset. Once an observed balance falls, credits take its place until that same reset. A week that opens at 60,000 credits and receives 2,000 more has a base of 62,000. At a balance of 31,000 the bar is half full. Both balances round to whole credits for display. A later top up increases the base by the amount added.

The provider keeps timestamped balance observations for the signed in account under `$CODEX_HOME/codex-statusline/credits.sqlite3` (normally `~/.codex/codex-statusline/credits.sqlite3`). Chats can contribute observations in any order. If no available session recorded the balance at reset, the first observation in the window supplies the opening value. Purchases and usage that happen between two observations can only be seen as a net change.

**API usage.** A chat using an API key shows its reported input and output tokens and the CLI’s dollar estimate when available. That estimate covers the thread. Codex supplies the pricing data. Usage across every chat or the whole API key requires separate billing access.

**Output speed.** `Live` updates when Codex records the output token count for a model response during the turn. The rate uses the elapsed time from the first recorded output item in that response, so a shell command between responses doesn’t slow down the reading. `Last` keeps the most recent rate after the turn ends. For older chats that only record whole turn timing, the rate includes time spent waiting for tools.

**Tool activity.** A batch of three shell requests adds three to both `Tools` and `Shell`. Start and completion records with the same request ID count once. Terminal input and polls appear under `Terminal I/O`. A failed request stays in its category and also counts toward `Failed`.

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

Develop one behavior per cycle. The [contribution guide](CONTRIBUTING.md) covers testing and review. See the [code of conduct](CODE_OF_CONDUCT.md) for community standards and the [security policy](SECURITY.md) for private reports.

## Measuring coverage

Contributors can run the same coverage check as CI with `cargo coverage`. It measures which Rust source lines the tests execute and requires 90% coverage overall and in each measured source file. Building and running the footer requires neither coverage tool.

To measure coverage locally, run these commands once inside the isolated development shell.

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.2 --locked
```

## Releases and license

The project uses the MIT License. The changelog tracks work awaiting a release. [Changelog](CHANGELOG.md) · [Release process](docs/releases.md) · [License](LICENSE)
