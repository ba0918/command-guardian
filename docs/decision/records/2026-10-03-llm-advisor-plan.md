# LLM助言層の計画上の配置と検証

## Context

LLM助言層の仕様は承認され、コミット8ccf11d671a02680b3943c526f5c318d4a32b287にある。
実装計画には、既存の層を壊さない配置と、実サービスを呼ばずに得られる証拠の範囲を記す必要がある。
現行のengine、設定読込、解析worker、影ログ、三ホストの入口を読んだが、会話の実行前境界の取得経路はまだ検証されていない。
ここでは計画上の技術選択だけを記録し、承認済みの判定、入力、期限、送信、保存、失敗の意味を変更しない。

Position: A7とR5の文書改訂は独立レビューを経た。利用者が追加承認したREQ-advisor-008の検証分担をA8とR6で計画へ反映する。今回は文書改訂だけを行い、部分実装とステージ済み変更は維持する。この追加改訂の独立レビューは呼出元へ引き渡す。計画の名称とブランチ全体の比較元は変更しない。

## Agreements

- A1 純粋な質問、助言値、分布検証と候補合成を新規guardian-advisorへ置き、TypeSafe接続を別のguardian-advisor-typesafeへ置く。appが組み立て、policyは助言設定値を利用し、advisorはcoreより上へ依存しない。
  - why: [PROJECT.md](../../../PROJECT.md#stack-and-layout)の一方向の層構成と[モデル境界REQ-advisor-012](../../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)を守るため。現行[engine.rs](../../../crates/guardian-app/src/engine.rs)は機械判定と観測の組立てを担い、接続固有のHTTPをpolicyや共通値へ入れる理由はない。traitには本番TypeSafeと制御試験の二つの現在の利用者がある
  - decided_by: AI（計画実装担当。承認者ではない）
- A2 解析workerは起動とソケット継承のパターンだけを再利用し、助言の通信と寿命管理はappの専用モジュールへ置く。
  - why: [worker.rs](../../../crates/guardian-app/src/runtime/worker.rs)の既存枠はlittle-endianで、本文と上限も[助言IPC](../../ir/advisor/runtime.md#req-advisor-019-長さと要求対応を検証する内部通信)とは異なる。解析の死因とDropのwaitを流用すると[助言失敗の復帰](../../ir/advisor/runtime.md#req-advisor-018-同一バイナリの子と強制打切り)と単一期限に反する。[承認済みA62](2026-10-03-llm-advice-layer.md#agreements)の起動時dispatchは保つ
  - decided_by: AI（計画実装担当。承認者ではない）
- A3 実ホストの取得根拠を先に対応表へ残し、裏付けがない版・経路を文脈なしとして実装へ渡す。
  - why: [承認済みA33](2026-10-03-llm-advice-layer.md#agreements)、[A44](2026-10-03-llm-advice-layer.md#agreements)、[A56](2026-10-03-llm-advice-layer.md#agreements)が三ホスト対象と未対応時の復帰を定めている。現行[hook.rs](../../../src/hook.rs)と[guardian.ts](../../../plugins/opencode/src/guardian.ts)は会話を保持せず、[index.ts](../../../plugins/opencode/src/index.ts)のsessionIDだけでも人間由来と実行前境界は証明できない。公開契約とリリース済みソース、隔離ホストで生成した架空データの対応を確認し、parser fixtureだけの証拠と分ける
  - decided_by: AI（計画実装担当。承認者ではない）
- A4 助言用の安全state保存をappへ置き、rootの助言ログはそれを利用する。影ログの既存本文と保存条件は変えない。
  - why: [log.rs](../../../src/log.rs)にはstate場所とopenatの既存パターンがあるが、appからrootへ参照すると依存が逆転する。[助言キャッシュ](../../ir/advisor/context.md#req-advisor-010-必要な経路だけのセッションキャッシュ)と[助言ログ](../../ir/advisor/context.md#req-advisor-011-助言ログと影ログの分離)は所有者と複数hardlinkも検査するため、[影ログ](../../ir/cli.md#req-019-影実行のログ)とは別の保存境界が必要である
  - decided_by: AI（計画実装担当。承認者ではない）
- A5 接続と子の試験は制御transport、試験用AdvisorClient、起動境界のfixture子、本物の配布バイナリdispatchを組み合わせて行う。公開試験用フラグやURL指定を追加しない。
  - why: [接続境界](../../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)には制御試験が明示され、[設定の公開境界](../../ir/advisor/provider.md#req-advisor-015-利用者だけが助言を設定する)は任意URLや実行ファイル指定を禁止する。[PROJECT.md](../../../PROJECT.md#製品の実装と試験)はlibtest再起動を禁止する。制御した失敗は期限と復帰の証拠になるが、[REQ-advisor-005](../../ir/advisor/policy.md#req-advisor-005-モデルの品質を構造試験と区別する)に従いモデル精度や実サービス適合の証拠とはしない
  - decided_by: AI（計画実装担当。承認者ではない）
- A6 実装の範囲内の試験充足と、統合前の全体検査を別に報告する。比較元と最終候補は呼出元がGitから固定する。
  - why: [PROJECT.md](../../../PROJECT.md#変更とirの照合)は独立担当による記録とcheck/changes reviewの両exit0を統合条件にしている。一方、計画対象外の問題やstatusのcompleteは対象実装の証明とは別である。仕様の承認コミットと計画承認後の実装branch baseを混同せず、記録コミット後の最終HEADで再検査する
  - decided_by: AI（計画実装担当。承認者ではない）
- A7 [利用者が承認した検証分担A63](./2026-10-03-llm-advice-layer.md#agreements)に従い、[計画](https://github.com/ba0918/command-guardian/blob/c2a7b5f265bc3b2cdbbab278bedccd9cb6eadc6d/docs/plans/llm-advisor.md#verification-map)のREQ-advisor-002とEX-advisor-003、004、053をS10の独立質問レビューへ対応付ける。REQ-advisor-004の分離はS2/S5の単体試験のままとし、19助言要求と48助言例に既存EX-112、113を加えた50例を機械試験の充足対象とする。
  - why: 固定質問の文字列一致や偽Assessmentによる合成は質問の意味の証拠ではないため、実際の実装パスと書かれた契約、三例ごとの判断根拠を独立担当の証拠として要求する。現在の部分実装の試験不足件数はこの実装前の対象数とは別に報告する。分類、閾値、送信、実行時の挙動、計画名と比較元は変更しない
  - decided_by: 利用者（実装中の検証分担改訂に「OK」。計画への反映は文書実装担当）

- A8 [利用者が追加承認した検証分担A64](./2026-10-03-llm-advice-layer.md#agreements)に従い、[計画](https://github.com/ba0918/command-guardian/blob/c2a7b5f265bc3b2cdbbab278bedccd9cb6eadc6d/docs/plans/llm-advisor.md#verification-map)のREQ-advisor-008とEX-advisor-015、016をS10の独立レビューへ対応付ける。REQ-advisor-007とEX-advisor-013、014はS4の構造試験に分け、REQ-advisor-006、010、003の単体試験を維持する。充足対象は18助言要求・46助言例の単体試験と3助言要求・7助言例のレビューとし、既存EX-112、113を加えた48例を機械試験の対象とする。
  - why: [A7](./2026-10-03-llm-advisor-plan.md#agreements)の質問の意味と構造の分担を、追加承認された要求だけへ広げるため。固定質問、分類定義、符号化後の実際の文脈データの照合と二例ごとの独立した判断根拠を求め、偽Assessmentの合成や文字列一致を意味の検証にしない。S4の取得・窓・保存の振る舞いは変えず、実装前の対象数を現在の残件数や実モデルの精度と混同しない。実行時の試験不足の免除、計画名と比較元の変更は行わない
  - decided_by: 利用者（REQ-advisor-008の検証分担の追加改訂を明示承認。計画への反映は文書実装担当）

## Reuse evidence

| 層 | 採用または自作 | 根拠 |
|---|---|---|
| 機械判定と出口 | 既存採用 | engineとCLI/hookの既存結果を基準にする |
| 質問と合成 | 最小自作 | 承認済み分類表を表し、参考guardrailの意味を流用しない |
| 設定 | 既存拡張 | Config/Layerとconfig_loaderの境界を使う |
| 子と期限 | 既存パターンと標準API採用 | UnixStreamとstd::process、単調時計を使う |
| 会話取得 | ホスト契約優先と最小変換 | 対応版の証拠がない経路を追加実装済みとしない |
| state | 既存パターンとrustix採用 | 下向きの保存部品で助言固有の検査を行う |
| HTTP/TLS | 承認済みureq採用 | [REQ-advisor-014](../../ir/advisor/provider.md#req-advisor-014-typesafeへの一要求)が採用を固定する。版とライセンスは実装時に確認する |
| TypeSafeと制御IPC | 最小変換を自作 | provider表現を内側に閉じ、承認済み枠と交渉式を実装する |

探索は現行コードと[承認済み再利用証拠](2026-10-03-llm-advice-layer.md#reuse-evidence)に限定し、ネットワーク確認もコードコピーも行っていない。
参考ba-toysのMIT Licenseと著作権は同記録で確認済みである。
将来コピーする場合はその表示と許諾文を保持する。

## Revisions

- R1 IQ001として、計画と本記録の相対リンクと実在するGitHub見出しアンカーを修正した。要求の正規参照と決定番号は表示に保持し、承認済み文書にはアンカーを追加していない。
- R2 IQ002として、S3とS4のmanifest変更に対応するCargo.lock更新を各段階の変更範囲へ明記し、更新後にlocked付きGREENを実行する順序を補った。
- R3 SPEC001として、期限内の文脈取得失敗を助言全体の失敗と混同したS9の記述を訂正した。文脈なしの高確率重大破壊はenforceでblockとする既存仕様を明記し、取得が助言期限を使い切るtimeoutとは分けた。
- R4 SPEC002として、S7の本文不在の検査を通常の助言ログと新しい助言分類理由・警告・診断に限定した。許されたdebug本文と既存機械JSONの対象パス・理由、本文入り影ログを維持する。

これらは計画著者の誤記と不足の訂正であり、新しい利用者判断や承認済み仕様の改訂ではない。

- R5 実装中に利用者が承認した[A63](./2026-10-03-llm-advice-layer.md#agreements)と[A7](./2026-10-03-llm-advisor-plan.md#agreements)に基づく検証方法の改訂を、S2/S5/S10の証拠と完了条件、検証対応表、充足対象数へ反映した。R1〜R4の誤記訂正とは区別する。呼出元が固定したブランチ全体BASEは9c5a25884a2d430dfaafd56758f515a66992330eのままであり、最終の全差分レビュー後に変更したIRのbytesを含めて両役割の変更照合記録を更新する。
- R6 利用者が追加承認した[A64](./2026-10-03-llm-advice-layer.md#agreements)と[A8](./2026-10-03-llm-advisor-plan.md#agreements)に基づき、S4の構造試験とS10のREQ-advisor-008の意味のレビューを分け、検証対応表と充足対象数を更新した。A63の文書改訂は独立レビューを経たが、この追加改訂の独立レビューは呼出元へ引き渡す。要求の振る舞い、計画名、ブランチ全体BASEは変えず、更新したIRのbytesは最終の独立レビューと両役割の変更照合に含める。

## Delegated

- D1 内部の関数・helper・試験名と、依存方向を保つ小さな抽出だけを実装担当へ残す。
  - why: [計画](https://github.com/ba0918/command-guardian/blob/c2a7b5f265bc3b2cdbbab278bedccd9cb6eadc6d/docs/plans/llm-advisor.md#left-to-the-implementer)は新入力、受理境界、保存形式、期限、送信範囲、失敗の意味を未指定の実装選択にしない
  - decided_by: AI（計画実装担当。承認者ではない）
