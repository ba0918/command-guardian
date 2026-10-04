# Plan: askをエージェントの権限判断に委ねる任意設定

## Goal

利用者設定で `mode.defer_ask = true` にした人は、Claude Code・Codex・OpenCode のフックで guardian の ask に止められず、エージェント本来の権限判断（Claude Code の auto モード、OpenCode の run --auto など）で作業を進められ、委ねた実行を影ログと同じファイルで後から追える。

## Specification

IR は `docs/ir/` にある。判断の経緯は `docs/decision/records/2026-10-04-auto-allow-ask.md`（A1〜A17）にある。この計画が扱うのは次のとおり。

- `docs/ir/defer-ask.md#REQ-059`、`#REQ-060`、`#REQ-061`、`#REQ-062`、`#REQ-063`、`#REQ-064`
- 委任への参照と出典を足した既存の要求: `docs/ir/agents.md#REQ-022`、`docs/ir/cli.md#REQ-018`、`docs/ir/config.md#REQ-013`、`#REQ-014`、`#REQ-015`、`#TBL-001`、`docs/ir/opencode.md#REQ-048`、`#REQ-049`、`#REQ-050`
- 例: EX-114 から EX-132（すべて `docs/ir/defer-ask.md`）
- 用語: `docs/ir/CONTEXT.md` の「ask委任」

各要求は `kotowari query REQ-nnn` で読む。例は `kotowari query EX-nnn` で読む。REQ-064 は verification が review なので試験を付けない。

## Approach and why

設定は既存の `mode.enforce` と同じ道を通す。`crates/guardian-policy/src/layers.rs` の層の読み取りに `mode.defer_ask` を足し、`crates/guardian-policy/src/config.rs` の `Config` に既定 false の真偽値として持たせる。プロジェクト層の扱いは advisor 節の除去（`parse_layer_kind` の `removed_advisor`）と同じ形にする。TOML 構文解析の直後、一般設定の検証の前に `mode` 表から `defer_ask` を取り除き、警告を出す。こうすると、型が不正でも同じファイルの他の設定は捨てない（REQ-059、REQ-015）。

委任の判断はフックの入口だけに置く。`check` と判定の合成（guardian-app、guardian-policy の評価）には手を入れない。REQ-060 は「最終判定が ask のもの」を対象にしていて、判定の理由を区別しないからだ。助言で上がった ask も、`crate::advisor::run` が `report` を書き換えた後の `report.verdict` を見れば同じ扱いになる。委任するかどうかは、エージェント、最終判定、`permission_mode`、`defer_ask`、`enforce` を受け取る純粋な関数に切り出す。助言の block は実モデルなしでは実バイナリから作れないため、`src/hook.rs` の `advisor_output_tests` が使う `crate::advisor::operational_tests::fixture_report` でこの関数を試験する。

- Claude Code・Codex の写像は `src/hook.rs` の `run` と `response` にある。影実行の分岐（`!engine.config().enforce`）を先に評価したまま、その後で `defer_ask` かつ最終判定 ask なら出力を出さない。こうすれば REQ-063 の優先順位がコードの順序で保たれる。
- OpenCode は `src/hook/opencode.rs` の `native_response` が本体の応答を作り、プラグインの `plugins/opencode/src/gate.ts` の `response` と `authorize` が解釈する。プラグインは設定を読まない（REQ-048）。だから委任は本体の応答に新しい値として載せ、プラグインはそれを allow と同じ「guardian 由来の承認要求を出さない」扱いにする。

記録は `src/log.rs` の `write_shadow` と同じ保存先・同じ保存先の検査（symlink と非通常ファイルの拒否）を使う。行の形は既存の影ログの行（時刻、判定、理由、対象パス、コマンド本文のタブ区切り）を変えず、委任の行だけを見分けられる印を足す。既存の影ログの行を変えないのは、影ログを読む利用者がいる外部契約（REQ-019）だからだ。

試験は、既存の実バイナリ試験（`tests/hook.rs`、`tests/shadow.rs`、`tests/opencode_hook.rs`、`tests/app_config.rs`）と、policy の単体試験（`crates/guardian-policy/tests/layers.rs`）、プラグインの単体試験・結合試験に足す。どれもフィクスチャの HOME と XDG ディレクトリを使い、実利用者の設定と影ログに書かない（PROJECT.md の規約）。`tests/hook.rs` の `run_hook_env` は XDG_STATE_HOME を設定しないため、`defer_ask` を有効にする試験では XDG_STATE_HOME をフィクスチャのディレクトリへ向ける。

