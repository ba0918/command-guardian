# モデル境界と利用者設定

guardian側の質問と最終判定を維持したまま、コンパイル済みのTypeSafe接続へ評価を依頼する型、検証、設定を扱う。
接続先固有の表現はアダプター内に閉じ、外部実行プラグインや動的ライブラリを導入しない。

## Requirements

### REQ-advisor-012: 現在の利用と試験に必要なモデル境界

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A30, docs/decision/records/2026-10-03-llm-advice-layer.md#A32, docs/decision/records/2026-10-03-llm-advice-layer.md#A43, docs/decision/records/2026-10-03-llm-advice-layer.md#A54
- verification: unit

"AdvisorClient::assess"は、guardianが定義した質問と検査済み状態、利用者のモデル名、残期限を受け取り、Assessmentまたは分類済み失敗を返す。
失敗は認証、通信、期限、サービス応答、不正応答として表し、生のレスポンスや認証情報を呼出元の理由へ渡さない。
このtraitの利用者はTypeSafe接続クレートと、失敗、遅延、不正応答を制御する試験用実装である。
TypeSafeのHTTP型、質問型、confidenceや未知フィールドを領域型へ漏らさず、参考実装の汎用Judgeやguardrailのverdictを公開契約として採用しない。
プロバイダー追加は再ビルドを必要とし、単一バイナリへ静的に組み込む。

### REQ-advisor-013: 分布の境界検証

- kind: event_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A8, docs/decision/records/2026-10-03-llm-advice-layer.md#A35, docs/decision/records/2026-10-03-llm-advice-layer.md#A37, docs/decision/records/2026-10-03-llm-advice-layer.md#A40, docs/decision/records/2026-10-03-llm-advice-layer.md#A44
- verification: unit

Assessmentを採用する前に、riskとscopeの両分布について定義済みラベルが各1回だけ現れ、欠落や余分なラベルがなく、全値が有限かつ0以上1以下で、和と1との差が0.000001以下であることを共通の境界検証で確かめる。
選択ラベルは分布内の一意な最大値であることを必要とし、同率最大、選択の不一致、重複キー、不正数値、未知の応答種別を不正応答として扱う。
正規化して不正分布を修復せず、providerのconfidenceではなく選択ラベルの分布確率を介入閾値と比較する。
scopeが形式上妥当なmatchedでも、確認済み指示がない場合はguardianがmatchedを使用しない。

### REQ-advisor-014: TypeSafeへの一要求

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A30, docs/decision/records/2026-10-03-llm-advice-layer.md#A31, docs/decision/records/2026-10-03-llm-advice-layer.md#A32, docs/decision/records/2026-10-03-llm-advice-layer.md#A34, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A43, docs/decision/records/2026-10-03-llm-advice-layer.md#A54, docs/decision/records/2026-10-03-llm-advice-layer.md#A61
- verification: unit

初期アダプターは"https://api.typesafe.ai/v1/systemone"へのHTTPS POSTを最大1回行い、guardianの状態とriskとscopeの2つのchoice質問を同じ要求へまとめる。
既定のモデル名は"jev-latest"とし利用者の指定をそのまま使い、別モデルへの自動切替え、モデル一覧の追加取得、通信やHTTPエラーの再試行、リダイレクト追従を行わない。
HTTPとTLSにはureqを採用し、残期限内の通信期限と65536バイトの応答上限を設け、それだけで親の強制打切りを代用しない。
認証値は"TYPESAFE_API_KEY"からアダプターが取得し、欠落は認証失敗とする。
質問と合成はguardianが所有し、ba-toysの変更は要求しない。
ここでのTypeSafe形式は参考実装の構造に基づく仕様であり、現在の実サービスとの適合を実呼出しで検証したという意味ではない。

### REQ-advisor-015: 利用者だけが助言を設定する

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A18, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A46
- verification: unit

advisorの設定は組み込み既定と利用者ファイルからだけ取り、プロジェクト設定のadvisor節は信頼済みプロジェクトでも全キーを無視して警告する。
プロジェクトファイルはTOML構文を解析した後、advisor固有の型、値、未知キーの検証前にトップレベルのadvisor要素全体を除去する。
無視する要素が不正な型、値、範囲を持っていても、そのために他の正常な保護規則を不採用にしない。
既存の3層設定の一般キーの契約は維持し、advisorだけは後段のプロジェクトマージへ渡さない。
modeは"off"、"observe"、"enforce"のいずれかとし、observeを導入時の推奨とするが無指定時の既定はoffである。
利用者ファイルのadvisorの値の不正、型の不正、時間や長さの換算オーバーフローは一般設定誤りとしてREQ-015の設定ファイル全体不採用を使う。
どちらの層でもTOML構文エラーと読取失敗、実際に適用する一般設定の不正にはREQ-015を適用する。
新しい公開CLIフラグ、provider選択スイッチ、実行ファイル指定、任意URL指定は追加しない。

### REQ-advisor-016: 設定可能な既定と不正値

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A15, docs/decision/records/2026-10-03-llm-advice-layer.md#A27, docs/decision/records/2026-10-03-llm-advice-layer.md#A34, docs/decision/records/2026-10-03-llm-advice-layer.md#A35, docs/decision/records/2026-10-03-llm-advice-layer.md#A36, docs/decision/records/2026-10-03-llm-advice-layer.md#A39, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A54
- verification: unit

実際に採用するadvisor設定の既定と値域はTBL-advisor-002に従い、介入閾値は全分岐で同じ設定値を使う。
モデル名は非空の文字列で制御文字を拒否し、時間と量は正整数、往復数は0以上の整数、閾値は0超1以下の有限値とする。
設定値が表現可能な期限や長さを超える場合は設定誤りとし、黙って上限を縮めない。
再試行は0、要求数は最大1で固定し、日次や月次の予算管理をguardianへ追加しない。

