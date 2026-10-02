# OpenCode V2 integration

The plugin checks the agent's `shell` tool using the existing guardian binary. It covers normal and background calls and shell calls through Code Mode. It does not check a human's direct shell commands or commands executed inside MCP tools.

## Supported environment

Use Linux x86_64 or WSL with OpenCode V2 2.0.21, explicitly configured Bash, and no other hooks that change the command, working directory, or shell. Linux tests use GNU and musl guardian binaries. WSL has not been tested separately. V1 and later V2 releases are not verified; the plugin rejects an unverified host version.

This is accident prevention, not a security sandbox. `allow` is not proof of safety. Changes to files during approval and changes by other hooks are outside the input-consistency guarantee.

## Install the matching bundle

Install the binary through [the existing mise or source-build route](../README.md#install). Obtain the matching `command-guardian-v<VERSION>-x86_64-unknown-linux-musl.tar.gz` and `.sha256` file from the same GitHub release. Verify the checksum before extracting:

```sh
sha256sum -c command-guardian-v<VERSION>-x86_64-unknown-linux-musl.tar.gz.sha256
tar -xzf command-guardian-v<VERSION>-x86_64-unknown-linux-musl.tar.gz
```

Place the extracted `opencode/` directory at a stable, user-owned absolute path. Keep `server.js`, `package.json`, `source/`, and `licenses/` together. The manifest version is generated from Cargo.toml. Update the binary and plugin together. No separate npm publication or runtime dependency installation is required; the JavaScript bundle includes dependencies and license notices.

Before this feature is released, build the same archive locally:

```sh
bun install --frozen-lockfile --cwd plugins/opencode
cargo build --release --locked --target x86_64-unknown-linux-musl
scripts/package-release.sh target/x86_64-unknown-linux-musl/release/command-guardian x86_64-unknown-linux-musl target/package
```

## Register and connect manually

Edit the OpenCode V2 global configuration yourself, preserving existing settings. Its usual location is `$XDG_CONFIG_HOME/opencode/opencode.json`, or `~/.config/opencode/opencode.json` when XDG_CONFIG_HOME is unset. Add the plugin directory and explicitly select Bash:

```json
{
  "shell": "/bin/bash",
  "plugins": [
    {
      "package": "/absolute/path/to/opencode",
      "options": {
        "serverUrl": "http://127.0.0.1:4097",
        "passwordEnv": "OPENCODE_SERVER_PASSWORD"
      }
    }
  ]
}
```

`serverUrl` must identify the server hosting this plugin. It does not discover your service or credentials. `passwordEnv` names an environment variable visible to that server, not a password stored in JSON. Authentication uses HTTP Basic with the host's `opencode` username. Do not point it at another server. Use loopback or protected HTTPS; do not transmit credentials over untrusted HTTP.

Set a private password in the server process environment, then start the same server at the configured address:

```sh
read -rs OPENCODE_SERVER_PASSWORD
export OPENCODE_SERVER_PASSWORD
opencode2 serve --hostname 127.0.0.1 --port 4097
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
