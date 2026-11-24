# Publishing a release

Record changes under **Unreleased** in the [changelog](../CHANGELOG.md). Before tagging a release, give the section its version and date. Update the version in `Cargo.toml` and refresh `Cargo.lock`. Use a tag such as `v0.1.0` for version `0.1.0`.

Run `make check` and `make build`. Test the release binary in a fresh Codex home. Check installation, a narrow main chat, a side chat, and removal. Leave the standard keybindings enabled. Record the tested Codex versions and platforms in the release notes.

Once CI passes on that commit, tag it and push the tag to GitHub. Prepare a draft release with the changelog entry, installation instructions, and a SHA256 checksum for each binary. Publish after verifying the downloads against the tested artifacts.
