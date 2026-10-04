# OpenCode V2 の接続や承認の経路が使えないとき

プラグインが自身をホストする OpenCode の HTTP サーバーへ接続できないとき、または承認要求を作れないときの判定と承認を扱う。接続できないのは、接続オプションが未指定で自身の管理サービスへの自動接続を確立できない場合（登録が無い場合と、登録の検証に失敗した場合を含む）と、接続オプションを明示したが設定が不完全か、接続や認証に失敗した場合である。"opencode run --standalone" と、管理サービスを設定で無効にした場合が前者にあたる。プラグインの都合で承認の経路が使えないことを理由に、実行を止めない。接続できる場合の承認は opencode.md が扱う。

## Requirements

### REQ-065: 接続が無いときの判定

- kind: state_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A19, docs/decision/records/2026-10-04-opencode-standalone.md#A2, docs/decision/records/2026-10-04-opencode-standalone.md#A4, docs/decision/records/2026-10-04-opencode-standalone.md#A13, docs/decision/records/2026-10-04-opencode-standalone.md#A22, docs/decision/records/2026-10-04-opencode-standalone.md#A25, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
- verification: unit

プラグインが自身をホストするサーバーへ接続できないとき、プラグインは登録に書かれた宛先へ通信せず、shell の "create.before" フックが受け取る実際の shell、cwd、コマンドを guardian の判定へ渡す。shell を "/unknown-shell" として判定しない。判定が allow なら guardian 由来の確認も拒否も出さず OpenCode 自身の権限判断に委ね、block なら拒否する。本体の応答で影実行と確定できた場合は opencode.md の REQ-048 を優先し、guardian 由来の確認も拒否も出さない。

接続オプションを明示したが設定が不完全か、接続や認証に失敗したときは、管理サービスの自動接続へ切り替えず、設定の誤りをプラグインの標準エラーへの警告で、読み込み時に1回と実行ごとに1回示したうえでこの要求に従う。

### REQ-066: 接続が無いときの承認が要る実行

- kind: state_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A22, docs/decision/records/2026-10-04-opencode-standalone.md#A7
- verification: unit

REQ-065 の状態で、判定が ask の実行、判定結果を取得できない実行（REQ-051）、Bash 以外の shell の実行（REQ-055）、判定と起動の一致を確かめられない実行（REQ-047）について、プラグインは guardian 由来の確認も停止も出さず、OpenCode 自身の権限判断に委ねる。OpenCode の規則が許可すれば実行され、確認を求めれば OpenCode 自身の確認になり、拒否すれば実行されない。

自身の管理サービスか明示した接続で接続できる場合は、opencode.md の REQ-050 から REQ-052 の承認の経路を使い、この要求を使わない。

### REQ-067: 承認要求を作れないとき

- kind: event_driven
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A23, docs/decision/records/2026-10-04-opencode-standalone.md#A24, docs/decision/records/2026-10-04-opencode-standalone.md#A25
- verification: unit

接続がある状態で、判定が ask の実行、判定結果を取得できない実行（REQ-051）、Bash 以外の shell の実行（REQ-055）、判定と起動の一致を確かめられない実行（REQ-047）の承認要求を作れなかったとき、プラグインはそのたびにプラグインの標準エラーへ警告を出し、その実行を OpenCode 自身の権限判断に委ねる。作成の応答を受け取れなかった場合（失敗した場合と、成功したか分からない場合）も作れなかったものとして扱う。承認待ちは作成の応答を受け取った時点から始まる。作れなかったものとして扱った実行について、残った guardian の確認にあとから返答があっても、その実行には何もしない。警告が画面に表示されるかは保証しない。承認待ちの途中で通信の終了やエラーを検知した実行は、opencode.md の REQ-053 のとおり取りやめ、後の承認や再接続でも再開しない。

### REQ-068: 接続が無いときの対応条件の説明

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A19, docs/decision/records/2026-10-04-opencode-standalone.md#A9, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A22
- verification: review
- how_to_verify: docs/opencode.md の OpenCode の導入手順とプラグインのエラーの文面を人が読み、"opencode run --standalone" が接続設定なしで動くこと、接続が無いときは guardian の承認が要る実行を OpenCode 自身の権限判断に委ねること、standalone に明示の接続設定を求める案内が残っていないことを確かめる

command-guardian の導入手順は、"opencode run --standalone" が接続設定なしで動くと説明する。接続が無いときは、guardian の承認が要る実行を OpenCode 自身の権限判断に委ね、guardian の確認が出ないことを明記する。standalone に明示の接続設定を求める案内をしない。

### REQ-069: 接続が無いときの検証

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A8, docs/decision/records/2026-10-04-opencode-standalone.md#A18, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A26
- verification: review
- how_to_verify: 結合試験に使う OpenCode の固定版をモデルなしに隔離して起動する結合試験（接続が無いときは "serve --stdio"）の実行結果を確かめ、allow の実行、block の拒否、承認が要る実行が OpenCode の規則に従って実行または確認または拒否されること、明示した接続設定の失敗と承認要求の作成失敗で警告が出て OpenCode に委ねられることの証拠があり、普段の OpenCode の設定とセッションを使っていないことを確認する

