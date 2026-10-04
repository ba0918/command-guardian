# askの委任

利用者が明示的に有効化したときだけ、フックで最終判定が `ask` のものを返さず、エージェント本来の権限判断に委ねる任意設定を扱う。推奨しない機能であり、既定では無効である。判定そのものは judgment.md、フックの通常の写像は agents.md、OpenCode の承認は opencode.md が扱う。

## Requirements

### REQ-059: 委任の有効化

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A3, docs/decision/records/2026-10-04-auto-allow-ask.md#A15
- verification: unit

`ask委任` は、利用者設定の "mode.defer_ask" が true のときだけ有効になる。既定は false とする。

プロジェクト設定の "mode.defer_ask" は、"trusted_projects" にあるリポジトリでも反映せず、警告を出す。この値はTOML構文解析後、一般設定の型の検証前に取り除き、型や値が不正でも同じファイルの他の設定を不採用にしない。

利用者設定の "mode.defer_ask" の型や値が不正なときは、REQ-015 に従いそのファイル全体を不採用にして警告し、`ask委任` は無効のままにする。

### REQ-060: フックでの委任

- kind: state_driven
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A1, docs/decision/records/2026-10-04-auto-allow-ask.md#A2, docs/decision/records/2026-10-04-auto-allow-ask.md#A4, docs/decision/records/2026-10-04-auto-allow-ask.md#A8, docs/decision/records/2026-10-04-auto-allow-ask.md#A9, docs/decision/records/2026-10-04-auto-allow-ask.md#A14, docs/decision/records/2026-10-04-auto-allow-ask.md#A16, docs/decision/records/2026-10-04-auto-allow-ask.md#A7
- verification: unit

`ask委任` が有効で "mode.enforce" が true のとき、command-guardian は、フックとして呼ばれ最終判定が `ask` になった入力について、確認を求める出力を返さない。Claude Code と Codex のフックでは標準出力を空にし、終了コードを0にする。エージェントへ補足の文脈や理由も渡さない。

委任の対象は、最終判定が `ask` のものすべてである。パスの分類、読めない構文、判定の内部エラー・gitの失敗・解析の失敗（REQ-010）、カスタムのルール、guard 規則、助言で `ask` になったものを区別しない。`block` は委任せず、従来どおり拒否を返す。助言で `block` になったものも拒否する。

判定に至らなかったフックの失敗（フックの入力をJSONとして読めない場合を含む）は委任に含めず、REQ-022 と REQ-023 の従来の扱いに従う。

"command-guardian check" には `ask委任` を適用せず、本来の判定と終了コードを返す。

### REQ-061: OpenCode での委任

- kind: state_driven
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A5, docs/decision/records/2026-10-04-auto-allow-ask.md#A12, docs/decision/records/2026-10-04-auto-allow-ask.md#A14, docs/decision/records/2026-10-04-auto-allow-ask.md#A17, docs/decision/records/2026-10-04-auto-allow-ask.md#A7
- verification: unit

`ask委任` が有効で "mode.enforce" が true のとき、Rust 本体の "hook --agent opencode" は、最終判定が `ask` の入力について、`allow` と区別できる委任の応答を返す。プラグインは委任の応答を受けたとき、guardian 由来の承認要求を出さず、OpenCode 自身の権限判断だけで実行の可否を決める。プラグインは設定を読み直さない。

guardian の `block` は従来どおり拒否する。判定結果を取得できない場合（REQ-051）は guardian の `ask` ではないため委任せず、REQ-051 に従う。委任の記録の保存に失敗しても委任の応答は変えず、保存失敗を警告として応答に含める。プラグインはその警告を示すだけで承認要求を出さず、無効な応答として扱わない。
委任の応答の JSON の具体的なキーと値は、実装担当が契約として記録する。

### REQ-062: 委任の記録

- kind: event_driven
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A6, docs/decision/records/2026-10-04-auto-allow-ask.md#A10, docs/decision/records/2026-10-04-auto-allow-ask.md#A11, docs/decision/records/2026-10-04-auto-allow-ask.md#A16
- verification: unit

フックで `ask委任` を行ったとき（判定に至らなかったフックの失敗を除く）、command-guardian は、影実行のログと同じ保存先に1行を記録する。その行には、時刻、判定、理由、対象パス、コマンド本文と、影実行の行と見分ける印を含める。印の具体的な書き方は README に契約として記載する。

