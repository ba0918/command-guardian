# Plan: コマンドの読み方を OSS パーサと正規化 AST に置き換える

## Goal

コマンドの読みが `brush-parser` と正規化した構文木に置き換わり、走査されない位置が残らず、読めない入力は安全に ask に落ちる。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- `docs/ir/parser.md#REQ-035`、`#REQ-036`、`#REQ-037`、`#REQ-038`、`#REQ-039`、`#REQ-041`（`#REQ-040` は deferred のため対象外）
- 既存の実装を載せ替える対象: `docs/ir/judgment.md#REQ-001`、`#REQ-002`、`#REQ-009`、`docs/ir/cli.md#REQ-017`、`docs/ir/messages.md#REQ-011`
- 例: EX-046 から EX-050、EX-052 から EX-056（EX-051 は deferred）。既存の EX-001 から EX-045 は回帰として通す

判断の出典は `docs/decision/records/2026-10-01-parser.md`（A1〜A20）と `docs/decision/records/2026-09-30-hook-guardian-scope.md`。作業は既存のブランチ m1-guard-cli とその worktree（`.agents/worktrees/m1-guard-cli`）の続きで行う。

## Approach and why

- `crates/guardian-parser` を新設し、`brush-parser`（=0.4.0 に固定）を背後に隠す。外へ公開するのは正規化した構文木（A19 の契約）だけにする（A6・A11）
- 走査は 1 つの訪問で、すべての単語・置換の本体（再帰）・リダイレクト・複合構文を拾う（A3・A8）。置換本体は文字列で返るため、再帰で読み直す（A11）。置換の再帰読みは累計のサイズと段数で上限を測る（A18）
- 読めない形（解析の失敗、知らないノード）と上限超えと panic は ask にし、解析が失敗した命令からは効果を取り出さない（A7・A12・A20）
- シェル起動の認識は A16（"-c" の本体、"script.sh" の起動、非 POSIX 系の一覧、ラッパー越し）
- 旧 `crates/guardian-core/src/parse.rs` は削除し、core は正規化した構文木だけを見る。字句解析は設定の照合（見張りの例の分割と custom ルールの本文）とも共有する（A17）
- 構文解析由来の ask は最悪値で合成し、読めている block を上書きしない（A14・REQ-009）。いまの engine は parse のエラーで verdict を上書きするので、ここを直す
- 既存の REQ-001・REQ-002 のテスト（手書きの事例）が回帰の網になる。実測の誤ブロック 355 コマンドのコーパスはこのリポジトリに無いため、人が用意したときに別途回す（この計画の完了条件には含めない）

## Scope of change

- `Cargo.toml`、`Cargo.lock`（workspace への `guardian-parser` の追加と `brush-parser` の固定）
- `crates/guardian-parser/`（新規）
- `crates/guardian-core/`（`parse.rs` の削除、正規化した構文木の消費、効果の抽出とパス解決の載せ替え）
- `crates/guardian-policy/`（コンパイルのための載せ替え、構文解析の ask の合成、見張りの例と custom ルールの字句解析）
- `src/`（操作の無い ask の文面と JSON の最上位 "reason"）
- `tests/`（文法要素のコーパスと例のテスト）

## Step order and prerequisites

S1 の前に、main にある f69d6e8（parser の IR）とこの計画のコミットを m1-guard-cli に取り込む（マージ）。取り込み後、worktree の `kotowari` が新しい ID を見えることを確かめる。

S1 で crate とパーサの口、S2 で正規化した構文木と総走査、S3 で読めないときと上限、S4 でシェル起動の認識、S5 で core の載せ替え、S6 と S7 で設定の照合と合成・文面、S8 でコーパスと全体確認。S5 は S1〜S4 に依存し、S6・S7 は S5 の後、S8 は最後。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-036 | — |
| S2 | REQ-037, REQ-041 | — |
| S3 | REQ-038, REQ-039 | — |
| S4 | REQ-035 | — |
| S5 | REQ-001, REQ-002 | EX-046, EX-047, EX-048, EX-049, EX-050, EX-054, EX-055, EX-056（既存の EX-001〜045 は回帰） |
| S6 | REQ-036 | — |
| S7 | REQ-009, REQ-011, REQ-017 | — |
| S8 | 全要件の確認（REQ-036・REQ-041 の review を含む） | EX-052, EX-053（review の証拠） |

## Left to the implementer

