# エージェントへの写像

Claude Code と Codex のフックとして呼ばれたときの入出力の契約を扱う。判定は judgment.md、文面は messages.md が扱う。

## Requirements

### REQ-022: Claude Code の写像

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A24, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35, docs/decision/records/2026-10-04-auto-allow-ask.md#A1, docs/decision/records/2026-10-04-auto-allow-ask.md#A3, docs/decision/records/2026-10-04-auto-allow-ask.md#A7
- verification: unit

Claude Code のフックとして呼ばれたとき、command-guardian は、"hookSpecificOutput" の封筒に "hookEventName" を "PreToolUse" として置き、`allow` のときは何も返さず、`ask` のときは "permissionDecision" に "ask" と "permissionDecisionReason" を返し、`block` のときは "deny" と理由を返す。"permission_mode" が "dontAsk" か "bypassPermissions" のときは、`ask` を何も返さないに落とす。

"mode.defer_ask" が有効で "mode.enforce" が true のときの `ask` は、defer-ask.md の REQ-060 に従う。

### REQ-023: Codex の写像

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A24, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35
- verification: unit

Codex のフックとして呼ばれたとき、command-guardian は、"hookSpecificOutput" の封筒に "hookEventName" を "PreToolUse" として置き、`block` のときだけ "permissionDecision" に "deny" と "permissionDecisionReason" を返し、`allow`、`ask`、内部エラーのときは何も返さない。Codex には `ask` の出口がないため、確認は Codex 本来の承認フローに委ねる。

### REQ-024: 判定しない入力

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A40
- verification: unit

command-guardian は、フックの入力のうち、Bash のコマンドを含むものだけを判定する。コマンドを含まない入力では、何も返さずに終わる。

## Examples

```gherkin
@id=EX-020 @about=REQ-022 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: Claude Code の確認は permissionDecision ask で返す
  Given 未追跡のファイルがある
  When Claude Code のフックとして "rm notes.txt" の入力を判定する
  Then 出力の "permissionDecision" は "ask" で、理由が付く

@id=EX-021 @about=REQ-023 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: Codex の ask は何も返さない
  Given 未追跡のファイルがある
  When Codex のフックとして "rm notes.txt" の入力を判定する
  Then 何も返さず、Codex 本来の承認フローに委ねる

@id=EX-022 @about=REQ-024 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A40
Scenario: コマンドを含まない入力
  When "tool_name" が "Write" の入力を判定する
  Then 何も返さずに終わる
```
