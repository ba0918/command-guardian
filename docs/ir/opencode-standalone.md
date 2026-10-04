# OpenCode V2 の接続が無いときの判定と承認

プラグインが自身をホストする OpenCode のHTTPサーバーへ接続できないときの判定と承認を扱う。対象は、接続オプションが未指定で、自身の管理サービスへの自動接続を確立できない場合である。登録が無い場合と、登録の検証に失敗した場合を含む。"opencode run --standalone" と、管理サービスを設定で無効にした場合がこれにあたる。接続できる場合の承認と、接続オプションを明示した場合は opencode.md が扱う。

## Requirements

### REQ-065: 接続が無いときの判定

- kind: state_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A19, docs/decision/records/2026-10-04-opencode-standalone.md#A2, docs/decision/records/2026-10-04-opencode-standalone.md#A4, docs/decision/records/2026-10-04-opencode-standalone.md#A13, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
- verification: unit

接続オプションが未指定で、自身の管理サービスへの自動接続を確立できないとき（登録が無い場合と、登録の検証に失敗した場合を含む）、プラグインは登録に書かれた宛先へ通信せず、プラグインは shell の "create.before" フックが受け取る実際の shell、cwd、コマンドを guardian の判定へ渡す。shell を "/unknown-shell" として判定しない。判定が allow なら実行し、block なら拒否する。承認の経路が無いことは、判定が ask の実行だけに影響する。本体の応答で影実行と確定できた場合は opencode.md の REQ-048 を優先し、guardian 由来の確認も拒否も出さない。

接続オプションを明示した場合は、この要求を適用しない。明示した設定が不完全か認証に失敗したときは opencode.md の REQ-052 に従い、この要求の経路へ切り替えない。

### REQ-066: 接続が無いときの承認

- kind: event_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A3, docs/decision/records/2026-10-04-opencode-standalone.md#A7, docs/decision/records/2026-10-04-opencode-standalone.md#A10, docs/decision/records/2026-10-04-opencode-standalone.md#A11, docs/decision/records/2026-10-04-opencode-standalone.md#A12, docs/decision/records/2026-10-04-opencode-standalone.md#A15
- verification: unit

REQ-065 の状態で判定が ask になったとき、プラグインは OpenCode の権限の "evaluate" フックでその実行の結果を ask にし、OpenCode 自身の承認要求として出す。判定不能（REQ-051）、Bash 以外の shell（REQ-055）、判定と起動の一致を確かめられない場合（REQ-047）も同じ経路で承認に回す。guardian の理由は "evaluate" の結果の "message" に入れ、OpenCode がそれを表示するかは保証しない。返答があるまで対象コマンドを起動させない。承認の返答では実行し、拒否の返答では実行しない。"run --auto" による自動承認は有効な承認として扱う。

"create.before" での判定と "evaluate" の呼び出しは、実行ごとの識別子と、コマンド・cwd・shell の一致で対応付ける。一つに決まらなければ承認が要る扱いにし、REQ-067 で止める。判定の結果は実行の完了か中断で捨てる。

OpenCode の承認に "always" が返った場合は OpenCode の標準の扱いに従い、それによって承認待ちの他の実行が承認されることを妨げない。以後の新しい実行には、opencode.md の REQ-050 のとおり承認要求を出す。

自身の管理サービスか明示した接続で接続できる場合は、opencode.md の REQ-050 と REQ-052 の承認の経路を使い、この要求の経路を使わない。

### REQ-067: 承認に回らなかった ask

- kind: event_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A6, docs/decision/records/2026-10-04-opencode-standalone.md#A3, docs/decision/records/2026-10-02-opencode-v2-hook.md#A15
- verification: unit

REQ-065 の状態で判定が ask になった実行について、起動の前にその実行に対応する "evaluate" フックの呼び出しも承認の返答も無いとき、プラグインは理由を示してその実行を止める。この停止は guardian が危険と判定した block と区別する。

### REQ-068: 接続が無いときの対応条件の説明

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A19, docs/decision/records/2026-10-04-opencode-standalone.md#A5, docs/decision/records/2026-10-04-opencode-standalone.md#A9
- verification: review
- how_to_verify: docs/opencode.md の OpenCode の導入手順とプラグインのエラーの文面を人が読み、"opencode run --standalone" が接続設定なしで動くこと、他のプラグインが権限の結果を上書きしないことが対応条件であること、standalone に明示の接続設定を求める案内が残っていないことを確かめる

command-guardian の導入手順は、"opencode run --standalone" が接続設定なしで動くと説明する。他のプラグインの権限の "evaluate" フックが guardian の結果を上書きしないことを対応条件とし、上書きされた場合の振る舞いは保証しない。standalone に明示の接続設定を求める案内をしない。

