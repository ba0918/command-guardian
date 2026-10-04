# OpenCode V2 integration

The plugin checks the agent's `shell` tool using the existing guardian binary. It covers normal and background calls and shell calls through Code Mode. It does not check a human's direct shell commands or commands executed inside MCP tools.

## Supported environment

Use Linux x86_64 or WSL with OpenCode V2, explicitly configured Bash, and no other hooks that change the command, working directory, or shell. No other plugin's permission `evaluate` hook may override guardian's result; if one does, the behavior is not guaranteed. Linux tests use GNU and musl guardian binaries. WSL has not been tested separately.

Support is defined by the plugin API the plugin relies on, not by the OpenCode version. The plugin does not check the host version and never fails or warns because of it. When it loads, it checks that these members of the plugin context exist:

- `shell.hook`, for the `create.before` hook that receives the actual shell, working directory, and command
- `permission.hook`, for the `evaluate` hook that keeps guardian approvals as native confirmations
- `permission.reply`, to withdraw guardian approvals that are no longer needed
- `tool.transform`, to wrap the `shell` tool
- `location.directory`, the directory used to resolve the working directory

If any is missing, loading fails with an error that names the missing members. What OpenCode then does with shell commands is decided by OpenCode and is not guaranteed. The integration tests pin OpenCode V2 2.0.21 only to make the test environment reproducible; that version is not a support condition. OpenCode V1 has not been tested.

This is accident prevention, not a security sandbox. `allow` is not proof of safety. Changes to files during approval and changes by other hooks are outside the input-consistency guarantee.

## Optional LLM advice

LLM advice is available from guardian 0.2.0. Use the plugin from the same release as the binary.
Configure advice in the guardian user's configuration, not in `opencode.json` or project TOML. Start with `observe` and supply `TYPESAFE_API_KEY` to the guardian process environment. Read the [advice setup and data-transfer notice](../README.md#optional-llm-advice) before enabling it.

The plugin keeps its normal six-second outer deadline unless guardian sends a valid advice notification and receives acknowledgment through the inherited control channel.
On acceptance, the outer deadline becomes the notification reception time plus `advisor.timeout_ms` plus 1,000ms. Long durations use bounded timer intervals without shortening that deadline.
Without acknowledgment, guardian skips advice and keeps the mechanical verdict. The plugin does not grant a longer deadline to an old binary or an invalid notification.
The guardian parent still enforces its own advice deadline, including context acquisition and communication; the extra second is for the outer protocol, not extra model time.

`advisor.mode = "observe"` does not change native approval or mechanical judgments. With `enforce`, guardian may tighten a verdict, but `ask` still requires native approval.
The separate `[mode] enforce = false` setting is shadow mode. Its log still includes command text; the advice log excludes request text by default.

## Automatic connection to the background service

