# 解析と実行基盤のクレート境界

## Context

parser がホスト起動と判定予算を持ち、policy が解析と外部観測を組み立てていた。
そのため、共通の値だけを使う judge にも OSS パーサへの依存が届き、効果と guard が同じ本文を別々に読んでいた。
本記録は、既存の機能要求を保って責務を分ける[承認済み計画](../../plans/crate-boundaries.md)の設計判断を記録する。
承認された計画のコミットは d29e3a8 であり、新しい機能要求の決定は含めない。

Position: 承認済み計画に基づく実装後の記録。機能要求の変更なし。実装の独立レビューは別途行う。

## Agreements

- A1 共通の値と診断を core、正規化 AST と純粋な構文解析を parser、意味解析を新しい analysis、外部観測を judge、値からの規則判定を policy、実行基盤と組立てを新しい app に分ける。policy の通常内部依存は core だけにする。
  - why: 共通型を使うだけの層から解析実装への依存を切り、外部アクセスなしで規則を検証できるようにする。
  - decided_by: 利用者（d29e3a8 の計画を承認）
- A2 app の Engine が ParserRuntime を所有し、判定ごとに JudgmentSession が借用する。設定の読込には累積判定予算を持たない ValidationSession を使う。同一バイナリの子 dispatch は main の先頭だけで行う。
  - why: worker の回収と予算の寿命を所有関係で表し、設定例の個数、検証順、前の判定に次の判定を左右させない。
  - decided_by: 利用者（承認済み計画の session と隔離方式）
- A3 analysis が AST と Context を保持して効果、Invocation、診断を一つの CommandFacts に集める。resolver は値だけを返し、語の子の訪問は共有 walker が行う。literal loop の効果は束縛ごとに評価し、guard の構文位置は反復と分けて収集する。
  - why: 効果の取りこぼしと二重訪問を防ぎ、guard に環境展開したパスを渡さずに内側本文と診断を共有する。
  - decided_by: 利用者（承認済み計画の共有走査）
- A4 末尾 slash の対象を一度観測し、同じ実体パスをルート照合と既定分類に渡す。git が必要な場合だけ起動し、子の待ちと出力の取得を判定の残時間内で打ち切って回収する。通常の GitFailed は ask、期限超過は Limit として block に合成する。
  - why: 同じ判定内で別のリンク先を分類することと、外部待ちが判定期限を取りこぼすことを防ぐ。
  - decided_by: 利用者（承認済み計画の観測と期限）
- A5 浅い構文契約と意味解析は純粋または注入した parser で、設定と Engine は app 統合で、子の死亡と深い入力は root の実製品バイナリで検証する。分類から verdict への対応はテスト内に複製せず実 policy を使う。
  - why: パッケージの libtest を暗黙のホストにする契約をなくし、試験が製品の判定処理を置き換えて成功する状態を避ける。
  - decided_by: 利用者（承認済み計画の検証配置）

## Prohibitions

- A6 親での入力由来の直接解析 fallback、別の配布 worker バイナリ、追加の外部依存、完全なシェル評価器、新しい権限 sandbox は導入しない。
  - why: [解析契約](../../ir/parser.md#REQ-039)と単一バイナリを保った構造変更の範囲を超えるため。
  - decided_by: 利用者（承認済み計画の範囲と停止条件）

## Revisions

- [parser A6](./2026-10-01-parser.md#A6) のクレート責務を A1 で具体化し、実行基盤を app に分けた。
- [parser A8](./2026-10-01-parser.md#A8) の core 以降の責務を A1 と A3 で改訂した。正規化 AST とその構文上の深さ検査は parser に残し、AST を読む意味解析は analysis に限定した。
- [parser A22](./2026-10-01-parser.md#A22) と [A23](./2026-10-01-parser.md#A23) の上限と失敗の帰属は変えず、A2 と A4 で所有と待ちの実装を明示した。
