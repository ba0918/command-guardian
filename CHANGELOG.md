# Changelog

## [Unreleased]

- Add an OpenCode V2 2.0.21 shell-tool plugin and `hook --agent opencode` protocol. Native approval remains required for guardian `ask`, including after saved permissions; confirmed shadow mode retains native permissions without guardian enforcement.
- Include the plugin and runtime dependencies in the binary release archive. Registration and authenticated connection are manual; V1 and later V2 releases are not verified.

## [0.1.1] - 2026-10-02

- Stop treating literal redirects to `/dev/null`, including quoted paths and stderr redirects, as destructive truncation. Other targets and destructive commands remain checked.

## [0.1.0] - 2026-10-02

- Provide a CLI and agent hook that inspect Bash commands for destructive effects and return `allow`, `ask`, or `block` based on target paths and configuration.
- Distribute a single Linux x86_64 binary through GitHub Releases.

[0.1.1]: https://github.com/ba0918/command-guardian/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/command-guardian/compare/92db973828fe07d40fe5114d1714456d2f6fffaf...v0.1.0