From guardian 0.1.3, normal `opencode` startup needs no connection options. This feature is not present in 0.1.2. Keep Bash explicitly selected and register the matching plugin, for example from a mise-managed release bundle:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "shell": "/bin/bash",
  "plugins": ["/absolute/path/to/mise/installs/github-ba0918-command-guardian/latest/opencode"]
}
```

The mise `latest` link selects the binary's matching plugin without a second Git package update. Restart the OpenCode server after an upgrade to load the new plugin. Register only one copy of guardian; remove an older Git package entry before switching to the bundle.

The plugin reads the existing service registration once and validates its PID, URL, and credentials before sending any HTTP request. It accepts only an authenticated loopback HTTP endpoint, then uses the OpenCode SDK to confirm that the server reports its own PID; it does not compare the server version. The validated URL and credentials stay fixed, and automatic connections reject HTTP redirects. It does not start a service, write credentials, or set password environment variables. A remote or explicitly started server needs the connection options below; `opencode run --standalone` needs none. Explicit options take precedence; incomplete options never fall back to discovery. If no registration exists or it fails validation, the plugin sends nothing to the registered address and runs without a connection, as described next.

## Standalone runs and running without a connection

In releases after 0.2.0, `opencode run --standalone` works without connection settings. A standalone run starts a private server whose password changes on every run, so neither automatic connection nor the explicit settings below can reach it. The same applies when the managed service is disabled in OpenCode's configuration.

Without a connection, the plugin judges each command in the shell's `create.before` hook, using the shell, working directory, and command that OpenCode is about to start:

- `block` refuses the command.
- `allow` adds no guardian confirmation or refusal.
- Commands that would need a guardian approval — `ask`, a missing guardian or an invalid response or timeout, a shell other than Bash, or input that cannot be matched to what will start — get no guardian confirmation. They are left to OpenCode's own permissions: OpenCode's rules allow them, ask through OpenCode's own shell confirmation, or deny them. The plugin does not stop them because it lacks an approval route.
- Confirmed shadow mode still adds no guardian refusal.

If connection options are set but incomplete, or the connection or authentication fails, the plugin does not switch to automatic connection. It writes a warning to its standard error once when it loads and once per command, then behaves as without a connection. OpenCode keeps that output in its log; whether it is shown on screen is not guaranteed.

## Install with mise and a GitHub package

The plugin is available from guardian 0.1.2. Install the binary with mise, then register the plugin from the same release tag:

```sh
mise use -g github:ba0918/command-guardian@0.2.0
opencode plugin add 'github:ba0918/command-guardian#v0.2.0::path:plugins/opencode'
```

For another release, replace `0.2.0` in both commands and the configuration below with that release's version. OpenCode V2 supports GitHub package specifications with tags and repository-subdirectory selectors. The `plugin add` command installs the package and adds it to your global configuration. You do not need to place the plugin files yourself or install a separate npm package. See [OpenCode's plugin configuration guide](https://opencode.ai/v2/docs/plugins).

The isolated installation check uses the pinned host's public `plugin add` command with an immutable local Git commit and `::path:plugins/opencode`. It installs the source package and its dependencies. Native approval tests then load that installed package in the real host. Downloading this plugin from the published GitHub tag has not been tested. Releases before 0.1.2 do not contain the plugin.

For 0.1.2 or an explicit server, complete the connection settings below after adding the package. Update the guardian binary and the plugin's tag together.

## Alternative: install the matching bundle

Obtain `command-guardian-v0.2.0-x86_64-unknown-linux-musl.tar.gz` and its `.sha256` file from [the matching GitHub release](https://github.com/ba0918/command-guardian/releases/tag/v0.2.0). For another release, replace `0.2.0` in the filenames with that release's version. Verify the checksum before extracting:

```sh
sha256sum -c command-guardian-v0.2.0-x86_64-unknown-linux-musl.tar.gz.sha256
tar -xzf command-guardian-v0.2.0-x86_64-unknown-linux-musl.tar.gz
```

Install the binary through [the existing mise or source-build route](../README.md#install), or place the extracted `command-guardian` on PATH. Place the extracted `opencode/` directory at a stable, user-owned absolute path. Keep `server.js`, `package.json`, `source/`, and `licenses/` together. The manifest version is generated from Cargo.toml. Update the binary and plugin together. No separate npm publication or runtime dependency installation is required; the JavaScript bundle includes dependencies and license notices.

To build the same archive locally from the matching release's source checkout:

```sh
bun install --frozen-lockfile --cwd plugins/opencode
cargo build --release --locked --target x86_64-unknown-linux-musl
scripts/package-release.sh target/x86_64-unknown-linux-musl/release/command-guardian x86_64-unknown-linux-musl target/package
```

## Register and connect to an explicit server

Edit the OpenCode V2 global configuration yourself, preserving existing settings. Its usual location is `$XDG_CONFIG_HOME/opencode/opencode.json`, or `~/.config/opencode/opencode.json` when XDG_CONFIG_HOME is unset. Replace the package string added by `plugin add` with the object below to supply connection options; keep your other plugin entries. Use the same release tag as above and explicitly select Bash:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "shell": "/bin/bash",
  "plugins": [
    {
      "package": "github:ba0918/command-guardian#v0.2.0::path:plugins/opencode",
      "options": {
        "serverUrl": "http://127.0.0.1:4097",
        "passwordEnv": "OPENCODE_SERVER_PASSWORD"
      }
    }
  ]
}
```

