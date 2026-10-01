# パイプの解析状態

構文の対象はparser.md、パスの解決はjudgment.mdが扱う。ここではshellの実行モードを確定できないときのパイプの状態伝播を定める。

## Requirements

### REQ-046: 最終段の親状態への影響

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-ir-friction-contracts.md#A3
- verification: unit

複数段のパイプを読むとき、command-guardianは最終段が親の状態を変更する場合と変更しない場合を合流する。両方で同じ値となる変数・cwdだけを後続へ確定伝播し、異なる値は未解決とする。最終段以外の状態変更を親や他段へ伝播しない。shellの種類やlastpipe等の実行モードを推測して一方の状態を選ばない。各段の効果と起動の収集は維持する。

## Examples

```gherkin
@id=EX-074 @about=REQ-046 @source=docs/decision/records/2026-10-02-ir-friction-contracts.md#A3
Scenario: 最終段の代入が親へ伝わるか確定できない
  Given Sの値が"/tmp/x"である
  When "printf x | eval 'S=/etc/x'; rm \"$S\""を判定する
  Then 後続の削除対象は未解決として分類する

@id=EX-075 @about=REQ-046 @source=docs/decision/records/2026-10-02-ir-friction-contracts.md#A3
Scenario: 最終段が親と同じ値を設定する
  Given Sの値が"/etc/x"である
  When "printf x | eval 'S=/etc/x'; rm \"$S\""を判定する
  Then 後続の削除対象は"/etc/x"として分類する
```
