# Plan: 解析・観測・規則・実行基盤の境界を再構成する

## Goal

単一バイナリと既存の判定契約を保ち、純粋な解析・規則からプロセス実行と外部観測を分離して、各責務を独立に検証できる構造にする。

## Specification

- IR store: `docs/ir/`。調査時の基準コミットは `72abd624b5ce565b17ece6e7ee9d5237aab7efcd`。仕様・用語集・問題記録・決定記録はコミット済みで、`kotowari 0.2.0` の `kotowari check` は指摘なし。
- 解析契約: [REQ-035](../ir/parser.md#REQ-035)、[REQ-036](../ir/parser.md#REQ-036)、[REQ-037](../ir/parser.md#REQ-037)、[REQ-038](../ir/parser.md#REQ-038)、[REQ-039](../ir/parser.md#REQ-039)、[REQ-041](../ir/parser.md#REQ-041)。
- 効果・観測・判定: `docs/ir/judgment.md#REQ-001`、`#REQ-002`、`#REQ-003`、`#REQ-004`、`#REQ-005`、`#REQ-006`、`#REQ-007`、`#REQ-008`、`#REQ-009`、`#REQ-010`。
- 設定: `docs/ir/config.md#REQ-013`、`#REQ-014`、`#REQ-015`、`#REQ-026`。guard: `docs/ir/guards.md#REQ-027`、`#REQ-028`、`#REQ-029`、`#REQ-030`、`#REQ-031`、`#REQ-032`、`#REQ-033`、`#REQ-034`。
- 外部互換の検証: `docs/ir/cli.md#REQ-016`、`#REQ-017`、`#REQ-018`、`#REQ-019`、`#REQ-020`、`#REQ-021`、`docs/ir/agents.md#REQ-022`、`#REQ-023`、`#REQ-024`、`docs/ir/messages.md#REQ-011`、`#REQ-012`、`docs/ir/release.md#REQ-025`。
- このファイルは未承認の設計案である。機能要求の変更は提案しない。実装開始には本計画の承認とコミットが必要で、IRと衝突する判断は実装者へ委ねない。

## Approach and why

クレート数の最小化ではなく、不要な依存をコンパイル境界で切る。新設は `guardian-analysis` と `guardian-app` の二つとし、worker専用の第三のクレートは作らない。解析実装を共通型から切り離すことで、judgeが型を使うだけでOSSパーサを間接依存に持つ状態を解消する。appは判定を組み立てる唯一の所有者であり、現時点で独立利用者のないIPCを別パッケージにする必要はない。

| クレート | 最終的な責務 | 通常の内部依存 |
|---|---|---|
| guardian-core | Effect、Target、分類、Verdict、Invocation、共通の診断という値と純粋な操作 | なし |
| guardian-parser | brush-parserの隠蔽、正規化AST、構文復旧、引用除去、構文上のサイズ・深さ検査 | core |
| guardian-analysis（新設） | コマンド意味論、状態付き走査、値・パス解決、効果と起動情報の抽出 | core、parser |
| guardian-judge | ファイルシステム・gitの観測と、その結果からの分類 | core |
| guardian-policy | 設定の解釈・マージ・信頼条件、ルート上書き、規則照合、最悪値合成、文面 | core |
| guardian-app（新設） | 設定探索・読込、worker・通信、判定セッション、解析・観測・規則の組立て | 上記すべて |
| command-guardian | CLI、フック固有の写像、環境取得、影ログ | app、外部契約に必要なcore/policy |

- coreはASTを持たない。既存parserのASTを維持し、抽象的な共通診断だけをcoreへ移す。parserの `Failure` は同じvariantを持つ共通診断型の再公開とし、OSS型やOSの終了状態をcoreへ入れない。
- policyからparser、analysis、judgeへの通常依存をなくす。policyが受け取るのはcoreの値と検証済みの設定材料であり、設定例の解析もappが隔離して行う。
- 内部Rust APIの移動・引数変更は本計画の設計案に含む。未公開クレートの旧入口を維持するための逆依存shimは作らない。CLI、設定、出力、判定の外部契約は維持する。

調査で確認した問題と、この計画での扱いは次のとおり。テスト・実入力の再現実行は調査段階では行っていない。

| 確認した事実 | 根拠 | 扱い |
|---|---|---|
| parserが呼出元を再起動し、解析入口でも子モードを検出する | `crates/guardian-parser/src/lib.rs:90-120`、`src/worker.rs:83-129,286-313` | 実行基盤をappへ移し、子モードはmain先頭の明示dispatchだけにする |
| 予算はTLSに残り、coreが開始、policyが超過フラグを確認するだけ | parser `src/budget.rs:11-76`、core `src/extract.rs:134-148`、policy `src/engine.rs:190-194` | Engineが一判定のセッションを所有し、終了時に実経過時間も確認する |
| guardはeval等の内側を深さ16で黙って打ち切り、解析失敗を捨てる | policy `src/guard.rs:454-464,638-640` | 既存要求に基づく挙動修正として、移動とは別に失敗テストから直す |
| 引数付き命令の先頭代入にある置換をcoreが走査していない一方、guardは二重に走査する | core `src/extract.rs:345-375`、policy `src/guard.rs:579-595` | 語の子の訪問と値解決を分け、置換の効果漏れ・二重抽出を防ぐ |
| policyとjudgeが同じ末尾slash付き対象を別々にcanonicalizeする | policy `src/engine.rs:314-320`、judge `src/classify.rs:70-75` | 同じ実体パスの観測結果をルート照合と分類で共有する |
| coreの共通型を使うjudgeもparserへの間接依存を持つ | core/judgeの `Cargo.toml`、judge `src/classify.rs:3-5` | 共通モデルと解析実装のクレート境界を分ける |

再利用・自作の判断は層ごとに行った。外部依存の追加は不要で、既存コードと標準ライブラリで足りる。

| 層 | 採用する資産と理由 |
|---|---|
| 文法・AST・復旧・構文深さ | 既存parserを採用。成熟パーサの調達と正規化契約をやり直さない |
| IPC・codec・死亡分類 | 既存worker/wireを移動して採用。汎用IPCや新しいwire形式を自作しない |
| 判定セッション | 既存予算計算を明示所有へ組替え。時間は標準ライブラリを使い、期限テストに必要な時計の差し替えだけ設ける |
| コマンド意味論・解決 | 既存words/Context/Value/Resolvedと効果別処理を採用。共有歩行と副作用のないresolverへ分解する |
| fs/git | 既存canonicalize、作業ツリー探索、GitRunnerを採用。実観測とテスト観測のための最小の境界だけ設ける |
| 設定・規則・文面 | 既存TOML/regex/マージ/照合器/messageを採用。別の設定形式や規則エンジンを作らない |

解析は `CommandFacts { effects, invocations, diagnostics }` を返す。効果の入力を平坦なInvocation一覧へ置き換えない。既存AST、Context、ループ束縛、パイプ供給元を保持する。トップレベルのOutcomeはappが渡し、evalとリテラルなshell本文の再解析だけを `FnMut(ParseRequest) -> Outcome` の関数引数で依頼する。通常のコマンド置換・プロセス置換はASTにある本文を訪問し、解析し直さない。

語の置換を訪問する責務はwalkerへ集め、resolverは値を返すだけにする。Invocationの語はguard仕様どおり見かけの語と本文の代入名から作り、パス解決用Contextの値へ置き換えない。literal forの効果は値ごとに評価するが、guardのための構文位置の収集は反復評価から分離する。これは内容の文字列だけによる重複除去や、効果の平坦化で実現しない。共通化するのは子の辿り方・内側解析・診断伝搬で、構文深さを子内で検査するparserの走査は別責務として残す。

ParserRuntimeはworkerを所有し、一判定はこれを可変借用するJudgmentSessionを所有する。解析回数・残時間はセッションに保持し、global workerとTLS予算を除く。通常終了と失敗後のkill/waitの責務を一か所に置く。本番の入力由来の構文解析・引用除去には、親での直解析経路を設けない。

設定例の検証は、設定読込中だけ存在するValidationSessionで隔離する。これはJudgmentSessionではなく、設定全体・規則全体への累積1000回/5秒の上限を新設しない。各要求の既存のサイズ・スタック・起動待ち・応答待ち制限と失敗時の警告は維持し、例の個数や検証順によって正常な規則を無効化しない。その後のcheckは新しいJudgmentSessionから始まる。多数の正常な例を受理できることと、設定検証・判定・次判定の予算の独立性を検証する。設定用の新しい累積受理上限が必要なら仕様判断へ戻す。

時計の差し替えは期限テスト、観測の差し替えはsymlinkの変化・失敗とgit起動条件のテストで使用する。それ以外の汎用registry、factory、交換を予測したtraitは追加しない。初回移動ではwire形式、nonce、ready、mode検査、frame上限、子内の8MiB/32MiBスタックを維持する。

待ち時間は起動・書込・読込ごとに残時間を計算し直す。gitの子プロセス待ちも判定残時間に収め、通常のgit故障と全体予算超過を区別する。終了時の期限検査はフラグだけでなく経過時間を見る。これをOSの全syscallを5秒で強制中断する保証とは呼ばない。fs syscallを強制中断する追加基盤が要求上必要と判明した場合は、下記の停止条件に従う。

構造変更で固定するものと、意図的に変わるものを区別する。

- 固定するもの: 対象コマンド、分類順序、設定信頼、リンクの扱い、出力と終了コード、エラーのask/block対応、単一バイナリ。同じ構文に対する既存Context伝播を勝手に実シェルの評価器へ改造しない。
- 意図的な修正: guardの無言打切り・診断廃棄、先頭代入の置換の効果漏れ、重複した子の訪問、時間予算の取りこぼし。各修正の根拠となる要求とRED/GREENを個別に示す。
- 解析共有化によって実際の解析回数が減る。旧実装の重複解析で1000回を超えた入力が、上限内として通常判定される場合がある。これは挙動不変と説明せず、実際の解析回数を制限するREQ-039との整合を確認する。
- guardのshell識別とcoreの識別の差、非リテラルprogramの扱いは、共通classifier導入時に一覧化する。REQ-035で通常命令とされるshellの非 `-c` 起動は通常起動として残す。それ以外の未規定差は推測で統一しない。

## Scope of change

- `Cargo.toml`、`Cargo.lock`、既存四クレートの `Cargo.toml`。
- `crates/guardian-core/src/{lib,types,extract}.rs` と既存coreテスト。extractの実装は新analysisへ移し、coreに残すのは共通モデルだけ。
- 新規 `crates/guardian-analysis/Cargo.toml`、`src/{lib,command,walk,resolve,effects}.rs`、`tests/`。効果別の小モジュールへの分割はこのディレクトリ内で許可する。
- parserの `src/{lib,words,worker,budget,wire}.rs`、必要な `normalize.rs` の参照、既存parserテスト。AST契約を変えずにruntime部分を移す。
- judgeの `src/{lib,classify,git}.rs`、新規 `src/observation.rs`、既存judgeテスト。
- policyの `src/{lib,engine,config,guard,message}.rs` と既存policyテスト。
- 新規 `crates/guardian-app/Cargo.toml`、`src/{lib,engine,config_loader,report}.rs`、`src/runtime/{mod,worker,budget,wire}.rs`、`tests/`。
- `src/main.rs`、`src/hook.rs` の接続とimport。`src/log.rs` のログ動作は変えない。
- `tests/{cli,hook,shadow,isolation}.rs`。実プロセスの隔離検証はバイナリ側に残す。
- `PROJECT.md`、新規 `docs/decision/records/2026-10-01-crate-boundaries.md`、必要な既存決定の改訂リンク。IRの機能要求の意味変更、FLAG解消、無関係なTODO整理は含めない。

## Step order and prerequisites

S1で既存チェックを実行し、失敗を新構造の問題と混同しない。S2は現構造で要求違反を確認・修正してから移動する。S3はモデルと解析の切り離し、S4はappへの実行基盤・組立ての縦方向の移動で、途中にもCargo依存循環を作らない。S5でセッションを明示化し、S6で解析・guard走査を共有する。S7で観測を一本化し、S8でpolicyとloaderの純粋境界を仕上げる。S9でテストの責務・文書・全体検証を揃える。

各構造移動は既存テストを維持して検証し、既に真であることだけを測る新テストを増やさない。挙動修正は先に失敗テストを走らせ、修正後に同じテストを走らせる。一つのステップに複数の修正がある場合も、関心ごとに分けて検証・コミットする。

## Verification map

| Step | 要求 | シナリオ・検証資産 |
|---|---|---|
| S1 | 本計画の既存契約全体 | workspace testsとkotowariの基準状態。未実行のベースラインを成功扱いしない |
| S2、S6 | REQ-001、002、008、027～034、035、037～039 | EX-001、006、007、010、029、037～049、054～057。既存extract/resolve/guard/asksと、要求に根拠のある欠陥の回帰テスト |
| S3、S4 | REQ-036、041、025 | EX-052、053。AST契約、Cargo依存、単一バイナリのビルド。配布作業はしない |
| S5 | REQ-010、038、039 | EX-011、049、050、057。panic・起動失敗・深い入力・1000/1001回・期限・回収・独立した次判定 |
| S7 | REQ-003～010、020 | EX-002～009、011、019、024、028。既存classify/git/symlink、同じ観測を使うルートと既定分類 |
| S8 | REQ-005、006、009、013～015、026～034 | EX-014～016、030、035～045。三層、信頼、規則・例の検証、警告、最悪値 |
| S9 | REQ-011、012、016～019、021～025と上記全要求 | EX-012、013、017、018、020～023、025～027、031～034。CLI/hook/shadow/isolationの本物の入口、文面レビューと代表入力の計測 |

REQ-012、021、025、036、041と、それだけに対応するシナリオはreviewによる検証を使い、印の数を満たすだけのテストは作らない。既存の `FLAG-001` は本計画の解消対象ではない。変更対象外の問題で `kotowari status` が未完了でも、それだけで本計画を失敗としない。

## Left to the implementer

- 指定ディレクトリ内の私有関数名、借用・所有の詳細、効果別モジュール名。クレート名、依存方向、sessionの所有者、同一バイナリ隔離方式は本計画の承認対象で、任意変更しない。
- 再解析の口は関数引数を第一候補とする。追加traitは同じ変更で実際に必要なテストまたは複数実装を示せる場合だけ採用できる。
- 同じ振る舞いを保つ内部ヘルパーの分割。構文深さ・回数の数え方、失敗時の判定、設定の信頼、Context伝播を選択事項にしない。

## Stop conditions

- 承認済み要求にない意味の決定、既存仕様からの逸脱、不可逆・特権・危険対象の操作、変更事故の拡大、変更した方法でも進展しない場合は停止する。
- 基準コミット以後の利用者の変更が対象にある場合は差分を調べ、勝手に上書きしない。S1で失敗した既存テストを無関係な修正で通さない。
- Contextの伝播、非リテラルprogram、例の受理条件について、既存テストとIRだけでは期待値を決められない場合は仕様判断へ戻す。
- subprocess隔離を外す、親で入力由来の解析を行う、新しい実行ファイル・プラットフォーム・外部依存が必要になる場合は停止する。
- 全fs syscallを期限内で強制中断する保証が必要になった場合、または期限処理の追加が既存ask/blockの帰属を変更する必要を生んだ場合は、実行基盤の仕様を確認する。
- 代表入力で100ms未満を満たさない、または旧方式に比べ無視できない性能退行が出た場合は結果を示し、無断で隔離・検査を減らさない。

## Test command

既存 `lefthook.yml` のチェックを使う。git fixtureは `CARGO_TARGET_TMPDIR` に置かれるため、targetを一時領域へ移すと分類の前提が変わる。`CARGO_TARGET_DIR=/tmp/...` やHOMEの一律差し替えはしない。既存CLI/hookテストの専用HOME/XDGを維持し、実設定や実shadow.logで手動検証しない。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/kotowari-check.sh
cargo build --release --locked
```

Cargo.tomlのpath依存変更時はCargo.lockを更新してから `--locked` を使う。段階の検証は対象packageへ絞り、最後に全workspaceを走らせる。実プロセスの契約は `cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation` で検証する。危険な深さの入力はこの隔離されたバイナリ検証へ置き、純粋parserのテストプロセスを落とす試験にはしない。

S4では子dispatchを変えるのと同時にテストの解析経路も移行する。parserの浅い契約試験は純粋な直接解析、analysisの浅い意味解析試験はテスト専用の解析関数または準備したOutcomeの注入を使う。policyの照合試験は解析済みInvocationを渡し、設定・Engine・実workerを必要とする試験はappまたはrootへ移す。子の起動・死亡・病的入力はrootの実バイナリで試し、package test実行ファイルのmainを再起動する仕組みには依存しない。テスト用依存を加える場合もparser/analysis/policyからappへの逆依存を作らない。

応答時間はrelease版、fixture内のHOME/XDG/cwd、既存corpusの代表的な浅い入力で、旧版と新しい版のcold起動・同一Engine内の再利用を別々に計測する。各群の入力と10回の計測値・最大値を記録して100ms未満を確認し、負荷条件も併記する。病的な深さの入力を代表入力の性能判定に混ぜない。調査時点の実測結果はない。

## Out of scope

- 実装開始、リリース、push、配布用CIの新設、バージョン変更。
- bashの完全な実行器、分岐の状態合流、関数呼出しの評価、対象シェル・破壊的コマンドの追加。
- seccomp・namespace・cgroup等の権限/メモリサンドボックス。プロセス分離だけを権限隔離や実行時のTOCTOU防止と説明しない。
- 汎用IPC、ASTの全面置換、新設定形式、旧名称の互換、未保管の実測コーパスの再構築。

## Steps

### S1: 仕様と既存検証の基準状態を確かめる

- Purpose: 構造移動の前に既存契約と実行環境の失敗を分離する。
- Specification: `docs/ir/parser.md#REQ-036`、`docs/ir/parser.md#REQ-041`、`docs/ir/cli.md#REQ-017`、`docs/ir/cli.md#REQ-021`。
- Prerequisites: 本計画が承認・コミット済みで、実装branchと一writerの作業場所が決まっている。
- May change: なし。計測・検証の記録は作業用成果物とし、本番ソースを変更しない。
- Done when: Test commandの既存チェックとrelease buildの結果、基準のCargo依存、代表入力の計測結果が揃い、既存失敗を識別できる。
- Shown by: check — Test commandの五コマンド、`cargo tree -p guardian-judge --depth 2`、`cargo tree -i brush-parser`、記載した性能計測を実行する。
- Left to the implementer: なし。
- Stop and hand back if: ベースラインが失敗する、fixtureの分類前提が変わる、基準コードとの差が利用者の未承認変更である。

### S2: 解析経路の取りこぼしを現構造で修正する

- Purpose: guardと効果抽出の要求違反を、ファイル移動の影響と混同せず修正する。
- Specification: `docs/ir/judgment.md#REQ-001`、`docs/ir/judgment.md#REQ-009`、`docs/ir/guards.md#REQ-033`、`docs/ir/parser.md#REQ-035`、`docs/ir/parser.md#REQ-038`、`docs/ir/parser.md#REQ-039`、`docs/decision/records/2026-09-30-hook-guardian-scope.md#A44`。
- Prerequisites: S1。
- May change: core `src/extract.rs` と `tests/extract.rs`、policy `src/{guard,engine}.rs` と `tests/{guards_shape,asks}.rs`、`tests/cli.rs`。
- Done when: 16段を越えるliteral eval内のguardが失われず、解析失敗が最悪値で合成され、先頭代入にある置換の効果を取り出し、shellの非-c起動を通常起動として扱う。各変更は要求を根拠に独立の修正として説明できる。
- Shown by: test — 16/17段のguard、先頭代入の置換、非-c shell起動の欠陥に対応するRED→GREENを示す。診断廃棄はguardの結果境界で実際にSyntaxとなる入力を使い検出する。既に通るEngineの最悪値合成と上限境界は維持検証とし、Internal/Panic/Limitのruntime検証はS5で行う。core/policyとCLIの既存テストも走らせる。
- Left to the implementer: 診断を返す暫定結果型の名称。後の共有結果で置き換えるため、新しい重複解析は追加しない。
- Stop and hand back if: 再現が予想と違う、通常の構文深さとevalの再解析深さを同じ入力条件として扱わないと期待値を説明できない、非リテラルprogramの新しい受理規則が必要になる。

### S3: 共通モデルから意味解析の実装を切り離す

- Purpose: judgeとpolicyが解析実装を依存に持たず共通の値を利用できるようにする。
- Specification: `docs/ir/judgment.md#REQ-001`、`docs/ir/judgment.md#REQ-002`、`docs/ir/parser.md#REQ-036`、`docs/ir/parser.md#REQ-041`。
- Prerequisites: S2。
- May change: workspace/Cargo.lock、core/parser/judge/policyのmanifests、core `src/` と既存テスト、新analysis `src/` と移管テスト、parser `src/{lib,words,normalize}.rs`、policy/srcとroot/srcのimport。
- Done when: coreに外部・parser依存がなく、既存の共通型・診断がそこにあり、extract/Context/解決とwrapper・shell意味論はanalysisへ移り、parserのASTとFailure variant契約を維持する。
- Shown by: check — core/analysis/parser/judge/policyのpackage tests、`cargo tree -p guardian-core --depth 1`、`cargo tree -p guardian-judge --depth 2`、`cargo tree -i brush-parser`、compilerで依存と公開型を確認する。
- Left to the implementer: 型の既存名称を維持できる再公開、analysis内の私有モジュール分割。
- Stop and hand back if: coreにASTやOS固有型を移す必要が出る、旧API互換のため逆依存が必要になる。

### S4: 実行基盤と判定入口をappへ縦方向に移す

- Purpose: parserからホスト起動の暗黙契約を外し、policyから実行の組立てを外す。
- Specification: `docs/ir/parser.md#REQ-036`、`docs/ir/parser.md#REQ-038`、`docs/ir/parser.md#REQ-039`、`docs/ir/config.md#REQ-013`、`docs/ir/config.md#REQ-034`、`docs/ir/cli.md#REQ-016`。
- Prerequisites: S3。
- May change: 新app `Cargo.toml`、`src/`、tests、workspace/manifests/Cargo.lock、parser `src/{lib,worker,budget,wire}.rs` とテスト、analysisの解析入口とtests、policy `src/{lib,engine,config,guard}.rs` とtests、`src/{main,hook}.rs`、root tests。
- Done when: Engineと設定I/O、worker/wire/bounded threadがappにあり、main先頭だけで子モードをdispatchし、本番の全解析はappの隔離経路を使う。Test commandのテスト経路を同時に移行し、移動途中にもpolicy→appやテスト用の逆依存を作らず、wire・スタック・死亡分類を維持する。
- Shown by: check — parser/app/analysis/policy tests、rootのcli/hook/shadow/isolation tests、`cargo tree -i brush-parser`、process/env/time importと本番parser呼出しの確認。
- Left to the implementer: 同一app内のruntime私有モジュール分割。移行中のguardと設定検証には解析関数を渡してよい。
- Stop and hand back if: parserテストを成立させるため本番の親解析fallbackや別workerバイナリが必要になる、接続移動がCLI/configの形式変更を必要とする。

### S5: workerと一判定の予算を明示所有する

- Purpose: worker回収と予算の寿命をEngineとsessionの型で表し、呼出順やTLSに依存させない。
- Specification: `docs/ir/parser.md#REQ-038`、`docs/ir/parser.md#REQ-039`、`docs/ir/judgment.md#REQ-010`、`docs/ir/guards.md#REQ-034`。
- Prerequisites: S4。
- May change: app `src/runtime/`、`src/engine.rs`、`src/config_loader.rs`、app tests、`tests/{cli,isolation}.rs`。
- Done when: global worker/TLS予算がなく、本体・内側・引用除去が同じJudgmentSessionを使い、ValidationSessionには新しい累積判定予算を適用しない。設定検証と次判定には予算が漏れず、各待ちの残時間と終了時の経過時間を確認し、通常終了・死亡・失敗後の子を回収する。
- Shown by: test — 1000/1001回、引用除去の回数非消費、1000件を越える正常な設定例の受理とその後の判定、独立した次判定、期限切れ、panic/起動失敗、失敗後の再起動と通常終了の回収を要求に基づいて検証し、root isolationの実プロセステストを実行する。
- Left to the implementer: 期限テストに使用する最小の時計差し替え口。偽のエラーを測るだけの未到達分岐テストは追加しない。
- Stop and hand back if: startup故障と予算超過の帰属を既存要求から決められない、回収の保証が新しい外部依存や追加の特権を必要とする。

### S6: 状態付き解析を共有しguardの独自走査をなくす

- Purpose: 同じ語と内側本文の解釈・診断を効果抽出とguardで共有する。
- Specification: `docs/ir/judgment.md#REQ-001`、`docs/ir/judgment.md#REQ-002`、`docs/ir/judgment.md#REQ-008`、`docs/ir/guards.md#REQ-027`、`docs/ir/guards.md#REQ-031`、`docs/ir/guards.md#REQ-033`、`docs/ir/parser.md#REQ-035`、`docs/ir/parser.md#REQ-037`、`docs/ir/parser.md#REQ-038`、`docs/ir/parser.md#REQ-039`。
- Prerequisites: S5。
- May change: analysis `src/` と移管テスト、coreの共通結果型、policy `src/guard.rs` とguard tests、app `src/engine.rs` と解析接続、manifests/Cargo.lock。
- Done when: appが一つのCommandFactsを利用し、policyにAST walkやparser呼出しがなく、resolverが置換を訪問せず、全語位置の置換・リダイレクト・複合構文を取り出す。cwd/変数/ループ束縛/パイプ供給元を保持し、guardと効果の解析失敗を失わない。
- Shown by: check — analysisのextract/resolveとcorpus由来テスト、policyの全guard tests、S2の回帰テスト、root isolationの回数境界、重複解析・二重抽出・policyのparser依存がないことのソース確認。
- Left to the implementer: 共有walkerの内部データ構造。Invocationを効果の入力にする平坦化や文字列だけの構文位置同一視はしない。
- Stop and hand back if: 既存Context伝播を変えなければ共有できない、guardの語を環境展開後の値に変える必要がある、共通化が正常な入力の新しいask/blockを仕様の根拠なしに生む。

### S7: パス観測を共有し必要な外部待ちだけを行う

- Purpose: 設定ルートと通常分類が同じ実体パスを使い、不要なfs/gitの観測を増やさない。
- Specification: `docs/ir/judgment.md#REQ-003`、`docs/ir/judgment.md#REQ-004`、`docs/ir/judgment.md#REQ-005`、`docs/ir/judgment.md#REQ-006`、`docs/ir/judgment.md#REQ-007`、`docs/ir/judgment.md#REQ-008`、`docs/ir/judgment.md#REQ-010`、`docs/ir/cli.md#REQ-020`、`docs/ir/parser.md#REQ-039`。
- Prerequisites: S6。
- May change: judge `src/{lib,classify,git,observation}.rs` と既存テスト、policyのルート照合、appの観測接続/sessionとtests、coreの必要な観測値。
- Done when: dereference対象の実体パスを一度取得し同じ値を使い、元Targetは表示用に維持する。設定ルート/既定分類で確定した対象にはgitを起動せず、必要なgit待ちだけをsession残時間で打ち切る。期限切れしたgitの終了・回収と出力取得の終了を確認し、通常GitFailedと全体Limitを区別する。
- Shown by: test — slashなし/あり・canonicalize失敗・観測間にリンクが変わる条件の結果、roots優先順位、children/glob、git無効/作業ツリー外を検証する。SystemGitと同じ起動・出力取得経路に制御したgit代替プロセスを渡し、応答しない場合と大量出力の場合に外側watchdog内で判定が返り、全体期限超過はLimit/block、通常起動失敗はGitFailed/askで、子と出力readerを回収することを確認する。結果だけを返すfakeでこの実プロセス試験を代用せず、judge testsとroot CLI symlink testsも実行する。
- Left to the implementer: 既存GitRunnerを再利用した観測の最小差し替え口と、同じ順序を保つ純粋分類関数の分割。
- Stop and hand back if: 設定ルートを通常分類より後にする必要が出る、全パスのcanonicalizeやgit先行実行が必要になる、観測共有を原子的fs snapshotとみなさなければ成立しない。

### S8: 純粋policyと設定loaderの境界を仕上げる

- Purpose: policyを値から判定する層にし、設定の場所と読み取りはappへ閉じる。
- Specification: `docs/ir/config.md#REQ-013`、`docs/ir/config.md#REQ-014`、`docs/ir/config.md#REQ-015`、`docs/ir/config.md#REQ-026`、`docs/ir/guards.md#REQ-027`、`docs/ir/guards.md#REQ-028`、`docs/ir/guards.md#REQ-029`、`docs/ir/guards.md#REQ-030`、`docs/ir/guards.md#REQ-031`、`docs/ir/guards.md#REQ-032`、`docs/ir/guards.md#REQ-033`、`docs/ir/guards.md#REQ-034`、`docs/ir/judgment.md#REQ-009`。
- Prerequisites: S7。
- May change: policy `src/` と既存テスト、app `src/{engine,config_loader,report}.rs` とtests、coreの必要な結果型、manifests/Cargo.lock。
- Done when: policyの通常依存がcoreだけとなり、fs/process/env/時計/parser呼出しがなく、appが設定探索・信頼パス観測・隔離された例の解析を担当する。三層と警告、例の無効化、allow制限と最悪値合成を維持する。
- Shown by: check — 純粋policy testsと移管したapp設定統合tests（1000件を越える正常な例でも規則を受理し、順序や後の判定に累積予算が漏れない検証を含む）、`cargo tree -p guardian-policy --depth 1`、禁止する外部アクセスのソース確認、root cli/hook/shadow tests。
- Left to the implementer: Reportと文面入力の内部型配置。ただしpolicyがappの型へ依存する構造にはしない。
- Stop and hand back if: trustedの値をプロジェクト側から選べる構造になる、壊れた一層を無視する挙動を変える必要がある、例の新しい受理境界が必要になる。

### S9: 検証配置と文書を整え全体の互換を確かめる

- Purpose: 再構成後の責務と証拠を、次の実装者と利用者が追える形にする。
- Specification: `docs/ir/parser.md#REQ-036`、`docs/ir/parser.md#REQ-041`、`docs/ir/messages.md#REQ-011`、`docs/ir/messages.md#REQ-012`、`docs/ir/cli.md#REQ-016`、`docs/ir/cli.md#REQ-017`、`docs/ir/cli.md#REQ-018`、`docs/ir/cli.md#REQ-019`、`docs/ir/cli.md#REQ-021`、`docs/ir/agents.md#REQ-022`、`docs/ir/agents.md#REQ-023`、`docs/ir/agents.md#REQ-024`、`docs/ir/release.md#REQ-025`。
- Prerequisites: S8。
- May change: 対象内のテスト配置/import/印、`PROJECT.md`、指定した新決定記録と既存決定の改訂リンク、接続整理に必要な `src/{main,hook}.rs`。機能追加と無関係な履歴修正はしない。
- Done when: judgeテストが独自verdict計算でpolicyを代用せず、純粋処理・app統合・バイナリ隔離をそれぞれの境界で検証し、文書の依存図とコマンドが実体と一致する。外部出力と単一バイナリを維持し、代表入力の計測とスコープ内の仕様対応が揃う。
- Shown by: check — Test commandの全コマンド、root契約tests、性能計測、対象IDの `kotowari query` と `kotowari check`、Cargo依存と文面の独立レビュー。変更ファイル/対象IDにエラーがなく、必要な印が維持され、未実測事項を成功扱いしない。
- Left to the implementer: 文書の説明順とテストファイル名。改訂の理由は既存要求と承認済み本計画に基づき、未記録の利用者判断を捏造しない。
- Stop and hand back if: 外部契約の差、性能退行、未承認の意味変更、対象外のkotowari指摘の解消が完了条件に混ざる。
