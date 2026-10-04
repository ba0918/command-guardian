# OpenCode V2 の実行前連携

OpenCode V2 のエージェントによる shell 実行へ guardian の判定を接続する契約を扱う。導入条件と配布は opencode-delivery.md、既存の判定は judgment.md が扱う。

## Requirements

### REQ-047: 対象となる実行

- kind: event_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A2, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A19, docs/decision/records/2026-10-02-opencode-v2-hook.md#A24, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

OpenCode のエージェントが "shell" ツールを呼ぶとき、プラグインはその入力を既存の guardian の判定へ渡す。通常実行、バックグラウンド実行、Code Mode 経由の実行、およびコマンドを付けないリダイレクトによる切り詰めを含める。人が OpenCode から直接実行する shell と、MCP ツール内部の実行は連携の対象にしない。

対応条件内で、guardian が判定するコマンド本文・cwd・shellと、承認対象および起動入力を一致させ、その実行への対応を承認待ちの間も保持する。省略・相対cwdと、同じ本文でcwdが違う並行実行も検証する。一致を確認できない場合は判定不能として承認を求める。影実行と確定済みの場合は REQ-048 を優先する。他のhookによる変更と、承認待ち中のファイル状態変化はこの一致の保証に含めない。
接続や承認の経路が使えない場合は opencode-standalone.md の REQ-066 と REQ-067 に従う。

### REQ-048: 共通設定と影実行

- kind: state_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A6, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-02-opencode-v2-hook.md#A19, docs/decision/records/2026-10-02-opencode-v2-hook.md#D1, docs/decision/records/2026-10-03-llm-advice-layer.md#A45
- verification: unit

Rust 本体の "hook --agent opencode" は既存の guardian の設定・影実行・ログを使い、プラグインはこれらの設定を別に読み直さない。本体の応答で "mode.enforce" が false の影実行と確定できた場合だけ、guardian 由来の確認・拒否を出さず、OpenCode 本来の権限判断を維持する。確定済みの影実行では、影ログ保存失敗と対応外shellは警告にとどめる。ログは影実行時だけ記録する。OpenCode 用の応答は判定の無い影実行と判定不能を区別する。影実行か確定できない応答失敗は REQ-051 に従う。JSON の具体的なキーは委譲範囲内で実装担当が契約として記録する。"check" と Claude Code・Codex の既存 hook 契約は変更しない。

ここでのログは影ログを指す。
助言のメタデータログはadvisor/context.mdに従う別の保存であり、プラグインによる設定の再読込を追加しない。

### REQ-049: 権限判断を弱めない合成

- kind: invariant
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A4, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

影実行と確定済みの場合は REQ-048 を優先し、以下のguardian判定の強制は行わない。接続や承認の経路が使えない場合は opencode-standalone.md の REQ-066 と REQ-067 に従う。

プラグインは OpenCode の権限判断と guardian の判定の厳しいほうを採用する。OpenCode の拒否・確認を guardian の `allow` で緩和しない。OpenCode が許可していても、guardian の `ask` は承認要求を出し、`block` は実行を拒否する。`block` は "run --auto" でも拒否する。

### REQ-050: 実行ごとの承認

- kind: event_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A5, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A16, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

影実行と確定済みの場合は REQ-048 を優先し、以下のguardian由来の承認要求は出さない。接続や承認の経路が使えない場合は opencode-standalone.md の REQ-066 と REQ-067 に従う。

guardian が `ask` と判定した実行では、その実行の承認要求を OpenCode の承認フローへ渡し、`OpenCode承認`を得る前に対象コマンドを実行しない。OpenCode の保存済み許可でこの要求を省略しない。"run --auto" による自動承認は有効な承認として扱い、人の確認を追加で強制しない。要求が拒否された場合は対象コマンドを実行せず、同じセッションの他の保留要求も拒否する OpenCode の標準挙動に従う。

### REQ-051: 判定結果を取得できない場合

- kind: event_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A6, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

バイナリ未検出、タイムアウト、無効な応答により guardian の判定結果を取得できず、影実行か確定できない場合、プラグインは判定できなかった理由を示して OpenCode の承認フローに渡す。連携異常だけで危険と断定して `block` にせず、承認なしで実行もしない。承認の意味は REQ-050 に従う。接続が無い場合や承認要求を作れない場合は opencode-standalone.md の REQ-066 と REQ-067 に従う。