- 正規化した構文木の内部表現と型の名前、訪問の実装
- `brush-parser` のどの API を使うか、置換本体の再帰の深さ管理
- `guardian-parser` のモジュールの分け方
- 文法要素コーパスの置き場（S2 で作る）

## Stop conditions

- main の取り込みが競合して解けない
- `brush-parser` 0.4.0 が A9 の文法要素コーパスの一部を読めない
- 既存の例（EX-001〜045）の解釈が新しい読みで変わる必要が出た
- 応答時間が 100ms の目標を満たせない
- panic が捕捉できない形（abort）で出る
- 実測の 355 コマンドのコーパスが必要になった（人が用意するまで待つ）

## Test command

```sh
cargo test --workspace
```

## Out of scope

- REQ-040（別シェルのパーサの追加。deferred）
- 実測の誤ブロック 355 コマンドのコーパス（人が用意したときに別途回す）
- 既存テストの意味の変更（回帰として通すことだけを求める）
- m1 の残り（S17 のレビューと S19 の配布）

## Steps

### S1: guardian-parser の骨格と brush-parser の導入

- Purpose: 新しい crate を作り、版を固定した OSS パーサの口と、panic を捕まえる口を用意する
- Specification: docs/ir/parser.md#REQ-036
- Prerequisites: m1-guard-cli の worktree に main の f69d6e8 とこの計画が取り込まれている（worktree の kotowari が新しい ID を見える）、cargo が使える
- May change: Cargo.toml, Cargo.lock, crates/guardian-parser/**
- Done when: `guardian-parser` がコマンド文字列を受けて `brush-parser` の解析結果を返し、依存の版が "=0.4.0" に固定され、panic を捕まえて Err に変える口がある
- Shown by: test — 単純なコマンドのパースの単体テストと、意図的に panic する解析関数を差し込んで境界が Err に変えることを確かめるテスト（差し込める形にする）
- Left to the implementer: モジュール構成と型の名前、捕獲と差し込みの実装
- Stop and hand back if: `brush-parser` がビルドできない、panic を捕まえられない、main の取り込みが競合して解けない

### S2: 正規化した構文木と総走査

- Purpose: すべての単語・置換の本体（再帰）・リダイレクト・複合構文を訪問し、契約どおりの構文木を返す
- Specification: docs/ir/parser.md#REQ-037, docs/ir/parser.md#REQ-041
- Prerequisites: S1
- May change: crates/guardian-parser/**, tests/**
- Done when: 正規化した構文木が A19 の契約を満たし、置換と複合構文の入れ子が再帰で現れ、文法要素のコーパス（このステップで作る。既知の走査漏れの事例として "bash -c" の後続引数と、ラッパーの値付きオプションの値を含める）で、訪問されない位置が無いことを、パーサの木の単語の断片とリダイレクトを正規化した構文木の対応する断片と数え合わせる性質テストで示す
- Shown by: test — 契約の単体テストと、文法要素のコーパスに対する性質テスト
- Left to the implementer: 訪問の実装、内部表現、コーパスの置き場
- Stop and hand back if: `brush-parser` の構文木から契約の事実を取り出せない要素がある

### S3: 読めないときの扱いと入力の上限

- Purpose: 解析の失敗と知らない形、サイズ、深さ、panic を ask の原因として返す
- Specification: docs/ir/parser.md#REQ-038, docs/ir/parser.md#REQ-039
- Prerequisites: S2
- May change: crates/guardian-parser/**
- Done when: サイズは解析の前に累計（置換の再帰読みを含む）で測られ、深さは再帰の各段で深くなる前に測られ、解析の失敗と知らない形と上限超えと panic が区別して返り、解析が失敗した命令からは効果を返さない
- Shown by: test — 1 MiB と 128 段の境界（累計の事例を含む）と、解析の失敗の単体テスト
- Left to the implementer: 原因の型
- Stop and hand back if: 深さを段で測る方法が取れない

### S4: シェル起動の認識

- Purpose: "-c" の本体の読み、ファイル起動の除外、非 POSIX 系の一覧、ラッパー越しの認識を作る
- Specification: docs/ir/parser.md#REQ-035
- Prerequisites: S3
- May change: crates/guardian-parser/**, crates/guardian-core/**
- Done when: A16 の 5 つのルールが正規化した構文木の段で判別でき、"-lc" のまとめ書きも "-c" として扱われる
- Shown by: test — 起動の形ごとの認識の単体テスト
- Left to the implementer: 認識の戻し方、既存のラッパー外しとの接続
- Stop and hand back if: 既存の sudo と doas の外し方と噛み合わない

### S5: guardian-core と guardian-policy の載せ替え

- Purpose: 旧 `parse.rs` を削除し、効果の抽出とパス解決を正規化した構文木の上で動かす。guardian-policy がコンパイルできるところまで載せ替える
- Specification: docs/ir/judgment.md#REQ-001, docs/ir/judgment.md#REQ-002, docs/ir/parser.md#REQ-037
- Prerequisites: S4
- May change: crates/guardian-core/**, crates/guardian-policy/**, tests/**
- Done when: 既存の例（EX-001〜045）と新しい例（EX-046〜050、EX-054〜056）に対応するテストが通り、`crates/guardian-core/src/parse.rs` が消え、guardian-policy を含む workspace がコンパイルされて全テストが通る
- Shown by: test — 既存の全テストと、新しい例の抽出テストと CLI テスト
- Left to the implementer: 載せ替えの順序
- Stop and hand back if: 既存の例の解釈が新しい読みで変わる必要が出た

### S6: 設定の照合との字句解析の共有

- Purpose: 見張りの例の分割と custom ルールの本文の引用の除去を、guardian-parser の字句解析に寄せる
- Specification: docs/ir/parser.md#REQ-036
- Prerequisites: S5
- May change: crates/guardian-policy/**
- Done when: 2 箇所の分割が同じ実装になり、既存の見張りのテスト（EX-037〜045 を含む）が通る
- Shown by: test — 例の分割と custom ルールの既存テスト
- Left to the implementer: 共有の形
- Stop and hand back if: 共有で見張りの意味論が変わる

### S7: 構文解析由来の ask の合成と文面・JSON

- Purpose: engine が構文解析の ask で block を上書きしないようにし、操作の無い ask の理由を文面と JSON に出す
- Specification: docs/ir/judgment.md#REQ-009, docs/ir/parser.md#REQ-038, docs/ir/cli.md#REQ-017, docs/ir/messages.md#REQ-011
- Prerequisites: S5
- May change: crates/guardian-policy/**, src/**, tests/**
- Done when: 読める保護削除と読めない部分が混ざる入力（例: `rm -rf /etc/x; if true; then`）が block になり、効果が無い入力は ask のままになり、JSON に最上位の "reason" が出て、構文解析由来の ask の文面に「構文を読めない」「読めないシェル」「入力が大きすぎる」が現れる
- Shown by: test — 合成の CLI テストと、JSON と文面のテスト
- Left to the implementer: 合成と文面の実装
- Stop and hand back if: どの効果が読めていてどれが読めないかの区別が取れない

### S8: コーパスと全体の確認

- Purpose: 文法要素のコーパスと、読めない構文は ask のコーパスを通し、対象の全部に印があることと依存の境界を確かめる
- Specification: 対象の全要件（docs/ir/judgment.md#REQ-001, docs/ir/judgment.md#REQ-002, docs/ir/judgment.md#REQ-009, docs/ir/cli.md#REQ-017, docs/ir/messages.md#REQ-011, docs/ir/parser.md#REQ-035, docs/ir/parser.md#REQ-036, docs/ir/parser.md#REQ-037, docs/ir/parser.md#REQ-038, docs/ir/parser.md#REQ-039, docs/ir/parser.md#REQ-041）
- Prerequisites: S6, S7
- May change: tests/**
- Done when: 文法要素のコーパスと、読めない構文は ask のコーパスが通り、対象の全 ID に印付きのテストがあり、kotowari check がこのブランチで変えたファイルとこの計画の ID に error を出さず、fmt と clippy とテストが通る
- Shown by: check — `kotowari list | jq -r '.items[] | select(.kind == "requirement" and .verification != "review" and (.deferred | not) and .tests == []) | .id'` の出力が空であること、`kotowari list | jq -r '.items[] | select(.kind == "scenario" and .tests == []) | .id' | sort | tr '\n' ' '` の出力が EX-013 EX-023 EX-027 EX-051 であること、`cargo tree -p guardian-core --depth 1`（直接依存に OSS のシェルパーサが無いこと）と `cargo tree -i brush-parser`、core 以降が公開型だけを使っていることの読み（REQ-036・REQ-041 の review の証拠）、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`
- Left to the implementer: コーパスの陳列
- Stop and hand back if: 実測コーパスの再判定で解釈の食い違いが出る
