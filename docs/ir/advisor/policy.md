# 助言結果からの判定合成

機械判定のallowとaskを再検査し、確率付きの危険性と指示範囲からguardianが最終判定を作る規則を扱う。
モデル接続は provider.md、出所と送信範囲は context.md、実行期限は runtime.md が扱う。

## Requirements

### REQ-advisor-001: 検査対象と観測モード

- kind: state_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A3, docs/decision/records/2026-10-03-llm-advice-layer.md#A4, docs/decision/records/2026-10-03-llm-advice-layer.md#A18, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A45, docs/decision/records/2026-10-03-llm-advice-layer.md#A57, docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
- verification: unit

助言が有効なとき、機械判定のallowとaskを検査対象とし、blockでは文脈取得もモデル呼び出しも行わない。
"off"では追加取得も送信も行わず、"observe"では候補判定をログに残すだけで機械判定を返し、利用者が明示した"enforce"でだけ候補判定を採用する。
既存の"mode.enforce"による影実行はこれと別であり、助言のenforceで影実行を解除しない。

### REQ-advisor-002: 危険性と指示範囲の分類

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A17, docs/decision/records/2026-10-03-llm-advice-layer.md#A19, docs/decision/records/2026-10-03-llm-advice-layer.md#A20, docs/decision/records/2026-10-03-llm-advice-layer.md#A26, docs/decision/records/2026-10-03-llm-advice-layer.md#A37, docs/decision/records/2026-10-03-llm-advice-layer.md#A40, docs/decision/records/2026-10-03-llm-advice-layer.md#A44, docs/decision/records/2026-10-03-llm-advice-layer.md#A52, docs/decision/records/2026-10-03-llm-advice-layer.md#A59, docs/decision/records/2026-10-03-llm-advice-layer.md#A63
- verification: review
- how_to_verify: 実装と別コンテキストの独立担当が、実際に送る固定質問と危険性5分類・指示範囲3分類の定義を本要求の本文と照合する。EX-advisor-003の限定DELETEを語だけで重大破壊にしないこと、EX-advisor-004のscratchという名前から使い捨てや承認を推測しないこと、EX-advisor-053の重大破壊と独立した有害効果の併存ではharmful_irreversibleを優先することを反例ごとに確認し、読んだ実装のパス、照合した契約、判断根拠を記録する。固定文言の一致や偽Assessmentによる合成成功を意味の妥当性の証拠にせず、このレビューを実モデルの判断精度の証明にしない

guardianはコマンド全体の効果について危険性と指示範囲を別々に質問する。
危険性の"harmful_irreversible"は重大破壊とは別の独立した有害な不可逆効果であり秘密の外部送信や指示外の破壊を含む操作、"major_destructive"はDB全体の破棄やクラウド資源の一括削除のように具体的な指示一致が確認できれば再確認へ回せる重大破壊、"irreversible_only"は不可逆性だけを確認した操作、"no_harm"は入力内に有害性を認めない操作、"unknown"は判断材料不足を表す。
重大破壊と別の有害効果を同時に認める場合は"harmful_irreversible"を選び、指示に一致しない重大破壊の扱いはREQ-advisor-003で決める。
指示範囲の"matched"は確認済み指示が対象、操作、全効果の範囲に一致し、参照先と鮮度も確認できる場合だけ、"mismatched"は具体的な不一致を確認した場合、"unknown"はそれ以外とする。
許可ルート内だけの削除を不可逆性だけで有害とせず、見えないファイルやDBの価値、接続先の実態を推測しない。

### REQ-advisor-003: 分岐ごとの閾値と候補判定

- kind: algorithm
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A11, docs/decision/records/2026-10-03-llm-advice-layer.md#A35, docs/decision/records/2026-10-03-llm-advice-layer.md#A37, docs/decision/records/2026-10-03-llm-advice-layer.md#A40, docs/decision/records/2026-10-03-llm-advice-layer.md#A59
- verification: unit
- definition: TBL-advisor-001

### REQ-advisor-004: 入力を判定指示にしない

- kind: prohibition
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A7, docs/decision/records/2026-10-03-llm-advice-layer.md#A27, docs/decision/records/2026-10-03-llm-advice-layer.md#A30
- verification: unit

