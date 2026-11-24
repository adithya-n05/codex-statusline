# Contributing

A useful bug report tells us which Codex CLI version you used, whether the footer was in a main chat or a side chat, and what you expected to see. For a layout issue, include the terminal width in columns if you know it. A small example with invented paths and session data usually shows the problem clearly. For confidential reports, use the [security policy](SECURITY.md).

## Changing a behavior

Open the [project development shell](README.md#development) before installing any tools. Keep one observable change in each cycle.

1. Write the test first. Run `cargo test --workspace --locked --no-fail-fast` and check that it fails for the intended reason.
2. Implement that behavior. Run the whole suite again, including the terminal tests. When changing wrapping, exercise a narrow side chat and a resize back to full width.
3. Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo coverage`. Coverage must reach 90% overall and in each measured source file.
4. Commit the passing change with a Conventional Commit subject such as `fix(layout): keep the usage row visible after resize`. Push the commit to your branch before starting the next behavior.

In the pull request, describe what changed for someone using the footer and how you verified it. Use synthetic fixtures for account state and session events. Follow the [code of conduct](CODE_OF_CONDUCT.md).
