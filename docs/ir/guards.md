# コマンドの見張り

設定の "[[commands.guard]]" の規則で、プログラムごとの使い方（"git push" など）を止める見張りを扱う。判定の合成は judgment.md、設定の層は config.md が扱う。この見張りは事故を減らす柵であり、境界ではない。悪意ある迂回を強制力で防ぐ仕組み（実行の隔離やアクセス制御）は持たず、判定はコマンド本文と設定だけで行う。

## Requirements

### REQ-027: 規則の形

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A42, docs/decision/records/2026-09-30-hook-guardian-scope.md#A44, docs/decision/records/2026-09-30-hook-guardian-scope.md#A47
- verification: unit

見張りの規則は、設定の "[[commands.guard]]" に書く。各規則は、"program"（語の basename で照合する名前）と "reason"（空でない文字列）と "verdict"（"ask" か "block"、既定は "ask"）を持ち、"options-with-value"、"for"、"deny"、"deny-flags"、"deny-option-values"、"deny-env"、"only"、"examples.deny"、"examples.allow" を任意で持つ。"deny"、"deny-flags"、"deny-option-values"、"deny-env"、"only" のどれも持たない規則は、形の誤りとする。

### REQ-028: 語の照合

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
- verification: unit

"deny" と "for" の語、"deny-option-values" の値、"deny-env" の名前は、語全体と照合する。"/" で始まり "/" で終わる 2 文字以上の文字列は、囲まれた部分を正規表現として語全体に当て、それ以外は語との完全一致とする。"options-with-value"、"deny-flags"、"deny-option-values" のオプション名は完全一致とする。

### REQ-029: 使い方の先頭一致

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
- verification: unit

"deny" と "for" の各項目は語の並びで、位置ごとに 1 つの語か、代替の語の一覧を書く。照合の前に、プログラム名の直後から "-" で始まる語を読み飛ばす。"options-with-value" に書いた名前の語は次の語も値として読み飛ばし、"=" を含む語はその 1 語だけを読み飛ばし、それ以外の "-" で始まる語は値を取らないものとして読み飛ばす。"--" の語が来たら、それを読み飛ばして読み飛ばしを終える。残った語の並びの先頭が "deny" の項目にすべて当たれば一致とする。項目より後ろの語は問わない。

### REQ-030: 位置を問わないフラグとオプションの値

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
- verification: unit

"deny-flags" の各フラグは、プログラム名より後で最初の "--" より前の語に照合する。語が "=" を含むときは最初の "=" より前の部分を照合する。"-" に続けて 1 文字のフラグは、"-" で始まり "--" で始まらない語に含まれる文字にも当てる。"deny-option-values" は、最初の "--" より前で、その名前の語の次の語か、"名前=値" の最初の "=" より後ろの部分を、名前ごとの値の一覧と照合する。規則が "for" を持つときは、"for" に当たる起動にだけ、この 2 つを当てる。

### REQ-031: 環境変数

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
- verification: unit

"deny-env" の各項目は、環境変数の名前とする。コマンド本文の先頭の "NAME=value" と、ラッパー越しの代入の名前に、どれかの項目が当たれば一致とする。フック自身の環境は見ない。規則が "for" を持つときは、"for" に当たる起動にだけ当てる。

### REQ-032: 通す使い方だけを書く

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A42, docs/decision/records/2026-09-30-hook-guardian-scope.md#A44, docs/decision/records/2026-09-30-hook-guardian-scope.md#A48
- verification: unit

"only" の各項目は "deny" と同じ語の並びで、同じ読み飛ばしをする。"only" を持つ規則は、読み飛ばしの後の先頭の語の並びが "only" のどの項目にも当たらない起動を一致とする。この判定は、"deny" 系のどれにも当たらなかった起動にだけ行う。"only" を持つ規則が複数あるときは、規則ごとに別々に行い、すべての "only" に当たる起動だけが一致しない。

### REQ-033: 判定への合成

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A13, docs/decision/records/2026-09-30-hook-guardian-scope.md#A20, docs/decision/records/2026-09-30-hook-guardian-scope.md#A43
- verification: unit

一致した規則ごとに、その "verdict" と理由を返す。複数の規則が一致してよい。判定の合成は、効果の判定と同じ最悪値にする。プロジェクト設定の規則も、同じ規則で反映する。

