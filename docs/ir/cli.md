# コマンドの契約

実行ファイル、"check" と "hook" の契約、影実行、ログ、git の起動条件、応答時間を扱う。エージェントのプロトコルへの写像は agents.md が扱う。

## Requirements

### REQ-016: 実行ファイルとコマンド

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A19, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35
- verification: unit

M1 の成果物は、"command-guardian" という 1 つの実行ファイルであり、"hook" と "check" の 2 つのコマンドを持つ。"hook" は "--agent claude" か "--agent codex" を引数に取る。

### REQ-017: check の契約

- kind: event_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A19, docs/decision/records/2026-09-30-hook-guardian-scope.md#A27, docs/decision/records/2026-09-30-hook-guardian-scope.md#A34, docs/decision/records/2026-10-01-parser.md#A13
- verification: unit

"command-guardian check" が呼ばれたとき、command-guardian は、コマンド文字列と作業ディレクトリを受け取り、判定を標準出力に出す。既定は人が読む形式で、"--format json" のときは、"verdict" と "reason"（判定の理由）、効果ごとの "op"、"path"、"class"、"verdict"、"reason" を持つ JSON を出す。効果が無いときは、効果の一覧は空になる。終了コードは、"allow" が 0、"ask" が 1、"block" が 2、判定を出せない失敗が 3 にする。影実行のときも判定を出す。

### REQ-018: 影実行

- kind: state_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
- verification: unit

"mode.enforce" が false のとき、command-guardian は、フックとして判定を返さず、判定と理由をログに書く。M1 では、影実行のときだけログに書く。

### REQ-019: 影実行のログ

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
- verification: unit

影実行のログは、"XDG_STATE_HOME" が指すディレクトリの "command-guardian" の下（"XDG_STATE_HOME" が無いときは "~/.local/state/command-guardian"）に、所有者だけが読める権限で書く。1 行に、時刻、判定、理由、対象パス、コマンド本文を含める。

### REQ-020: git の起動条件

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
- verification: unit

command-guardian は、git のコマンドを、対象のパスが git の作業ツリーの中にあるときだけ起動する。

### REQ-021: 応答時間

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
- verification: review
- how_to_verify: 代表的な入力 100 件で判定の時間を計測し、git の起動を含む場合でも 100ms 未満であることを確かめる

判定の応答は、代表的な入力で 100ms 未満にする。

## Examples

```gherkin
@id=EX-017 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: check の終了コード
  When "command-guardian check 'rm -rf /tmp/scratch/x' --cwd /tmp/scratch" を実行する
  Then 判定は allow で、終了コードは 0 になる

@id=EX-018 @about=REQ-018 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: 影実行は止めずに記録する
  Given "mode.enforce" が false である
  When 保護領域の削除を判定する
  Then 何も返さず、判定と理由がログに書かれる

@id=EX-019 @about=REQ-020 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
Scenario: 作業ツリーの外では git を起動しない
  Given 対象のパスが git の作業ツリーの外にある
  When 判定する
  Then git のコマンドは起動されない

@id=EX-025 @about=REQ-016 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A19,docs/decision/records/2026-09-30-hook-guardian-scope.md#A40
Scenario: 2 つのコマンドを受け付ける
  Given M1 の実行ファイルがある
  When "command-guardian check 'true' --cwd /tmp" を実行する
  And "command-guardian hook --agent claude" に "tool_name" が "Write" の入力を与える
  Then どちらもコマンドとして受け付けられる

@id=EX-026 @about=REQ-019 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: ログの場所と権限
  Given 影実行で判定が記録される
  When ログのファイルを調べる
  Then 所有者だけが読める権限で、時刻、判定、理由、対象パス、コマンド本文が 1 行に入っている

@id=EX-027 @about=REQ-021 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
Scenario: 応答時間の目標
  Given 代表的な入力 100 件がある
  When 判定の時間を計測する
  Then 100ms 未満である

@id=EX-031 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: ask の終了コード
  Given git の作業ツリーに未追跡のファイルがある
  When "command-guardian check 'rm notes.txt' --cwd /home/you/work/repo" を実行する
  Then 判定は ask で、終了コードは 1 になる

@id=EX-032 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: block の終了コード
  When "command-guardian check 'rm -rf /etc/nginx' --cwd /tmp/scratch" を実行する
  Then 判定は block で、終了コードは 2 になる

@id=EX-033 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: 失敗の終了コード
  When "command-guardian check" をコマンド文字列なしで実行する
  Then 終了コードは 3 になる

@id=EX-034 @about=REQ-018 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A22,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: 影実行でも check は判定を出す
  Given "mode.enforce" が false である
  When "command-guardian check 'rm -rf /etc/nginx' --cwd /tmp/scratch" を実行する
  Then 判定は block として表示される
```