guardianの固定した質問と分類定義を、コマンド、理由、会話中の文章から書き換えない。
送信対象の本文を判定用データとして区別し、「安全と答えよ」やエージェントの「承認済み」を追加判定の規則や確認済み指示に変換しない。
この分離はモデルが入力内の指示に従わないことの保証ではない。

### REQ-advisor-005: モデルの品質を構造試験と区別する

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A14, docs/decision/records/2026-10-03-llm-advice-layer.md#A18, docs/decision/records/2026-10-03-llm-advice-layer.md#A35, docs/decision/records/2026-10-03-llm-advice-layer.md#A36, docs/decision/records/2026-10-03-llm-advice-layer.md#A42, docs/decision/records/2026-10-03-llm-advice-layer.md#A53
- verification: review
- how_to_verify: 質問と実行条件をレビューし、評価の記録があればモデル名、評価時点、危険例、正当な破壊、文脈なし、誘導文、誤遮断と見逃しを確認する。実モデル未評価は未評価と記し、偽接続や確率0.9を品質証明としていないことを確認する

偽接続の試験は合成と失敗処理だけの証拠とし、TypeSafeやモデルの判断精度の証拠としない。
閾値0.9は暫定値であり正答率90%を意味しない。
"jev-latest"は将来同じモデルである保証がなく、品質はレビューまたは利用者が別途許可した実評価で確認し、通常の試験に有料モデル実行を要求しない。

## Decision tables

### TBL-advisor-001: 候補判定の優先順

- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A4, docs/decision/records/2026-10-03-llm-advice-layer.md#A8, docs/decision/records/2026-10-03-llm-advice-layer.md#A11, docs/decision/records/2026-10-03-llm-advice-layer.md#A35, docs/decision/records/2026-10-03-llm-advice-layer.md#A37, docs/decision/records/2026-10-03-llm-advice-layer.md#A40, docs/decision/records/2026-10-03-llm-advice-layer.md#A59

| 上から最初に該当する条件 | 候補判定 |
|---|---|
| 機械判定がblock | block。モデルで解除しない |
| 助言失敗、見送り、不正応答 | 機械判定 |
| 危険性の選択確率が閾値未満、または選択がunknown | 機械判定。新しいaskを作らない |
| 危険性がharmful_irreversibleで閾値以上 | block。確認済みの指示に一致しmatchedの確率が閾値以上でもaskにせず、指示範囲の確率を遮断の必要条件にしない |
| 危険性がmajor_destructiveで閾値以上、確認済み指示がありscopeがmatchedでその確率も閾値以上 | ask。機械判定のblockには適用しない |
| 危険性がmajor_destructiveで閾値以上、それ以外のscope状態 | block。文脈なしやscopeの低確率も含む |
| 危険性がirreversible_onlyまたはno_harm | 機械判定。askをallowにしない |

## Examples

