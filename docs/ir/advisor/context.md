# 文脈の出所と送信前の保護

当該実行に先行する限定された会話を取得し、出所、鮮度、秘密、サイズを検査する契約と、その保存を扱う。
危険性の合成は policy.md、時間の境界は runtime.md が扱う。

## Requirements

### REQ-advisor-006: 当該実行への出所の対応付け

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A26, docs/decision/records/2026-10-03-llm-advice-layer.md#A33, docs/decision/records/2026-10-03-llm-advice-layer.md#A39, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A48
- verification: unit

文脈取得はエージェントが管理する会話データを優先する。
取得アダプターは、ホストと版、ローカルのセッション識別、発言ID、発言順序、当該実行の前という境界、role、取得経路、既知の合成入力の識別情報を照合し、別セッション、実行後の発言、既知の合成されたuser入力を利用者の指示として扱わない。
対応する入力形式と版をfixtureで裏付けていない場合、欠落、読取失敗、セッションや実行境界の対応付け不能は文脈なしとし、モデルには利用者指示を取得できなかったことを渡す。
role=userは取得データの発言種別であって人間がその場で発言した証明ではなく、合成入力の識別が不完全な場合はその限界も明示する。
CLIのcheckには確認できる会話元がないため文脈なしとし、本文や環境から指示を創作しない。

### REQ-advisor-007: 送信範囲と会話の上限

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A6, docs/decision/records/2026-10-03-llm-advice-layer.md#A22, docs/decision/records/2026-10-03-llm-advice-layer.md#A27, docs/decision/records/2026-10-03-llm-advice-layer.md#A34, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A48
- verification: unit

送信対象はコマンド本文、cwd、機械判定の結果と理由、当該実行に先行する直近の利用者発言とエージェント返答の限定した文脈とする。
既定で直近3往復以内を古い順に扱い、件数は利用者が変更でき、0なら文脈なしとする。
1往復は一つのuser発言とそれに続くassistant返答であり、末尾の返答のないuser発言も1往復として数え、連続する同じroleを都合よく一発言へ連結して上限を迂回しない。
assistantの文章は参照先の理解にだけ使い、ツール呼び出し、ツール結果、ファイル本文を含む内容ブロックは除く。
除いた内容が指示範囲の参照先なら取得できたとは扱わない。
会話全体、ファイル内容、環境変数一覧、認証情報、ローカルのセッションIDや会話保存パス、設定一覧を送らない。
取得元の照合情報はローカルに残し、送信する出所情報はrole、相対的な順序、確認状態とその限界に限定する。

### REQ-advisor-008: 指示の鮮度と参照不足

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A25, docs/decision/records/2026-10-03-llm-advice-layer.md#A26, docs/decision/records/2026-10-03-llm-advice-layer.md#A27, docs/decision/records/2026-10-03-llm-advice-layer.md#A37, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A64
- verification: review
- how_to_verify: 実装と別コンテキストの独立担当が、実際に送る固定質問、指示範囲の分類定義、符号化後の文脈データと確認状態・限界の項目を本要求の本文へ照合する。固定質問の実装から送信本文の組立てまで追い、EX-advisor-015では窓内の撤回と後の限定指示を反映し古い全体破棄の指示を流用しないこと、EX-advisor-016では取得窓外の参照先を推測で埋めずmatchedとしないことを、質問と定義と実際に渡るデータの組合せで確認する。矛盾、別操作への切替え、窓外の制約の限界、TTLが許可の有効性を保証しないことも照合し、読んだ実装パス、照合した契約、二例ごとの判断根拠と未解決事項を記録する。文言一致や偽Assessmentによる合成成功を意味の妥当性の証拠にせず、出所・順序・窓・ブロック除外・TTL・合成の単体試験と区別し、このレビューを実モデルの判断精度の証明にしない

確認済み指示は対象と操作範囲が具体的で、当該実行に先行し、取得窓内で撤回、変更、別操作への切替えを確認したらそれを反映する。
「その案で」の参照先を取得できない場合、矛盾した指示を解消できない場合、別操作への古い許可しかない場合はscopeをmatchedとして使用しない。
上限窓の外の制約や撤回が見えない可能性を明示し、窓内に発言があることだけで承認範囲を完全に知ったとしない。
キャッシュのTTLは保存の有効期限であり、その時間内の指示が当該操作への有効な許可であることを保証しない。

### REQ-advisor-009: 秘密とサイズと符号化による見送り