接続が無いときの判定と委任は、結合試験に使う OpenCode の固定版を "serve --stdio" でモデルなしに隔離して起動した結合試験で検証する。明示した接続設定の失敗と承認要求の作成失敗は、同じ固定版をモデルなしに隔離して起動した既存の結合試験の仕組みで検証し、作成失敗には作成を失敗させる偽の仕組みを使う。その版は試験の環境であり、対応の条件ではない。"opencode run --standalone" の通し実行は必須にしない。

## Examples

```gherkin
@id=EX-133 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A4,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無くても実際の shell で判定して allow を実行する
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And 設定した shell が "/bin/bash" で、OpenCode の権限の規則がすべてを許可している
  When エージェントが allow になるコマンドを実行する
  Then guardian は "/bin/bash" で判定し、guardian 由来の確認なしでコマンドが実行される

@id=EX-134 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無くても block は拒否する
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  When エージェントが block になるコマンドを実行する
  Then コマンドは実行されず拒否される

@id=EX-135 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A22,docs/decision/records/2026-10-03-opencode-managed-service.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A25
Scenario: 明示した接続設定の失敗は警告して接続が無いときの扱いにする
  Given 接続オプションを明示したが認証に失敗する
  When エージェントが allow になるコマンドを実行する
  Then 管理サービスの自動接続へ切り替えず、ホストの標準エラーに設定の誤りの警告が出て、コマンドは実行される

@id=EX-136 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A21,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの ask は OpenCode の規則が許可すれば実行される
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And OpenCode の権限の規則がすべてを許可している
  When エージェントが ask になるコマンドを実行する
  Then guardian 由来の確認は出ず、コマンドが実行される

@id=EX-137 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A21,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの ask は OpenCode の規則が確認を求めれば OpenCode の確認になる
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And OpenCode の権限の規則が shell の実行に確認を求める
  When エージェントが ask になるコマンドを実行し、試験が OpenCode の承認の API でその確認に拒否を返す
  Then guardian 由来の確認は出ず、OpenCode 自身の shell の確認が出て、拒否によりコマンドは実行されない

@id=EX-138 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A7
Scenario: 接続できる場合は従来の承認の経路を使う
  Given プラグインが自身の管理サービスへ接続できる
  When エージェントが ask になるコマンドを実行する
  Then 承認要求は従来の HTTP の承認の経路で出る

@id=EX-140 @about=REQ-065 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A13,docs/decision/records/2026-10-04-opencode-standalone.md#A2
Scenario: 検証に失敗した登録には通信せず接続が無いときの扱いにする
  Given 接続オプションが未指定で、管理サービスの登録の PID がプラグインの PID と異なる
  When エージェントが allow になるコマンドを実行する
  Then 登録に書かれた宛先へ通信せず、guardian の判定でコマンドが実行される

@id=EX-141 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A21,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: 接続が無いときの判定不能も OpenCode に委ねる
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And OpenCode の権限の規則がすべてを許可している
  And guardian の判定がタイムアウトする
  When エージェントがコマンドを実行する
  Then guardian 由来の確認も停止も出ず、コマンドが実行される

@id=EX-146 @about=REQ-066 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A21,docs/decision/records/2026-10-04-opencode-standalone.md#A8
Scenario: コマンドの無いリダイレクトの ask も OpenCode に委ねる
  Given プラグインが "serve --stdio" で起動したホストの中で動き接続オプションが未指定である
  And OpenCode の権限の規則がすべてを許可している
  When エージェントがコマンドを付けないリダイレクトで既存のファイルを切り詰める
  Then guardian 由来の確認も停止も出ず、切り詰めが実行される

@id=EX-147 @about=REQ-067 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A23,docs/decision/records/2026-10-04-opencode-standalone.md#A25,docs/decision/records/2026-10-04-opencode-standalone.md#A26
Scenario: 承認要求を作れなければ警告して OpenCode に委ねる
  Given プラグインが自身の管理サービスへ接続している
  And 承認要求の作成が失敗する
  When エージェントが ask になるコマンドを実行する
  Then ホストの標準エラーに警告が出て、OpenCode 自身の権限判断で実行の可否が決まる

@id=EX-148 @about=REQ-067 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A23,docs/decision/records/2026-10-02-opencode-v2-hook.md#A14
Scenario: 承認待ちの途中で通信が切れた実行は取りやめる
  Given プラグインが自身の管理サービスへ接続し、ask になる実行が承認待ちである
  When 通信の終了を検知する
  Then その実行は起動しない

@id=EX-149 @about=REQ-067 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A24,docs/decision/records/2026-10-04-opencode-standalone.md#A25
Scenario: 作成の応答を受け取れなければ作れなかったものとして扱う
  Given プラグインが自身の管理サービスへ接続している
  And 承認要求の作成の要求を送った後、応答を受け取る前に通信が切れる
  When エージェントが ask になるコマンドを実行する
  Then ホストの標準エラーに警告が出て、OpenCode 自身の権限判断で実行の可否が決まる
```