### REQ-052: 承認経路を利用できない場合

- kind: event_driven
- source: docs/decision/records/2026-10-03-opencode-managed-service.md#A3, docs/decision/records/2026-10-03-opencode-managed-service.md#A2, docs/decision/records/2026-10-02-opencode-v2-hook.md#A15, docs/decision/records/2026-10-04-opencode-standalone.md#A13, docs/decision/records/2026-10-04-opencode-standalone.md#A17, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A22, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

プラグインは自身をホストする同一 OpenCode HTTP サーバーで承認要求を作る。接続オプションが未指定なら管理サービスの登録情報を一度だけ読み、通信前に登録PIDが自身の実行プロセスと一致し、URLと非空の認証情報が文字列であり、URLがloopback HTTPであることを確認する。検証済みのURLと認証情報を固定したSDK接続でサーバー情報を取得し、応答PIDが自身と一致する場合だけ採用し、相手の版は照合しない。自動接続ではHTTPリダイレクトを拒否し、別サービスを起動しない。接続オプションを一つでも指定した場合は明示設定を使い、不完全な設定や認証失敗から自動探索へ切り替えない。接続オプションを指定した場合の不完全な設定や接続・認証の失敗と、接続がある状態で承認要求を作れない場合は、opencode-standalone.md の REQ-065 から REQ-067 に従い、警告を出してプラグインの都合で実行を止めない（block は拒否する）。

接続オプションが未指定で、登録が無いか登録の検証に失敗して自身の管理サービスへ接続できない場合は、opencode-standalone.md の REQ-065 と REQ-066 に従い、登録に書かれた宛先へは通信しない。

### REQ-053: 承認待ちの寿命

- kind: event_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A14, docs/decision/records/2026-10-02-opencode-v2-hook.md#A21, docs/decision/records/2026-10-02-opencode-v2-hook.md#A22, docs/decision/records/2026-10-03-llm-advice-layer.md#A16, docs/decision/records/2026-10-03-llm-advice-layer.md#A41
- verification: unit

プラグイン独自の承認待ち期限と承認履歴の保存は設けない。承認待ち中にセッション中断・プラグイン解除・通信の終了またはエラーを検知した場合は対象コマンドの実行を取りやめ、後の承認・再接続でも再開しない。通信が無応答になっただけの状態の即時検知と、接続の生存監視は保証に含めない。中断と要求作成が競合して確認表示が残った場合も、対象コマンドは再開しない。確認表示の後始末は可能な範囲で行い、必ず消せる保証は付けない。guardianの機械判定の時間上限は変更しない。

助言有効時の別枠と外側の打切りの整合はadvisor/runtime.mdに従い、助言の期限を承認待ちの期限へ適用しない。

### REQ-054: 並行実行の承認の分離

- kind: invariant
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A5, docs/decision/records/2026-10-02-opencode-v2-hook.md#A14, docs/decision/records/2026-10-02-opencode-v2-hook.md#A18
- verification: unit

複数の shell 実行が並行するときは、ある実行への承認を別の実行の承認として使わない。一つの実行を承認しても、別の承認待ちの実行を承認前に開始しない。中断した実行への後の承認を、別の実行または再接続後の実行へ移さない。

### REQ-055: 対応外の shell

- kind: event_driven
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A8, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A20, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-04-opencode-standalone.md#A21, docs/decision/records/2026-10-04-opencode-standalone.md#A23
- verification: unit

Bash 以外の shell では安全に判定できたと見なさず、対象外である理由を示して OpenCode の承認フローに渡す。影実行と確定済みの場合は REQ-048 を優先する。Bash を明示設定し、他の hook が実行コマンド・cwd・shell を変更しないという対応条件は REQ-057 に従う。
接続や承認の経路が使えない場合は opencode-standalone.md の REQ-066 と REQ-067 に従う。

## Examples

