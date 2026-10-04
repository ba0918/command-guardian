# Changelog

## [Unreleased]

- Make the OpenCode V2 plugin work with `opencode run --standalone` and with the managed service disabled, without connection settings. Without a connection it judges the actual shell input, refuses `block`, and leaves commands that would need a guardian approval to OpenCode's own permissions instead of stopping them.
- Warn on standard error instead of stopping commands when explicit connection options are incomplete or fail, or when a guardian approval request cannot be created; those commands are left to OpenCode's own permissions. Failed explicit options never fall back to automatic connection.
- Stop requiring OpenCode 2.0.21. The plugin no longer checks the host version; it checks at load time that the plugin API it relies on exists and names any missing member.
- Add the opt-in, not recommended `mode.defer_ask` user setting. When enabled, hooks leave `ask` verdicts to the agent's own permission decision instead of requesting confirmation, while `block` is still denied and `check` is unchanged. Every deferral is recorded in the shadow log with a sixth `deferred` field, and OpenCode receives a separate `deferred` response.

## [0.2.0] - 2026-10-04

- Add opt-in, deadline-bound LLM advice through TypeSafe for CLI and agent hooks. Advice is off by default, accepts user configuration only, and can tighten mechanical verdicts without releasing a block. Observe mode records advice without changing the verdict.
- Bound conversation context, skip external sending on detected secrets or oversized requests, and keep request text out of advice logs unless debug logging is enabled. Secret detection is incomplete; real-model accuracy and current TypeSafe service compatibility remain unverified.
- Negotiate the advice deadline with the OpenCode plugin, including long timeouts, without shortening the accepted duration. Advice failures preserve the mechanical verdict.

## [0.1.3] - 2026-10-03

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

[0.2.0]: https://github.com/ba0918/command-guardian/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/ba0918/command-guardian/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ba0918/command-guardian/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ba0918/command-guardian/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/command-guardian/compare/92db973828fe07d40fe5114d1714456d2f6fffaf...v0.1.0
