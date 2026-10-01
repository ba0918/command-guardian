# コマンドの読み方

コマンド文字列を構文解析し、正規化した構文木を効果の抽出へ渡すまで（`guardian-parser`）を扱う。効果の抽出と判定は judgment.md、設定は config.md が扱う。

## Requirements

### REQ-035: 対象のシェル

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A1, docs/decision/records/2026-10-01-parser.md#A16
- verification: unit

コマンドを読むとき、hook-guardian は bash と POSIX sh の文法を対象にする。bash 系のシェル（"bash"、"sh"、"dash"、"zsh"、"ksh"、"mksh"）を "-c" で起動するコマンド（"-lc" のようなまとめ書きを含む）は、その本体を bash 互換の文法として読む。"-c" を伴わない起動（"bash script.sh"、"sh < file"）は、本体がコマンド文字列の外にあるため読まず、通常のコマンドとして判定する。知っている非 POSIX 系のシェル（"fish"、"csh"、"tcsh"、"elvish"、"xonsh"、"nu"、"pwsh"）は、本体の有無にかかわらず ask にする。知らないプログラムはシェルとみなさない。シェルの起動の認識は、ラッパー（sudo と doas）を外した後のプログラムの語で行う。

### REQ-036: 構文解析の担い手と境界

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A2, docs/decision/records/2026-10-01-parser.md#A6, docs/decision/records/2026-10-01-parser.md#A10, docs/decision/records/2026-10-01-parser.md#A11, docs/decision/records/2026-10-01-parser.md#A17
- verification: review
- how_to_verify: `cargo tree -p guardian-core --depth 1` の直接依存に OSS のシェルパーサが無いこと、`cargo tree -i brush-parser` で依存の向きを確かめ、`guardian-parser` の公開する型だけを core 以降が使っていることを確かめる

字句解析と構文解析は `guardian-parser` が担い、成熟した OSS のシェルパーサ `brush-parser` を使う。版を固定する。`guardian-parser` の外へは正規化した構文木だけを公開し、ほかの crate は OSS の型に依存しない。字句解析は、判定の経路と、設定の照合（見張りの例の分割と、カスタムルールの本文の引用の除去）で共有する。

### REQ-041: 正規化した構文木の契約

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A8, docs/decision/records/2026-10-01-parser.md#A19
- verification: review
- how_to_verify: `guardian-parser` のテストと core 以降の利用箇所を読み、次の事実が保たれていることを確かめる。コマンド列の順序と区切り、プログラムと引数の語（順序、引用の状態、語の断片、置換の本体）、リダイレクトの種類と対象とファイル記述子、複合構文の本体

正規化した構文木は、次を保つ。コマンド列の順序と区切り（";"、"&&"、"||"、パイプ）、各コマンドのプログラムの語と引数の語（順序、引用の状態、語を構成する断片、置換の本体）、各リダイレクトの種類（読み、書き、追記、切り詰め、読み書き、複製）と対象の語とファイル記述子、複合構文（条件、繰り返し、グループ、サブシェル、関数）の本体。

### REQ-037: 総走査

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A3, docs/decision/records/2026-10-01-parser.md#A8
- verification: property
- how_to_verify: 文法要素のコーパスを入力に、構文木に現れるすべての置換・リダイレクト・複合構文が正規化した構文木にも現れることを性質テストで確かめる

`guardian-parser` は、すべての単語、すべてのコマンド置換とプロセス置換の本体（再帰）、すべてのリダイレクト、すべての複合構文を 1 回ずつ訪問し、正規化した構文木として返す。訪問されない位置を残さない。

### REQ-038: 読めないときは ask

- kind: event_driven
- source: docs/decision/records/2026-10-01-parser.md#A2, docs/decision/records/2026-10-01-parser.md#A7, docs/decision/records/2026-10-01-parser.md#A11, docs/decision/records/2026-10-01-parser.md#A14, docs/decision/records/2026-10-01-parser.md#A20
- verification: unit

構文解析が失敗したとき、知らないノードの形に出会ったとき、置換の再帰読みの途中で読めなくなったとき、hook-guardian は ask の判定を出す。解析が失敗した命令からは効果を取り出さない。この ask はほかの効果の判定と合成し、より重い判定（block）があればそれが勝つ。

### REQ-039: 入力の上限

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A11, docs/decision/records/2026-10-01-parser.md#A12, docs/decision/records/2026-10-01-parser.md#A18, docs/decision/records/2026-10-01-parser.md#A21, docs/decision/records/2026-10-01-parser.md#A22, docs/decision/records/2026-10-01-parser.md#A23
- verification: unit