```gherkin
@id=EX-106 @about=REQ-052 @source=docs/decision/records/2026-10-03-opencode-managed-service.md#A3,docs/decision/records/2026-10-04-opencode-standalone.md#A17
Scenario: 接続設定なしで自身の管理サービスを使う
  Given プラグインが認証付きloopbackの管理サービス内で動いている
  And 接続オプションが未指定である
  When shellツールを実行する
  Then 通信前に登録を検証し固定した接続先の応答PIDをSDKで照合して接続する
  And allowとblockと承認待ちが明示設定時と同じように働く

@id=EX-107 @about=REQ-052 @source=docs/decision/records/2026-10-03-opencode-managed-service.md#A3,docs/decision/records/2026-10-03-opencode-managed-service.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A13
Scenario: 別サーバーや不整合な登録へ承認を送らない
  Given 登録PIDが自身と異なるか接続先の応答PIDが一致しない
  When 自動接続先を確定する
  Then その接続を採用せず登録に書かれた宛先へ承認要求を送らない

@id=EX-108 @about=REQ-052 @source=docs/decision/records/2026-10-03-opencode-managed-service.md#A3,docs/decision/records/2026-10-04-opencode-standalone.md#A13
Scenario: 未認証やリモートの登録情報を採用しない
  Given 登録情報が未認証かloopback HTTP以外である
  When 自動接続先を確定する
  Then その宛先へ認証付き通信を行わず接続を採用しない

@id=EX-110 @about=REQ-052 @source=docs/decision/records/2026-10-03-opencode-managed-service.md#A3
Scenario: 検証済みの自動接続先を固定する
  Given 自身の認証付きloopback HTTPの登録情報を通信前に検証した
  When 登録情報が差し替えられるかサーバーがHTTPリダイレクトを返す
  Then 登録差替えやリダイレクトにより別の宛先へ接続しない

@id=EX-109 @about=REQ-052 @source=docs/decision/records/2026-10-03-opencode-managed-service.md#A2,docs/decision/records/2026-10-04-opencode-standalone.md#A22
Scenario: 不完全な明示設定や認証失敗を自動接続で隠さない
  Given 自身の管理サービスへ自動接続できる環境である
  And 明示した接続オプションが不完全か認証に失敗する
  When shellツールを実行する
  Then 自動接続へ切り替えず、設定の誤りを警告し、接続が無いときの扱いで判定と委任を行う
```