この記録は、エージェントの種類と "permission_mode" に関係なく行う。Claude Code の "dontAsk"・"bypassPermissions" と Codex のように、`ask委任` が無くても何も返さない場合を含む。`ask委任` が無効なときは、この記録を行わない。

保存先の拒否と保存失敗は REQ-019 の条件に従い、その記録を省略して標準エラーへ警告し、フックの出力と終了コードは変えない。

### REQ-063: 影実行の優先

- kind: state_driven
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A7
- verification: unit

"mode.enforce" が false のとき、command-guardian は、"mode.defer_ask" に関係なく影実行（REQ-018、REQ-048）として振る舞い、`ask委任` の応答と記録を行わない。

### REQ-064: 推奨しない旨の説明

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-auto-allow-ask.md#A13, docs/decision/records/2026-10-04-auto-allow-ask.md#A2, docs/decision/records/2026-10-04-auto-allow-ask.md#A10
- verification: review
- how_to_verify: README.md と README-ja.md の "mode.defer_ask" の説明を人が読み、推奨しないこと、読めない構文と判定できない場合も委任されること、委任の記録の印の書き方が書かれているかを確認する

command-guardian の README.md と README-ja.md は、"mode.defer_ask" を推奨しない任意設定として説明し、読めない構文と判定できない場合も委任されるという危険を記載する。判定ごとの警告は出さない。

## Examples

