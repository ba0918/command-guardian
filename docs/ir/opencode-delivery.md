# OpenCode V2 連携の導入と配布

TypeScript プラグインの配布、導入条件、検証範囲を扱う。実行前の判定と承認は opencode.md、既存バイナリの配布は release.md が扱う。

## Requirements

### REQ-056: プラグインの配布と登録

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A9, docs/decision/records/2026-10-02-opencode-v2-hook.md#A12, docs/decision/records/2026-10-02-opencode-v2-hook.md#A13
- verification: review
- how_to_verify: リリース成果物にTypeScriptプラグインが含まれ、本体と同じ版で更新されることを確かめる。導入手順を読んでプラグインの配置とグローバル登録を再現し、個人設定の自動変更とnpmへの別公開を前提にしないことを確認する。

TypeScript で実装したプラグインを guardian のリリースに同梱し、本体と同じ版で更新する。npm への別公開は行わない。利用者が OpenCode のグローバル設定へプラグインを登録する配置・登録手順を用意し、今回の作業で個人設定を自動変更しない。

### REQ-057: 対応条件と保証の限界

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A8, docs/decision/records/2026-10-03-opencode-managed-service.md#A3, docs/decision/records/2026-10-03-opencode-managed-service.md#A2, docs/decision/records/2026-10-02-opencode-v2-hook.md#A17, docs/decision/records/2026-10-02-opencode-v2-hook.md#A20, docs/decision/records/2026-10-02-opencode-v2-hook.md#A21, docs/decision/records/2026-10-02-opencode-v2-hook.md#A22, docs/decision/records/2026-10-04-opencode-standalone.md#A5, docs/decision/records/2026-10-04-opencode-standalone.md#A18
- verification: review
- how_to_verify: 導入手順でLinux x86_64・WSL、Bashの明示設定、入力を変更する他のhookがない条件、同一HTTPサーバーへの接続・認証設定、プラグインが頼るフックと文脈のAPIの一覧を確認する。OpenCodeの版を対応の条件にしていないこと、無応答の即時検知、競合時の確認表示の完全な後始末を保証していないことを確かめる。

対応条件は Linux x86_64・WSL 上で Bash を明示設定し、他の hook が実行コマンド・cwd・shell を変更しない環境である。自身の管理サービスへの自動接続と、それ以外の同一 HTTP サーバーへの明示的な接続・認証設定を導入手順に示す。対応の範囲は OpenCode の版ではなく、プラグインが頼るプラグインの仕様（使うフックと文脈の API）で示し、導入手順にその一覧を書く。結合試験に使う OpenCode の固定版は試験の再現のための環境として扱う。任意の hook の変更後の最終入力の判定、無応答状態の即時検知、要求作成と中断の競合時の確認表示の完全な後始末は保証に含めない。

接続が無いときの対応条件は opencode-standalone.md の REQ-068 に従う。

### REQ-058: 隔離した連携検証

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-opencode-v2-hook.md#A18, docs/decision/records/2026-10-04-opencode-standalone.md#A8
- verification: review
- how_to_verify: モデルを呼ばない自動試験と隔離V2サーバーでの承認API連携試験の実行結果を確認する。実バイナリの判定、承認前の未実行、拒否・中断・並行実行を検証し、普段のOpenCode設定とセッションを変更していないことを確認する。モデルを呼ぶ実使用試験を必須のゲートにしない。

モデルを呼ばない自動試験と、隔離した V2 サーバーでの承認 API 連携試験を必須とする。実バイナリの判定、承認前に実行されないこと、拒否・中断・並行実行を検証する。普段の OpenCode 設定とセッションは変更しない。モデルを使う実使用試験は任意とする。接続が無いときの検証は opencode-standalone.md の REQ-069 に従う。

### REQ-070: OpenCode の版に依存しない動作

- kind: ubiquitous
- source: docs/decision/records/2026-10-04-opencode-standalone.md#A16, docs/decision/records/2026-10-04-opencode-standalone.md#A20
- verification: unit

プラグインは OpenCode の版を実行時に確かめず、版が結合試験の固定版と違うことを理由に例外を出したり実行を止めたり警告を出したりしない。読み込み時には頼るフックと文脈の API がそろっているかを確かめ、足りなければ足りないものの名前を示して読み込みを失敗させる。読み込みに失敗した後の OpenCode の shell の扱いは保証しない。

## Examples

```gherkin
@id=EX-096 @about=REQ-056 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A9,docs/decision/records/2026-10-02-opencode-v2-hook.md#A12,docs/decision/records/2026-10-02-opencode-v2-hook.md#A13
Scenario: 本体と同梱したプラグインを利用者が登録する
  Given guardianのリリースに同じ版のTypeScriptプラグインが含まれる
  When 利用者が配置とグローバル登録の手順に従う
  Then npmへの別公開を必要とせずプラグインを導入できる

@id=EX-097 @about=REQ-056 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A9
Scenario: 個人設定の自動変更を導入の前提にしない
  When プラグインの導入手順と今回の作業を確認する
  Then 個人のOpenCode設定を勝手に変更していない

@id=EX-098 @about=REQ-057 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A8,docs/decision/records/2026-10-02-opencode-v2-hook.md#A10,docs/decision/records/2026-10-02-opencode-v2-hook.md#A20,docs/decision/records/2026-10-04-opencode-standalone.md#A18
Scenario: 対応の範囲と導入条件を明記する
  When 導入手順を読む
  Then プラグインが頼るフックと文脈のAPIの一覧とLinux x86_64・WSLのBash明示設定と入力を変更する他のhookがない条件を確認できる
  And 同一HTTPサーバーへの接続と認証の設定を確認できる

@id=EX-099 @about=REQ-057 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A18,docs/decision/records/2026-10-02-opencode-v2-hook.md#A20,docs/decision/records/2026-10-02-opencode-v2-hook.md#A21,docs/decision/records/2026-10-02-opencode-v2-hook.md#A22
Scenario: 未検証の互換性と保証できない境界を明記する
  When 導入手順を読む
  Then OpenCodeの版を対応の条件にしていない
  And 任意のhookの変更後の最終入力の判定と無応答の即時検知と競合時の確認表示の完全な後始末を保証していない

@id=EX-100 @about=REQ-058 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A18
Scenario: モデルを呼ばず承認の連携を検証する
  When 自動試験と隔離V2サーバーでの承認API連携試験を実行する
  Then 実バイナリの判定と承認前の未実行と拒否と中断と並行実行の証拠を確認できる

@id=EX-101 @about=REQ-058 @source=docs/decision/records/2026-10-02-opencode-v2-hook.md#A18
Scenario: 普段のセッションを検証に使わない
  When 必須の連携検証を実行する
  Then 普段のOpenCode設定とセッションを変更しない
  And モデルを呼ぶ実使用試験を必須にしない

@id=EX-144 @about=REQ-070 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A16
Scenario: 結合試験と違う版の OpenCode でも判定どおりに動く
  Given OpenCode が報告する版が結合試験の固定版と異なる
  When エージェントが allow になるコマンドを実行する
  Then 版を理由に例外や警告を出さず、コマンドが実行される

@id=EX-145 @about=REQ-070 @source=docs/decision/records/2026-10-04-opencode-standalone.md#A20
Scenario: 頼る API が無ければ名前を示して読み込みを失敗させる
  Given OpenCode のプラグインの文脈に shell のフックの API が無い
  When プラグインを読み込む
  Then 足りない API の名前を示して読み込みが失敗する
```
