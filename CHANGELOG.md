# Changelog

## [Unreleased]

- Automatically connect the OpenCode V2 plugin to its own authenticated local background service when connection options are omitted. Explicit servers still use `serverUrl` and `passwordEnv`; missing or mismatched service registration does not permit execution.

## [0.1.2] - 2026-10-03

- Add an OpenCode V2 2.0.21 shell-tool plugin and `hook --agent opencode` protocol. Native approval remains required for guardian `ask`, including after saved permissions; confirmed shadow mode retains native permissions without guardian enforcement.
- Install the binary with mise and register the matching release tag's plugin with OpenCode's GitHub package installer. Configure its authenticated connection to the same OpenCode server after registration. No separate npm package is required; V1 and later V2 releases are not verified.
- Include the same-version plugin and its runtime dependencies in the binary release archive as an alternative installation route.

## [0.1.1] - 2026-10-02

- Stop treating literal redirects to `/dev/null`, including quoted paths and stderr redirects, as destructive truncation. Other targets and destructive commands remain checked.

## [0.1.0] - 2026-10-02

- Provide a CLI and agent hook that inspect Bash commands for destructive effects and return `allow`, `ask`, or `block` based on target paths and configuration.
- Distribute a single Linux x86_64 binary through GitHub Releases.

[0.1.2]: https://github.com/ba0918/command-guardian/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ba0918/command-guardian/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/command-guardian/compare/92db973828fe07d40fe5114d1714456d2f6fffaf...v0.1.0