### REQ-034: 例の検証と、壊れた規則の扱い

- kind: event_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A45, docs/decision/records/2026-09-30-hook-guardian-scope.md#A48, docs/decision/records/2026-10-02-guard-example-diagnostics.md#A1
- verification: unit

設定を読み込むとき、"examples.deny" の各例がその規則で一致し、"examples.allow" の各例が一致しないことを確かめる。例は、シェルと同じ引用の規則で語に分け、先頭の "NAME=value" を環境として読む。合わない例、壊れた正規表現、形の誤りがある規則は、その規則だけを無効にして警告を出す。設定の全体は、組み込みの既定で続ける。

例のトップレベルまたは内側の意味解析に診断があるときは、回復した起動が一致しても、その規則だけを無効化して警告する。診断のない例の一致判定と他の規則は維持する。

## Properties

### PROP-001: 柵であって境界ではない

- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A46

この見張りは、事故を減らす柵であり、境界ではない。悪意ある迂回を強制力で防ぐ仕組みは持たず、判定はコマンド本文と設定だけで行う。

## Examples

```gherkin
@id=EX-037 @about=REQ-033 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A42,docs/decision/records/2026-09-30-hook-guardian-scope.md#A43
Scenario: push の使い方を ask にする
  Given "program" が "git"、"deny" が [["push"]]、"verdict" が "ask" の規則がある
  When "git push origin main" を判定する
  Then 判定は ask になり、理由に規則の reason が出る

@id=EX-038 @about=REQ-029 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
Scenario: 先頭一致なので引数の中の語には当たらない
  Given "program" が "git" で "deny" が [["push"]] の規則がある
  When "git commit -m push" を判定する
  Then この規則は一致しない

@id=EX-039 @about=REQ-030 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
Scenario: 別名を作る値だけを止める
  Given "program" が "git"、"options-with-value" が ["-c", "--config-env"]、"deny-option-values" が "-c" と "--config-env" に ["/alias[.].*/"] の規則がある
  When "git -c alias.p=push p" と "git --config-env=alias.p=ENV p" を判定する
  Then どちらも一致する

@id=EX-040 @about=REQ-028 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
Scenario: ラッパー越しとパス付きの起動にも当たる
  Given "program" が "git" で "deny" が [["push"]] の規則がある
  When "sudo git push" と "/usr/bin/git push" を判定する
  Then どちらも一致する

@id=EX-041 @about=REQ-034 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A45
Scenario: 期待と違う例ではその規則だけを無効にする
  Given "examples.deny" に "git status" を持つ規則がある
  When 設定を読み込む
  Then その規則は無効になり、警告が出る

@id=EX-042 @about=REQ-032 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A42,docs/decision/records/2026-09-30-hook-guardian-scope.md#A48
Scenario: only に書いた使い方だけを通す
  Given git の規則が "only" に "status" と "commit" を持つ
  And 判定する起動は、その規則の "deny"、"deny-flags"、"deny-option-values"、"deny-env" のどれにも当たらない
  When "git status" と "git commit" と "git push" を判定する
  Then "git status" と "git commit" はその規則に一致せず、"git push" は "only" に当たらないため、その規則の "verdict" になる

@id=EX-043 @about=REQ-034 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A45
Scenario: 壊れた正規表現の規則だけを無効にする
  Given "deny" が [["/pu(sh/"]] の規則がある
  When 設定を読み込む
  Then その規則は無効になり、警告が出る

@id=EX-044 @about=REQ-027 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A42,docs/decision/records/2026-09-30-hook-guardian-scope.md#A45,docs/decision/records/2026-09-30-hook-guardian-scope.md#A47
Scenario: 形の誤りの規則だけを無効にする
  Given "deny"、"deny-flags"、"deny-option-values"、"deny-env"、"only" のどれも持たない規則がある
  When 設定を読み込む
  Then その規則は無効になり、警告が出る

@id=EX-045 @about=REQ-031 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A44
Scenario: コマンド本文の環境変数の代入に当たる
  Given "program" が "git" で "deny-env" が ["GIT_CONFIG_COUNT"] の規則がある
  When "GIT_CONFIG_COUNT=1 git status" と "git status" を判定する
  Then 最初だけが一致する
```
