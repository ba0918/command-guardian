# Plan: OpenCode V2の実行前連携

## Goal

利用者が同梱のTypeScriptプラグインをOpenCode V2へ登録し、既存のguardian設定による判定をエージェントのshell実行前に適用できるようにする。

## Specification

仕様の承認済みコミットは97b046e86ac0507cccbf72c5dd5d475a56274393。IRは `docs/ir/` にある。まず各要求と対応する例を読む。

- [REQ-016](../ir/cli.md#REQ-016)、[REQ-018](../ir/cli.md#REQ-018)、[REQ-045](../ir/cli.md#REQ-045)
- [REQ-047](../ir/opencode.md#REQ-047)、[REQ-048](../ir/opencode.md#REQ-048)、[REQ-049](../ir/opencode.md#REQ-049)、[REQ-050](../ir/opencode.md#REQ-050)、[REQ-051](../ir/opencode.md#REQ-051)、[REQ-052](../ir/opencode.md#REQ-052)、[REQ-053](../ir/opencode.md#REQ-053)、[REQ-054](../ir/opencode.md#REQ-054)、[REQ-055](../ir/opencode.md#REQ-055)
- [REQ-056](../ir/opencode-delivery.md#REQ-056)、[REQ-057](../ir/opencode-delivery.md#REQ-057)、[REQ-058](../ir/opencode-delivery.md#REQ-058)
- 挙動の決定と委譲の境界は [OpenCode V2の決定記録](../decision/records/2026-10-02-opencode-v2-hook.md)、開発方式は [計画の方式記録](../decision/records/2026-10-02-opencode-v2-plan.md)を読む。

## Approach and why

先にRustの入出力を定め、次にプラグインの実行前判定と承認待ちを別々に試験する。その後、隔離したネイティブ承認APIを通す。最後に配布と導入を検証する。仕様の型検査や出典調査は実機連携の成功を意味しない。

再利用を層ごとに選ぶ。以下は実装開始前の探索結果と採用方針であり、実装担当は採用した版・理由・契約を方式記録へ追記する。

| 層 | 採用・自作 | 根拠 |
|---|---|---|
| 判定・設定・影ログ | 既存処理を採用 | `src/hook.rs` のEngine読込と `src/log.rs` を使い、判定エンジンをプラグインへ複製しない |
| エージェント実行への接続 | 上流のtool transformを採用 | [V2 plugin guide](https://opencode.ai/v2/docs/build/plugins)のexecutor置換は元のshellを委譲先として保持できる。権限hook単体はリダイレクトだけの入力を捕捉できない |
| バイナリ起動・中断 | ホストの標準プロセス機能を採用 | Bash経由の文字列組立てを避け、標準入力でコマンド本文を渡す。ホストで使えるNode互換APIを先に検討する |
| HTTP・承認イベント | 公式クライアントを採用候補 | [V2 client guide](https://opencode.ai/v2/docs/build/client)と2.0.21の型を確認済み。プラグインcontextだけには要求作成APIがないため、同一サーバーへの明示接続が必要 |
| 実行と承認の対応付け | 最小の状態処理を自作 | 通信や承認UIはホストへ任せ、実行ごとの要求IDと中断状態のみ保持する。独自の履歴DBや期限は不要 |
| TypeScript検証 | Bun testとTypeScriptを採用 | 上流2.0.21のplugin/clientもBunで試験している。追加のテストフレームワークを入れない |
| 同梱配布 | 既存tar.gz工程を拡張 | `.github/workflows/ci.yml` の既存成果物に加え、ローカルでも同じ組立てを実行する |
| 要求・変更照合 | 既存kotowariを採用 | TypeScriptは汎用の印で関連付ける。言語解析器・新しい検査サービスを作らない |

上流資料は [v2.0.21](https://github.com/anomalyco/opencode/tree/v2.0.21)へ固定して確認する。現行のオンライン説明との差を上流の同版の型・実際の挙動で検証する。内部APIへの依存や対応条件の追加が必要なら止める。

## Scope of change

- `src/hook.rs` と必要な新規 `src/hook/` 内のアダプター。`src/main.rs` は入口・共通関数への最小限の配線のみ。
- `tests/hook.rs`、`tests/shadow.rs`、`tests/cli.rs`、必要な新規 `tests/opencode_hook.rs`。
- 新規 `plugins/opencode/` のTypeScript、package.json、tsconfig、lockfile、モデルを呼ばないunit/integration試験。依存展開先と生成物はコミットしない。
- `.kotowari/config.yaml`、`.kotowari/changes/implementation.yaml`、`.kotowari/changes/review.yaml`、`.gitignore` の新しい生成物の除外のみ。
- 新規 `scripts/package-release.sh`、`.github/workflows/ci.yml` のTypeScript検証と同梱工程、`PROJECT.md` の新しい検証コマンドの案内。
- `README.md`、新規 `docs/opencode.md`、`CHANGELOG.md` のUnreleased。
- 既存の委譲範囲内の具体化に限る `docs/decision/records/` と関連IRの追加。承認済み要求の変更・削除は禁止。

## Step order and prerequisites

S1 → S2 → S3 → S4 → S5 → S6。単一の実装ブランチ名は `opencode-v2-hook` とする。実装担当とreviewerは同じ作業ツリーへ同時に書かない。作業場所の選択は既存のworktree規約に従う。

Rustの既存チェックを使える環境に加え、Bun、TypeScriptのロック済み依存、OpenCode V2 2.0.21、kotowari 0.3.0以降が必要。試験用サーバーは一時環境で起動し、Bashと入力を変えないhook構成を明示する。実行ファイルの場所は試験用環境変数 `OPENCODE_TEST_BIN` で渡し、版を確認してから起動する。これは導入用の設定ではない。

仕様コミット時には利用者の承認でkotowariフックだけを一度省略した。既定のkotowariが0.2.0の場合は、既に導入された0.3.0以降をこの作業のPATHで選ぶ。個人のmise設定を変更しない。未実装テスト不足31件が承認時の基準であり、計画対象の不足は最後に全て解消する。中間コミットでフックが止まっても、今回の一度の省略許可を流用しない。

計画のコミット自体も、現在のpre-commitでは同じテスト不足で止まる。計画への承認だけをフック省略の許可と解釈せず、計画コミット前に利用者から別の明示許可を得る。得られなければステージ済みの計画で止め、S1へ進まない。中間コミットは、全対象テストを揃えて通常の検査が通るまで待つことができる。

変更照合のbranch-wide baseは計画コミット後、実装ブランチを切るときに呼出元がGitから完全なSHAで確定する。最終candidateもGitから確定し、二つの照合記録の自己申告でbaseを決めない。

## Verification map

| Step | Requirements | Examplesと確認 |
|---|---|---|
| S1 | REQ-016、REQ-018、REQ-045、REQ-048、REQ-055 | EX-025、EX-018、EX-071の既存回帰、EX-080、EX-104、EX-105の本体側 |
| S2 | REQ-047、REQ-048、REQ-049、REQ-051、REQ-055 | EX-078、EX-079、EX-080、EX-081、EX-082、EX-083、EX-086、EX-087、EX-094、EX-095、EX-102、EX-103、EX-104、EX-105 |
| S3 | REQ-050、REQ-052、REQ-053、REQ-054 | EX-084、EX-085、EX-088、EX-089、EX-090、EX-091、EX-092、EX-093、EX-102。要求拒否・session中断・unload・作成との競合も条文から試験する |
| S4 | REQ-047〜REQ-055、REQ-058 | unitの代用品だけで完了せず、実バイナリと2.0.21のネイティブ承認APIで連携を観測する。EX-100、EX-101をreview evidenceで確認 |
| S5 | REQ-056、REQ-057 | EX-096〜EX-099をローカルアーカイブと隔離登録で確認。公開先の既存binary導入経路を壊さない |
| S6 | 全対象 | 対象IDの印、reviewの成果物、二役の照合記録と最終candidateを照合する |

REQ-047〜REQ-055とEX-078〜EX-095・EX-102〜EX-105の全てに意味を検証する試験と `@kotowari` の印を付ける。review検証のREQ-045・REQ-056〜REQ-058とその例は印だけで完了扱いにせず、how_to_verifyに沿った観測を残す。既存例には必要な回帰だけを加え、同じ印の数を増やすための試験は作らない。

## Left to the implementer

[D1](../decision/records/2026-10-02-opencode-v2-hook.md#D1)の範囲で、JSONの具体的なキー、接続・認証設定の表現、内部型と分割、必須依存の版を選び、理由と外部契約を決定記録に残す。OpenCode依存の型はアダプターに閉じ込め、判定の合成と待機状態遷移は純粋に試験できる部品へ分ける。

開発packageの `typecheck`、`test:unit`、`test:integration` script名とコマンドは計画で固定する。内部試験名・fixture配置は自由だが、振る舞いと対象IDが追えること。runtimeでの依存解決と同梱範囲は採用パッケージの方式に合わせて記録し、npmへの公開や別サービスを足さない。

## Stop conditions

- 承認済みの意味が足りない、承認済み要求の変更が必要、委譲範囲外の設定・入力・保存状態が必要なら、実装せず利用者へ戻す。
- 破壊的・権限を要する・外部から見える操作は実施前に利用者へ戻す。公開、push、tag、個人設定の変更はこの計画の実行許可に含まれない。
- 既存のClaude/Codex/check契約へ影響が広がる、または方式を変えても検証が進まない場合は止める。
- 公開V2 APIで承認の強制、入力の一致、検知済み中断後の未実行を満たせない場合は、保証を黙って弱めず根拠を返す。
- TypeScriptの同版公式パッケージを再現可能に導入できない、型escapeが必要、モデルなしで必須連携を観測できない場合は代案を返す。

## Test command

TypeScript依存は `bun install --frozen-lockfile --cwd plugins/opencode`。unitの基本コマンドは `bun run --cwd plugins/opencode test:unit`、型検査は `bun run --cwd plugins/opencode typecheck`。packageのtypecheckはロック済みTypeScriptの `tsc --noEmit` とし、unitは `bun test tests/unit`、integrationは `bun test tests/integration` を使う。初回のlockfile作成後からfrozenを必須にする。

S4で `cargo build --locked` 後、`GUARDIAN_TEST_BIN="$PWD/target/debug/command-guardian" OPENCODE_TEST_BIN="<2.0.21の実行ファイル>" bun run --cwd plugins/opencode test:integration` を実行する。GNU/muslそれぞれのbuild出力でも対象binaryを切り替えて実バイナリ連携を確認する。OpenCodeサーバーの起動オプション・一時状態・認証は2.0.21の公開CLIで確かめてfixtureへ閉じ込める。モデル呼出しと常用サービス発見は行わない。CIでは選定したBunの版とOpenCode 2.0.21をjob内へ固定版で導入し、公式の取得元・検証方法・導入コマンドを記録する。版を実行時にも確認し、OPENCODE_TEST_BINを明示する。既設ツールやcheckout外の個人キャッシュに依存しない。

Rustは `PROJECT.md` のfmt・clippy・全test・release buildをGNU/musl両targetで実行する。TypeScript試験もCIへ追加する。最後は作業用PATHで選択したkotowariを使って `kotowari check --format json` と `kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format json` を実行する。要求の印は `kotowari query ID` で確認する。

## Out of scope

V1、未検証の後続V2、他shellの解析対応、人が直接実行するshell、MCP内部、入力変更hookへの最終入力保証、無応答の即時検知、残った確認表示の完全な消去、実行済みbackgroundの停止、モデル必須試験、個人設定の自動変更、上流改修、npm公開、リリース公開。

## Steps

### S1: RustのOpenCode用入口と影実行応答

- Purpose: 既存のエージェント契約を保持し、プラグインが本体の判定と影実行を区別できる入口を追加する。
- Specification: docs/ir/cli.md#REQ-016, docs/ir/cli.md#REQ-018, docs/ir/cli.md#REQ-045, docs/ir/opencode.md#REQ-048, docs/ir/opencode.md#REQ-055
- Prerequisites: 承認・コミット済みの計画、クリーンな実装ブランチ、作業用kotowari 0.3.0以降。具体的な入出力はD1内で根拠とともに記録してからfixtureへ固定する。
- May change: src/hook.rs, src/hook/, src/main.rsの最小限の配線, tests/hook.rs, tests/shadow.rs, tests/cli.rs, tests/opencode_hook.rs, docs/decision/records/内の方式記録, 委譲内の関連IR追加。
- Done when: 実バイナリ試験で三判定・正常な影実行・不正入力・対応外shellの応答を区別でき、既存hook/checkの回帰がなく、hookヘルプが新しい入口を案内する。
- Shown by: test RED→GREEN→REFACTORをcargo test -p command-guardian --locked --test hook --test shadow --test cliで観測し、新規opencode_hookを作る場合は同testも実行する。REQ-045は実際のヘルプでreviewする。
- Left to the implementer: D1内のJSONキーと内部分割。既存Engineとshadow logを採用し、プロトコルの応答型を記録する。
- Stop and hand back if: 既存hookの静かな失敗契約を変更する必要がある、または設定をプラグインへ複製する必要が出る。

### S2: 実行前の判定アダプターとunit検証

- Purpose: 元のshell executorへの委譲前に判定を入れ、入力と実行の対応・影実行の優先順位・異常時の分岐を試験する。
- Specification: docs/ir/opencode.md#REQ-047, docs/ir/opencode.md#REQ-048, docs/ir/opencode.md#REQ-049, docs/ir/opencode.md#REQ-051, docs/ir/opencode.md#REQ-055
- Prerequisites: S1の応答契約と2.0.21の公開型。Bunとロック済み依存の導入を確認する。
- May change: plugins/opencode/, .kotowari/config.yamlのTS試験・変更対象, .gitignoreの依存展開先・生成物, docs/decision/records/内の方式記録, 委譲内の関連IR追加。
- Done when: executorの元入力・実効cwd・shellの対応を保ち、通常/background/Code Modeの共通shell経路で判定し、承認前のdelegate呼出しをunit試験で防げる。対応外の直接shell/MCPを巻き込まない。
- Shown by: test bun run --cwd plugins/opencode test:unitでRED→GREEN→REFACTORを観測し、bun run --cwd plugins/opencode typecheckで公開型との一致を確認する。
- Left to the implementer: 内部関数名・型・fixture分割、標準プロセスAPIの選定。既存設定は本体へ委譲し、guardian invocationはargvとstdinを使う。
- Stop and hand back if: transformだけでは対象経路を覆えない、cwd/shellの解決に非公開APIが必要、または接続設定以外の新しい利用者設定が必要になる。

### S3: ネイティブ承認と中断の状態管理

- Purpose: 実行ごとの承認要求・中断状態を接続し、要求作成と返信や中断の競合で実行を取り違えないようにする。
- Specification: docs/ir/opencode.md#REQ-050, docs/ir/opencode.md#REQ-052, docs/ir/opencode.md#REQ-053, docs/ir/opencode.md#REQ-054
- Prerequisites: S2。イベント購読を要求作成より前に開始できることを公開型と試験で確認する。
- May change: plugins/opencode/, docs/decision/records/内の方式記録, 委譲内の関連IR追加。
- Done when: 各要求IDに対応する承認だけが対象実行を進め、拒否・session中断・unload・検知した通信終了で未実行を保つ。要求作成直後の返信と作成中の中断も取り違えず、取りやめた実行は後の返信・再接続で開始しない。
- Shown by: test bun run --cwd plugins/opencode test:unitで早期返信・並行実行・中断前後・要求作成競合のRED→GREEN→REFACTORを観測し、typecheckを通す。
- Left to the implementer: 公式clientの接続・認証表現と未保存要求のID対応付け。待機を独自に再接続して再開する設計は選ばない。
- Stop and hand back if: 保存済み許可が今回の要求を自動省略する、承認結果と実行を対応付けられない、または解除後の古いexecutorが実行を開始し得る。

### S4: 隔離V2サーバーで実バイナリ連携を検証

- Purpose: モデルを呼ばず、unitで仮定した承認APIと実行経路の契約を実環境で確かめる。
- Specification: docs/ir/opencode.md#REQ-047, docs/ir/opencode.md#REQ-048, docs/ir/opencode.md#REQ-049, docs/ir/opencode.md#REQ-050, docs/ir/opencode.md#REQ-051, docs/ir/opencode.md#REQ-052, docs/ir/opencode.md#REQ-053, docs/ir/opencode.md#REQ-054, docs/ir/opencode.md#REQ-055, docs/ir/opencode-delivery.md#REQ-058
- Prerequisites: S3、実guardian binary、版を確認したV2 2.0.21。テストfixture専用HOME・XDG・状態・認証・作業場所を準備し、git分類対象はtarget配下へ置く。
- May change: plugins/opencode/のintegration試験と必要な修正, tests/opencode_hook.rs, .github/workflows/ci.ymlのTS連携検証, PROJECT.mdの試験コマンド, docs/decision/records/内の方式記録, 委譲内の関連IR追加。
- Done when: 実OpenCodeホストの通常の登録機構で本番pluginをロードし、transform後のホスト由来のshell executorを公開契約に沿ってモデルなしで呼び、実binaryとnative承認APIにより承認前の未実行・許可・拒否・自動once・保存済み許可後の新要求・中断・並行実行を確かめ、両Rust targetで再現できる。fixture内でwrapperを直接組み立てた試験だけでは完了にしない。実際のCLI run --autoの応答方針との対応も記録する。
- Shown by: test Test commandのtest:integrationを実行し、サーバー版・実行前後のfixtureファイル状態・要求ID・返信・実行取消を観測する。通常/background/Code Modeで同じwrapperが使われることは上流経路とモデルなし試験で確認する。
- Left to the implementer: 公開APIと公開executor契約を使うfixture構成、BunとOpenCodeのjob内の固定版導入方法。自動onceの試験とCLI run --autoのソース確認を区別し、モデルを使わない試験を実モデルE2Eと呼ばない。
- Stop and hand back if: モデルなしでホスト登録後のexecutorを公開契約に沿って呼べない、固定版ツールをCIへ再現可能に導入できない、常用サービスへの副作用が起きる、または上流仕様の仮定と実挙動が食い違う。

### S5: 同梱成果物と導入手順

- Purpose: 個人設定を変更せず、同じ版のプラグインを取得・登録できる配布物と手順を用意する。
- Specification: docs/ir/opencode-delivery.md#REQ-056, docs/ir/opencode-delivery.md#REQ-057, docs/ir/cli.md#REQ-045
- Prerequisites: S4の連携検証とruntime依存の確定。本体の版の正本はCargo.tomlのpackage.version。
- May change: scripts/package-release.sh, .github/workflows/ci.ymlの同梱工程, plugins/opencode/の配布用manifest・組立て, README.md, docs/opencode.md, CHANGELOG.mdのUnreleased, docs/decision/records/内の方式記録。
- Done when: ローカルでCIと同じtar.gz組立てを行い、checkout外で開発用node_modulesを参照できない隔離環境へ取り出し、アーカイブと宣言済みのホスト前提だけでpluginを登録・実行でき、本体とpluginの版が一致する。runtime依存を同梱するか登録時に解決するかを記録し、その手順を実際に通す。導入手順の接続・認証・対応条件・限界がREQ-057と一致する。
- Shown by: artifact scripts/package-release.shでローカル成果物を生成し、tarの一覧と隔離ロードの結果をreviewする。REQ-056/057のhow_to_verifyとEX-096〜EX-099を確認し、CI構文もプラットフォームの検査で確認する。
- Left to the implementer: スクリプト引数とアーカイブ内のplugin配置。既存binaryとLICENSE、既存導入経路を保持し、同梱に必要なmanifest・runtime依存を含める。
- Stop and hand back if: miseの既存導入が壊れる、npm公開・新サービスが必要、または公開済み版・タグを変更する必要がある。

### S6: 全チェックと二役の変更照合を揃える

- Purpose: 実装ブランチの最終候補に検証証拠と独立した変更照合を揃え、利用者へ引き渡す。
- Specification: docs/ir/cli.md#REQ-016, docs/ir/cli.md#REQ-018, docs/ir/cli.md#REQ-045, docs/ir/opencode.md#REQ-047, docs/ir/opencode.md#REQ-048, docs/ir/opencode.md#REQ-049, docs/ir/opencode.md#REQ-050, docs/ir/opencode.md#REQ-051, docs/ir/opencode.md#REQ-052, docs/ir/opencode.md#REQ-053, docs/ir/opencode.md#REQ-054, docs/ir/opencode.md#REQ-055, docs/ir/opencode-delivery.md#REQ-056, docs/ir/opencode-delivery.md#REQ-057, docs/ir/opencode-delivery.md#REQ-058
- Prerequisites: S5。呼出元がbranch-wide BASEとcandidateをGitから確定し、実装と別のコンテキストのreviewerを用意する。
- May change: .kotowari/changes/implementation.yamlは実装担当のみ, .kotowari/changes/review.yamlは独立reviewerのみ, 必要な方式判断を記録するdocs/decision/records/。修正は該当S1〜S5へ戻す。
- Done when: 全対象のunit印とreview成果物が揃い、GNU/muslとTSの必須検証が成功し、二役が同じbase・candidate bytesと関連IRを独立に照合する。記録コミット後に最終HEADを再確定し、統合ゲートを再実行する。
- Shown by: check PROJECT.mdの全Rustチェック、typecheck・test:unit・test:integration、対象IDのkotowari query、kotowari check --format json、kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format jsonを実行する。対象の不足を0件とし、統合には両kotowari検査のexit 0を必要とする。
- Left to the implementer: 自分の照合記録のentry分割のみ。reviewer記録はコピーや役割名の差替えで作らず、reviewer自身が根拠を判断する。
- Stop and hand back if: scope外の既存findingが統合を妨げる、reviewerが未実施、または比較元や候補の変更で記録が古くなる。scope外を勝手に直さず、必要な再照合を報告する。