このリポジトリの pre-commit フックは、コミットのたびに `kotowari check` を通すことを求める。この計画の要求と例に試験の印が揃うまで、どの途中コミットもフックで止まる。そのため S1 から S4 ではコミットせず、S5 で仕様の残り・実装・試験・記録をまとめて1つのコミットにする。仕様（f823840）とこの計画は、承認した利用者がフックを飛ばしてコミットする。実装者は `--no-verify` を使わない。

統合の前の変更照合は PROJECT.md の手順に従う。ブランチ全体の比較の基点は main との分岐点 46bf07490feb15418df6e0ae69a3b50922ef39ea、候補の先端は記録のコミット後に呼び出し元が確定する HEAD とする。実装者が `.kotowari/changes/implementation.yaml` を、実装と別のコンテキストのレビューが `.kotowari/changes/review.yaml` を、それぞれ自分で書く。統合の条件は `kotowari check --format json` と `kotowari changes --base 46bf07490feb15418df6e0ae69a3b50922ef39ea --head <確定した HEAD> --phase review --format json` の両方が 0 で終わることと、テストコマンドが通ることである。

## Scope of change

- `crates/guardian-policy/src/config.rs`
- `crates/guardian-policy/src/layers.rs`
- `crates/guardian-policy/tests/layers.rs`
- `src/hook.rs`
  - `advisor_output_tests` を含む
- `src/hook/opencode.rs`
- `src/log.rs`
- `tests/hook.rs`、`tests/shadow.rs`、`tests/opencode_hook.rs`、`tests/app_config.rs`
  - 既存の試験の期待は変えない。足すだけにする
- `plugins/opencode/src/gate.ts`、`plugins/opencode/src/guardian.ts`
- `plugins/opencode/tests/unit/gate.test.ts`、`plugins/opencode/tests/unit/guardian.test.ts`、`plugins/opencode/tests/integration/host.test.ts`
  - 結合試験の補助が要る場合だけ `plugins/opencode/tests/integration/bridge/` の下
- `README.md`、`README-ja.md`、`docs/opencode.md`
  - 既存の見出しは変えない。`site/build.py` が README の見出しで節を探すため
  - `docs/opencode.md` は "Approval behavior and limits" の節に委任の一行を足すだけにする
- `CHANGELOG.md`
  - `## [Unreleased]` の節だけ
- `docs/decision/records/2026-10-04-auto-allow-ask.md`
  - 実装者が決める OpenCode 応答の形と記録の印を、新しい番号の決定として足すときだけ
- `.kotowari/changes/implementation.yaml`
  - `.kotowari/changes/review.yaml` は実装と別のコンテキストのレビューが書く。実装者は書かない

## Step order and prerequisites

S1 を最初にする。S2 と S3 はどちらも `Config` の `defer_ask` を読むからだ。S2 は S3 より先にする。S3 は S2 で作る委任の記録の書き込みを OpenCode の経路から呼ぶからだ。S4 の README は、S2 と S3 で決まった記録の印と応答の形を書くので、その後にする。S5 は全部の後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-059, REQ-013, REQ-014, REQ-015, TBL-001 | EX-116 |
| S2 | REQ-059, REQ-060, REQ-062, REQ-063, REQ-022, REQ-018 | EX-114, EX-115, EX-117, EX-118, EX-119, EX-120, EX-121, EX-122, EX-126, EX-127, EX-128, EX-129, EX-130, EX-132 |
| S3 | REQ-061, REQ-048, REQ-049, REQ-050 | EX-123, EX-124, EX-125, EX-131 |
| S4 | REQ-064 | なし（review） |
| S5 | 上の全部の試験の印と、変更照合の記録 | 上の全部 |

## Left to the implementer

- `Config` の欄名、層の欄名、補助関数の名前と分け方
- 委任を判断する純粋な関数の名前と引数の並び（影実行の分岐の後で評価する順序は変えない）
- 委任の記録の印の具体的な書き方。既存の影ログの行の形を変えず、委任の行と影実行の行を区別できること。決めたら決定記録に新しい番号で残し、README に契約として書く（REQ-062、決定 A10）
- OpenCode の委任の応答の JSON のキーと値、保存失敗の警告の載せ方。allow とも shadow とも unavailable とも区別できること、既存の応答の形を変えないこと。決めたら決定記録に新しい番号で残す（REQ-061、決定 A12・A17）。既存の応答の形は `docs/decision/records/2026-10-02-opencode-v2-protocol.md` の A2 にある
- 試験の名前と、既存の試験ファイルのどこに足すか

## Stop conditions

- 既存の試験の期待を変えないと通らない。既存の契約と新しい要求がぶつかっている可能性がある
- 影ログの既存の行の形を変えないと委任の行を区別できない
- OpenCode の委任の応答を、既存の judged・shadow・unavailable の応答の分類を変えずに足せない
- Claude Code のフックで、委任を「何も出力しない」以外の形で返す必要が出てきた（A1・A9 と食い違う）
- 助言の判定を試験で作るのに、実モデルや外部サービスへの送信が要る
- pre-commit フックや push 前の検査を飛ばさないと先へ進めない

