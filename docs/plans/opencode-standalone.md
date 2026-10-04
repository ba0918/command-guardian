# Plan: OpenCode の standalone 実行と版に依存しない動作

## Goal

`opencode run --standalone` や管理サービスを無効にした OpenCode でも、command-guardian のプラグインが実際の shell で判定して block を拒否し、それ以外を OpenCode 自身の権限判断に委ねるようになる。接続の失敗や承認要求の作成失敗でもプラグインの都合で実行を止めず警告し、OpenCode を更新しても版の違いだけでプラグインが止まらなくなる。

## Specification

IR は `docs/ir/` にある。判断の経緯と、固定版 OpenCode のコードを調べて確かめた事実は `docs/decision/records/2026-10-04-opencode-standalone.md` にある（A1〜A30、Investigation の節）。A3、A6、A10〜A15 は A21〜A23 で改められている。この計画が扱うのは次のとおり。

- `docs/ir/opencode-standalone.md#REQ-065`、`#REQ-066`、`#REQ-067`、`#REQ-068`、`#REQ-069`
- `docs/ir/opencode-delivery.md#REQ-070`、`#REQ-057`、`#REQ-058`
- `docs/ir/opencode.md#REQ-052`（あわせて REQ-047、REQ-049、REQ-050、REQ-051、REQ-055 に足した委任への参照）
- 例: EX-133 から EX-138、EX-140、EX-141、EX-144 から EX-153、EX-089、EX-106、EX-107、EX-108、EX-109、EX-098、EX-099

各要求は `kotowari query REQ-nnn` で、例は `kotowari query EX-nnn` で読む。REQ-068、REQ-069、REQ-057、REQ-058 は verification が review なので試験を付けない。`docs/ir/FLAGS.md` の FLAG-002 は変更前からの出典の不足で、この計画では扱わない。

## Approach and why

変更の中心は `plugins/opencode/src/index.ts` と `plugins/opencode/src/connection.ts` である。今は接続（`connection`）があるときだけ設定から shell を読み、接続が無いと "/unknown-shell" で判定して、すべての実行が判定不能から承認要求へ進み、承認を作る `approve` が例外を出して止まる。

まず `connect` の結果を三つに分ける。「接続できた」「接続オプションが未指定で接続できない」「接続オプションを明示したが不完全か、接続や認証に失敗した」である。明示した設定の失敗では管理サービスの自動接続へ切り替えない（REQ-052）。後の二つを「接続が無い」状態として同じ経路に入れ、明示した設定の失敗だけはプラグインの標準エラー（`console.warn`）へ、読み込み時に1回と実行ごとに1回警告する（REQ-065、決定 A22、A25）。

接続が無い状態では、ツールの包みで "/unknown-shell" による判定をしない。shell の `create.before` フックが受け取る実際の shell・cwd・コマンドで guardian に判定させる（REQ-065）。block なら例外を投げて拒否する。allow と、承認が要る結果（ask、判定不能、Bash 以外の shell、一致を確かめられない場合）は、guardian 由来の確認も停止も出さずにフックから戻り、OpenCode 自身の権限判断に委ねる（REQ-066）。本体の応答で影実行と確定できた場合は何もしない（REQ-048）。今の `create.before` は AsyncLocalStorage の入れ物が無いと何もせず戻る（エージェントの実行と人が直接打つ shell を分けるため、REQ-047）。接続が無い経路では判定が `create.before` だけになるので、バックグラウンド実行と Code Mode 経由の実行でもこの入れ物が引き継がれることを `serve --stdio` の試験で確かめる。`create.before` で例外を投げれば起動が止まることは、計画の検討時に固定版で実測した。

接続がある状態では、今の HTTP の承認の経路を使う（REQ-050〜052）。承認要求の作成が失敗したとき、または作成の応答を受け取れなかったとき（成功したか分からない場合を含む）は、そのたびに標準エラーへ警告し、その実行を OpenCode 自身の権限判断に委ねる。承認待ちは作成の応答を受け取った時点から始まり、その後の通信の終了やエラーでは従来どおり取りやめる（REQ-067、REQ-053、決定 A23、A24）。作れなかったものとして扱った実行に残った guardian の確認へ後から返答があっても、その実行には何もしない。

