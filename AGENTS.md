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
| writing or revising IR or decision records, acting on `kotowari check`, placing `@kotowari` marks in tests, or reading `kotowari mutants` | kotowari |
| deciding where a new request starts (use this, not ba0918-using-workflow) | kotowari-using-workflow |

Refer to each rule by its skill name. Read every rule that applies before starting the work it
governs. A rule once read stays in force for the rest of the context: read it again only after
the context has been compacted or cleared, or when the rule itself has changed. On a delegated
task, a rule the delegation prompt names as already inlined is in force from that prompt — do
not read it again; read every other rule this table routes to the work as usual.

The two kotowari rows are hand-maintained in this repository, not generated from skill
metadata. Keep them when regenerating the table.

## Project Context

Project-specific context — what this repository is, how to build and test it, and the
conventions that apply only here — lives in `PROJECT.md`. Read it before making changes.

## kotowari

This project manages its specification as an IR (`docs/ir/`).

Before integrating a branch, follow the change-conformance procedure in `PROJECT.md`.
The implementer and an independent reviewer each author their own records for the same
branch-wide comparison base and candidate bytes. Read kotowari's `changes` reference before
writing or reconciling them. Do not add `kotowari changes` to pre-commit hooks.

Specification gaps may be recorded as concrete IR additions only within the approved scope,
with repository evidence and a decision record. Do not change or delete approved requirements,
contradict an existing decision, or choose unsupported consequential meaning without asking.
Passing `kotowari changes` does not prove that the recorded judgment is correct.