## Test command

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bun install --frozen-lockfile --cwd plugins/opencode
bun run --cwd plugins/opencode typecheck
bun run --cwd plugins/opencode test:unit
cargo build --locked
GUARDIAN_TEST_BIN="$PWD/target/debug/command-guardian" OPENCODE_TEST_BIN="$PWD/plugins/opencode/node_modules/@opencode/cli-linux-x64/bin/opencode" bun run --cwd plugins/opencode test:integration
kotowari check --format json
```

## Out of scope

- `check` の出力や終了コードの変更（REQ-060 で適用しないと決まっている）
- エージェントへ補足の文脈を渡す出力（決定 A9 で採らないと決まっている）
- 判定のたびの警告（決定 A13）
- 版の更新とリリース。CHANGELOG の Unreleased に足すだけにする
- 紹介ページ（`site/`）の変更
- 実モデルを使う評価

## Steps

### S1: 設定の層で mode.defer_ask を読む

- Purpose: 利用者設定の `mode.defer_ask` を既定 false の真偽値として読み、プロジェクト設定の値は型の検証前に取り除いて警告する
- Specification: `docs/ir/defer-ask.md#REQ-059`, `docs/ir/config.md#REQ-013`, `docs/ir/config.md#REQ-014`, `docs/ir/config.md#REQ-015`, `docs/ir/config.md#TBL-001`
- Prerequisites: なし
- May change: `crates/guardian-policy/src/config.rs`, `crates/guardian-policy/src/layers.rs`, `crates/guardian-policy/tests/layers.rs`, `tests/app_config.rs`
- Done when: 利用者層の true が `Config` に反映され、無指定なら false であり、利用者層の文字列 "yes" でそのファイル全体が不採用になり、プロジェクト層（信頼済みでも）の値は反映されず警告が出て、プロジェクト層の文字列 "yes" が同じファイルの保護ルートを捨てない（EX-116）
- Shown by: test — layers.rs に REQ-059 の利用者層・既定・不正値・プロジェクト層（信頼の有無）の各規則に1つずつ、app_config.rs に EX-116
- Left to the implementer: 欄名と、除去を advisor 節の除去と共通化するかどうか
- Stop and hand back if: プロジェクトの `[mode] defer_ask = "yes"` を除去しても、同じファイルの他の設定が捨てられてしまう

### S2: Claude Code と Codex のフックで ask を委ね、記録する

- Purpose: `defer_ask` が有効で影実行でないとき、最終判定 ask を何も出力せずに終え、委任の行を影ログと同じ保存先へ書く
- Specification: `docs/ir/defer-ask.md#REQ-059`, `docs/ir/defer-ask.md#REQ-060`, `docs/ir/defer-ask.md#REQ-062`, `docs/ir/defer-ask.md#REQ-063`, `docs/ir/agents.md#REQ-022`, `docs/ir/cli.md#REQ-018`
- Prerequisites: S1
- May change: `src/hook.rs`, `src/log.rs`, `tests/hook.rs`, `tests/shadow.rs`
- Done when: EX-114、EX-115、EX-117 で "permissionDecision" が "ask" のまま、EX-118・EX-119 で標準出力が空かつ終了コード0、EX-120・EX-122 で "deny"、EX-121 で check の終了コードが1、EX-126・EX-127 で印付きの1行が書かれ、EX-128 で行が増えず、EX-129 で警告して出力が変わらず、EX-130 で影実行の行だけが書かれ、EX-132 で読めない入力に委任の行が書かれない
- Shown by: test — tests/hook.rs に EX-114、EX-115、EX-117、EX-118、EX-119、EX-120、EX-121、EX-132（`defer_ask` を有効にする試験は XDG_STATE_HOME をフィクスチャへ向ける）、src/hook.rs の advisor_output_tests に EX-122（fixture_report の harmful_irreversible で委任の判断の関数を試す）、tests/shadow.rs に EX-126、EX-127、EX-128、EX-129、EX-130（各例に1つずつ）
- Left to the implementer: 記録の印の書き方（決定記録と README に残す）、委任の判断を切り出す関数の形
- Stop and hand back if: `fixture_report` で助言の block を外部送信なしに作れない、または既存の影ログの行の形を変えないと印を足せない

### S3: OpenCode で委任の応答を返し、プラグインが承認要求を出さない

