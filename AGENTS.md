# Agent Instructions

## Core

- Serve the stated goal; do not widen the requested scope.
- Distinguish what is confirmed from what is inferred and what is unverified.
- After changing something, verify it by a means appropriate to the change.
- Do not perform irreversible, destructive, or externally visible actions without approval.
- Apply the project's own instructions where they are more specific than these.

## Rule Routing

| When | Read |
|---|---|
| Always | ba0918-design, ba0918-placement, ba0918-readability, ba0918-secrets |
| commit | ba0918-commit |
| delegate | ba0918-delegation |
| design | ba0918-reuse |
| diff-review | ba0918-diff-review |
| gui | ba0918-gui-structure |
| implement | ba0918-tdd |
| release | ba0918-release |
| review | ba0918-verification |

Refer to each rule by its skill name. Read every rule that applies before starting the work it
governs. A rule once read stays in force for the rest of the context: read it again only after
the context has been compacted or cleared, or when the rule itself has changed. On a delegated
task, a rule the delegation prompt names as already inlined is in force from that prompt — do
not read it again; read every other rule this table routes to the work as usual.

## Project Context

Project-specific context — what this repository is, how to build and test it, and the
conventions that apply only here — lives in `PROJECT.md`. Read it before making changes.

## kotowari

This project manages its specification as an IR (`docs/ir/`).