- kind: event_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A7, docs/decision/records/2026-10-03-llm-advice-layer.md#A34, docs/decision/records/2026-10-03-llm-advice-layer.md#A38, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A49, docs/decision/records/2026-10-02-ir-friction-contracts.md#A4
- verification: unit

送信前に本文、cwd、理由、文脈、モデル名を含む最終送信本文全体を秘密検査し、秘密らしい値を検出したら1回も外部送信せず機械判定を維持する。
認証ヘッダーはこの状態本文とは別にアダプター内で付け、検査済み状態へ追加しない。
最終的なHTTP要求のJSON本文をUTF-8で符号化したバイト長を測り、固定質問とモデル名も含めて上限65536バイトの既定または設定値を超えたら見送る。
上限超過を本文や指示の切り詰めで隠さず、必要なフィールドを符号化できない場合も見送る。
非UTF-8のcwdは機械判定では受理し、その追加送信を見送るだけとする。
秘密検出が完全である保証は付けない。

### REQ-advisor-010: 必要な経路だけのセッションキャッシュ

- kind: state_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A39, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A50
- verification: unit

ホストの会話データを直接利用できる経路では専用キャッシュを作らず、別hook間の取得に必要な経路だけセッションごとの限定窓と照合情報を保存する。
保存先は既存のstate領域の"command-guardian/advisor-context"とし、セッション名をそのままパス要素にせず、所有者を確認した0700のディレクトリと0600の通常ファイルを用い、symlinkと複数hardlinkを拒否する。
セッションの不一致、壊れた版や枠、不明な保存時刻、現在より未来の保存時刻、保存時点から既定24時間または設定TTL以上の経過では利用せず、期限切れは次回アクセスで削除する。
削除や保存に失敗しても古い文脈へ復帰せず文脈なしとし、処理は助言期限で打ち切る。

### REQ-advisor-011: 助言ログと影ログの分離

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A29, docs/decision/records/2026-10-03-llm-advice-layer.md#A38, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A45, docs/decision/records/2026-10-03-llm-advice-layer.md#A51, docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-10-02-ir-friction-contracts.md#A2
- verification: unit

助言の観測と介入はstate領域の"command-guardian/advisor.jsonl"へ記録し、既定では時刻、モード、機械判定、候補と最終判定、分類された理由、処理時間、失敗または見送り種別、モデル名だけを残す。
理由はguardianの列挙型の分類とし、モデルの自由文、HTTPエラー本文、コマンド、cwd、文脈、発言やセッションの識別子を通常ログへ含めない。
モデル名を含むメタデータでも検出した秘密を伏せる。
"advisor.debug_text"を利用者が明示した場合だけ送信候補本文の記録を許し、検出した秘密を伏せ、認証情報と生の通信エラーを記録しない。
保存権限とsymlink拒否はREQ-advisor-010の規則を使い、保存失敗は内容を含まない警告で継続し、判定を変更しない。
本文ログは秘密検出の限界から機密を含み得るものとして扱う。
REQ-019の影ログの保存条件と本文の契約は変更しない。

## Decision tables

### TBL-advisor-003: 文脈取得の確認状況

- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A33, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A48, docs/decision/records/2026-10-03-llm-advice-layer.md#A56, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-02-opencode-v2-hook.md#A17

| エージェント | リポジトリで確認した現在の入口 | この草稿時点の会話対応 | 採用条件 |
|---|---|---|---|
| Claude Code | src/hook.rsはcommand、cwd、permission_modeを読む | 会話形式と発言対応は未検証。文脈なし | ホストの会話元とセッションと実行前境界を対応版fixtureで確認してから採用 |
| Codex | src/hook.rsの同じ入力抽出を使う | 会話形式と発言対応は未検証。文脈なし | 合成入力の既知の印も含む対応版fixtureで確認してから採用 |
| OpenCode V2 2.0.21 | pluginはToolContextのsessionIDを持つがguardianへのInvocationはcommand、cwd、shellだけ | セッション内の会話と実行境界の写像は未検証。文脈なし | 同一ホストの会話元とToolContextをfixtureで対応付け、取得も助言期限内で行えることを確認してから採用 |
| それ以外の版または直接check | 確認できる会話元を宣言していない | 文脈なし | 対応表の根拠を追加するまでは採用しない |

## Examples

