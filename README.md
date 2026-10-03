# command-guardian

Check shell commands for destructive effects before execution. command-guardian is a Rust CLI that returns `allow`, `ask`, or `block` based on the command, its target paths, and your configuration. The same binary can handle agent hook requests.

It does not execute the command you submit. It does read configuration, inspect filesystem paths, and query Git when classifying targets.

This is an accident-prevention tool, not a security sandbox. An `allow` verdict does not prove that a command is safe. The caller must decide whether to execute it, and agent hooks must be installed and honored by the host.

## Install

The recommended installation method is [mise](https://mise.jdx.dev/):

```sh
mise use -g github:ba0918/command-guardian
command-guardian --help
```

This installs the Linux x86_64 binary from GitHub Releases. To use changes listed under Unreleased, build from source below; published releases do not contain them yet.

### Build from source

Use Linux and a recent stable Rust toolchain with Cargo. The runtime uses Unix sockets; process-isolation tests run on Linux. Windows-native and macOS builds are not provided.

```sh
git clone https://github.com/ba0918/command-guardian.git
cd command-guardian
cargo build --release --locked
cargo install --path . --locked
command-guardian --help
```

`cargo install` places the binary in Cargo's bin directory, which must be on your `PATH`. You can also run `./target/release/command-guardian` without installing it.

## Check a command

Pass the entire command as one quoted argument. Single quotes prevent your shell from expanding variables or running substitutions before the checker sees them.

```sh
command-guardian check 'rm -rf /etc/nginx' --cwd /tmp
command-guardian check 'rm -rf /etc/nginx' --cwd /tmp --format json
command-guardian check 'S=/etc/nginx; rm -rf "$S"'
```

These commands submit text for analysis. They do not run `rm`.

`--cwd` defaults to the current working directory. `--format` accepts `text` or `json` and defaults to `text`. Use `--` before a command string that would otherwise be interpreted as an option.

```sh
command-guardian check -- '--help'
command-guardian check --help
```

| Exit code | Meaning |
|---|---|
| `0` | `allow`, or a successful help request |
| `1` | `ask`: review the command before executing it |
| `2` | `block` |
| `3` | Unable to produce a judgment, such as invalid CLI arguments or an output failure |

Text output starts with the verdict. JSON includes `verdict`, `effects`, `rules`, `message`, and `reason`. Configuration warnings go to standard error.

Command text and interpreted option names and format values must be UTF-8. On Unix, `--cwd` accepts OS paths with non-UTF-8 bytes. JSON and text path displays are not a lossless encoding of those bytes.

### How targets affect the verdict

By default:

| Target classification | Verdict |
|---|---|
| Children of temporary or configured allowed directories | `allow` |
| Git worktree paths for which `git status --porcelain -uall` reports no changes | `allow` |
| Protected paths | `block` |
| Resolved paths that cannot be classified | `ask` |
| Explicit paths whose values cannot be resolved | `block` |

Temporary directories include `/tmp`, `/var/tmp`, and `TMPDIR`. Their roots are not allowed by the temporary-directory rule. Protected paths include system directories, `.git`, configured protected roots, and the roots of the current worktree, working directory, and home directory.

For unknown target collections, such as deletion through `find` or `xargs`, the checker uses the source directory when it can determine one. An unknown source produces `ask` by default. Globs use their directory base rather than expanding the filesystem pattern. `find` filters such as `-name` do not narrow the classification.

The final verdict is the most restrictive result: `block` takes precedence over `ask`, which takes precedence over `allow`. See the [path and verdict specification](docs/ir/judgment.md) for exact rules.

## Configuration

The checker reads three layers in order:

1. Built-in defaults.
2. `$XDG_CONFIG_HOME/command-guardian/config.toml`, falling back to `$HOME/.config/command-guardian/config.toml`.
3. The first `.command-guardian.toml` found by searching upward from the working directory.

Lists append; later scalar values override earlier ones. Relative user-config paths resolve against the configuration directory. Relative project-config paths resolve against the Git worktree root, or the configuration directory if no worktree root is found.

A minimal project configuration can protect data and require review before a push:

```toml
[paths]
protected_roots = ["data"]

[[commands.guard]]
program = "git"
reason = "Review the destination and commits before pushing."
verdict = "ask"
deny = [["push"]]

[commands.guard.examples]
deny = ["git push origin main"]
allow = ["git status"]
```

Guard examples are checked when configuration loads. A malformed individual guard or a failing example disables that guard and produces a warning. An invalid `commands` section or guard list discards the entire affected file, as do general configuration errors. Other valid layers still apply, and the checker warns about the rejected file.

Project configuration is untrusted by default. It can add protected roots and restrictive rules, but cannot add allowed roots, disable built-in rules, or otherwise relax protection. To allow a project's settings to relax protection, add its path to `trusted_projects` in your user configuration. Do this only for projects whose configuration you trust. Custom `allow` rules are accepted only from user configuration.

| Setting | Default | Purpose |
|---|---|---|
| `paths.allowed_roots` | Temporary directories | Allow effects beneath these roots, subject to protected-path rules |
| `paths.protected_roots` | Empty | Add protected directories and their descendants |
| `unknown.verdict` | `"ask"` | Choose `ask` or `block` for unclassified targets |
| `rules.disable` | Empty | Disable extraction of `delete`, `truncate`, or `format` effects |
| `rules.custom` | Empty | Match command text with named regular-expression rules |
| `commands.guard` | Empty | Match program arguments, flags, option values, or environment assignments |
| `git.enabled` | `true` | Enable Git-based classification |
| `mode.enforce` | `true` | Return hook decisions instead of only logging them |
| `trusted_projects` | Empty | Allow listed projects to use otherwise restricted settings |
| `advisor.mode` | `"off"` | Available from 0.2.0. Disable advice, observe it, or enforce tighter verdicts; user configuration only |

Disabling an effect means the checker no longer extracts it. It is not just a way to hide its warning. See [configuration](docs/ir/config.md) and [command guards](docs/ir/guards.md) for the full contracts.

## Optional LLM advice

LLM advice is available from guardian 0.2.0.
Advice is off by default. Start with `observe`, which records advice without changing the mechanical verdict. Switch to `enforce` only after reviewing the results and accepting the external data transfer.

Add this section to `$XDG_CONFIG_HOME/command-guardian/config.toml`, or `$HOME/.config/command-guardian/config.toml` when XDG is unset:

```toml
[advisor]
mode = "observe"
model = "jev-latest"
timeout_ms = 2000
max_request_bytes = 65536
intervention_threshold = 0.9
context_exchanges = 3
context_ttl_hours = 24
debug_text = false
```

These are the default values except for `mode`, whose default is `"off"`.
`max_request_bytes` limits the encoded outgoing request. `context_exchanges = 0` disables conversation context, not advice. `context_ttl_hours` limits cache lifetime; `debug_text` controls request-text logging.
Only user configuration can enable or configure advice. Project `[advisor]` settings are ignored with a warning, even for trusted projects. Invalid user advisor settings discard that entire user configuration file; other valid layers still apply.
Provide authentication through `TYPESAFE_API_KEY` in the guardian process environment. Do not put the key in TOML or a command that will enter shell history.

When enabled, the TypeSafe adapter sends at most one HTTPS request to `https://api.typesafe.ai/v1/systemone` per eligible judgment, with no retries.
The request includes the command, working directory, mechanical result and available bounded conversation context. Context defaults to the last three exchanges; assistant replies help resolve references but do not establish user approval.
It excludes tool results, file contents, the full conversation and environment-variable listings. Detected secrets, encoding failures and requests above the byte limit skip sending rather than truncate the request. Secret detection is incomplete; do not enable advice for inputs you cannot send to the provider.

`enforce` can tighten mechanical `allow` and `ask` verdicts, but cannot release `block` or turn `ask` into `allow`.
Harmful irreversible effects at or above the threshold are blocked. For major destruction at or above the threshold, `ask` also requires a confirmed instruction covering the target and all effects, with instruction-match probability meeting the threshold; otherwise it is blocked.
Missing context does not by itself skip model evaluation. Timeout, invalid responses, communication failures and uncertain assessments preserve the mechanical verdict rather than add a new confirmation.

`timeout_ms` covers context acquisition, child startup, validation and communication, separately from the five-second mechanical budget. The parent enforces the advice deadline.
`intervention_threshold` is a model-probability threshold, not an accuracy guarantee. `jev-latest` can change upstream. Real-model accuracy and compatibility with the current TypeSafe service have not been verified.

Advice logs go to `$XDG_STATE_HOME/command-guardian/advisor.jsonl`, falling back to `$HOME/.local/state/command-guardian/advisor.jsonl`.
By default they contain metadata, including verdicts, model, elapsed time and failure or skip classification, without command or conversation text. `debug_text = true` adds request text with detected secrets masked; treat it as sensitive data.
Context caching, where a separate hook is needed, stores a bounded window with owner-only access. The default lifetime is 24 hours; expired entries are not used and are removed on access. Logs and cache are separate: disabling debug text does not disable context caching.
The existing shadow log still contains command text and target paths. `advisor.mode = "observe"` and `[mode] enforce = false` are different settings.

See the [advisor requirements](docs/ir/advisor/policy.md), [context verification](docs/verification/llm-advisor-context.md) and [verification results](docs/verification/llm-advisor.md) for supported context routes and verification limits.

## Agent hooks

For OpenCode, see [OpenCode V2 plugin setup](#opencode-v2) below. The following hook instructions apply to Claude and Codex.

Invoke the binary from a pre-execution hook and send one JSON request on standard input:

```sh
printf '%s\n' '{"tool_name":"Bash","tool_input":{"command":"rm -rf /etc/nginx"},"cwd":"/tmp"}' |
  command-guardian hook --agent claude
```

Use `--agent codex` for the Codex output mapping. Both modes currently accept the same request shape shown above. Only `tool_name: "Bash"` requests with a nonempty `tool_input.command` are judged. If `cwd` is omitted, the checker uses its current directory.

Register the command in your agent's hook configuration using that host's supported mechanism. The binary does not install hooks or translate other tool-request formats. Verify that the host supplies this request shape and honors `hookSpecificOutput` before relying on the integration.

| Verdict | Claude mode | Codex mode |
|---|---|---|
| `allow` | No output | No output |
| `ask` | `permissionDecision: "ask"` | No output; confirmation belongs to the host's approval flow |
| `block` | `permissionDecision: "deny"` | `permissionDecision: "deny"` |

Decisions use a `hookSpecificOutput` envelope with `hookEventName: "PreToolUse"` and a reason. In Claude mode, `permission_mode` values `dontAsk` and `bypassPermissions` suppress `ask` output; they do not suppress `block`.

The `hook` subcommand always exits with `0`. Hosts must read its JSON decision, not its exit status. Invalid input or an unrecognized agent produces no decision. This is not fail-closed enforcement. See the [agent protocol specification](docs/ir/agents.md).

### OpenCode V2

The plugin is available from guardian 0.1.2. Install the binary with mise, then register the plugin from the same release tag with OpenCode V2 2.0.21:

```sh
mise use -g github:ba0918/command-guardian@0.2.0
opencode plugin add 'github:ba0918/command-guardian#v0.2.0::path:plugins/opencode'
```

For another release, replace `0.2.0` in both commands with that release's version. The GitHub package specification selects the plugin subdirectory at the matching tag. No separate npm package is required.
From 0.1.3, the plugin automatically connects to its own local background service when connection options are omitted. Explicit servers and 0.1.2 still need connection settings. See the [installation guide](docs/opencode.md#automatic-connection-to-the-background-service).

Use Linux x86_64 or WSL with explicitly configured Bash and no other hooks that change the command, working directory, or shell.
Follow the [installation guide](docs/opencode.md) for connection settings and the alternative release-archive installation.
The isolated installation check uses the public Git package installer with an immutable local Git commit and the same subdirectory selector. Downloading this plugin from the published GitHub tag has not been tested.

### Shadow mode

To observe hook judgments without returning decisions, put this in your user configuration:

```toml
[mode]
enforce = false
```

This setting affects hook decisions, not `check` verdicts or exit codes. Shadow mode writes one record per judgment to `$XDG_STATE_HOME/command-guardian/shadow.log`, falling back to `$HOME/.local/state/command-guardian/shadow.log`. Records contain the command text, target paths, verdict, reason, and timestamp. Treat the log as sensitive data.

The log uses owner-only permissions. The checker rejects symlinks at the dedicated `command-guardian` directory or `shadow.log` file and rejects nonregular log files. If logging fails, it warns on standard error, emits no hook decision, and still exits with `0`. It does not promise to reject every symlink in parent directories or hard links.

## Scope and limitations

The parser targets Bash and POSIX sh syntax. It reads nested command text in recognized `-c` shell invocations and `eval`, without executing it. It resolves known assignments and paths, and keeps uncertain state changes unresolved rather than choosing a branch or a pipeline execution mode.

Extracted effects include deletion through `rm`, `rmdir`, `unlink`, `find`, `xargs`, and `shred`; truncation through output redirection, `dd`, and `truncate`; and formatting through `mkfs`, `wipefs`, or `dd` writes to block devices.

The current implementation does not extract effects from `git clean`, overwrites by `mv` or `cp`, `sed -i`, or `rsync --delete`. Command guards can flag selected uses, but they do not add complete effect analysis for those programs. External scripts and programs can also perform actions that are absent from the submitted command text.

Ordinary parse failures, internal errors, and Git failures can produce `ask`. Input-attributed resource-limit violations can produce `block`. Limits include 1 MiB of command text, nesting depth 128, 1,000 parses per judgment, and a five-second judgment budget. That budget is not a guarantee that every filesystem system call can be interrupted within five seconds.

Do not use the checker as an access-control boundary or as the sole protection against malicious commands. See [parser behavior and isolation](docs/ir/parser.md) and [pipeline state](docs/ir/shell-state.md) for details.

## Development

### Repository layout

| Path | Responsibility |
|---|---|
| `src/` | CLI, environment input, hook protocol, and shadow logging |
| `crates/guardian-core/` | Shared values and diagnostics |
| `crates/guardian-advisor/` | Advice types, classification, context bounds, and secret detection |
| `crates/guardian-advisor-typesafe/` | TypeSafe request encoding, authentication, and HTTPS transport |
| `crates/guardian-parser/` | Shell parsing and normalized syntax trees |
| `crates/guardian-analysis/` | Effects, program invocations, and path-state analysis |
| `crates/guardian-judge/` | Filesystem observation and path classification |
| `crates/guardian-policy/` | Configuration, guards, verdict composition, and messages |
| `crates/guardian-app/` | Engine, configuration loading, isolated workers, advice deadlines, and local state |
| `tests/` | CLI, hook, configuration, and isolation integration tests |
| `docs/ir/` | Requirements and examples checked by kotowari |
| `docs/decision/records/` | Reasons and approvals behind specification decisions |

### Run checks

Install [kotowari](https://github.com/ba0918/kotowari) 0.3.0 or later with the `changes` command, and make it available on `PATH`. Then run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/kotowari-check.sh
cargo build --release --locked
```

For a narrower check of the binary's external contracts:

```sh
cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation
```

Tests isolate `HOME` and XDG directories so they do not read personal configuration or write personal logs. Keep Git fixtures under `CARGO_TARGET_TMPDIR`. Moving them to `/tmp` changes their classification and can invalidate the behavior being tested.

Production parsing runs in a child of the same binary through the app's runtime session. Keep pure parser and analysis tests shallow. Test deeply nested input, worker failures, and process isolation through the root binary rather than restarting the Rust test runner as a worker.

### Change the implementation and specification together

Read [PROJECT.md](PROJECT.md) for project conventions and the integration procedure. Requirements live in `docs/ir/`; much of the specification and decision history is currently in Japanese.

Before integrating a branch, establish its full comparison base and final candidate commit. The implementer and an independent reviewer each write their own change-conformance record in `.kotowari/changes/`. Both records must cover the same candidate and relevant specification files. Commit the records, then run the product checks above and the specification checks:

```sh
# Set BASE to the full commit ID chosen as the branch's comparison base.
HEAD_SHA=$(git rev-parse HEAD)
kotowari check --format json
kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format json
```

`kotowari check` validates specification structure and references. `kotowari changes` checks coverage and freshness of the recorded judgments. Neither command proves that a semantic judgment is correct; the independent review must compare the implementation with the requirements. Do not integrate unresolved specification decisions, and do not copy the implementer's record under a reviewer label.

If code or specification meaning changes after review, revisit the affected records and review them again. Do not run `kotowari changes` as a pre-commit requirement. See [PROJECT.md](PROJECT.md#conventions-specific-to-this-project) for the full procedure.

## License

Licensed under the [MIT License](LICENSE). Copyright (c) 2026 ba0918.