For the archive installation instead, set `package` to the extracted plugin directory's stable absolute path, such as `/absolute/path/to/opencode`.

`serverUrl` must identify the server hosting this plugin. It does not discover your service or credentials. `passwordEnv` names an environment variable visible to that server, not a password stored in JSON. Authentication uses HTTP Basic with the host's `opencode` username. Do not point it at another server. Use loopback or protected HTTPS; do not transmit credentials over untrusted HTTP. Do not use these settings for `opencode run --standalone`; it needs none. If the settings are incomplete or the connection fails, the plugin warns and works [without a connection](#standalone-runs-and-running-without-a-connection) rather than stopping commands.

Set a private password in the server process environment, then start the same server at the configured address:

```sh
read -rs OPENCODE_SERVER_PASSWORD
export OPENCODE_SERVER_PASSWORD
opencode serve --hostname 127.0.0.1 --port 4097
```

The terminal waits for password input without echoing it. Keep the password out of source control, arguments, and shared logs. Attach the OpenCode client using its normal authenticated connection flow. The plugin does not configure models, edit personal settings, or start another service.

The server must find `command-guardian` on PATH. Guardian reads its existing configuration and shadow-log settings; the plugin does not duplicate them. `command-guardian hook --agent opencode` reads JSON containing `command`, absolute `cwd`, and absolute `shell` on standard input. See [the representation decision](decision/records/2026-10-02-opencode-v2-protocol.md).

## Approval behavior and limits

- `allow` retains native permissions; it does not override native denial or confirmation.
- `ask` creates a native `command-guardian` approval for this execution. Saved permission does not skip it. `run --auto` can approve with `once`; guardian adds no second human prompt.
- `block` prevents execution, including with automatic approval enabled.
- Confirmed shadow mode adds no guardian approval or rejection. Native permissions remain. Shadow-log failures and unsupported shells are warnings.
- Missing guardian, invalid responses, process timeout, and unsupported shells require approval.
- If an approval request cannot be created, or no response to its creation arrives, the plugin writes a warning to its standard error and leaves that command to OpenCode's own permissions. A later reply to a leftover guardian confirmation does nothing to that command. A rejection that arrives before the creation response still stands: the command does not run even if creation then fails. If the approval event stream ends while a request is being created and the creation response then reports a pending approval, the command is cancelled rather than left to OpenCode.
- If the approval event stream has already ended when a command needs approval, the plugin reconnects the stream and then creates the approval request as usual. If reconnecting fails, it warns and leaves that command to OpenCode's own permissions.
- Without a connection, including failed explicit options, the rules in [Standalone runs and running without a connection](#standalone-runs-and-running-without-a-connection) apply instead.

Approval waits have no plugin-specific deadline or saved history. A wait starts when the approval request's creation response arrives. Detected cancellation, unload, stream termination, or stream error permanently cancels pending execution. A later approval or reconnection does not replay it. Silent stalls are not detected immediately. Orphaned approval-display cleanup is best effort, including creation/cancellation races. The plugin does not stop already launched background processes.

## Verification scope

Model-free tests load the production plugin in an isolated real host, the pinned 2.0.21 test environment, obtain its transformed shell through public `tool.list()`, and use the real guardian and native approvals. The no-connection behavior is tested by launching the same host with `serve --stdio`, the route `run --standalone` uses, with no connection options; a full `run --standalone` run is not part of the tests. Failed explicit options and failed approval-request creation are tested with the ordinary isolated launch. Code Mode tests use the pinned official interpreter with that host-derived executor. They do not invoke a model or the CLI's complete agent loop. Source inspection establishes how `run --auto` replies `once`; fixture replies exercise that native response.

The RPC executor's cancellation signal is tested. Actual active model-driven session interruption, WSL-specific execution, and model-driven end-to-end use are not claimed as observed by these tests.