```gherkin
@id=EX-078 @about=REQ-047 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A2,docs/decision/records/2026-10-02-opencode-v2-hook.md#A7
Scenario: 通常とバックグラウンドとCode Modeの実行を判定する
  When エージェントが通常実行とバックグラウンド実行とCode Mode経由のshell実行を要求する
  Then 各実行をguardianで判定する

@id=EX-079 @about=REQ-047,REQ-050 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A7,docs/decision/records/2026-10-02-opencode-v2-hook.md#A5,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: リダイレクトだけでも確認を省略しない
  Given "mode.enforce" がtrueである
  Given コマンドを付けないリダイレクトがguardianでaskになる
  When エージェントがその実行を要求する
  Then 承認前に対象を切り詰めない

@id=EX-080 @about=REQ-048 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 影実行はguardianの判定を強制しない
  Given 本体の応答で "mode.enforce" がfalseの影実行と確定できる
  When guardianがblockに相当する入力を判定する
  Then 判定を影ログへ残しguardian由来の拒否を出さない
  And OpenCode本来の権限判断を維持する

@id=EX-081 @about=REQ-048,REQ-051 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A6,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23,docs/decision/records/2026-10-02-opencode-v2-hook.md#D1
Scenario: 無効な応答を影実行と誤認しない
  Given guardianの応答が無効である
  When プラグインがその応答を受け取る
  Then 影実行と見なして承認なしで実行しない

@id=EX-082 @about=REQ-049 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A4
Scenario: guardianのallowでOpenCodeの確認を外さない
  Given OpenCodeが確認を求めguardianがallowと判定する
  When エージェントが実行を要求する
  Then OpenCodeの確認を維持する

@id=EX-083 @about=REQ-049 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A7,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 自動承認でもblockは通さない
  Given "mode.enforce" がtrueである
  Given "run --auto" が有効である
  When guardianがblockと判定する
  Then 対象コマンドを実行しない

@id=EX-084 @about=REQ-050 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A7,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 明示的な自動承認を尊重する
  Given "mode.enforce" がtrueである
  Given "run --auto" が有効である
  When guardianのaskへの承認要求が自動承認される
  Then 人の確認を追加で強制せずその承認を受け入れる

@id=EX-085 @about=REQ-050 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A5,docs/decision/records/2026-10-02-opencode-v2-hook.md#A7,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 保存済み許可は現在のaskを省略しない
  Given "mode.enforce" がtrueである
  Given OpenCodeに保存済みの許可がある
  When guardianが今回の実行をaskと判定する
  Then 今回の承認要求を出し承認前に実行しない

@id=EX-086 @about=REQ-051 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A6
Scenario: guardianが見つからない理由を示して確認する
  Given guardianのバイナリが見つからない
  When エージェントがshell実行を要求する
  Then バイナリ未検出の理由を示して承認を求める

@id=EX-087 @about=REQ-051 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A6
Scenario: 判定のタイムアウトを黙って通さない
  Given guardianの呼出しがタイムアウトする
  When エージェントがshell実行を要求する
  Then タイムアウトの理由を示し承認なしで実行しない

@id=EX-088 @about=REQ-052 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A10
Scenario: 明示した同一サーバーで承認要求を作る
  Given プラグインをホストする同一HTTPサーバーへの接続と認証を明示設定している
  When 承認要求を作る
  Then その接続と認証を使う

@id=EX-089 @about=REQ-052 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A22
Scenario: 明示した接続の認証に失敗しても止めずに警告する
  Given 明示した接続設定でHTTPサーバーへの認証に失敗する
  When エージェントがshell実行を要求する
  Then 認証失敗を警告し、接続が無いときの扱いで判定と委任を行う

@id=EX-090 @about=REQ-053 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A14
Scenario: 独自期限で承認待ちを打ち切らない
  Given OpenCodeへの接続が継続し承認待ちである
  When 利用者がまだ承認も拒否もしていない
  Then プラグイン独自の承認期限では取りやめない

@id=EX-091 @about=REQ-053 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A14,docs/decision/records/2026-10-02-opencode-v2-hook.md#A21,docs/decision/records/2026-10-02-opencode-v2-hook.md#A22
Scenario: 接続断後の承認で再開しない
  Given 承認待ち中に通信エラーを検知して実行を取りやめた
  When 再接続後に残った確認表示へ承認が返る
  Then 取りやめた実行を再開しない

@id=EX-092 @about=REQ-054 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A5,docs/decision/records/2026-10-02-opencode-v2-hook.md#A18
Scenario: 並行した実行はそれぞれの承認を待つ
  Given 同じセッションで複数の実行が承認待ちである
  When 一つの実行だけが承認される
  Then 他の実行は承認待ちのままである

@id=EX-093 @about=REQ-054 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A5,docs/decision/records/2026-10-02-opencode-v2-hook.md#A14
Scenario: 取りやめた実行の承認を別の実行へ使わない
  Given 一つの実行が中断され別の実行が承認待ちである
  When 中断された実行へ後から承認が返る
  Then 別の実行をその承認で開始しない

@id=EX-094 @about=REQ-055 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A8,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 対応外のshellの理由を示して確認する
  Given 影実行と確定しておらずBash以外のshellで実行する入力である
  When プラグインが実行を受け付ける
  Then 対象外である理由を示して承認を求める

@id=EX-095 @about=REQ-055 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A8,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 対応外のshellを安全と見なさない
  Given 影実行と確定しておらずBash以外のshellで実行する入力である
  When その入力をBashとして解析できるように見える
  Then そのことを根拠に承認なしで実行しない

@id=EX-102 @about=REQ-047,REQ-054 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A24
Scenario: 同じ本文でも別のcwdの実行を取り違えない
  Given 同じコマンド本文で異なるcwdの実行が並行する
  When 省略または相対指定のcwdを使う実行への承認が返る
  Then その実行のcwdを判定と承認と起動で一致させる

@id=EX-103 @about=REQ-047 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A24
Scenario: 起動入力との一致を確認できなければ確認する
  Given 影実行と確定しておらず判定入力と起動入力の一致を確認できない
  When エージェントが実行を要求する
  Then 判定不能として承認を求める

@id=EX-104 @about=REQ-048 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 確定済みの影実行ではログ保存失敗を強制判定にしない
  Given 本体の応答で影実行と確定でき影ログの保存に失敗する
  When プラグインがその応答を受け取る
  Then 警告にとどめguardian由来の確認も拒否も出さない

@id=EX-105 @about=REQ-048,REQ-055 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 確定済みの影実行では対応外shellも観測にとどめる
  Given 本体の応答で影実行と確定でき対応外shellを使う
  When プラグインがその応答を受け取る
  Then 警告にとどめguardian由来の確認も拒否も出さない
```