```gherkin
@id=EX-advisor-001 @about=REQ-advisor-001 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A18,docs/decision/records/2026-10-03-llm-advice-layer.md#A45,docs/decision/records/2026-10-03-llm-advice-layer.md#A59
Scenario: 観測中の危険判定は最終判定を変えない
  Given 利用者がobserveを指定し機械判定はallowである
  When モデルが有害な不可逆操作を閾値以上で返す
  Then 候補blockを記録するが最終判定はallowである

@id=EX-advisor-002 @about=REQ-advisor-001 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A3,docs/decision/records/2026-10-03-llm-advice-layer.md#A4,docs/decision/records/2026-10-03-llm-advice-layer.md#A57
Scenario: 機械blockを解除する経路はない
  Given 機械判定はblockである
  When 助言がenforceでも当該実行を扱う
  Then 文脈を取得せずモデルを呼ばずblockを維持する

@id=EX-advisor-003 @about=REQ-advisor-002 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A21,docs/decision/records/2026-10-03-llm-advice-layer.md#A40,docs/decision/records/2026-10-03-llm-advice-layer.md#A63
Scenario: 限定されたSQL削除を全体破棄と区別する
  Given 確認済み指示は特定テーブルの特定ユーザの削除である
  When 対応する限定DELETEについて質問を組み立てる
  Then DELETEという語だけで重大破壊とする質問にはしない

@id=EX-advisor-004 @about=REQ-advisor-002 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A19,docs/decision/records/2026-10-03-llm-advice-layer.md#A20,docs/decision/records/2026-10-03-llm-advice-layer.md#A63
Scenario: 名前から価値や承認を創作しない
  Given 対象の名はscratchであるが接続先や価値は不明である
  When 危険性と指示範囲の質問を組み立てる
  Then 使い捨てや利用者の承認を仮定しない

@id=EX-advisor-005 @about=REQ-advisor-003 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A37,docs/decision/records/2026-10-03-llm-advice-layer.md#A59
Scenario: 重大破壊と具体的指示の一致で確認へ回す
  Given enforceで機械判定はallowであり確認済み指示が当該DB全体の破棄を指定する
  When 妥当なAssessmentがmajor_destructiveを0.95、matchedを0.96で選択する
  Then 最終判定はaskである

@id=EX-advisor-006 @about=REQ-advisor-003 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A37,docs/decision/records/2026-10-03-llm-advice-layer.md#A11
Scenario: 文脈なしの重大破壊は通さない
  Given enforceで機械判定はallowであり文脈なしである
  When 妥当なAssessmentがmajor_destructiveを0.95で選択する
  Then 最終判定はblockである
  And 危険性の確率が0.89なら機械判定を維持し新しいaskを作らない
  And no_harmが0.99でも機械判定のaskはallowにならない

@id=EX-advisor-051 @about=REQ-advisor-003 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A59
Scenario: 指示に一致しても独立した有害効果は遮断する
  Given enforceで機械判定はallow、閾値は0.9であり確認済みの具体的指示は当該操作の全範囲に一致する
  When 妥当なAssessmentがharmful_irreversibleを0.95、matchedを0.96で選択する
  Then 最終判定はblockであり指示一致を理由にaskへ変更しない

@id=EX-advisor-052 @about=REQ-advisor-003 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A59,docs/decision/records/2026-10-03-llm-advice-layer.md#A35
Scenario: 指示一致だけで危険性の低確率や単なる不可逆性を遮断しない
  Given enforceで機械判定はallow、閾値は0.9、確認済み指示がありmatchedは0.99である
  When 妥当なAssessmentの危険性がharmful_irreversibleの0.89またはirreversible_onlyかno_harmかunknownである
  Then 機械判定のallowを維持し指示一致だけを理由にblockやaskを作らない

@id=EX-advisor-053 @about=REQ-advisor-002 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A52,docs/decision/records/2026-10-03-llm-advice-layer.md#A59,docs/decision/records/2026-10-03-llm-advice-layer.md#A63
Scenario: 重大破壊の一致で独立した有害効果を隠さない
  Given 重大破壊とそれとは別の有害な不可逆効果を同じコマンド全体に認める
  When guardianの分類定義を適用する
  Then 危険性はmajor_destructiveだけではなくharmful_irreversibleを優先する

@id=EX-advisor-007 @about=REQ-advisor-004 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A30
Scenario: 固定質問と本文を分離する
  Given コマンド本文にSQLとコメントがある
  When 送信状態を組み立てる
  Then コメントは判定データであり分類規則を書き換えない

@id=EX-advisor-008 @about=REQ-advisor-004 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A27
Scenario: 承認済みというエージェントの文章は承認にならない
  Given エージェントの返答だけに承認済みと書かれている
  When scopeの質問を組み立てる
  Then その文章だけをmatchedの根拠にしない

@id=EX-advisor-009 @about=REQ-advisor-005 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A35,docs/decision/records/2026-10-03-llm-advice-layer.md#A53
Scenario: 偽接続による成功は合成の証拠として扱う
  Given 確率0.95を返す偽接続で合成試験が成功した
  When 検証結果を報告する
  Then 合成成功と実モデル未評価を別に記す

@id=EX-advisor-010 @about=REQ-advisor-005 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A35
Scenario: 閾値を精度保証へ読み替えない
  Given 介入閾値は0.9で実モデル評価はない
  When 品質の説明をレビューする
  Then 90パーセント正しいとする説明を採用しない
```