版の扱いは二か所を消す。`index.ts` の `ctx.app.version!=="2.0.21"` の例外と、`connection.ts` の `info.version==="2.0.21"` の条件である（REQ-070、REQ-052）。代わりに、読み込み時に頼る API（`ctx.shell.hook`、`ctx.permission.hook`、`ctx.tool.transform` など、実装が実際に使うもの）がそろっているかを確かめ、足りなければその名前を示して `setup` を失敗させる（REQ-070、決定 A20）。結合試験の `host()` が固定版 "opencode v2.0.21" を確かめる部分は、試験の再現のための環境なので残す（決定 A18）。

結合試験は `plugins/opencode/tests/integration/host.test.ts` の `host()` に、`serve --stdio --port 0` で起動する選択肢を足して行う。standalone はこの起動経路を子プロセスで使うので、モデルなしで同じ状態を作れる（決定 A8）。固定版の stdio モードについて分かっている事実は次のとおり。標準入力が閉じると serve は終わるので、標準入力はパイプのままにして `stopHost` で閉じる。待ち受けの URL は "server listening on" ではなく標準出力の JSON（`{"url":...}`）で示される。パスワードは起動時に OPENCODE_SERVER_PASSWORD から読まれた後に環境から消されるので、試験は OPENCODE_SERVER_PASSWORD を設定して HTTP で操作できる。プラグインの接続オプションは外す。EX-140 は、PID の合わない service.json を、接触を記録する待ち受けに向けて作る。OpenCode 自身の権限の規則は `host()` の既存の `nativeDeny` と同じ要領で、試験ごとに許可・確認・拒否を設定する。

このリポジトリの pre-commit フックの `kotowari check` は、ステージしたファイルが `*.{rs,md,yaml}` に当たるときだけ走り、この計画の要求と例に試験の印が揃うまで失敗する。TypeScript と試験だけのコミットは止まらないので、S1 から S3 はそれぞれ関心ごとにコミットしてよい。文書（`.md`）と `.kotowari/changes/implementation.yaml` のコミットは、すべての印が揃った後に行う。実装者は `--no-verify` を使わない。

統合の前の変更照合は PROJECT.md の手順に従う。ブランチ全体の比較の基点は main との分岐点 46bf07490feb15418df6e0ae69a3b50922ef39ea、候補の先端は記録のコミット後に呼び出し元が確定する HEAD とする。実装者が `.kotowari/changes/implementation.yaml` を、実装と別のコンテキストのレビューが `.kotowari/changes/review.yaml` を、それぞれ自分で書く。統合の条件は `kotowari check --format json` と `kotowari changes --base 46bf07490feb15418df6e0ae69a3b50922ef39ea --head <確定した HEAD> --phase review --format json` の両方が 0 で終わることと、テストコマンドが通ることである。

## Scope of change

- `plugins/opencode/src/index.ts`
- `plugins/opencode/src/connection.ts`
- `plugins/opencode/src/approval.ts`
  - 承認要求の作成失敗と、応答を受け取れない場合の扱いのため
- 経路を分けるための `plugins/opencode/src/` の下の新しいファイル
- `plugins/opencode/src/gate.ts`
  - 新しい経路から既存の型や処理を使うのに必要な場合だけ
- `plugins/opencode/tests/unit/` の下（既存のファイルへの追加と新しいファイル）
- `plugins/opencode/tests/integration/host.test.ts`、`plugins/opencode/tests/helpers/host-process.ts`
  - 既存の試験の期待は、REQ-052 の改定（版の照合をやめる、明示した設定の失敗と承認要求の作成失敗で止めない）に当たるものだけ改める
- `docs/opencode.md`、`README.md`、`README-ja.md`
  - 既存の見出しは変えない。`site/build.py` が README の見出しで節を探すため
- `site/template.html`
  - 紹介ページの OpenCode の版を対応の条件とする記述だけ（決定 A30）
- `CHANGELOG.md`
  - `## [Unreleased]` の節だけ
- `.kotowari/changes/implementation.yaml`
  - `.kotowari/changes/review.yaml` は実装と別のコンテキストのレビューが書く。実装者は書かない

Rust 本体（`src/`、`crates/`）は変えない。

## Step order and prerequisites

S1 を最初にする。版の例外が残っていると、版の違う環境での試験が書けず、後の試験もこの例外に引っかかりうるからだ。S2 は S1 の後にする。`connect` の三つの結果を S2 で作り、S3 は接続がある状態の失敗を扱うので S2 の後にする。S4 の文書は S1〜S3 で決まった振る舞いと頼る API の一覧を書くので、その後にする。S5 は全部の後にする。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-070, REQ-052 | EX-144, EX-145, EX-106, EX-107, EX-108 |
| S2 | REQ-065, REQ-066, REQ-052 | EX-133, EX-134, EX-135, EX-136, EX-137, EX-138, EX-140, EX-141, EX-146, EX-089, EX-109 |
| S3 | REQ-067 | EX-147, EX-148, EX-149, EX-150, EX-151, EX-152, EX-153 |
| S4 | REQ-068, REQ-069, REQ-057, REQ-058 | EX-098, EX-099（review） |
| S5 | 上の全部の試験の印と、実装者の変更照合の記録 | 上の全部 |