```gherkin
@id=EX-114 @about=REQ-059 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A3,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: 既定では委任しない
  Given 利用者設定に "mode.defer_ask" が無い
  When Claude Code のフックとして ask になる入力を判定する
  Then 出力の "permissionDecision" は "ask" である

@id=EX-115 @about=REQ-059 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A3,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: 信頼済みプロジェクトでも委任を有効にできない
  Given 利用者設定の "trusted_projects" にリポジトリのパスがある
  And そのリポジトリの ".command-guardian.toml" で "mode.defer_ask" が true である
  When Claude Code のフックとして ask になる入力を判定する
  Then 出力の "permissionDecision" は "ask" で、警告が出る

@id=EX-116 @about=REQ-059 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A15
Scenario: プロジェクトの不正な値で保護規則を失わない
  Given プロジェクト設定の "mode.defer_ask" が文字列 "yes" である
  And 同じファイルに正常な保護ルートの指定がある
  When その保護ルートの中の削除を判定する
  Then 保護ルートは反映され、"mode.defer_ask" は無視されて警告が出る

@id=EX-117 @about=REQ-059 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: 利用者設定の不正な値では委任しない
  Given 利用者設定の "mode.defer_ask" が文字列 "yes" である
  When Claude Code のフックとして ask になる入力を判定する
  Then そのファイル全体が不採用になり警告が出る
  And 出力の "permissionDecision" は "ask" である

@id=EX-118 @about=REQ-060 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A1,docs/decision/records/2026-10-04-auto-allow-ask.md#A9,docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15
Scenario: Claude Code では ask を何も返さずに委ねる
  Given 利用者設定の "mode.defer_ask" が true である
  When Claude Code のフックとして未追跡の "notes.txt" を消す "rm notes.txt" を判定する
  Then 標準出力は空で、終了コードは0である

@id=EX-119 @about=REQ-060 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A2,docs/decision/records/2026-10-04-auto-allow-ask.md#A14,docs/decision/records/2026-10-04-auto-allow-ask.md#A1
Scenario: 判定できない ask も委ねる
  Given 利用者設定の "mode.defer_ask" が true である
  When Claude Code のフックとして読めない構文のコマンドを判定する
  Then 標準出力は空で、終了コードは0である

@id=EX-120 @about=REQ-060 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A2,docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24
Scenario: block は委任しない
  Given 利用者設定の "mode.defer_ask" が true である
  When Claude Code のフックとして "rm -rf /etc/nginx" を判定する
  Then 出力の "permissionDecision" は "deny" で、理由が付く

@id=EX-121 @about=REQ-060 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A4,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: check には委任を適用しない
  Given 利用者設定の "mode.defer_ask" が true である
  When "command-guardian check" で ask になるコマンドを判定する
  Then 判定は ask で、終了コードは 1 になる

@id=EX-122 @about=REQ-060 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A24,docs/decision/records/2026-10-03-llm-advice-layer.md#A18
Scenario: 助言の block は委任時も拒否する
  Given 利用者設定の "mode.defer_ask" が true で、助言が enforce である
  And 機械判定は ask で、助言の候補判定は block である
  When Claude Code のフックとして判定する
  Then 出力の "permissionDecision" は "deny" である

@id=EX-123 @about=REQ-061 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A5,docs/decision/records/2026-10-04-auto-allow-ask.md#A12
Scenario: OpenCode では guardian の承認要求を出さない
  Given 利用者設定の "mode.defer_ask" が true で、"mode.enforce" が true である
  And OpenCode が実行を許可している
  When guardian が ask と判定する
  Then 本体の応答は allow と区別できる委任を示す
  And プラグインは guardian 由来の承認要求を出さずに実行する

@id=EX-124 @about=REQ-061 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A5
Scenario: 委任しても OpenCode 自身の確認は残る
  Given 利用者設定の "mode.defer_ask" が true である
  And OpenCode が確認を求める
  When guardian が ask と判定する
  Then OpenCode の確認を維持し、承認前に実行しない

@id=EX-125 @about=REQ-061 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A14,docs/decision/records/2026-10-02-opencode-v2-hook.md#A6
Scenario: 判定結果を取得できないときは委任しない
  Given 利用者設定の "mode.defer_ask" が true である
  And guardian のバイナリが見つからない
  When エージェントが shell 実行を要求する
  Then 判定できなかった理由を示して承認を求める

@id=EX-131 @about=REQ-061 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A17,docs/decision/records/2026-10-02-ir-friction-contracts.md#A2,docs/decision/records/2026-10-04-auto-allow-ask.md#A6,docs/decision/records/2026-10-04-auto-allow-ask.md#A7
Scenario: OpenCode で記録に失敗しても委任は変わらない
  Given 利用者設定の "mode.defer_ask" が true で、"mode.enforce" が true である
  And ログの保存先が symlink である
  When guardian が ask と判定する
  Then 本体の応答は委任を示し保存失敗の警告を含む
  And プラグインは警告を示し、guardian 由来の承認要求を出さない

@id=EX-132 @about=REQ-060,REQ-062 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A16
Scenario: 判定に至らないフックの失敗は委任しない
  Given 利用者設定の "mode.defer_ask" が true である
  When Codex のフックへ JSON として読めない入力を与える
  Then REQ-023 の従来の扱いに従い、委任の印を含む行は記録されない

@id=EX-126 @about=REQ-062 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A6,docs/decision/records/2026-10-04-auto-allow-ask.md#A10
Scenario: 委任を印付きで記録する
  Given 利用者設定の "mode.defer_ask" が true で、"mode.enforce" が true である
  When Claude Code のフックとして ask になる入力を判定する
  Then 影実行のログと同じ保存先に、時刻、判定、理由、対象パス、コマンド本文と委任の印を含む1行が書かれる

@id=EX-127 @about=REQ-062 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A11
Scenario: もともと何も返さない場合も記録する
  Given 利用者設定の "mode.defer_ask" が true である
  When Codex のフック、または "permission_mode" が "bypassPermissions" の Claude Code のフックとして ask になる入力を判定する
  Then 標準出力は空で、委任の印を含む1行が記録される

@id=EX-128 @about=REQ-062 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A11
Scenario: 委任が無効なら記録しない
  Given "mode.defer_ask" が false で、"mode.enforce" が true である
  When Codex のフックとして ask になる入力を判定する
  Then ログに行は追加されない

@id=EX-129 @about=REQ-062 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A6,docs/decision/records/2026-10-02-ir-friction-contracts.md#A2,docs/decision/records/2026-10-04-auto-allow-ask.md#A1
Scenario: 記録に失敗しても委任は変わらない
  Given 利用者設定の "mode.defer_ask" が true である
  And ログの保存先が symlink である
  When Claude Code のフックとして ask になる入力を判定する
  Then 標準エラーへ警告し、標準出力は空で、終了コードは0である

@id=EX-130 @about=REQ-063 @source=docs/decision/records/2026-10-04-auto-allow-ask.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: 影実行を委任より優先する
  Given 利用者設定の "mode.defer_ask" が true で、"mode.enforce" が false である
  When Claude Code のフックとして ask になる入力を判定する
  Then 影実行として判定と理由をログに書き、委任の印を含む行は書かない
```
