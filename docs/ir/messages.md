# 判定の文面

allow 以外の判定を返すときの文面と、製品が生成する説明の言語、翻訳しない入力の境界を扱う。判定そのものは judgment.md、ヘルプは cli.md が扱う。

## Requirements

### REQ-011: 非 allow の文面

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A15, docs/decision/records/2026-09-30-hook-guardian-scope.md#A26, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28, docs/decision/records/2026-09-30-hook-guardian-scope.md#A30, docs/decision/records/2026-10-01-parser.md#A7, docs/decision/records/2026-10-01-parser.md#A12, docs/decision/records/2026-10-01-parser.md#A16, docs/decision/records/2026-10-01-parser.md#A22, docs/decision/records/2026-10-01-parser.md#A23
- verification: unit

command-guardian は、`allow` 以外の判定を返すとき、何の操作が、どのパスに対して行われるのか、なぜ止めるのか、代わりに何ができるのかを示す。理由には、分類と、失われるもの（未追跡で戻せない、未コミットの変更が失われる、パスを解決できない、管理外で戻せない、保護領域である）、操作の無い ask の理由（構文を読めない、読めないシェル、判定の内部で失敗した）、上限を超えた block の理由（大きすぎる、深すぎる、判定の上限を超えた）を含める。操作とパスが無い ask と、上限を超えた block では、それらに代えてその理由を示す。代替には、先にコミットする、リテラルのパスで指定し直す、一時領域や作業場所へ移してから消す、許可ルートに追加する、のうち当てはまるものを示し、当てはまるものが無いときは、無いと書く。文面は 2 行から 4 行にする。

### REQ-012: 宛先に応じた書き分け

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
- verification: review
- how_to_verify: 代表的な 5 つの文面（拒否 2 件、ask 3 件）を読み、エージェントが次の一手を打てるか、利用者がその場で判断できるかを確かめる

command-guardian は、拒否としてエージェントに返す文面は、エージェントが次の一手を打てるように書く。`ask` として利用者に返す文面は、利用者がその場で判断できるように書く。

### REQ-042: 製品が生成する説明の言語

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-cli-language-help.md#A1, docs/decision/records/2026-10-02-cli-language-help.md#A2, docs/decision/records/2026-10-02-cli-language-help.md#A5, docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28, docs/decision/records/2026-09-30-hook-guardian-scope.md#A30
- verification: review
- how_to_verify: check の通常出力と JSON の説明、引数エラー、設定の警告、Claude Code と Codex の拒否理由、影実行のログに記録する理由を読み、製品が生成する説明が自然な英語であることを確かめる。非 allow の文面は REQ-011 の内容と 2 行から 4 行の契約、REQ-012 の宛先別の書き方も満たすことを確かめる。利用者や外部由来の文字列は REQ-043 の境界で区別する

command-guardian が生成する、人が読む説明は英語にする。通常出力、エラー、警告、JSON に含む説明、hook の拒否理由、影実行のログに記録する理由にも同じ言語を用いる。非 allow の文面に必要な内容と行数は REQ-011、宛先に応じた書き方は REQ-012 のままとする。

### REQ-043: 翻訳しない情報と既存契約の保持

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-cli-language-help.md#A2, docs/decision/records/2026-10-02-cli-language-help.md#A5, docs/decision/records/2026-10-02-cli-language-help.md#A7, docs/decision/records/2026-10-02-cli-language-help.md#A8, docs/decision/records/2026-10-02-cli-language-help.md#A10, docs/decision/records/2026-09-30-hook-guardian-scope.md#A30
- verification: unit

英語にするのは製品が付ける説明だけとし、利用者指定の "reason"、パス、規則名、コマンド本文、OS やライブラリ由来のエラー詳細は翻訳せず、元のデータを保持する。これらを REQ-011 の 2 行から 4 行の製品説明へ埋め込むときだけ、元の文字列に含まれる改行を "\n" のようなエスケープ表記で表示する。元のデータを削除、切り詰め、書き換えたり、入力を単一行に制限したりしない。JSON の元データの値と、表示用に組み立てた説明の値を区別する。カスタム規則の名前と見張り規則の利用者指定 "reason" の保持は、既存の二種類の規則を使って別々に確認し、新しい設定項目は追加しない。既存の JSON のキーと終了コードは変更しない。出力全体を英語の文字だけに制限する要求ではない。

## Examples