## Left to the implementer

- `connect` の三つの結果の表し方、モジュールの分け方、関数と型の名前
- 接続が無いときに、ツールの包みで判定を省くか、判定せずに素通しするかの形。ただし判定に使う入力は `create.before` の実際の入力であること
- 頼る API の一覧の中身。実装が実際に使う API だけを並べる
- 警告の文面
- 試験の名前と、既存の試験ファイルのどこに足すか

## Stop conditions

- `create.before` から例外を投げても、固定版で block の起動を止められない
- `serve --stdio` の起動で、試験からホストのツールと承認の API に触れられず、REQ-069 の検証ができない
- OpenCode 自身の権限の規則（許可・確認・拒否）を、結合試験で試験ごとに設定できない
- 承認要求の作成の失敗や、応答を受け取る前の通信の切断を、結合試験で作れない
- プラグインの `console.warn` を、試験からホストの標準エラーとして観測できない
- `serve --stdio` のバックグラウンド実行や Code Mode 経由の実行で、`create.before` に AsyncLocalStorage の入れ物が引き継がれず、block を拒否できない
- 接続できる場合の既存の HTTP の経路の試験の期待を、REQ-052 の改定に当たらない理由で変えないと通らない
- 実モデルの呼び出し、または利用者の普段の OpenCode の設定やサービスへの接触が必要になる
- pre-commit フックや push 前の検査を飛ばさないと先へ進めない
- 比較の基点が、別のブランチ（`feat/defer-ask` など）の main への統合で古くなっている

## Test command

```sh
bun install --frozen-lockfile --cwd plugins/opencode
bun run --cwd plugins/opencode typecheck
bun run --cwd plugins/opencode test:unit
cargo build --locked
GUARDIAN_TEST_BIN="$PWD/target/debug/command-guardian" OPENCODE_TEST_BIN="$PWD/plugins/opencode/node_modules/@opencode/cli-linux-x64/bin/opencode" bun run --cwd plugins/opencode test:integration
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
kotowari check --format json
```

## Out of scope

- FLAG-002（REQ-057、EX-106、EX-088 の、変更前からの出典の不足）
- 最新版の OpenCode を自動で試す仕組み（決定 A18）
- 接続できて承認要求を作れる場合の承認の経路の変更（決定 A7）
- Rust 本体の変更
- 版の更新とリリース。CHANGELOG の Unreleased に足すだけにする
- 実モデルを使う評価と、`opencode run --standalone` の通し実行

## Steps

### S1: 版の照合を外し、頼る API を読み込み時に確かめる

- Purpose: OpenCode の版を理由にした例外と接続の拒否を消し、代わりに頼る API の有無を読み込み時に確かめる
- Specification: `docs/ir/opencode-delivery.md#REQ-070`, `docs/ir/opencode.md#REQ-052`
- Prerequisites: なし
- May change: `plugins/opencode/src/index.ts`, `plugins/opencode/src/connection.ts`, `plugins/opencode/src/` の下の新しいファイル, `plugins/opencode/tests/unit/`, `plugins/opencode/tests/integration/host.test.ts`
- Done when: OpenCode が固定版と違う版を報告しても版を理由にした例外や警告が出ず（EX-144。allow のコマンドが実行されることの確認は、接続が無い経路ができる S2 の後で同じ試験に足してよい）、頼る API が欠けた文脈では足りない名前を示して読み込みが失敗し（EX-145）、自動接続が版を照合せず PID などの照合だけで接続を採否する（EX-106、EX-107、EX-108）
- Shown by: test — EX-144 と EX-145 は、版や API を差し替えた文脈でプラグインの setup を動かす単体試験。EX-106、EX-107、EX-108 は connection の単体試験か既存の結合試験で、版の照合をやめた期待に改める。各例に1つずつ
- Left to the implementer: API の確認を関数に切り出す形、単体試験で文脈を差し替える方法。ただし setup を動かす単体試験では、XDG_STATE_HOME を一時ディレクトリにし、PATH には偽の guardian を置き、利用者の service.json や実際の guardian に触れないこと
- Stop and hand back if: プラグインの setup を単体試験で動かす手段が無く、EX-144 と EX-145 を固定版の結合試験でも作れない

