# OpenCode V2 integration

The plugin checks the agent's `shell` tool using the existing guardian binary. It covers normal and background calls and shell calls through Code Mode. It does not check a human's direct shell commands or commands executed inside MCP tools.

## Supported environment

Use Linux x86_64 or WSL with OpenCode V2 2.0.21, explicitly configured Bash, and no other hooks that change the command, working directory, or shell. Linux tests use GNU and musl guardian binaries. WSL has not been tested separately. V1 and later V2 releases are not verified; the plugin rejects an unverified host version.

This is accident prevention, not a security sandbox. `allow` is not proof of safety. Changes to files during approval and changes by other hooks are outside the input-consistency guarantee.

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

The plugin reads the existing service registration once and validates its PID, URL, and credentials before sending any HTTP request. It accepts only an authenticated loopback HTTP endpoint, then uses the OpenCode SDK to confirm that the server reports its own PID and version 2.0.21. The validated URL and credentials stay fixed, and automatic connections reject HTTP redirects. It does not start a service, write credentials, or set password environment variables. A standalone, remote, or explicitly started server still needs the connection options below. Explicit options take precedence; incomplete options never fall back to discovery. If its own service cannot be confirmed, execution does not proceed.

## Install with mise and a GitHub package

The plugin is available from guardian 0.1.2. Install the binary with mise, then register the plugin from the same release tag:

```sh
mise use -g github:ba0918/command-guardian@0.1.3
opencode plugin add 'github:ba0918/command-guardian#v0.1.3::path:plugins/opencode'
```

For another release, replace `0.1.3` in both commands and the configuration below with that release's version. OpenCode V2 2.0.21 supports GitHub package specifications with tags and repository-subdirectory selectors. The `plugin add` command installs the package and adds it to your global configuration. You do not need to place the plugin files yourself or install a separate npm package. See [OpenCode's plugin configuration guide](https://opencode.ai/v2/docs/plugins).

The isolated installation check uses the pinned host's public `plugin add` command with an immutable local Git commit and `::path:plugins/opencode`. It installs the source package and its dependencies. Native approval tests then load that installed package in the real host. Downloading this plugin from the published GitHub tag has not been tested. Releases before 0.1.2 do not contain the plugin.

For 0.1.2 or an explicit server, complete the connection settings below after adding the package. Update the guardian binary and the plugin's tag together.

## Alternative: install the matching bundle

Obtain `command-guardian-v0.1.3-x86_64-unknown-linux-musl.tar.gz` and its `.sha256` file from [the matching GitHub release](https://github.com/ba0918/command-guardian/releases/tag/v0.1.3). For another release, replace `0.1.3` in the filenames with that release's version. Verify the checksum before extracting:

```sh
sha256sum -c command-guardian-v0.1.3-x86_64-unknown-linux-musl.tar.gz.sha256
tar -xzf command-guardian-v0.1.3-x86_64-unknown-linux-musl.tar.gz
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
      "package": "github:ba0918/command-guardian#v0.1.3::path:plugins/opencode",
      "options": {
        "serverUrl": "http://127.0.0.1:4097",
        "passwordEnv": "OPENCODE_SERVER_PASSWORD"
      }
    }
  ]
}
```

For the archive installation instead, set `package` to the extracted plugin directory's stable absolute path, such as `/absolute/path/to/opencode`.

`serverUrl` must identify the server hosting this plugin. It does not discover your service or credentials. `passwordEnv` names an environment variable visible to that server, not a password stored in JSON. Authentication uses HTTP Basic with the host's `opencode` username. Do not point it at another server. Use loopback or protected HTTPS; do not transmit credentials over untrusted HTTP.

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
- Missing guardian, invalid responses, process timeout, and unsupported shells require approval. If the approval connection or authentication fails, execution does not proceed.

Approval waits have no plugin-specific deadline or saved history. Detected cancellation, unload, stream termination, or stream error permanently cancels pending execution. A later approval does not replay it. Silent stalls are not detected immediately. Orphaned approval-display cleanup is best effort, including creation/cancellation races. The plugin does not stop already launched background processes.

## Verification scope

Model-free tests load the production plugin in an isolated real 2.0.21 host, obtain its transformed shell through public `tool.list()`, and use the real guardian and native approvals. Code Mode tests use the pinned official interpreter with that host-derived executor. They do not invoke a model or the CLI's complete agent loop. Source inspection establishes how `run --auto` replies `once`; fixture replies exercise that native response.

The RPC executor's cancellation signal is tested. Actual active model-driven session interruption, WSL-specific execution, and model-driven end-to-end use are not claimed as observed by these tests.
