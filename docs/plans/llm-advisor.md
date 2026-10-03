# LLM助言層の実装計画

## Goal

既存の機械判定と各ホストの出口を保ったまま、利用者が明示した場合だけ、期限付きのLLM助言で判定を厳しくできる単一バイナリを提供する。

## Specification

IRは`docs/ir/`にある。当初の承認済み仕様のコミットは`8ccf11d671a02680b3943c526f5c318d4a32b287`であり、その時点の試験不足は20要求と53例、通知は2件だった。実装中に利用者が承認した[検証分担の改訂A63](../decision/records/2026-10-03-llm-advice-layer.md#agreements)と[追加改訂A64](../decision/records/2026-10-03-llm-advice-layer.md#agreements)を本計画へ反映する。改訂後の実装前の試験不足対象は18助言要求と、46助言例に既存EX-112、113を加えた48例であり、現在の部分実装の残件数とは分ける。A63の文書改訂は独立レビューを経たが、A64の追加文書改訂の独立レビューは未実施である。

- [docs/ir/advisor/policy.md#REQ-advisor-001](../ir/advisor/policy.md#req-advisor-001-検査対象と観測モード)、[docs/ir/advisor/policy.md#REQ-advisor-002](../ir/advisor/policy.md#req-advisor-002-危険性と指示範囲の分類)、[docs/ir/advisor/policy.md#REQ-advisor-003](../ir/advisor/policy.md#req-advisor-003-分岐ごとの閾値と候補判定)、[docs/ir/advisor/policy.md#REQ-advisor-004](../ir/advisor/policy.md#req-advisor-004-入力を判定指示にしない)、[docs/ir/advisor/policy.md#REQ-advisor-005](../ir/advisor/policy.md#req-advisor-005-モデルの品質を構造試験と区別する)。定義は[docs/ir/advisor/policy.md#TBL-advisor-001](../ir/advisor/policy.md#tbl-advisor-001-候補判定の優先順)。
- [docs/ir/advisor/context.md#REQ-advisor-006](../ir/advisor/context.md#req-advisor-006-当該実行への出所の対応付け)、[docs/ir/advisor/context.md#REQ-advisor-007](../ir/advisor/context.md#req-advisor-007-送信範囲と会話の上限)、[docs/ir/advisor/context.md#REQ-advisor-008](../ir/advisor/context.md#req-advisor-008-指示の鮮度と参照不足)、[docs/ir/advisor/context.md#REQ-advisor-009](../ir/advisor/context.md#req-advisor-009-秘密とサイズと符号化による見送り)、[docs/ir/advisor/context.md#REQ-advisor-010](../ir/advisor/context.md#req-advisor-010-必要な経路だけのセッションキャッシュ)、[docs/ir/advisor/context.md#REQ-advisor-011](../ir/advisor/context.md#req-advisor-011-助言ログと影ログの分離)。
- [docs/ir/advisor/provider.md#REQ-advisor-012](../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)、[docs/ir/advisor/provider.md#REQ-advisor-013](../ir/advisor/provider.md#req-advisor-013-分布の境界検証)、[docs/ir/advisor/provider.md#REQ-advisor-014](../ir/advisor/provider.md#req-advisor-014-typesafeへの一要求)、[docs/ir/advisor/provider.md#REQ-advisor-015](../ir/advisor/provider.md#req-advisor-015-利用者だけが助言を設定する)、[docs/ir/advisor/provider.md#REQ-advisor-016](../ir/advisor/provider.md#req-advisor-016-設定可能な既定と不正値)。設定の定義は[docs/ir/advisor/provider.md#TBL-advisor-002](../ir/advisor/provider.md#tbl-advisor-002-利用者ファイルのadvisor節)。
- [docs/ir/advisor/runtime.md#REQ-advisor-017](../ir/advisor/runtime.md#req-advisor-017-文脈取得を含む単一の助言期限)、[docs/ir/advisor/runtime.md#REQ-advisor-018](../ir/advisor/runtime.md#req-advisor-018-同一バイナリの子と強制打切り)、[docs/ir/advisor/runtime.md#REQ-advisor-019](../ir/advisor/runtime.md#req-advisor-019-長さと要求対応を検証する内部通信)、[docs/ir/advisor/runtime.md#REQ-advisor-020](../ir/advisor/runtime.md#req-advisor-020-opencodeの外側期限との整合)、[docs/ir/advisor/runtime.md#REQ-advisor-021](../ir/advisor/runtime.md#req-advisor-021-最終判定だけを既存の出口へ渡す)。
- 変更された既存要求は[docs/ir/config.md#REQ-013](../ir/config.md#req-013-設定のファイルと層)、[docs/ir/config.md#REQ-014](../ir/config.md#req-014-プロジェクト設定の信頼)、[docs/ir/config.md#REQ-015](../ir/config.md#req-015-壊れた設定)、[docs/ir/cli.md#REQ-018](../ir/cli.md#req-018-影実行)、[docs/ir/cli.md#REQ-021](../ir/cli.md#req-021-応答時間)、[docs/ir/parser.md#REQ-039](../ir/parser.md#req-039-入力の上限)、[docs/ir/opencode.md#REQ-048](../ir/opencode.md#req-048-共通設定と影実行)、[docs/ir/opencode.md#REQ-053](../ir/opencode.md#req-053-承認待ちの寿命)。これらと出口の直接依存だけを回帰確認する。
- 計画上の配置と証拠の選択は[新しい計画決定記録 A1](../decision/records/2026-10-03-llm-advisor-plan.md#agreements)に置く。承認済みの[助言層の決定記録 A59](../decision/records/2026-10-03-llm-advice-layer.md#agreements)は改訂しない。

各要求と例は`kotowari query ID`で読む。定義表と要求の`referenced_by`の`about`も読む。対象にdeferredはない。助言の21要求は18件がunit、REQ-advisor-002、005、008の3件がreviewである。助言の53例のうちEX-advisor-003、004、053、009、010、015、016の7例はレビュー専用であり、試験必須の46例とは分ける。既存EX-112とEX-113を加えた48例が実装前の試験不足対象である。EX-111もレビュー専用なので、機械試験で報告の正しさを捏造しない。

## Approach and why

現在の`Engine::check`を機械判定として残し、その直後にappの助言サービスを呼ぶ。`src/main.rs`、`src/hook.rs`、`src/hook/opencode.rs`は入力と出口の写像だけを持つ。質問、分布の検証、確認済み指示の扱い、候補合成は新しい純粋な`guardian-advisor`へ置く。`AdvisorClient::assess`もここで定義し、TypeSafeの接続クレート`guardian-advisor-typesafe`と試験用実装が利用する。appが両者を組み立てる。依存はrootからapp、appからpolicyと接続クレート、policyと接続クレートからadvisor、advisorからcoreの方向だけとする。advisorはHTTP、ファイル、環境取得、プロセス、フレームワークに依存しない。TypeSafeのHTTP型とconfidenceを共通値へ漏らさない。

既存の解析workerはlittle-endianの独自本文と別の上限を使う。起動と継承ソケットのパターンは再利用するが、通信形式や死因の写像をそのまま助言へ転用しない。助言の版1、大端長、nonce、JSON検証は専用モジュールに置く。解析workerのDropの無期限waitも助言には移さない。

| 独立して選ぶ層 | 採用または自作 | 根拠 |
|---|---|---|
| 機械判定と出口 | 既存を採用 | engineとCLI/hookの写像を保ち、モデルから効果一覧を作らない |
| 質問、分布、候補合成 | 最小自作 | guardian固有の二つの分類と承認済み表を既存guardrailのverdictでは表せない |
| 設定 | 既存を拡張 | policyのConfig/Layerとappの読込を使い、プロジェクトadvisorだけを固有検証前に除去する |
| プロセスと期限 | 既存パターンと標準APIを採用 | 同一バイナリ、UnixStream、std::process、単調時計を使い、助言失敗を別に扱う |
| 文脈取得と対応付け | ホストの既存契約を優先、最小の変換を自作 | 公式の対応版の証拠を先に集め、未対応は文脈なしにする |
| state保存 | 既存のstate配置とrustixを採用 | log.rsのdescriptor基準のopenatを参考にappへ助言用保存を追加し、所有者とhardlinkの検査を補う。影ログを強化する別作業にはしない |
| HTTP/TLS | 承認済みureqを採用 | 標準ライブラリにHTTP/TLSはなく、仕様が接続層へのureq採用を固定している |
| TypeSafe変換と制御IPC | 最小自作 | サービス表現を接続内に閉じ、承認済み枠と期限交渉だけを実装する |

探索は現行コードと承認済み決定記録の再利用証拠に限定した。ureqの具体的な版、依存ライセンス、オフラインでの取得可能性はS5で確認する。参考ba-toysのMIT確認は承認済み記録にある。将来コピーする場合は著作権表示と許諾文を残す。参考guardrailの意味や汎用Judgeは採用せず、ba-toysは変更しない。

助言は既定off、利用者設定のみ、厳しくするだけである。offは追加取得も送信もせず、機械blockは解除しない。observeの候補と最終結果を分け、既存mode.enforceの影実行も別に保つ。独立したharmful_irreversibleは指示一致でも閾値以上ならblock、major_destructiveだけは確認済み指示とrisk/scope両閾値でask、それ以外は表に従う。失敗、秘密、符号化不能、不正応答、未知や低確率から新しいaskを作らない。非UTF-8 cwdは機械判定で受理し、助言だけを見送る。

文脈の取得経路は三ホストとも未検証である。S1で公式の公開契約またはリリース済みソースと、対応版を隔離して動かした架空会話の出力を照合する。個人の履歴を読まない。parser fixtureのパス形式と、ホストが実際に生成するセッション・実行・発言対応の証拠を区別する。latestUserInputやrole=userという名前だけを人間由来の証明にしない。版や経路の未対応、通常の欠落、期限内に終了した取得失敗は文脈なしの正常な入力であり、他の送信条件を満たせばモデル評価へ進む。確認済み指示がない場合、妥当なmajor_destructiveの選択確率が閾値以上ならenforceの最終判定はblockになる。取得処理が助言期限を使い切った場合だけは助言全体のtimeoutとして機械判定を維持する。

コードの振る舞いを足す各段階で、対象試験を先に書いてshellで失敗を確認するRED、最小実装で通すGREEN、振る舞いを変えない整理後の再実行REFACTORを記録する。すでに成り立つ既存動作は試験を増やさず再利用し、対応する例を本当に検証する既存試験へだけマークを足す。一つの振る舞い試験に複数の要求・例を付けてよい。質問への誘導文の試験は固定質問とデータの分離の証拠であり、実モデルの誘導耐性の証明ではない。

試験は実認証、実モデル、個人のサービスなしで行う。HTTP変換は制御したtransport、合成は試験用AdvisorClient、親の期限と死亡は注入可能な起動境界と独立したfixture子を使う。同時に配布バイナリの本物の助言dispatchと継承ソケットを実行し、認証欠落や不正通信でも機械結果が返ることを確かめる。公開の試験用フラグやURL切替えを作らず、libtestのmainをworkerとして再起動しない。

コマンドはPROJECT.mdのものを使う。Rustは`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo build --release --locked`。manifestを変更した段階では、その変更に対応するCargo.lockだけを更新してから`--locked`付きのGREENとREFACTORを実行する。S3とS4の更新をS5へ先送りしない。対象を絞るときは`cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation`に、新しい助言試験と`--test opencode_hook`を加える。`scripts/kotowari-check.sh`は試験不足を埋め終わるまでは通らないので途中の成功を主張しない。

OpenCodeはBun 1.4.2と固定依存、公式2.0.21を使う。`bun install --frozen-lockfile --cwd plugins/opencode`、`bun run --cwd plugins/opencode typecheck`、`bun run --cwd plugins/opencode test:unit`、`cargo build --locked`の後、`GUARDIAN_TEST_BIN="$PWD/target/debug/command-guardian" OPENCODE_TEST_BIN="$PWD/plugins/opencode/node_modules/@opencode/cli-linux-x64/bin/opencode" bun run --cwd plugins/opencode test:integration`を実行する。fixtureサービスだけを使い、既存のGNU/musl release binaryとcheckout外の配布検査も維持する。

## Scope of change

- 新規`crates/guardian-advisor/`と`crates/guardian-advisor-typesafe/`のmanifest、`src/`、`tests/`。純粋値と接続を分ける。
- `Cargo.toml`、`Cargo.lock`、`crates/guardian-app/Cargo.toml`、`crates/guardian-policy/Cargo.toml`。必要な静的依存と試験用依存だけを追加する。
- `crates/guardian-policy/src/config.rs`、`layers.rs`、`lib.rs`と対応する既存・新規試験。一般設定の既存契約は変えない。
- `crates/guardian-app/src/config_loader.rs`、`engine.rs`、`lib.rs`、新規`advisor/`と`state.rs`。`runtime/worker.rs`と`runtime/mod.rs`は共有できる起動の小さな抽出が必要な場合だけ変更し、解析の通信・予算・死因は変えない。
- `src/main.rs`、`src/hook.rs`、`src/hook/opencode.rs`、新規`src/advisor_log.rs`。`src/log.rs`は既存stateパスの共有に必要な抽出だけ。助言の保存安全性を影ログの新契約にしない。
- `tests/app_config.rs`、`tests/cli.rs`、`tests/hook.rs`、`tests/shadow.rs`、`tests/isolation.rs`、`tests/opencode_hook.rs`、新規`tests/advisor_*.rs`、`tests/fixtures/advisor/`。個人HOME/XDGを使わず、git fixtureはCARGO_TARGET_TMPDIRに置く。
- `plugins/opencode/src/guardian.ts`、`index.ts`、必要な新規制御・文脈モジュールと`tests/unit/`、`tests/integration/host.test.ts`、既存bridgeの助言fixture部分。approval/gate/connectionは既存の境界を接続する最小変更だけ。
- 新規`docs/verification/llm-advisor-context.md`と`docs/verification/llm-advisor.md`。前者は対応表と出所証拠、後者は試験、時間測定、未評価事項の記録。仕様の代用品ではない。
- 最終変更照合の`.kotowari/changes/implementation.yaml`と、独立担当の`review.yaml`。実装中に先回りしてレビュー記録を書かない。

検証分担の文書改訂はA63に基づくREQ-advisor-002の検証方法と三例の出典、A64に基づくREQ-advisor-008の検証方法と二例の出典、関連決定記録と本計画の検証対応だけを変更する。それ以外の承認済みIR、用語集、問題記録、既存決定記録、CIやhook設定、release版は変更しない。取得・保存・期限の実行時の試験不足を免除しない。ガイド変更が必要なら既存の指示を読み、承認された範囲を呼出元へ確認する。

## Step order and prerequisites

将来のブランチ名は`llm-advisor`。計画の承認コミット後、呼出元がGitからブランチ全体の比較元の完全IDを固定する。仕様コミットは上記IDだが、実装ブランチの比較元は計画の承認コミットを含むため同じIDと決めつけない。実装者は渡されたブランチと作業場所で作業し、この計画からbranch、worktree、mergeを勝手に作らない。

S1で取得の裏付けを先に集める。S2とS3で純粋な規則と利用者設定を固め、S4の文脈と保存、S5の送信境界へ進む。S6はこれらを同一バイナリの子と期限へ組み込む。S7の記録を分離し、S8でOpenCodeの外側期限を整合させる。S9で全入口を一度だけ最終結果を出す経路へ接続する。S10で範囲内の試験充足と製品検査を確認し、独立レビューと最終候補の変更照合へ引き渡す。コードを未完のまま本番入口へ接続しない。

S1に対応版の証拠がないホストが残っても、S4以降でその入力を文脈なしとして扱える。より広い履歴の読み取りや新しい入力形式を必要とする案を代わりに採用してはならない。TypeSafeの現在の提供状況とモデル精度は未検証であり、有料評価を実装の前提にしない。

## Verification map

表中のIDはSpecificationの完全リンクと`kotowari query ID`から追う。リンクの表示は正規の`docs/ir/<文書>.md#REQ-ID`参照を保ち、hrefはこの計画からの相対パスと実際のGitHub見出しアンカーを使う。決定番号はリンクの表示に残し、hrefはその番号がある節を指す。範囲表記は両端を含む。試験名は振る舞いの名前の例であり、内部関数名や固定のfixture配置を新契約にはしない。

| 段階 | 要求 | 例 | 証拠の単位 |
|---|---|---|---|
| S1、S4 | REQ-advisor-006 | EX-advisor-011、012 | 対応表の公開根拠と隔離ホストの生成記録、取得境界の試験 |
| S2、S9 | REQ-advisor-001 | EX-advisor-001、002 | observeは候補だけ、offとblockは取得ゼロ、影実行を解除しない |
| S2、S5 | REQ-advisor-004 | EX-advisor-007、008 | 悪意あるコマンド・理由・会話でも固定質問と分類定義を変更せず、assistantの自己承認を構造化入力の確認済み指示へ昇格させない単体試験 |
| S10 | REQ-advisor-002 | EX-advisor-003、004、053 | 独立担当が実際の固定質問と分類定義を契約と三つの反例へ照合し、実装パスと判断根拠を記録する。試験マークは不要 |
| S2、S9 | REQ-advisor-003 | EX-advisor-005、006、051、052 | 合成表の分岐と両閾値の境界。文脈なしの高確率重大破壊はenforceの最終出力でblock |
| S10 | REQ-advisor-005 | EX-advisor-009、010 | how_to_verifyのレビュー。試験マークは不要 |
| S4 | REQ-advisor-007 | EX-advisor-013、014 | 順序、往復数、role、参照材料の保持とツール・ファイルブロック除外の単体試験 |
| S10 | REQ-advisor-008 | EX-advisor-015、016 | 独立担当が実際の固定質問、分類定義、送信データを要求と二例へ照合し、撤回後の限定指示と参照不足の判断根拠を記録する。試験マークは不要 |
| S5、S9 | REQ-advisor-009 | EX-advisor-017、018 | 最終HTTP本文の65536/65537境界、秘密、非UTF-8 cwdの送信ゼロ |
| S4 | REQ-advisor-010 | EX-advisor-019、020 | 実ファイルで所有者・権限・link・TTL・時刻・保存削除失敗を検証 |
| S7、S9 | REQ-advisor-011 | EX-advisor-021、022 | 通常ログの本文不在、debugの伏字、失敗時の継続 |
| S2、S5、S6 | REQ-advisor-012 | EX-advisor-023、024 | 本番接続と試験接続が同じtraitを使い外部verdict実行経路がない |
| S2、S5、S6 | REQ-advisor-013 | EX-advisor-025、026 | 分布と選択の共通検証、重複キーと未知種別を拒否 |
| S5 | REQ-advisor-014 | EX-advisor-027、028 | 一つのPOSTに二質問、429/redirect/通信失敗でも送信回数を増やさない |
| S3 | REQ-advisor-015 | EX-advisor-029、030、043、044 | プロジェクトadvisor除去の順序とファイル全体不採用 |
| S3、S8 | REQ-advisor-016 | EX-advisor-031、032 | 全既定・値域・換算overflow、10秒設定と文脈0 |
| S6 | REQ-advisor-017 | EX-advisor-033、034 | 機械終了からの単一期限、取得停止ならモデル未実行 |
| S6 | REQ-advisor-018 | EX-advisor-035、036 | 子の成功と停止・死亡・panic・起動失敗、期限後採用なし |
| S6 | REQ-advisor-019 | EX-advisor-037、038 | 本物のソケットを使った版・nonce・長さ・JSON・EOF・末尾枠の境界 |
| S8 | REQ-advisor-020 | EX-advisor-039、040、045〜050 | 単調時計の期限計算とFD通信、旧相手・失ったack・長いT・配送の実測 |
| S9 | REQ-advisor-021 | EX-advisor-041、042 | Codex ask無出力、CLI失敗時の一回だけの機械結果、全ホスト出口 |
| S3 | REQ-013、014、015、034 | EX-014〜016、030、112、113 | 既存三層と信頼条件、容器誤りと規則誤りを別に検証 |
| S7、S9 | REQ-018、019、048 | EX-018、034、080、081、104、105 | 影ログと影実行封筒の既存試験を再使用 |
| S6、S9 | REQ-039 | EX-050、057 | 解析の既存予算と死亡写像の試験を再使用 |
| S8、S9 | REQ-049、050、051、053 | EX-090、091と各要求の既存about例 | 通常実行・background・code mode、承認・拒否・auto・取消し・判定不能の既存ホスト試験 |
| S10 | REQ-021 | EX-027、111 | offの100件の実測報告のレビュー。レビュー専用例に偽の試験は付けない |
| S9 | REQ-016、022、023、024 | 各要求の既存about例 | 単一バイナリ、Claude封筒、Codex無出力、非対象入力の既存試験 |

S10で表をqueryの実際の`tests`と照合する。試験必須の18助言要求、46助言例、EX-112とEX-113には、対象の振る舞いを検証する非空のマークが必要である。既存about例は現在の試験を読み直して再使用し、意味の変わらない動作に一例一試験を追加しない。REQ-advisor-002、005、008とREQ-021、そのレビュー専用例にはレビュー証拠を残す。質問数と既知ラベルの構造はREQ-advisor-014と013、固定質問と入力データの分離はREQ-advisor-004、出所と順序・窓・ブロック除外はREQ-advisor-006と007、TTLはREQ-advisor-010、合成はREQ-advisor-003の単体試験として扱う。文字列の固定や偽Assessmentの合成成功を質問の意味や指示の鮮度・参照不足の証拠にしない。

## Left to the implementer

内部の関数・モジュール・試験名、同じ依存方向を保つ小さなhelperの抽出、既存試験の内部整理だけを任せる。配置と依存方向はこの計画の通りにする。入力形式、対応版を裏付ける条件、判定境界、設定値域、期限、送信範囲、永続化契約、失敗時の挙動は任せない。fixtureの内部構造だけを根拠に新しい契約を決めない。

## Stop conditions

- 承認済み要求や決定に矛盾する意味、新しい入力種別、受理条件、保存形式、失敗処理が必要になったら、具体的な不足と該当IDを示してbrainstormへ返す。記録だけで許可を作らない。
- 裏付けられないホスト版や取得経路は文脈なしにする。限定窓や識別情報で実行との対応を確立できず、対応済みとするには送信範囲拡大や個人履歴の取得が必要ならその案を止める。
- ureqや固定Bun依存の取得・版・ライセンス、GNU/muslでのコンパイルに問題がある場合は依存の事実を報告する。実サービス呼出しや別HTTPライブラリへの黙った切替えで埋めない。
- 既存契約を壊す不具合、要求外へ広がる修正、特権・危険・不可逆・外部公開の操作、方式を変えても進まない状況は呼出元へ返す。既存の無関係な失敗を修正範囲へ追加しない。
- 500msの配送条件を実測で満たせなかった場合は数値と条件を報告する。予約値、成功時のT、既存判定不能経路を勝手に変えない。タイムアウトやfs syscall停止で元の期限を過ぎた場合の到達保証を作らない。

## Out of scope

実モデルの呼出し、精度評価、モデル探索、補助ネットワーク、再試行、任意URLや外部コマンド・動的プラグイン、askからallowへの変更、日次月次予算、ba-toys変更、実会話・実認証の取得を含めない。ホストの人間由来の完全証明、窓外の撤回の保証、秘密検出の完全性、子終了による課金取消し、全fs syscallの強制中断も保証しない。無関係な不具合修正、仕様の再承認、branch作成、merge、push、releaseはこの計画から実行しない。

## Steps

### S1: ホスト文脈の対応証拠を先に残す

- Purpose: 三ホストの文脈経路を、対応確認済みと未対応に区別して後続が推測で取得しないようにする。
- Specification: [docs/ir/advisor/context.md#REQ-advisor-006](../ir/advisor/context.md#req-advisor-006-当該実行への出所の対応付け)、[docs/ir/advisor/context.md#REQ-advisor-007](../ir/advisor/context.md#req-advisor-007-送信範囲と会話の上限)、[docs/ir/advisor/context.md#REQ-advisor-008](../ir/advisor/context.md#req-advisor-008-指示の鮮度と参照不足)。
- Prerequisites: 計画の承認コミットと呼出元が指定したブランチ・作業場所。PROJECT.mdと承認済みA33、A44、A48、A56を読む。
- May change: `docs/verification/llm-advisor-context.md`、`tests/fixtures/advisor/`の架空ホストデータと出所記録だけ。
- Done when: Claude Code、Codex、OpenCodeについて公式公開契約またはリリース済みソースの版と参照先、隔離した対応版の架空入力から生成した形式、セッション・発言・実行前境界・role・既知の合成入力の写像を対応表に記し、未確認経路を文脈なしと明示している。パス形式のfixtureと実ホスト生成の根拠を分け、role=userの限界を記している。
- Shown by: artifact `docs/verification/llm-advisor-context.md`と架空fixtureの生成手順・対応版・参照先。公開コードと生成結果の対応をレビューし、未対応欄も完了として扱う。個人会話と実モデルを使用しない。
- Left to the implementer: 架空発言とfixtureの内部名だけ。
- Stop and hand back if: 新しい入力種別、広い履歴、未承認の保存形式がないと対応を主張できない。単なる未対応版・欠落は止めず、表へ文脈なしと記す。

### S2: 純粋な助言値、固定質問、検証、合成を作る

- Purpose: providerやホストに依存しない一つの検証と候補合成を作る。
- Specification: [docs/ir/advisor/policy.md#REQ-advisor-001](../ir/advisor/policy.md#req-advisor-001-検査対象と観測モード)、[docs/ir/advisor/policy.md#REQ-advisor-002](../ir/advisor/policy.md#req-advisor-002-危険性と指示範囲の分類)、[docs/ir/advisor/policy.md#REQ-advisor-003](../ir/advisor/policy.md#req-advisor-003-分岐ごとの閾値と候補判定)、[docs/ir/advisor/policy.md#REQ-advisor-004](../ir/advisor/policy.md#req-advisor-004-入力を判定指示にしない)、[docs/ir/advisor/provider.md#REQ-advisor-012](../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)、[docs/ir/advisor/provider.md#REQ-advisor-013](../ir/advisor/provider.md#req-advisor-013-分布の境界検証)。
- Prerequisites: S1。TBL-advisor-001と承認済みA59をqueryと決定記録で確認する。
- May change: 新規`crates/guardian-advisor/`、`Cargo.toml`、`Cargo.lock`のworkspace登録と試験依存だけ。
- Done when: 定義済みラベルの分布と一意最大の選択を共通境界で検証し、固定質問と分類定義を本文から分離し、悪意あるコマンド・理由・会話を与えても固定質問が変わらず、assistantの自己承認が構造化入力の確認済み指示にならない。表の全分岐、閾値の直下・一致・直上、機械askをallowにしないこと、独立した有害効果の指示一致block、observeの最終維持を検証し、AdvisorClientの入力と分類済み失敗にprovider型がない。質問の意味と三つの反例の照合はS10の独立レビューへ渡す。
- Shown by: test RED→GREEN→REFACTORのshell出力。`cargo test -p guardian-advisor --locked`で「指示一致でも独立有害効果はblock」「重大破壊は両閾値と確認済み指示でask」「固定質問を入力文が変更しない」「欠落重複不正数値同率最大を採用しない」を検証し、EX-advisor-001、002、005〜008、025、026、051、052の該当マークを付ける。偽Assessmentの優先分岐試験は合成の証拠であり、分類定義の意味の証拠ではない。EX-advisor-003、004、053はS10で実際の固定質問と契約へ照合し、機械試験のマークは要求しない。実モデル精度を検証したとは記さない。
- Left to the implementer: 内部の値・helper・試験の名前だけ。
- Stop and hand back if: 固定質問に承認されていない判定条件を足す必要がある、または既存guardrailのverdictを共通契約へ入れる必要がある。

### S3: 利用者専用設定とプロジェクト除去を実装する

- Purpose: 助言の設定がプロジェクトから変わらず、無視する不正値で正常な保護を失わないようにする。
- Specification: [docs/ir/advisor/provider.md#REQ-advisor-015](../ir/advisor/provider.md#req-advisor-015-利用者だけが助言を設定する)、[docs/ir/advisor/provider.md#REQ-advisor-016](../ir/advisor/provider.md#req-advisor-016-設定可能な既定と不正値)、[docs/ir/config.md#REQ-013](../ir/config.md#req-013-設定のファイルと層)、[docs/ir/config.md#REQ-014](../ir/config.md#req-014-プロジェクト設定の信頼)、[docs/ir/config.md#REQ-015](../ir/config.md#req-015-壊れた設定)、[docs/ir/guards.md#REQ-034](../ir/guards.md#req-034-例の検証と壊れた規則の扱い)。
- Prerequisites: S2。TBL-advisor-002とA46、A60を読み、現行parse_layerとread_layerの一般設定エラーとguard単位の境界を保つ。
- May change: `crates/guardian-policy/src/config.rs`、`layers.rs`、`lib.rs`、そのmanifestと試験、`crates/guardian-app/src/config_loader.rs`、`tests/app_config.rs`、必要なroot試験依存。`Cargo.lock`はこの段階のmanifest変更に対応する更新だけを許す。
- Done when: TOML構文解析の後、信頼済みを含むプロジェクトadvisor全体を固有型・値・未知キーの検証より先に除去して警告し、利用者設定だけを適用する。全既定、値域、時間・長さ・枠計算overflowを検証し、不正な利用者設定または一般設定はファイル全体不採用、個別guardだけの不正は規則単位とする。公開CLI引数は増えない。
- Shown by: test 対象policy試験と`cargo test -p command-guardian --locked --test app_config`のRED→GREEN→REFACTOR。manifest変更時は対応するCargo.lockをこの段階で更新してから`--locked`付きのGREENを実行する。EX-advisor-029〜032、043、044、EX-112、113と既存EX-014〜016、030を実際の信頼済み・未信頼TOMLとfixture HOME/XDGで検証し、既存で成り立つ容器・規則境界は試験を再利用する。
- Left to the implementer: TOML解析後の小さなhelper抽出と内部試験名だけ。
- Stop and hand back if: 一般キーの受理や信頼条件の変更が必要、または表現可能性の不足を黙った上限縮小で処理する必要がある。

### S4: 限定文脈と必要な安全保存を実装する

- Purpose: 取得の出所と限定窓を照合し、取得できない場合は指示を創作せず文脈なしにする。
- Specification: [docs/ir/advisor/context.md#REQ-advisor-006](../ir/advisor/context.md#req-advisor-006-当該実行への出所の対応付け)、[docs/ir/advisor/context.md#REQ-advisor-007](../ir/advisor/context.md#req-advisor-007-送信範囲と会話の上限)、[docs/ir/advisor/context.md#REQ-advisor-008](../ir/advisor/context.md#req-advisor-008-指示の鮮度と参照不足)、[docs/ir/advisor/context.md#REQ-advisor-010](../ir/advisor/context.md#req-advisor-010-必要な経路だけのセッションキャッシュ)、[docs/ir/advisor/runtime.md#REQ-advisor-019](../ir/advisor/runtime.md#req-advisor-019-長さと要求対応を検証する内部通信)。
- Prerequisites: S1〜S3。取得処理をappに置き、S1の対応表で裏付けた版・経路だけを有効にする。
- May change: `crates/guardian-advisor/`の純粋な文脈値と窓処理、`crates/guardian-app/src/advisor/context/`、`state.rs`、`lib.rs`とmanifest、対応試験と`tests/fixtures/advisor/`、対応表。`Cargo.lock`はこの段階のmanifest変更に対応する更新だけを許す。
- Done when: 同じセッションの実行前限定窓だけを取得し、返答前のuserと連続roleを正しく数え、撤回・参照先不足・ツールとファイルブロック・既知の合成入力を区別する。ローカル照合情報は送信値と分離し、CLIと未対応は文脈なしになる。直接利用できる経路にキャッシュを作らず、必要な別hook経路だけ承認済み版と枠・照合情報を保存し、raw session名をパス要素にせず、所有者、0700/0600、通常ファイル、symlink、複数hardlink、TTL、未来・不明時刻を検査する。保存・期限切れ削除失敗でも古い指示へ復帰せず、取得量と処理時間を上限内にする。
- Shown by: test advisorの窓処理とappの取得・state試験のRED→GREEN→REFACTOR。manifest変更時は対応するCargo.lockをこの段階で更新してから`--locked`付きのGREENを実行する。EX-advisor-011〜014、019、020を、S1のホスト生成根拠付き架空データと実Unixファイル操作で検証する。別セッション、実行後、未対応版、壊れた枠、TTL境界、時刻不整合、読取・保存・削除失敗、link攻撃、取得停止を製品境界へ与え、fixture自体の構造検査を証拠にしない。撤回や参照先を判断するための発言と確認状態・限界を送信データへ保持する構造はここで確認し、EX-advisor-015、016の意味の照合はS10の独立レビューへ渡す。TTL内であることや偽Assessmentの合成を許可の鮮度の証拠にしない。
- Left to the implementer: helperと架空データの内部名だけ。
- Stop and hand back if: 承認済みの版・枠・照合項目では保存契約を表せない、または対応済みとするために取得範囲を広げる必要がある。欠落と未対応は文脈なしで継続する。

### S5: TypeSafe接続と送信許可の境界を実装する

- Purpose: 最終本文を検査した場合だけ、固定先へ二質問を一度送れる静的接続を作る。
- Specification: [docs/ir/advisor/policy.md#REQ-advisor-002](../ir/advisor/policy.md#req-advisor-002-危険性と指示範囲の分類)、[docs/ir/advisor/policy.md#REQ-advisor-004](../ir/advisor/policy.md#req-advisor-004-入力を判定指示にしない)、[docs/ir/advisor/context.md#REQ-advisor-009](../ir/advisor/context.md#req-advisor-009-秘密とサイズと符号化による見送り)、[docs/ir/advisor/provider.md#REQ-advisor-012](../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)、[docs/ir/advisor/provider.md#REQ-advisor-013](../ir/advisor/provider.md#req-advisor-013-分布の境界検証)、[docs/ir/advisor/provider.md#REQ-advisor-014](../ir/advisor/provider.md#req-advisor-014-typesafeへの一要求)。
- Prerequisites: S2〜S4。ureqのAPI版、依存ライセンス、取得可能性を確認し、A43、A49、A54、A61と参考実装の再利用証拠を読む。実サービスの確認はしない。
- May change: 新規`crates/guardian-advisor-typesafe/`、advisorの秘密検査と送信許可値、appの接続組立て、関連manifestと`Cargo.lock`、接続・送信境界の試験。
- Done when: fixed state/model/questionsの最終UTF-8 HTTP JSON本文全体を秘密検査とバイト測定した後にだけ送る。固定HTTPS先へrisk/scopeを一つのPOSTにまとめ、入力データと固定質問・分類定義の分離を符号化後も維持する。認証はアダプター内のTYPESAFE_API_KEYから別ヘッダーへ付ける。redirect、retry、モデル探索、補助要求、別モデルfallbackがなく、残期限と65536バイト応答上限を守る。重複キーを潰す前に拒否し、不正応答は共通検証へ渡して分類済み失敗にし、生本文・認証・confidenceを呼出元へ漏らさない。
- Shown by: test 接続クレートとadvisorのRED→GREEN→REFACTOR。EX-advisor-017、018、023〜028を制御transportで検証し、実際に符号化した65536/65537バイト、モデル名・質問・理由・cwd・文脈の秘密、非UTF-8 cwd、認証欠落、429、redirect、停止、応答超過と不正種別を扱う。REQ-advisor-004について悪意ある本文でも符号化した固定質問が変わらず、assistantの自己承認を確認済み指示へ昇格させないことを確認する。二質問と既知ラベルの構造はREQ-advisor-014と013の証拠として扱う。外部送信ゼロまたは最大1回を境界で観測し、実キー・実モデル・公開URL切替えを使わない。
- Left to the implementer: provider内の変換helperと試験名だけ。
- Stop and hand back if: 承認済みTypeSafe形式を実装するのに新しい公開型や通信が必要、またはureqの依存事実が満たせない。実サービスとの現在の適合は未検証と報告する。

### S6: 同一バイナリの助言子と単一期限を実装する

- Purpose: 文脈取得や接続が止まっても親が打ち切り、機械結果を保つ。
- Specification: [docs/ir/advisor/provider.md#REQ-advisor-012](../ir/advisor/provider.md#req-advisor-012-現在の利用と試験に必要なモデル境界)、[docs/ir/advisor/provider.md#REQ-advisor-013](../ir/advisor/provider.md#req-advisor-013-分布の境界検証)、[docs/ir/advisor/runtime.md#REQ-advisor-017](../ir/advisor/runtime.md#req-advisor-017-文脈取得を含む単一の助言期限)、[docs/ir/advisor/runtime.md#REQ-advisor-018](../ir/advisor/runtime.md#req-advisor-018-同一バイナリの子と強制打切り)、[docs/ir/advisor/runtime.md#REQ-advisor-019](../ir/advisor/runtime.md#req-advisor-019-長さと要求対応を検証する内部通信)、[docs/ir/parser.md#REQ-039](../ir/parser.md#req-039-入力の上限)。
- Prerequisites: S2〜S5。既存run_if_child/spawn_workerの起動順を読み、A62の継承ソケットと助言専用の失敗規則を使う。
- May change: `crates/guardian-app/src/advisor/`のservice、worker、wire、期限モジュール、`engine.rs`、`lib.rs`、必要なruntime起動helper抽出、`src/main.rs`の起動時dispatch、`tests/advisor_*.rs`、fixture子。
- Done when: 機械終了直後の単調時計から設定Tの期限を一度作り、通知、起動、取得、cache、検査、符号化、HTTP、応答検証の全待ちを含める。専用子は同一バイナリで公開引数処理前にdispatchし、版1、16バイトnonce、種別、大端長、UTF-8 JSONを継承Unixソケットで扱う。要求上限の加算overflowと巨大長を確保前に拒否し、版・nonce・種別・必須欄・重複キー・EOF・余分な枠を検査する。親が期限後の回答を捨て通信を閉じ子を終了させ、死亡やpanicを新しいblock/askにせず機械結果へ戻り、回収確認不能は定型警告だけで無期限waitしない。
- Shown by: test appと`cargo test -p command-guardian --locked --test isolation`および助言実バイナリ試験のRED→GREEN→REFACTOR。EX-advisor-033〜038を実ソケット、制御した遅延・死亡・停止のfixture子と本物の配布バイナリdispatchで検証する。機械4秒と助言2秒を分け、取得停止ではモデル未実行、認証欠落では機械結果、期限外回答では採用ゼロを観測し、既存EX-050、057を再実行する。
- Left to the implementer: 内部の起動helperと試験の名前だけ。
- Stop and hand back if: libtest再起動、公開実行ファイル指定、親の直接モデルfallback、解析の死因流用、期限外の回収待ちが必要になる。

### S7: 本文を既定で残さない助言ログを実装する

- Purpose: 助言の候補と失敗を影ログとは別に安全に観測できるようにする。
- Specification: [docs/ir/advisor/context.md#REQ-advisor-010](../ir/advisor/context.md#req-advisor-010-必要な経路だけのセッションキャッシュ)、[docs/ir/advisor/context.md#REQ-advisor-011](../ir/advisor/context.md#req-advisor-011-助言ログと影ログの分離)、[docs/ir/cli.md#REQ-018](../ir/cli.md#req-018-影実行)、[docs/ir/cli.md#REQ-019](../ir/cli.md#req-019-影実行のログ)、[docs/ir/opencode.md#REQ-048](../ir/opencode.md#req-048-共通設定と影実行)。
- Prerequisites: S4〜S6。state保存の下向きの部品を使い、appからrootのlog.rsを参照しない。
- May change: 新規`src/advisor_log.rs`、appの助言ログ値と`state.rs`、`src/log.rs`の配置共有helperだけ、助言ログ試験と既存`tests/shadow.rs`。
- Done when: advisor.jsonlに分類済みの時刻・モード・機械/候補/最終結果・時間・失敗/見送り・モデル名だけを既定で残し、検出したメタデータ秘密も伏せる。明示debug_textだけで検出秘密を伏せた送信候補本文を許し、認証と通信エラー本文は記録しない。新しい助言分類の理由・警告・診断にも本文や識別子を混ぜない。保存安全性は助言の所有者・権限・link規則を使い、保存失敗を本文なし警告で続け、判定・配送・子回収の期限を延長しない。既存の機械JSONのeffects.pathや対象を示す理由、影ログの本文と保存条件は維持し、それらにパスが見えることを助言ログの漏洩と混同しない。
- Shown by: test 助言ログとshadowのRED→GREEN→REFACTOR。EX-advisor-021、022を実stateファイルで検証し、debug_text=falseのadvisor.jsonlと新しいguardian生成の助言分類理由・警告・診断にコマンド、cwd、文脈、識別子がないことを確かめる。debug_text=trueでは許された送信候補本文と検出秘密の伏字を検証する。新しい助言ログ・分類理由・警告・診断と助言が標準出力へ追加する内容に、生HTTPエラーと認証を含めない。既存の機械JSONのeffects.pathと対象を示す理由、本文を含む影ログは従来どおり出ることも確認する。symlink/hardlink/保存停止・失敗を製品境界で試し、既存EX-018、034、080、081、104、105の影契約を再使用する。
- Left to the implementer: ログ組立てhelperと試験名だけ。
- Stop and hand back if: 生レスポンスを理由として返す、保存失敗で判定を変える、または配送前に期限外のログ待ちが必要になる。

### S8: OpenCodeの期限交渉を実装する

- Purpose: 長い利用者timeoutを固定6秒で縮めず、交渉失敗時は生成済み機械結果を配送する。
- Specification: [docs/ir/advisor/provider.md#REQ-advisor-016](../ir/advisor/provider.md#req-advisor-016-設定可能な既定と不正値)、[docs/ir/advisor/runtime.md#REQ-advisor-020](../ir/advisor/runtime.md#req-advisor-020-opencodeの外側期限との整合)、[docs/ir/opencode.md#REQ-048](../ir/opencode.md#req-048-共通設定と影実行)、[docs/ir/opencode.md#REQ-049](../ir/opencode.md#req-049-権限判断を弱めない合成)、[docs/ir/opencode.md#REQ-050](../ir/opencode.md#req-050-実行ごとの承認)、[docs/ir/opencode.md#REQ-051](../ir/opencode.md#req-051-判定結果を取得できない場合)、[docs/ir/opencode.md#REQ-053](../ir/opencode.md#req-053-承認待ちの寿命)。
- Prerequisites: S3、S6、S7。PROJECT.mdのBun 1.4.2、固定2.0.21と既存fixtureサービスを用意する。
- May change: appの助言制御IPC、`src/hook/opencode.rs`、`plugins/opencode/src/guardian.ts`と制御モジュール、必要なindex接続、unit/integration試験とbridge助言fixture。
- Done when: plugin起動時Oを固定し、利用者設定で有効な起動だけ機械判定前に一度probeする。guardianは送信直前Qと100ms以内の正しい応答からD0を作る。機械終了Mと設定TでDA=M+T、N=min(M+100ms,D0-500ms,DA)を計算し、N<=Mは即時配送する。pluginは元のOに500ms超の余裕がある初回通知だけ受理し、受信時からT+1000msへ外側期限を切り替えてackを返す。N未満のackだけで残DAの子を開始し、旧相手、FDなし、拒否、EOF、無ack、遅いackは再交渉・モデルなしで機械結果を返す。offはprobeなし、blockはprobe以外の助言なし、stdoutは最終JSONだけである。
- Shown by: test 制御期限のRust/Bun unitと固定ホストintegrationのRED→GREEN→REFACTOR。EX-advisor-039、040、045〜050の数値境界、10秒設定、probe遅延、予約境界、失ったack、元期限超過、取消し後の遅い応答を検証する。実FD通信で機械結果配送と500ms未満の配送条件を測定し、通常/background/code modeと承認・拒否・auto・影実行の既存試験を再実行する。助言だけの失敗をguardian全体の判定不能へ変えない。
- Left to the implementer: 制御helperと試験名だけ。
- Stop and hand back if: 継承FDが固定ホストで使えない、測定が配送余裕に反する、または成功時Tや既存承認/取消しの意味を変える必要がある。

### S9: 三ホストとCLIの最終結果を接続する

- Purpose: 全入口で機械判定後に一度だけ合成し、既存の出口と影実行を維持する。
- Specification: [docs/ir/advisor/policy.md#REQ-advisor-001](../ir/advisor/policy.md#req-advisor-001-検査対象と観測モード)、[docs/ir/advisor/policy.md#REQ-advisor-003](../ir/advisor/policy.md#req-advisor-003-分岐ごとの閾値と候補判定)、[docs/ir/advisor/context.md#REQ-advisor-009](../ir/advisor/context.md#req-advisor-009-秘密とサイズと符号化による見送り)、[docs/ir/advisor/context.md#REQ-advisor-011](../ir/advisor/context.md#req-advisor-011-助言ログと影ログの分離)、[docs/ir/advisor/runtime.md#REQ-advisor-021](../ir/advisor/runtime.md#req-advisor-021-最終判定だけを既存の出口へ渡す)、[docs/ir/cli.md#REQ-016](../ir/cli.md#req-016-実行ファイルとコマンド)、[docs/ir/cli.md#REQ-018](../ir/cli.md#req-018-影実行)、[docs/ir/agents.md#REQ-022](../ir/agents.md#req-022-claude-code-の写像)、[docs/ir/agents.md#REQ-023](../ir/agents.md#req-023-codex-の写像)、[docs/ir/agents.md#REQ-024](../ir/agents.md#req-024-判定しない入力)、[docs/ir/parser.md#REQ-039](../ir/parser.md#req-039-入力の上限)、[docs/ir/opencode.md#REQ-048](../ir/opencode.md#req-048-共通設定と影実行)、[docs/ir/opencode.md#REQ-053](../ir/opencode.md#req-053-承認待ちの寿命)。
- Prerequisites: S1〜S8。S1の対応証拠がない版・実行は文脈なしとして接続し、OpenCode FD非対応は助言全体を見送る。
- May change: `src/main.rs`、`src/hook.rs`、`src/hook/opencode.rs`、appのengine/service接続、`plugins/opencode/src/index.ts`と文脈接続、`tests/cli.rs`、`hook.rs`、`shadow.rs`、`opencode_hook.rs`、助言試験、既存ホストintegrationの直接依存部分。
- Done when: 機械Reportの効果一覧を変えず親で候補を合成し、厳しくした理由はguardian分類から作り、既存JSONと終了コードを一度だけ出す。Claudeのask抑止、Codex allow/ask無出力とblock deny、OpenCode shadow/unavailable/承認/auto/取消しを保つ。off、observe、秘密、送信量超過、符号化不能、不正応答、子や認証の失敗、助言期限超過では規定の機械結果を利用する。欠落・未対応・期限内の取得失敗は文脈なしとして、他の条件を満たせばモデル評価を続ける。妥当なmajor_destructiveが閾値以上で確認済み指示と高確率matchedがなければenforceの最終判定はblockとし、harmful_irreversibleが閾値以上なら指示一致でもblockとする。取得が助言期限を使い切った場合は文脈なしで続けず助言全体のtimeoutとして機械結果へ戻る。影実行を助言enforceで解除しない。
- Shown by: test 実バイナリCLI/hook/shadow/opencode_hookと固定ホストintegrationのRED→GREEN→REFACTOR。EX-advisor-041、042、001、002、005、006、018、021、051、052を最終出力と終了コードで検証する。EX-advisor-006はenforce・機械allow・文脈なしで妥当なmajor_destructive 0.95を受けたとき、最終blockとcheck終了コード2を確認し、0.89なら機械結果維持も確認する。期限内に終わる取得失敗で同じ文脈なし評価へ進む場合と、取得が助言期限を使い切りモデル未実行で機械結果へ戻る場合を別に検証する。Codex askにdenyがないこと、指示一致の独立有害効果はblockであること、助言失敗時にCLI3やOpenCode判定不能を作らないこと、off/blockの取得とモデルゼロを観測する。既存about例と非対象入力、解析隔離の試験を再使用する。
- Left to the implementer: 入口の共通呼出しhelperと内部試験名だけ。
- Stop and hand back if: 新しい公開CLI入力、モデル由来の効果一覧、機械結果があるのに判定不能を返す処理、またはホスト出口の意味変更が必要になる。

### S10: 対象の充足と最終候補の検査を引き渡す

- Purpose: 対象の実装証拠、未評価事項、独立変更照合を揃え、呼出元が最終候補を検査できるようにする。
- Specification: [docs/ir/advisor/policy.md#REQ-advisor-002](../ir/advisor/policy.md#req-advisor-002-危険性と指示範囲の分類)、[docs/ir/advisor/policy.md#REQ-advisor-005](../ir/advisor/policy.md#req-advisor-005-モデルの品質を構造試験と区別する)、[docs/ir/advisor/context.md#REQ-advisor-008](../ir/advisor/context.md#req-advisor-008-指示の鮮度と参照不足)、[docs/ir/cli.md#REQ-021](../ir/cli.md#req-021-応答時間)、SpecificationとVerification mapの全要求・例。
- Prerequisites: S1〜S9。呼出元がブランチ全体BASEと候補HEADの完全IDをGitから確定し、実装と別コンテキストの独立担当を割り当てる。
- May change: `docs/verification/llm-advisor.md`、対象試験の意味に合うマークの不足修正、実装者の`.kotowari/changes/implementation.yaml`。`review.yaml`は独立担当だけが著述する。
- Done when: queryで試験必須の18助言要求、46助言例、EX-112/113と対応する既存例のtestsが非空で、全対象と変更ファイルにcheckのerrorがない。REQ-advisor-002は実装と別コンテキストの独立担当が、実際に送る固定質問と危険性5分類・指示範囲3分類の定義を要求本文へ照合し、限定DELETEを重大破壊と即断しない、scratchから使い捨てや承認を推測しない、重大破壊と独立有害効果の併存ではharmful_irreversibleを優先する、の三例ごとに根拠を記録する。REQ-advisor-008は同じ独立担当が、実際の固定質問、分類定義、符号化後の文脈データと確認状態・限界を本文へ照合し、EX-advisor-015の撤回後の限定指示を反映して古い全体破棄の指示を流用しないこと、EX-advisor-016の窓外の参照を推測で埋めずmatchedとしないことを二例ごとに記録する。矛盾と別操作への切替え、窓外の制約の限界、TTLと許可の鮮度の違いも照合する。著者自身の完了宣言をこのレビューの代わりにしない。REQ-advisor-005のhow_to_verifyでは実行条件、実モデル未評価、暫定閾値、latestの非固定を確認する。off代表100入力をgit起動込みで計測し未達もそのまま報告し、有効時は制御接続の待ちを別に記す。製品検査を通し、同一BASE・候補bytes・関連IRについて実装者と独立担当が各自の記録を作る。両記録のコミット後に呼出元が最終HEADを再固定し、その候補の製品検査、checkとchanges reviewが全て0である場合だけ統合可能と報告する。
- Shown by: check Rustのfmt/clippy/test/release、Bunのtypecheck/unit/integration、既存GNU/muslと配布検査、`scripts/kotowari-check.sh`、各IDの`kotowari query`、レビュー証拠、呼出元の`kotowari check --format json`と`kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format json`の出力。REQ-advisor-002の独立レビュー証拠には、読んだ固定質問と分類定義の実装パス、照合した要求本文、EX-advisor-003、004、053ごとの判断根拠と未解決事項を残す。REQ-advisor-008には、読んだ固定質問、分類定義、送信データの実装パス、照合した要求本文、EX-advisor-015、016ごとの判断根拠と未解決事項を残す。固定質問の`crates/guardian-advisor/src/lib.rs`のQuestions::fixedから、`crates/guardian-advisor/src/context.rs`の文脈値と窓処理、`crates/guardian-advisor-typesafe/src/lib.rs`のprepareが符号化するquestions/stateまで追い、確認状態、除外した参照材料、窓外の制約、人間由来の限界が実際にどう渡るかを確認する。文言一致や偽Assessment試験を意味の証明とせず、モデル精度は未評価と区別する。範囲の充足とPROJECT.mdの全体統合ゲートを分け、無関係な残存エラーは報告して統合を止める。statusのcompleteは条件にしない。
- Left to the implementer: 検証報告の内部配置と試験名だけ。比較元・最終候補と承認は呼出元が決める。
- Stop and hand back if: 仕様と証拠が一致しない、未処理の意味判断がある、独立レビューがない、最終HEAD再確認後の検査がない、またはコード・IR・決定の意味変更やrebase等で記録が古い。同じentry全体を独立レビューし直して両記録を再著述するまでは統合しない。
