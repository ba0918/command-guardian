# 破壊的効果の判定

コマンドから破壊的効果を取り出し、対象パスを分類して、`allow`、`ask`、`block` を決めるまでを扱う。文面は messages.md、設定は config.md、エージェントへの返し方は agents.md が扱う。

## Requirements

### REQ-001: 破壊的効果の抽出

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A1, docs/decision/records/2026-09-30-hook-guardian-scope.md#A11, docs/decision/records/2026-09-30-hook-guardian-scope.md#A12, docs/decision/records/2026-09-30-hook-guardian-scope.md#A38
- verification: unit

コマンドを判定するとき、hook-guardian は、コマンド列、パイプ、引用、ヒアドキュメント、コマンド置換、sudo と doas のラッパー、シェルの "bash -c" と "eval" の内側を読み、3 種の破壊的効果と対象パスを取り出す。削除の効果は、"rm"、"rmdir"、"unlink"、"find" の "-delete" と "-exec rm"、"xargs" の "rm"、"shred" から取り出す。切り詰めの効果は、リダイレクトの ">"、"dd" の "of="、"truncate" から取り出す。フォーマットの効果は、"mkfs"、"wipefs"、"dd" のブロックデバイスへの書き込みから取り出す。M1 では、"git clean"、mv と cp の上書き、"sed -i"、"rsync --delete" の効果は取り出さない。

### REQ-002: パスの解決

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A7, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28, docs/decision/records/2026-09-30-hook-guardian-scope.md#A29, docs/decision/records/2026-09-30-hook-guardian-scope.md#A36
- verification: unit

判定のとき、hook-guardian は、コマンド自身が値の確定するパスを使う場合、そのパスを解決してから分類する。解決できるのは、絶対パス、相対パス、"~" の付いたパス、環境変数 "HOME"、"TMPDIR"、"PWD"、リテラルの代入、リテラルの "cd"、"mktemp" が作ったパスである。解決できないパスは未解決として扱う。glob を含む対象は、glob が広がり得る最も外側のディレクトリを base として分類し、base が保護領域に一致するときは、保護と同じ判定にする。

### REQ-003: 一時領域の分類

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A7, docs/decision/records/2026-09-30-hook-guardian-scope.md#A17
- verification: unit

パスが一時領域の中にあるとき、分類を `ephemeral` にする。一時領域は、"/tmp"、"/var/tmp"、"TMPDIR" が指すディレクトリ、およびその中でコマンド自身が "mktemp" で作ったパスとする。一時領域のルートそれ自体は `ephemeral` にしない。

### REQ-004: git の作業ツリーの分類

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A8
- verification: unit

パスが git の作業ツリーの中にあり、そのパスについて（ディレクトリならその配下を含めて）"git status --porcelain -uall" が何も報告しないとき、分類を `vcs` にする。M1 では、ステージ済みの変更とサブモジュールを特別に扱わない。

### REQ-005: 保護領域の分類

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A17, docs/decision/records/2026-09-30-hook-guardian-scope.md#A25, docs/decision/records/2026-09-30-hook-guardian-scope.md#A33
- verification: unit

パスが保護領域にあるとき、分類を `protected` にする。配下のすべてに当てるものは、システムの領域（"/etc"、"/usr"、"/bin"、"/sbin"、"/lib"、"/lib64"、"/boot"、"/dev"、"/proc"、"/sys"、"/run"、"/opt"、"/srv"、"/root"、"/var" のうち "/var/tmp" を除く）とその配下、ほかの利用者のホームとその配下、".git" とその配下、設定で追加された保護ルートとその配下である。それ自体だけに当てるものは、"/"、一時領域のルート、ホームディレクトリ、作業ディレクトリ、リポジトリのルートである。一時領域の配下は保護領域にしない。リポジトリのルートは、対象のパスを含む git の作業ツリーのルートとする。

### REQ-006: 分類と判定の対応

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A3, docs/decision/records/2026-09-30-hook-guardian-scope.md#A7, docs/decision/records/2026-09-30-hook-guardian-scope.md#A9, docs/decision/records/2026-09-30-hook-guardian-scope.md#A17, docs/decision/records/2026-09-30-hook-guardian-scope.md#A18, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28, docs/decision/records/2026-09-30-hook-guardian-scope.md#A39
- verification: unit

hook-guardian は、対象パスの分類に応じて判定を決める。`ephemeral` と `vcs` は `allow`、`protected` は `block`、`unknown` は `ask` にする。パスを解決できない場合は、分類は `unknown` のままで、判定だけを `block` にする。3 層設定の "paths.allowed_roots"（既定は一時領域のルート）の配下も `ephemeral` と同じく `allow` にし、許可ルートそれ自体はこの規則に含めない（それ自体は通常の分類で判定する）。"git.enabled" が false のときは、git による分類を行わない。

