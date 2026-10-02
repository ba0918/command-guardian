# Changelog

## [Unreleased]

## [0.1.1] - 2026-10-02

- Stop treating literal redirects to `/dev/null`, including quoted paths and stderr redirects, as destructive truncation. Other targets and destructive commands remain checked.

## [0.1.0] - 2026-10-02

- Provide a CLI and agent hook that inspect Bash commands for destructive effects and return `allow`, `ask`, or `block` based on target paths and configuration.
- Distribute a single Linux x86_64 binary through GitHub Releases.

[0.1.1]: https://github.com/ba0918/command-guardian/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/command-guardian/compare/92db973828fe07d40fe5114d1714456d2f6fffaf...v0.1.0