### S2: 接続が無いときに実際の shell で判定し、block 以外を OpenCode に委ねる

- Purpose: connect の結果を三つに分け、接続が無い状態では create.before の実際の入力で判定し、block だけを拒否して、それ以外を OpenCode 自身の権限判断に委ね、明示した設定の失敗を警告する
- Specification: `docs/ir/opencode-standalone.md#REQ-065`, `docs/ir/opencode-standalone.md#REQ-066`, `docs/ir/opencode.md#REQ-052`
- Prerequisites: S1
- May change: `plugins/opencode/src/index.ts`, `plugins/opencode/src/connection.ts`, `plugins/opencode/src/` の下の新しいファイル, `plugins/opencode/src/gate.ts`, `plugins/opencode/tests/unit/`, `plugins/opencode/tests/integration/host.test.ts`, `plugins/opencode/tests/helpers/host-process.ts`
- Done when: `serve --stdio` で起動したホストで、allow が "/bin/bash" で判定され guardian 由来の確認なしで実行され（EX-133）、block が拒否され（EX-134）、OpenCode の規則がすべてを許可するとき ask（EX-136）、判定のタイムアウト（EX-141）、コマンドの無いリダイレクトの切り詰め（EX-146）が guardian 由来の確認も停止も無く実行され、OpenCode の規則が確認を求めるとき ask が OpenCode 自身の shell の確認になり拒否で実行されず（EX-137）、検証に失敗した登録には通信せずこの扱いに入り（EX-140）、明示した接続設定の認証失敗や不完全な設定では自動接続へ切り替えずにホストの標準エラーへ警告してこの扱いに入り（EX-135、EX-089、EX-109）、接続できる場合は従来の承認の経路を使い（EX-138）、バックグラウンド実行と Code Mode 経由の実行でも block が拒否され、プラグインのエラーの文面に standalone へ明示の接続設定を求めるものが残らない
- Shown by: test — host.test.ts の `serve --stdio` の起動で EX-133、EX-134、EX-136、EX-137、EX-140、EX-141、EX-146、既存の起動で EX-135、EX-138、EX-089、EX-109（既存の試験の期待を REQ-052 の改定に合わせる）。各例に1つずつ。EX-133 は、偽の guardian などで判定に渡った shell が "/bin/bash" であることまで確かめる（接続が無いときは ask も判定不能も実行されるため、実行されたことだけでは区別できない）。`serve --stdio` でバックグラウンド実行と Code Mode 経由の block の拒否も確かめる
- Left to the implementer: `host()` がホストの標準エラー（logs）を試験へ返すようにする形、connect の三つの結果の表し方、接続が無いときのツールの包みの形、EX-141 の判定のタイムアウトを作る方法（既存の `host()` の `controlPeer` と同じ要領の偽の guardian など）
- Stop and hand back if: 計画の Stop conditions の最初の三つのどれかに当たる

### S3: 接続がある状態で承認要求を作れないときに警告して委ねる

- Purpose: 承認要求の作成が失敗したとき、作成の応答を受け取れなかったとき、作成の前に承認の経路（イベントの通信）がすでに止まっていてつなぎ直せないときは、警告して OpenCode 自身の権限判断に委ね、承認待ちに入った後の通信の終了と、作成中の実行の中断やプラグインの解除では従来どおり取りやめる
- Specification: `docs/ir/opencode-standalone.md#REQ-067`
- Prerequisites: S2
- May change: `plugins/opencode/src/index.ts`, `plugins/opencode/src/approval.ts`, `plugins/opencode/src/` の下の新しいファイル, `plugins/opencode/tests/unit/`, `plugins/opencode/tests/integration/host.test.ts`, `plugins/opencode/tests/helpers/host-process.ts`
- Done when: 承認要求の作成が失敗するとホストの標準エラーに警告が出て OpenCode 自身の権限判断で実行の可否が決まり（EX-147）、作成の要求を送った後に応答を受け取る前に通信が切れても同じ扱いになり（EX-149）、承認待ちに入った後の通信の終了ではその実行が起動しない（EX-148）。作成中に通信が切れた後に作成の応答が ask で届けばその実行を取りやめて OpenCode に委ねず（EX-150）、作成の応答より前に届いた拒否は作成が失敗しても守り（EX-151）、通信が終わった後に承認要求が要る実行が来たときは通信をつなぎ直して承認を求め（EX-152）、つなぎ直せなければ警告して OpenCode 自身の権限判断に委ねる（EX-153）。`tests/unit/approval.test.ts` の作成失敗を承認と見なさない試験（今は EX-089 の印）は期待と印を REQ-067・EX-147 に改め、既存の REQ-053 の試験は、作成の応答を確かめてから切断するよう同期だけを直してよい
- Shown by: test — EX-147 は、作成を失敗させる偽の仕組みを使う結合試験（管理サービスへ接続した既存の起動）で必須とし、approval の単体試験を足してもよい。EX-149 は応答の前に切る偽の仕組みを使う結合試験か approval の単体試験。EX-148 は既存の通信の終了の試験の印を確かめるか、足りなければ足す。EX-150、EX-151、EX-152、EX-153 は approval の単体試験。各例に1つずつ
- Left to the implementer: 作成の失敗と応答の欠落を作る偽の仕組みの形
- Stop and hand back if: 承認要求の作成の失敗や、応答を受け取る前の通信の切断を、試験で作れない

