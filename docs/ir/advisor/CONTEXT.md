# Glossary

LLMによる追加判断の判定合成、期限、限定した文脈、接続境界を扱う。
この文書群は承認待ちの仕様草稿であり、実装済みの保証ではない。

| Term | Meaning | Source |
|---|---|---|
| 機械判定 | 既存の解析、パスの観測と分類、設定規則の合成による判定。助言失敗時の復帰先 | docs/decision/records/2026-09-30-hook-guardian-scope.md#A10, docs/decision/records/2026-10-01-parser.md#A6, docs/decision/records/2026-09-30-hook-guardian-scope.md#A8, docs/decision/records/2026-09-30-hook-guardian-scope.md#A32, docs/decision/records/2026-09-30-hook-guardian-scope.md#A43, docs/decision/records/2026-10-03-llm-advice-layer.md#A8 |
| 助言期限 | 機械判定終了直後に始まる別枠の期限。文脈取得から応答検証までを含む | docs/decision/records/2026-10-03-llm-advice-layer.md#A41 |
| Assessment | 危険性と指示範囲の選択ラベルと確率分布。最終verdictではない | docs/decision/records/2026-10-03-llm-advice-layer.md#A40 |
| 確認済み指示 | 対応ホストの当該セッションから実行前に取得し、出所と順序を照合できる具体的な利用者指示。既知の合成入力を除く。人間がその場で入力した証明や全履歴の完全性を意味しない | docs/decision/records/2026-10-03-llm-advice-layer.md#A44 |
| 文脈なし | 取得失敗、未対応の版や対応付け、利用者指示の出所不足、参照不足により承認根拠を確認できない状態 | docs/decision/records/2026-10-03-llm-advice-layer.md#A27, docs/decision/records/2026-10-03-llm-advice-layer.md#A44 |