コマンド文字列が 1 MiB を超えるとき（置換の再帰読みでは累計で測る）、構文の入れ子と置換の再帰の段数が 128 段を超えるとき、1 回の判定で行う構文解析の回数（既定 1000 回）と判定の時間（既定 5 秒）の上限を超えるときは、理由をつけて block にする。サイズは構文解析の前に測る。構文解析は隔離した子プロセスで行い、その死は理由で分け、入力に帰せる死（スタックオーバーフロー、時間の上限の超過）は block、自分に帰せる死（panic、起動とプロトコルの失敗、判定できない死）は ask に落とす（判定は必ず返り、プロセスは落ちない）。深さは、正規化した構文木の走査で測り、128 段を超えた時点で打ち切る。正規化した構文木が断片として保つ位置（算術式の内側や、条件式の括弧）の入れ子は、数え落としの起きない過大評価で数えてよい。自前の字句解析による事前の深さ測定は行わない。

### REQ-040: 文法の追加

- kind: ubiquitous
- source: docs/decision/records/2026-10-01-parser.md#A5, docs/decision/records/2026-10-01-parser.md#A15
- deferred: docs/decision/records/2026-10-01-parser.md#A15
- verification: unit

別のシェルへの対応は、`guardian-parser` の I/F の背後に、そのシェルのパーサ（文法）を追加する形で行う。追加したパーサでも、読めない構文は ask にする。

## Examples

```gherkin
@id=EX-046 @about=REQ-035 @source=docs/decision/records/2026-10-01-parser.md#A1
Scenario: 非 POSIX 系のシェルは読まない
  When "fish -c 'rm -rf /etc/x'" を判定する
  Then 判定は ask になる

@id=EX-047 @about=REQ-037 @source=docs/decision/records/2026-10-01-parser.md#A8,docs/decision/records/2026-10-01-parser.md#A11
Scenario: 値を取るオプションの値の中の置換も読まれる
  When "sudo -u \"$(rm -rf /etc/x)\" true" を判定する
  Then 判定は block になる

@id=EX-048 @about=REQ-037 @source=docs/decision/records/2026-10-01-parser.md#A8,docs/decision/records/2026-10-01-parser.md#A16,docs/decision/records/2026-09-30-hook-guardian-scope.md#A11,docs/decision/records/2026-09-30-hook-guardian-scope.md#A17
Scenario: シェルの -c の本体が入れ子の構文として読まれる
  When "bash -c 'rm -rf /etc/x'" を判定する
  Then 本体の削除が取り出され、判定は block になる

@id=EX-049 @about=REQ-038 @source=docs/decision/records/2026-10-01-parser.md#A7
Scenario: 読めない構文は ask
  When "if true; then rm -rf /etc/x" を判定する
  Then 判定は ask になる

@id=EX-050 @about=REQ-039 @source=docs/decision/records/2026-10-01-parser.md#A12,docs/decision/records/2026-10-01-parser.md#A22
Scenario: 大きすぎる入力は止める
  Given 1 MiB を超えるコマンド文字列がある
  When 判定する
  Then 判定は block になり、理由が出る

@id=EX-051 @about=REQ-040 @source=docs/decision/records/2026-10-01-parser.md#A5
Scenario: 文法を追加したシェルは読める
  Given fish の文法が追加されている
  When "fish -c 'rm -rf /etc/x'" を判定する
  Then 本体が読まれ、判定は block になる

@id=EX-052 @about=REQ-036 @source=docs/decision/records/2026-10-01-parser.md#A6,docs/decision/records/2026-10-01-parser.md#A11
Scenario: core 以降は OSS の型に依存しない
  Given guardian-parser が brush-parser を使っている
  When core 以降の直接依存と公開型を調べる
  Then 直接依存に OSS のシェルパーサは現れず、guardian-parser の公開する型だけが使われている

@id=EX-053 @about=REQ-041 @source=docs/decision/records/2026-10-01-parser.md#A8,docs/decision/records/2026-10-01-parser.md#A19
Scenario: 正規化した構文木が必要な事実を保つ
  Given 引用・リダイレクト・置換・複合構文を含むコマンドがある
  When 構文解析して正規化した構文木を調べる
  Then 順序と区切り、引用の状態、リダイレクトの種類と対象、置換の本体、複合構文の本体が現れる

@id=EX-054 @about=REQ-035 @source=docs/decision/records/2026-10-01-parser.md#A16
Scenario: まとめ書きの -c の本体を読む
  When "bash -lc 'rm -rf /etc/x'" を判定する
  Then 本体の削除が取り出され、判定は block になる

@id=EX-055 @about=REQ-035 @source=docs/decision/records/2026-10-01-parser.md#A16
Scenario: 本体がファイルの起動は読まない
  When "bash script.sh" を判定する
  Then 削除の効果は取り出されず、判定は allow になる

@id=EX-056 @about=REQ-035 @source=docs/decision/records/2026-10-01-parser.md#A16
Scenario: ラッパー越しの非 POSIX 系のシェルは ask
  When "sudo fish -c 'rm -rf /etc/x'" を判定する
  Then 判定は ask になる

@id=EX-057 @about=REQ-039 @source=docs/decision/records/2026-10-01-parser.md#A21,docs/decision/records/2026-10-01-parser.md#A22
Scenario: 深い入れ子は止まり、プロセスは落ちない
  Given 128 段を超える入れ子を here-doc とバッククォートの中に置いた入力がある
  When 判定する
  Then 判定は block になり、プロセスは異常終了しない
```
