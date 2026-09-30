# 配布

リリースの成果物と配布の形を扱う。

## Requirements

### REQ-025: 配布

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A23
- verification: review
- how_to_verify: リリースのページで Linux x86_64 向けの実行ファイルを確かめ、"mise use -g github:ba0918/hook-guardian" で導入し、"hook-guardian check" が動くことを確かめる

リリースは、Linux の x86_64 向けの 1 つの実行ファイルを GitHub Releases に置き、"mise use -g github:ba0918/hook-guardian" で導入できるようにする。CI でビルドとテストを回す。Windows ネイティブと macOS 向けは作らない。

## Examples

```gherkin
@id=EX-023 @about=REQ-025 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A23
Scenario: mise で導入できる
  Given GitHub Releases に Linux x86_64 向けの実行ファイルがある
  When 利用者が "mise use -g github:ba0918/hook-guardian" を実行する
  Then "hook-guardian check" が動く
```