### REQ-069: 接続が無いときの検証

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A8, docs/decision/records/2026-10-04-opencode-standalone.md#A14, docs/decision/records/2026-10-04-opencode-standalone.md#A18
- verification: review
- how_to_verify: 結合試験に使う OpenCode の固定版を "serve --stdio" でモデルなしに隔離して起動する結合試験の実行結果を確かめ、allow の実行、block の拒否、ask が承認の返答まで起動しないこと、拒否の返答で実行しないこと、承認待ちの間の中断で実行しないこと、並行した実行の承認を取り違えないこと、承認に回らなかった ask が止まることの証拠があり、普段の OpenCode の設定とセッションを使っていないことを確認する

接続が無いときの判定と承認は、結合試験に使う OpenCode の固定版を "serve --stdio" でモデルなしに隔離して起動した結合試験で検証する。その版は試験の環境であり、対応の条件ではない。"opencode run --standalone" の通し実行は必須にしない。

## Examples

```gherkin
@id=EX-133 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A4,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無くても実際の shell で判定して allow を実行する
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And 設定した shell が "/bin/bash" である
  When エージェントが allow になるコマンドを実行する
  Then guardian は "/bin/bash" で判定し、承認なしでコマンドが実行される

@id=EX-134 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無くても block は拒否する
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  When エージェントが block になるコマンドを実行する
  Then コマンドは実行されず拒否される

@id=EX-135 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A2,docs/decision/records/2026-10-03-opencode-managed-service.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A13
Scenario: 明示した接続設定の失敗では接続が無いときの経路へ切り替えない
  Given 接続オプションを明示したが認証に失敗する
  When エージェントが allow になるコマンドを実行する
  Then 接続が無いときの経路へ切り替えず対象コマンドを実行しない

@id=EX-136 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A3,docs/decision/records/2026-10-04-opencode-standalone.md#A15,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの ask は OpenCode の承認の返答まで起動しない
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  When エージェントが ask になるコマンドを実行する
  Then OpenCode 自身の承認要求が出て、その内容に guardian の理由が入り、返答があるまでコマンドは起動しない
  And 承認の返答で実行される

@id=EX-137 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A3,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの ask は拒否の返答で実行しない
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  When エージェントが ask になるコマンドを実行し、承認要求に拒否が返る
  Then コマンドは実行されない

@id=EX-138 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A7
Scenario: 接続できる場合は従来の承認の経路を使う
  Given プラグインが自身の管理サービスへ接続できる
  When エージェントが ask になるコマンドを実行する
  Then 承認要求は従来の HTTP の承認の経路で出る

@id=EX-139 @about=REQ-067 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A6,docs/decision/records/2026-10-04-opencode-standalone.md#A8,docs/decision/records/2026-10-02-opencode-v2-hook.md#A15,docs/decision/records/2026-10-04-opencode-standalone.md#A3
Scenario: 承認に回らなかった ask は実行しない
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And ask と判定した実行について、対応する "evaluate" フックが呼ばれないまま起動に進む
  When プラグインがその実行の起動を受け付ける
  Then 承認に回らなかった理由を示して実行を止め、guardian の block の理由とは区別される

@id=EX-140 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A13,docs/decision/records/2026-10-04-opencode-standalone.md#A2
Scenario: 検証に失敗した登録には通信せず接続が無いときの経路に入る
  Given 接続オプションが未指定で、管理サービスの登録の PID がプラグインの PID と異なる
  When エージェントが allow になるコマンドを実行する
  Then 登録に書かれた宛先へ通信せず、guardian の判定でコマンドが実行される

@id=EX-141 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A10,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの判定不能も OpenCode の承認に回す
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And guardian の判定がタイムアウトする
  When エージェントがコマンドを実行する
  Then タイムアウトの理由を入れた OpenCode 自身の承認要求が出て、返答があるまでコマンドは起動しない

@id=EX-142 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A11,docs/decision/records/2026-10-04-opencode-standalone.md#A8,docs/decision/records/2026-10-04-opencode-standalone.md#A2
Scenario: 並行した実行の判定を取り違えない
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And 同じセッションで allow になる実行と ask になる実行が並行する
  When ask になる実行へまだ返答が無い
  Then allow になる実行は承認なしで実行され、ask になる実行は起動しない

@id=EX-143 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A11,docs/decision/records/2026-10-02-opencode-v2-hook.md#A14,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 承認待ちの間に中断した実行は起動しない
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And ask になる実行が承認待ちである
  When セッションが中断される
  Then その実行は起動せず、その判定の結果は捨てられる
```