```gherkin
@id=EX-012 @about=REQ-011 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A26,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
Scenario: 保護領域の拒否に理由と代替が出る
  When "rm -rf /etc/nginx" を判定する
  Then 文面に、操作、パス、分類の protected、失われるもの、代替が含まれる

@id=EX-013 @about=REQ-012 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
Scenario: 未追跡ファイルの確認は利用者向けに書く
  Given git の作業ツリーに未追跡のファイルがある
  When "rm notes.txt" を判定する
  Then ask の文面は、利用者が消すかどうかをその場で決められる形になっている

@id=EX-058 @about=REQ-042 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28,docs/decision/records/2026-09-30-hook-guardian-scope.md#A30
Scenario: 通常出力と JSON の製品説明は英語になる
  When "command-guardian check 'rm -rf /etc/nginx' --cwd /tmp" を実行する
  And 同じ判定を "--format json" で実行する
  Then 人が読む出力と JSON の製品生成の説明は英語である
  And 非 allow の文面は必要な理由と代替を含む 2 行から 4 行である

@id=EX-059 @about=REQ-042 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: 判定できない引数エラーも英語になる
  When "command-guardian check" をコマンド文字列なしで実行する
  Then 製品が付けるエラーの説明は英語である
  And 終了コードは 3 である

@id=EX-060 @about=REQ-042 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-09-30-hook-guardian-scope.md#A5
Scenario: 壊れた設定の警告も英語になる
  Given 利用者設定が TOML として壊れている
  When "command-guardian check 'true' --cwd /tmp" を実行する
  Then 製品が付ける警告の説明は英語である
  And 組み込みの既定で判定を続ける

@id=EX-061 @about=REQ-042 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-09-30-hook-guardian-scope.md#A35
Scenario: フックの拒否理由も英語になる
  Given "mode.enforce" が true である
  When Claude Code と Codex のフックとして "rm -rf /etc/nginx" の Bash 入力をそれぞれ判定する
  Then 両方の "permissionDecisionReason" の製品生成の説明は英語である

@id=EX-062 @about=REQ-042 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-10-02-cli-language-help.md#A5,docs/decision/records/2026-10-02-cli-language-help.md#A9,docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: 影実行の理由にも同じ言語を用いる
  Given "mode.enforce" が false である
  When フックとして "rm -rf /etc/nginx" の Bash 入力を判定する
  Then フックの判定は返さない
  And 影実行のログに記録する製品生成の理由は英語である

@id=EX-063 @about=REQ-043 @source=docs/decision/records/2026-10-02-cli-language-help.md#A5,docs/decision/records/2026-10-02-cli-language-help.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A32
Scenario: カスタム規則の日本語の名前は翻訳しない
  Given 利用者設定の "rules.custom" に "name" が "削除確認"、"pattern" が "rm"、"verdict" が "ask" のカスタム規則がある
  When "command-guardian check 'rm 資料.txt' --cwd /tmp --format json" を実行する
  Then 出力に含むカスタム規則の名前は "削除確認" のままである
  And 出力に含むパスの "資料.txt" も翻訳しない
  And JSON のキーと判定に対応する終了コードは変更しない

@id=EX-064 @about=REQ-043 @source=docs/decision/records/2026-10-02-cli-language-help.md#A5
Scenario: 外部エラー詳細は製品の説明と区別する
  Given 設定読込でライブラリ由来のエラー詳細が発生する
  When 警告にその詳細を含める
  Then 製品が付ける説明だけが英語になる
  And ライブラリ由来のエラー詳細は翻訳しない

@id=EX-065 @about=REQ-043 @source=docs/decision/records/2026-10-02-cli-language-help.md#A5,docs/decision/records/2026-10-02-cli-language-help.md#A9,docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: 影実行に記録するコマンド本文は保持する
  Given "mode.enforce" が false である
  When フックとして "rm '資料.txt'" の Bash 入力を判定する
  Then 影実行のログのコマンド本文は "rm '資料.txt'" のままである

@id=EX-072 @about=REQ-043 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-10-02-cli-language-help.md#A5,docs/decision/records/2026-10-02-cli-language-help.md#A7,docs/decision/records/2026-10-02-cli-language-help.md#A10,docs/decision/records/2026-09-30-hook-guardian-scope.md#A42,docs/decision/records/2026-09-30-hook-guardian-scope.md#A43
Scenario: 見張り規則の日本語の理由は翻訳しない
  Given 利用者設定の "commands.guard" に "program" が "git"、"deny" が [["push"]]、"verdict" が "ask"、"reason" が "先にバックアップを確認" の規則がある
  When "command-guardian check 'git push origin main' --cwd /tmp --format json" を実行する
  Then JSON を復号した "rules" の一致した見張り規則の "reason" は "先にバックアップを確認" のままである
  And 製品が付ける説明だけが英語になる

@id=EX-073 @about=REQ-043 @source=docs/decision/records/2026-10-02-cli-language-help.md#A2,docs/decision/records/2026-10-02-cli-language-help.md#A5,docs/decision/records/2026-10-02-cli-language-help.md#A8,docs/decision/records/2026-10-02-cli-language-help.md#A10,docs/decision/records/2026-09-30-hook-guardian-scope.md#A30,docs/decision/records/2026-09-30-hook-guardian-scope.md#A42,docs/decision/records/2026-09-30-hook-guardian-scope.md#A43
Scenario: 改行を含む理由の元データと表示用の説明を区別する
  Given 利用者設定の "commands.guard" に "program" が "git"、"deny" が [["push"]]、"verdict" が "ask" の規則がある
  And その "reason" は "先に確認" と "承認後に実行" の間に実際の改行を含む文字列である
  When "command-guardian check 'git push origin main' --cwd /tmp --format json" を実行する
  Then JSON を復号した "rules" の一致した見張り規則の "reason" は実際の改行を含む元の文字列と等しい
  And JSON を復号した "message" に埋め込む利用者の理由では、その改行だけを "\n" のようなエスケープ表記で表示する
  And その "message" は 2 行から 4 行であり、利用者の理由の情報を削除も切り詰めもしない
```