### S4: 導入手順で standalone と委任と頼る API を説明し、更新履歴に足す

- Purpose: docs/opencode.md と README で、standalone が接続設定なしで動くこと、接続が無いときは承認が要る実行を OpenCode 自身の権限判断に委ねること、対応の範囲を頼る API の一覧で示すこと、他のプラグインの上書きと読み込み失敗後の扱いを保証しないことを説明し、CHANGELOG の Unreleased に足し、紹介ページ（site/template.html）の版を対応の条件とする記述を改める
- Specification: `docs/ir/opencode-standalone.md#REQ-068`, `docs/ir/opencode-standalone.md#REQ-069`, `docs/ir/opencode-delivery.md#REQ-057`, `docs/ir/opencode-delivery.md#REQ-058`
- Prerequisites: S1, S2, S3
- May change: `docs/opencode.md`, `README.md`, `README-ja.md`, `site/template.html`, `CHANGELOG.md`
- Done when: 文書と紹介ページ（site/template.html）に OpenCode の版を対応の条件とする記述が残らず（結合試験の環境としての固定版の記述は残してよい）、頼る API の一覧と standalone と接続が無いときの委任の説明があり、standalone に明示の接続設定を求める案内が無く、既存の見出しが変わらず、`cargo build --release --locked` の後の `python3 site/build.py` が失敗しない
- Shown by: check — `cargo build --release --locked`、`python3 site/build.py`、`rg -n "2\.0\.21" README.md README-ja.md docs/opencode.md site/template.html` の結果が試験の環境の記述だけであることを確かめ、その後に REQ-068 と REQ-057 の how_to_verify の項目を、プラグインのエラーの文面（`plugins/opencode/src/` の Error と警告の文字列）も含めて人が読んで確かめる
- Left to the implementer: 文章と、説明を置く節の中の位置
- Stop and hand back if: 既存の見出しを変えないと説明を置けない

### S5: 印が揃った後に文書をコミットし、実装者の変更照合の記録を作る

- Purpose: 試験の印が揃った状態で、残りの実装・試験と文書をコミットし、続くコミットで実装者の記録を入れる
- Specification: `docs/ir/opencode-standalone.md#REQ-065`, `docs/ir/opencode-standalone.md#REQ-066`, `docs/ir/opencode-standalone.md#REQ-067`, `docs/ir/opencode-delivery.md#REQ-070`, `docs/ir/opencode.md#REQ-052`
- Prerequisites: S1, S2, S3, S4
- May change: `.kotowari/changes/implementation.yaml`, 上の各ステップの範囲のファイル
- Done when: この計画の unit の要求と例のすべてで `kotowari query ID` の tests が空でなく、`kotowari check --format json` がこのブランチで変えたファイルと上の ID に誤りを出さず、テストコマンドがすべて通り、実装・試験・文書がすべて pre-commit フックを通ってコミットされ、その後の別のコミットに、呼び出し元が渡した比較の基点に対する `.kotowari/changes/implementation.yaml`（前の比較の内容は置き換える）が入る。review.yaml、最終の HEAD の確定、`kotowari changes --phase review` の実行は呼び出し元が行い、このステップの完了条件に含めない
- Shown by: check — テストコマンドを上から順に、続けて実装者の自己確認として `kotowari changes --base <呼び出し元が渡した基点> --head HEAD --phase implementation --format json`
- Left to the implementer: コミットメッセージの文面
- Stop and hand back if: pre-commit フックが、この計画の範囲の外の指摘で止まる、または呼び出し元が渡した比較の基点が main への統合などで古くなっている