```gherkin
@id=EX-advisor-011 @about=REQ-advisor-006 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A44
Scenario: 対応確認済みのホストデータから取得する
  Given 対応版のfixtureで当該セッションと実行前の発言対応を検証した取得経路がある
  When 当該実行の文脈を取得する
  Then 同じセッションの実行前の発言だけを出所の限界付きで採用する

@id=EX-advisor-012 @about=REQ-advisor-006 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A33,docs/decision/records/2026-10-03-llm-advice-layer.md#A44
Scenario: userというroleだけでは出所を確認しない
  Given 入力のroleはuserだが既知の合成入力か実行への対応付けが不明である
  When 文脈を取得する
  Then 利用者の指示とせず文脈なしを明示する

@id=EX-advisor-013 @about=REQ-advisor-007 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A27
Scenario: 短い指示の参照先として返答を含める
  Given 直近3往復の中に対象を具体化したassistant返答とその案でというuser発言がある
  When 当該実行に関係する文脈を組み立てる
  Then roleを区別した必要な参照先を含め上限を超えない

@id=EX-advisor-014 @about=REQ-advisor-007 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A27,docs/decision/records/2026-10-03-llm-advice-layer.md#A48
Scenario: ツール結果を会話文脈に混ぜない
  Given 取得したメッセージにはツール結果とファイル本文のブロックが含まれる
  When 送信候補を組み立てる
  Then それらのブロックとローカルの保存パスを送信候補へ含めない

@id=EX-advisor-015 @about=REQ-advisor-008 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A26,docs/decision/records/2026-10-03-llm-advice-layer.md#A64
Scenario: 撤回後の指示で対象を確認する
  Given 取得窓内でDB全体破棄の指示を撤回し特定ユーザだけの削除へ変更した
  When 当該実行の指示範囲を質問する
  Then 後の限定指示を反映し古い全体破棄の指示を流用しない

@id=EX-advisor-016 @about=REQ-advisor-008 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A27,docs/decision/records/2026-10-03-llm-advice-layer.md#A64
Scenario: 窓外の参照を推測で埋めない
  Given 最新のuser発言はそれで進めてだが参照先は取得窓外である
  When 指示との一致を評価する
  Then matchedとして使用せず文脈不足を明示する

@id=EX-advisor-017 @about=REQ-advisor-009 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A34,docs/decision/records/2026-10-03-llm-advice-layer.md#A49
Scenario: 全送信本文が上限内なら一度だけ送れる
  Given 全フィールドの検査を通過し最終HTTP本文は65536バイトである
  When 助言期限内に送信する
  Then 上限内として最大1回の送信を許す

@id=EX-advisor-018 @about=REQ-advisor-009 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A7,docs/decision/records/2026-10-03-llm-advice-layer.md#A34,docs/decision/records/2026-10-03-llm-advice-layer.md#A49,docs/decision/records/2026-10-02-ir-friction-contracts.md#A4
Scenario: 秘密や超過や符号化不能は機械判定へ戻る
  Given 秘密を検出するか最終HTTP本文が65537バイトかcwdをUTF-8で符号化できない
  When 助言処理を行う
  Then 外部送信せず機械判定を維持し本文を切り詰めない

@id=EX-advisor-019 @about=REQ-advisor-010 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A39
Scenario: 必要なキャッシュは同じセッションだけで使う
  Given 別hook間の取得に必要な限定窓を所有者限定のファイルへ保存した
  When TTL内に同じセッションからアクセスする
  Then 出所と鮮度を再検査して限定窓を使う

@id=EX-advisor-020 @about=REQ-advisor-010 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A39,docs/decision/records/2026-10-03-llm-advice-layer.md#A44
Scenario: 期限切れを保存失敗で復活させない
  Given 保存から24時間が経過したか別セッションか保存先がsymlinkである
  When キャッシュへアクセスする
  Then 保存内容を利用せず期限切れは削除を試みる
  And 削除失敗でも期限切れの指示を採用しない

@id=EX-advisor-021 @about=REQ-advisor-011 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A29,docs/decision/records/2026-10-03-llm-advice-layer.md#A45
Scenario: 観測ログには本文を残さない
  Given debug_textはfalseで助言候補を得た
  When advisor.jsonlを記録する
  Then 判定と分類と時間だけを含めコマンドとcwdと文脈は含めない
  And 既存の影ログは別契約のままである

@id=EX-advisor-022 @about=REQ-advisor-011 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A38,docs/decision/records/2026-10-03-llm-advice-layer.md#A42
Scenario: デバッグ記録でも検出した秘密は伏せる
  Given debug_textを明示したが送信候補本文に秘密を検出した
  When 調査用ログを記録する
  Then 検出値を伏せ認証ヘッダーと通信エラー本文を残さない
```