### REQ-007: シンボリックリンクの扱い

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A21
- verification: unit

削除の対象がシンボリックリンクのとき、hook-guardian は、リンクそれ自体を分類して判定する。末尾に "/" を付けた削除だけ、リンク先を解決して分類する。ハードリンクは、1 つのリンクを消すだけとして扱う。

### REQ-008: 対象集合が未知の効果の判定

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A25
- verification: unit

効果の対象がコマンド本文に書かれていないとき（"find" の削除、"xargs"、"for" のループ）、hook-guardian は、供給元（"find" の起点、パイプの元、ループの供給元）の子の分類で判定する。供給元も確定できないときは `unknown` として `ask` にする。"-name" や "-path" の絞り込みは判定に使わない。

### REQ-009: 判定の合成

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A13
- verification: unit

1 つのコマンドから複数の効果と対象が出るとき、hook-guardian は、最も重い判定を返す。`block` が 1 つでもあれば `block`、`block` が無く `ask` があれば `ask`、どちらも無ければ `allow` にする。

### REQ-010: 内部エラーで止めない

- kind: prohibition
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A14
- verification: unit

hook-guardian は、判定の内部エラー、git の失敗、入力の解析の失敗によって `block` しない。判定できないときは `ask` にする。

## Examples

```gherkin
@id=EX-001 @about=REQ-002 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A29
Scenario: 代入された一時領域のパスを解決して通す
  Given 一時領域に作業用のディレクトリがある
  When "S=/tmp/scratch/review; rm -rf $S" を判定する
  Then 判定は allow になる

@id=EX-002 @about=REQ-006 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A3,docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15
Scenario: 未追跡のファイルの削除は確認を求める
  Given git の作業ツリーに未追跡のファイルがある
  When "rm notes.txt" を判定する
  Then 判定は ask になり、理由に未追跡であることが出る

@id=EX-003 @about=REQ-006 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A28
Scenario: 解決できないパスは止める
  Given 値の分からない変数 "X" がある
  When "rm -rf $X" を判定する
  Then 判定は block になり、理由に解決できないことが出る

@id=EX-004 @about=REQ-003 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17
Scenario: 一時領域のルートそれ自体は止める
  When "rm -rf /tmp" を判定する
  Then 判定は block になる

@id=EX-005 @about=REQ-004 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8
Scenario: 無視された生成物の削除を通す
  Given 作業ツリーに無視された "target" ディレクトリがある
  When "rm -rf target" を判定する
  Then 判定は allow になる

@id=EX-006 @about=REQ-008 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A25
Scenario: 無視されたディレクトリへの find の削除を通す
  Given 作業ツリーに無視された "node_modules/.vite" がある
  When "find node_modules/.vite -delete" を判定する
  Then 判定は allow になる

@id=EX-007 @about=REQ-008 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A25
Scenario: 未コミットの変更がある作業ツリーへの find の削除は確認を求める
  Given 作業ツリーに未コミットの変更がある
  When "find . -name __pycache__ -exec rm -rf {} +" を判定する
  Then 判定は ask になる

@id=EX-008 @about=REQ-007 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A21
Scenario: シンボリックリンクはリンクそれ自体を分類する
  Given 一時領域にホームを指すシンボリックリンク "link" がある
  When "rm /tmp/scratch/link" を判定する
  Then 判定は allow になり、リンク先は消えない

@id=EX-009 @about=REQ-009 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A13,docs/decision/records/2026-09-30-hook-guardian-scope.md#A17
Scenario: 最も重い判定を返す
  When "rm -rf /tmp/scratch/x /etc/foo" を判定する
  Then 判定は block になる

@id=EX-010 @about=REQ-001 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A11,docs/decision/records/2026-09-30-hook-guardian-scope.md#A38
Scenario: ラッパーの内側の削除も取り出す
  Given 一時領域に作業用のディレクトリがある
  When "sudo rm -rf /tmp/scratch/x" を判定する
  Then 判定は allow になる

@id=EX-011 @about=REQ-010 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A14
Scenario: git の失敗は確認に落とす
  Given git の起動が失敗する
  When 作業ツリーの中のパスを削除するコマンドを判定する
  Then 判定は ask になり、block にはならない

@id=EX-024 @about=REQ-005 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17
Scenario: システムの領域は止める
  When "rm -rf /usr/local/lib/foo" を判定する
  Then 判定は block になる

@id=EX-028 @about=REQ-004 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8
Scenario: 追跡済みのディレクトリでも配下の変更は確認を求める
  Given 追跡済みのディレクトリ "src" の配下に未コミットの変更がある
  When "rm -rf src" を判定する
  Then 判定は ask になる

@id=EX-029 @about=REQ-002 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A36
Scenario: glob は base の分類で判定する
  Given 作業ディレクトリがリポジトリのルートである
  When "rm -rf *" を判定する
  Then 判定は block になる
```
