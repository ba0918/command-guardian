# Project Context

## What this is

command-guardian is a pre-execution watcher that extracts the destructive effects of a Bash command and returns allow, ask, or block based on the target paths and the configuration.
The same executable provides the CLI `check` and the agent `hook`.
Only when the user enables it, deadline-bound LLM advice can make the mechanical verdict stricter.
Advice is off by default; see the [README](README.md#optional-llm-advice) for its configuration, external sending, and logs.
It is not a permission sandbox that blocks the commands being executed.

## Stack and layout

A Rust 2024 Cargo workspace.
The current runtime uses Unix sockets, and process isolation is tested on Linux.
Specifications live in `docs/ir/` and decisions in `docs/decision/records/`.

| Crate | Responsibility | Usual internal dependencies |
|---|---|---|
| guardian-core | Shared values and diagnostics | none |
| guardian-advisor | Advice types, classification, context limits, secret detection | core |
| guardian-advisor-typesafe | TypeSafe request encoding, authentication, HTTPS transport | advisor |
| guardian-parser | Hides brush-parser; normalized AST; pure parsing and quote removal | core |
| guardian-analysis | Context-holding semantic analysis; shared traversal for effects and Invocations | core, parser |
| guardian-judge | Path observation, classification, deadline-bound git execution | core |
| guardian-policy | Configuration interpretation and merging, rules, worst-case composition, messages | core, advisor |
| guardian-app | Configuration discovery and loading, worker and session, verdict assembly, advice deadlines and local state | core, parser, analysis, judge, policy, advisor, advisor-typesafe |
| command-guardian | CLI, hook mapping, environment acquisition, shadow log | app, core, policy |

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/kotowari-check.sh
cargo build --release --locked
```

To check only the external contract of the real binary:

```sh
cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation
```

Typical usage:

```sh
cargo run --locked -- check 'rm -rf /etc/x' --cwd /tmp --format json
```

## Conventions specific to this project

### OpenCode V2 verification

Use Bun 1.4.2 and the dependencies pinned in the package. The test executable is the official npm "@opencode/cli-linux-x64@2.0.21", fetched with the lockfile's integrity. The tests also check its version at the start. They do not discover personal services and do not call a model.

```sh
bun install --frozen-lockfile --cwd plugins/opencode
bun run --cwd plugins/opencode typecheck
bun run --cwd plugins/opencode test:unit
cargo build --locked
GUARDIAN_TEST_BIN="$PWD/target/debug/command-guardian" \
  OPENCODE_TEST_BIN="$PWD/plugins/opencode/node_modules/@opencode/cli-linux-x64/bin/opencode" \
  bun run --cwd plugins/opencode test:integration
```

Also run them against the GNU and musl release binaries by replacing GUARDIAN_TEST_BIN. CI builds the archive with the same script and also tests the plugin and binary extracted outside the checkout. The version in the distributed manifest is generated from Cargo.toml at assembly time.

### Reconciling changes with the IR

Requirements live in "docs/ir/". kotowari 0.3.0 or later checks their structure and references; it does not generate the IR or prove that the implementation matches it.

1. Within existing requirements, implement against them. When filling a specification gap, record the evidence, the decision maker, the decision record, and the necessary IR. Do not adopt the implementation as the correct specification as-is.
2. A reviewer in a context separate from the implementation compares the implementation with the requirements and checks the evidence, meaning, and approved scope.
3. Integrate only when "kotowari check --format json" and the existing product checks succeed. Do not integrate with an unresolved specification judgment left as deferred.

When a change in the meaning of code, IR, or decisions follows the review, review the affected parts again.

### CI and pre-push checks

CI and the lefthook pre-push hook run fmt, clippy, all tests, and the release build for both gnu and musl, plus kotowari check.
Local prerequisites are rustfmt, clippy, both Rust targets, musl-tools, and kotowari 0.3.0 or later. Enable the hooks with "lefthook install" in each clone.
Run "git fetch origin main" before pushing. The hook resolves a work branch's merge-base with origin/main, and main's remote old SHA; an unknown base, uncommitted changes, or pushing anything other than HEAD stops the push.
CI and hooks are development operations settings and add no requirements to the product IR.

### Releases

The canonical version is "[package].version" in "Cargo.toml".
When CHANGELOG is promoted to "## [VERSION] - YYYY-MM-DD" with the matching comparison link and integrated into main, the unpublished vVERSION is published automatically after all checks pass. Nothing is published while the section is Unreleased.
GitHub Releases carry a tar.gz of the Linux x86_64 musl binary and LICENSE, with its SHA256. The tag is placed on the checked commit, and a published tag is never moved or reused.

### Landing page

The landing page in "site/" is published to GitHub Pages by ".github/workflows/pages.yml" on every push to main.
The version comes from Cargo.toml, the release notes from CHANGELOG.md, and the tables and introduction from headings in README.md and README-ja.md; the verdict examples show the output of the real binary built on the spot.
Changing one of those README headings makes the page build fail, so update the headings in "site/build.py" as well.
When an example's result differs from its expectation, the build only warns and continues, and the page shows the actual output.
Locally, run "cargo build --release --locked" and then "python3 site/build.py" to generate "_site/".
The page is not part of the product's external contract and adds no requirements to the product IR.

### Product implementation and tests

- Production analysis of input-derived data goes through the app session. At the top of main, dispatch to a child of the same binary. Do not add a fallback that parses directly in the parent.
- Use shallow inputs in pure parser and analysis tests. Put child death and deep inputs in the root crate's real-binary tests. Do not re-launch libtest's main as a worker.
- Put git fixtures in `CARGO_TARGET_TMPDIR`. Moving target under `/tmp` changes their classification to temporary space, so do not move it when verifying.
- CLI and hook tests use the fixture's HOME and XDG directories and never write to the real user's configuration or shadow log.

## Constraints

Keep the external contracts: a single binary, the CLI exit codes, the three configuration layers and their trust conditions, JSON, hooks, and the shadow log.
The limits are 1 MiB of syntax, 128 levels of depth, 1000 parses per verdict, and 5 seconds per verdict.
The advice deadline is separate from the mechanical verdict's and, at the default of 2 seconds, covers acquisition, startup, checking, and transport.
The model is called at most once, with no retries.
Real-model accuracy and compatibility with the current TypeSafe service are unverified.
Validating configuration examples applies the per-request isolation limits, without a cumulative acceptance limit of 1000 parses / 5 seconds over the whole configuration.
Waiting on git and collecting its output are cut off by the verdict's remaining time, but there is no guarantee that every fs syscall is forcibly interrupted within 5 seconds.
The response target for representative shallow inputs is under 100 ms, measured separately from pathological deep inputs.

## Glossary

- **JudgmentSession**: A value that borrows the runtime only for the duration of one verdict and tracks the number of parses and elapsed time.
- **ValidationSession**: A value that analyzes examples in isolation only while configuration is loading. It has no cumulative verdict budget.
- **CommandFacts**: The effects, apparent invocation information, and diagnostics obtained from the same shared traversal.
- **ObservedPath**: A path resolved once. It is shared by configuration roots and default classification, and kept separate from the original Target used for display.
