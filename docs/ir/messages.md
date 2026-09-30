# 判定の文面

allow 以外の判定を返すときの文面を扱う。判定そのものは judgment.md が扱う。

## Requirements

### REQ-011: 非 allow の文面

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A15, docs/decision/records/2026-09-30-hook-guardian-scope.md#A26, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28, docs/decision/records/2026-09-30-hook-guardian-scope.md#A30, docs/decision/records/2026-10-01-parser.md#A7, docs/decision/records/2026-10-01-parser.md#A12, docs/decision/records/2026-10-01-parser.md#A16
- verification: unit

hook-guardian は、`allow` 以外の判定を返すとき、何の操作が、どのパスに対して行われるのか、なぜ止めるのか、代わりに何ができるのかを示す。理由には、分類と、失われるもの（未追跡で戻せない、未コミットの変更が失われる、パスを解決できない、管理外で戻せない、保護領域である）、および操作の無い ask の理由（構文を読めない、読めないシェル、入力が大きすぎる）を含める。操作とパスが無い ask では、それらに代えてその理由を示す。代替には、先にコミットする、リテラルのパスで指定し直す、一時領域や作業場所へ移してから消す、許可ルートに追加する、のうち当てはまるものを示し、当てはまるものが無いときは、無いと書く。文面は 2 行から 4 行にする。

### REQ-012: 宛先に応じた書き分け

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
- verification: review
- how_to_verify: 代表的な 5 つの文面（拒否 2 件、ask 3 件）を読み、エージェントが次の一手を打てるか、利用者がその場で判断できるかを確かめる

hook-guardian は、拒否としてエージェントに返す文面は、エージェントが次の一手を打てるように書く。`ask` として利用者に返す文面は、利用者がその場で判断できるように書く。

## Examples

```gherkin
@id=EX-012 @about=REQ-011 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A26,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
Scenario: 保護領域の拒否に理由と代替が出る
  When "rm -rf /etc/nginx" を判定する
  Then 文面に、操作、パス、分類の protected、失われるもの、代替が含まれる

@id=EX-013 @about=REQ-012 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
Scenario: 未追跡ファイルの確認は利用者向けに書く
  Given git の作業ツリーに未追跡のファイルがある
  When "rm notes.txt" を判定する
  Then ask の文面は、利用者が消すかどうかをその場で決められる形になっている
```