- Purpose: 本体が委任を allow と区別できる応答で返し、プラグインはその応答で guardian 由来の承認要求を出さず、記録の保存失敗を警告として示す
- Specification: `docs/ir/defer-ask.md#REQ-061`, `docs/ir/opencode.md#REQ-048`, `docs/ir/opencode.md#REQ-049`, `docs/ir/opencode.md#REQ-050`
- Prerequisites: S2
- May change: `src/hook/opencode.rs`, `src/log.rs`, `tests/opencode_hook.rs`, `plugins/opencode/src/gate.ts`, `plugins/opencode/src/guardian.ts`, `plugins/opencode/tests/unit/gate.test.ts`, `plugins/opencode/tests/unit/guardian.test.ts`, `plugins/opencode/tests/integration/host.test.ts`, `plugins/opencode/tests/integration/bridge/`, `docs/decision/records/2026-10-04-auto-allow-ask.md`
- Done when: EX-123 で本体の応答が委任を示しプラグインが承認要求を出さずに実行し、EX-124 で OpenCode 自身の確認が残り、EX-125 でバイナリ未検出のとき理由を示して承認を求め、EX-131 で記録の保存失敗の警告付きでも委任のまま承認要求を出さず、既存の応答の解釈が変わらない
- Shown by: test — tests/opencode_hook.rs に本体の応答の形（EX-123 の本体側、EX-131 の本体側）、gate.test.ts に委任の応答の解釈と警告付き応答の解釈、guardian.test.ts に EX-125（存在しない実行ファイルを渡す既存の形。結合試験の PATH は利用者の環境を引き継ぐため使わない）、host.test.ts に EX-123、EX-124、EX-131
- Left to the implementer: 委任の応答の JSON のキーと値、警告の載せ方（決定記録に新しい番号で残す）、プラグインの型の名前
- Stop and hand back if: 固定の OpenCode 2.0.21 の結合試験の仕組みで、OpenCode 自身の確認（EX-124）や保存失敗（EX-131）の状態を作れない

### S4: README で推奨しない理由と危険を説明し、更新履歴に足す

- Purpose: README.md と README-ja.md に `mode.defer_ask` を推奨しない任意設定として説明し、記録の印を契約として書き、CHANGELOG の Unreleased に足す
- Specification: `docs/ir/defer-ask.md#REQ-064`, `docs/ir/defer-ask.md#REQ-062`
- Prerequisites: S2, S3
- May change: `README.md`, `README-ja.md`, `CHANGELOG.md`, `docs/opencode.md`
- Done when: 両方の README に、推奨しないこと、読めない構文と判定できない場合も委任されること、委任の記録の印の書き方があり、`docs/opencode.md` の "Approval behavior and limits" に委任では承認要求を出さないことがあり、既存の見出しが変わらず、`cargo build --release --locked` の後の `python3 site/build.py` が失敗しない
- Shown by: check — `cargo build --release --locked`、`python3 site/build.py`、その後に REQ-064 の how_to_verify の項目を両方の README で人が読んで確かめる
- Left to the implementer: 新しい節の位置と文章
- Stop and hand back if: 既存の見出しを変えないと説明を置けない

### S5: 一つのコミットにまとめ、変更照合の記録を作る

- Purpose: 試験の印が揃った状態で実装・試験・文書を一つのコミットにし、続くコミットで実装者の記録を入れる
- Specification: `docs/ir/defer-ask.md#REQ-059`, `docs/ir/defer-ask.md#REQ-060`, `docs/ir/defer-ask.md#REQ-061`, `docs/ir/defer-ask.md#REQ-062`, `docs/ir/defer-ask.md#REQ-063`, `docs/ir/defer-ask.md#REQ-064`
- Prerequisites: S1, S2, S3, S4
- May change: `.kotowari/changes/implementation.yaml`, 上の各ステップの範囲のファイル
- Done when: この計画の要求（REQ-064 を除く）と EX-114 から EX-132 のすべてで `kotowari query ID` の tests が空でなく、`kotowari check --format json` がこのブランチで変えたファイルと上の ID に誤りを出さず、テストコマンドがすべて通り、実装・試験・文書が pre-commit フックを通って一つのコミットに入り、その後の別のコミットに、呼び出し元が渡した比較の基点に対する `.kotowari/changes/implementation.yaml`（前の比較の内容は置き換える）が入る。review.yaml、最終の HEAD の確定、`kotowari changes --phase review` の実行は、呼び出し元が別のコンテキストのレビューを立てて行い、このステップの完了条件に含めない
- Shown by: check — テストコマンドを上から順に、続けて実装者の自己確認として `kotowari changes --base <呼び出し元が渡した基点> --staged --phase implementation --format json`
- Left to the implementer: コミットメッセージの文面
- Stop and hand back if: pre-commit フックが、この計画の範囲の外の指摘で止まる、または呼び出し元が渡した比較の基点が main への統合などで古くなっている