## Decision tables

### TBL-advisor-002: 利用者ファイルのadvisor節

- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A47, docs/decision/records/2026-10-03-llm-advice-layer.md#A49, docs/decision/records/2026-10-03-llm-advice-layer.md#A38, docs/decision/records/2026-10-03-llm-advice-layer.md#A45

| キー | 既定 | 値の意味 |
|---|---|---|
| "advisor.mode" | "off" | off、observe、enforce |
| "advisor.model" | "jev-latest" | 接続先のモデル名 |
| "advisor.timeout_ms" | 2000 | 文脈取得を含む助言期限のミリ秒 |
| "advisor.max_request_bytes" | 65536 | 最終HTTP JSON本文全体のUTF-8バイト数 |
| "advisor.intervention_threshold" | 0.9 | 必要な選択確率の暫定閾値 |
| "advisor.context_exchanges" | 3 | 当該実行前の取得往復数。0で文脈なし |
| "advisor.context_ttl_hours" | 24 | 必要な専用キャッシュの保存期限 |
| "advisor.debug_text" | false | 検出秘密を伏せた調査用本文記録 |

## Examples

```gherkin
@id=EX-advisor-023 @about=REQ-advisor-012 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A43,docs/decision/records/2026-10-03-llm-advice-layer.md#A8
Scenario: 偽接続で通信失敗を制御できる
  Given 試験用実装が通信失敗を返す
  When guardianがAdvisorClientの境界から評価を受け取る
  Then provider固有の型を使わず失敗を扱い機械判定を維持する

@id=EX-advisor-024 @about=REQ-advisor-012 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A30
Scenario: 外部判定プラグインを実行しない
  Given 外部実行ファイルが最終verdictを返せる
  When 初期の追加判定経路を調べる
  Then その実行ファイルや動的ライブラリを読み込む経路はない

@id=EX-advisor-025 @about=REQ-advisor-013 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A40
Scenario: 正しい分布だけが合成へ進む
  Given riskとscopeに定義済みの全ラベルが各1回あり和は1で選択は一意な最大値である
  When 境界検証する
  Then 各選択ラベルの確率を合成へ渡す

@id=EX-advisor-026 @about=REQ-advisor-013 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A40,docs/decision/records/2026-10-03-llm-advice-layer.md#A8
Scenario: 不正分布を確信のある回答として採用しない
  Given 分布に欠落か重複かNaNか負値か和の超過か同率最大か未知ラベルがある
  When 境界検証する
  Then 不正応答とし正規化せず機械判定を維持する

@id=EX-advisor-027 @about=REQ-advisor-014 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A43
Scenario: 二つの質問を一つの要求で送る
  Given 検査済みの状態とモデル指定と有効な認証がある
  When 試験用HTTP接続へ要求を送る
  Then riskとscopeを同じPOSTに含めguardian側へAssessmentを返す

@id=EX-advisor-028 @about=REQ-advisor-014 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A34,docs/decision/records/2026-10-03-llm-advice-layer.md#A43,docs/decision/records/2026-10-03-llm-advice-layer.md#A8
Scenario: エラーや転送で要求数を増やさない
  Given HTTP 429かHTTPリダイレクトか通信失敗を受け取る
  When TypeSafe接続が失敗を扱う
  Then 再試行も転送先への送信もせず機械判定を維持する

@id=EX-advisor-029 @about=REQ-advisor-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A18
Scenario: 利用者が観測を明示する
  Given 利用者ファイルでadvisor.modeをobserveにしている
  When 設定を読む
  Then 観測モードを採用し無指定時だけoffを使う

@id=EX-advisor-030 @about=REQ-advisor-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A46
Scenario: 無視する不正な助言値でプロジェクトの保護を失わない
  Given 構文が正しいプロジェクトTOMLに正常な保護ルートとadvisorの型不正またはtimeout_msが0の指定がある
  When 信頼済みまたは未信頼のプロジェクト設定を読む
  Then advisor要素を固有検証前に捨てて警告し正常な保護ルートを採用する
  And 利用者の助言設定を維持する

@id=EX-advisor-043 @about=REQ-advisor-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A46
Scenario: 無視する助言節でもTOML構文エラーは無視できない
  Given プロジェクトTOMLのadvisor要素の記述で引用符が閉じていない
  When 設定を読む
  Then TOML構文エラーとしてそのファイル全体を不採用にし警告する

@id=EX-advisor-044 @about=REQ-advisor-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A46
Scenario: 実際に適用する設定の不正は残さない
  Given プロジェクトのadvisor要素以外の一般設定が不正か利用者ファイルのadvisor.timeout_msが0である
  When 設定を読む
  Then 不正のあるファイル全体を不採用にしそのファイルの正常部分も残さない

@id=EX-advisor-031 @about=REQ-advisor-016 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A42
Scenario: 必要な値だけを利用者が変更する
  Given 利用者がtimeout_msを10000、context_exchangesを0、modelを任意の非空名へ変更した
  When 設定を読む
  Then 助言期限は10秒で文脈なしとなり指定モデルを使う

@id=EX-advisor-032 @about=REQ-advisor-016 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A42,docs/decision/records/2026-10-02-ir-friction-contracts.md#A1
Scenario: 不正値を部分採用しない
  Given 利用者ファイルに0のtimeout_msか負の往復数か1超の閾値か時間換算のオーバーフローがある
  When 設定を読む
  Then その設定ファイル全体を不採用にし警告して既定と他の正常な層で続ける
```
