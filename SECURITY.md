# Security policy

Report vulnerabilities privately through [GitHub private vulnerability reporting](https://github.com/adithya-n05/codex-statusline/security/advisories/new). During development, report against a commit on `main`. Include the commit or release, your Codex CLI version, the impact, and a way to reproduce it with invented data.

The provider runs locally with access to the active workspace and local Codex data. A change that exposes a credential, discloses another account’s usage, or lets repository content execute a command through the footer belongs in a private report.

Keep API keys, access and refresh tokens, account identifiers, real prompts, and private repository names out of reports and attachments. A small synthetic event or a redacted screenshot is enough to begin. Never upload an entire Codex home, authentication file, session transcript, or database.

The maintainer will follow up in the private report, verify the issue, and coordinate a fix and disclosure there. Credit in an advisory is optional.
