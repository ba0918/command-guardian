# Plan: 危険な Bash コマンドの判定 CLI とフック（M1）

## Goal

危険な Bash コマンドをパスの意味論と program ごとの見張りで判定し、消えると戻せないと確信できる操作だけを止め、確信が持てないときは止めずに確認を返す `hook-guardian` が、Claude Code と Codex のフックとして動き、mise で導入できる。

## Specification

IR は `docs/ir/` にある。この計画は次を対象とする。

- 判定: `docs/ir/judgment.md#REQ-001`、`#REQ-002`、`#REQ-003`、`#REQ-004`、`#REQ-005`、`#REQ-006`、`#REQ-007`、`#REQ-008`、`#REQ-009`、`#REQ-010`
- 文面: `docs/ir/messages.md#REQ-011`、`#REQ-012`（review）
- 設定: `docs/ir/config.md#REQ-013`、`#REQ-014`、`#REQ-015`、`#REQ-026`
- コマンド: `docs/ir/cli.md#REQ-016`、`#REQ-017`、`#REQ-018`、`#REQ-019`、`#REQ-020`、`#REQ-021`（review）
- エージェント: `docs/ir/agents.md#REQ-022`、`#REQ-023`、`#REQ-024`
- 見張り: `docs/ir/guards.md#REQ-027`、`#REQ-028`、`#REQ-029`、`#REQ-030`、`#REQ-031`、`#REQ-032`、`#REQ-033`、`#REQ-034`、`#PROP-001`
- 配布: `docs/ir/release.md#REQ-025`（review）
- 例: EX-001 から EX-045。EX-013、EX-023、EX-027 は review だけの要件に属するためテストを要さない

判断の出典は `docs/decision/records/2026-09-30-hook-guardian-scope.md`（A1〜A48、U1）と `docs/ir/FLAGS.md#FLAG-001`。要件の本文と印付きのテストは `kotowari query REQ-001` のように読み、例は同じ出力の `referenced_by` で引く。

## Approach and why

判定の流れは 3 段にする。コマンド文字列を解析して効果（削除・切り詰め・フォーマット）と対象パスを取り出し（core）、一時領域と git の状態と設定のルートでパスを分類し（judge）、設定とルールと見張りから allow / ask / block を合成する（policy）。CLI はその上に薄く載せ、フックの口はプロトコルの写像だけを担う。

- 解析・分類・設定を分ける理由: 実測した誤ブロック 381 件のコマンド列を、設定なしの core と judge でそのままテストできる（crate の分割は A2・A10）
- 判定の既定は「確信が持てないときは ask」で、block は保護領域だけ。内部エラーで block しない（A3・A14）
- 解決できないパスだけは ask ではなく block にし、literal なパスでのやり直しを促す（A28）
- 設定は TOML の 3 層で、プロジェクト設定は締める方向だけ反映する（A18・A20）
- `commands.guard` は kakoi の `[[commands.guard]]` の語の照合をそのまま持ち、`verdict`（"ask" / "block"、既定 "ask"）を足す（A42〜A44）
- 見張りは柵であって境界ではない。実行の強制（隔離やアクセス制御）の仕組みは作らない（A46・PROP-001）
- テストは crate の中に置くため、`.kotowari/config.yaml` の `tests.files` に `crates/**/src/**/*.rs` と `crates/**/tests/**/*.rs` を足す
- テストには kotowari の印（テストの直前に `// @kotowari[REQ-001, EX-010]` の形で置く）を付け、印の無いテストを残さない
- git は対象が作業ツリーの中にあるときだけ起動し、git の失敗は ask に落とす（A16・A14）
- 影実行（`mode.enforce = false`）ではフックとして何も返さずログだけを残し、check は影実行でも判定を出す（A22・A34）

## Scope of change

- `Cargo.toml`、`Cargo.lock`（workspace の定義）
- `crates/guardian-core/`（新規。解析・効果の抽出・パスの解決・共通の型）
- `crates/guardian-judge/`（新規。fs と git によるパスの分類）
- `crates/guardian-policy/`（新規。設定・ルール・見張り・判定の合成・文面）
- `src/`（新規。バイナリ、CLI、フックの口、影実行のログ）
- `.kotowari/config.yaml`（`tests.files` の追加だけ）
- `.github/workflows/`（テストとリリース）

## Step order and prerequisites

S1 でワークスペースと解析を作り、S2 でパスの解決を足す。S3 と S4 で分類と判定、S5 と S6 で残りの判定規則、S7 で文面を足す。S8 で設定を差し込み、S9 から S13 でルールと見張りを足す。S14 から S16 は S1 から S13 の全部に依存する CLI とフックの口と影実行。S17 と S18 で全体を確かめ、S19 で人が公開する。

S1 から S16 は前のステップの上に積み、並べ替えられない。S8 は S3 の判定に設定の層を足す。S18 は S17 までが全部済んでから行う。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-001 | EX-010 |
| S2 | REQ-002 | — |
| S3 | REQ-003, REQ-005, REQ-006 | EX-001, EX-003, EX-004, EX-024, EX-029 |
| S4 | REQ-004, REQ-020 | EX-002, EX-005, EX-019, EX-028 |
| S5 | REQ-008 | EX-006, EX-007 |
| S6 | REQ-007, REQ-009, REQ-010 | EX-008, EX-009, EX-011 |
| S7 | REQ-011 | EX-012 |
| S8 | REQ-005, REQ-006, REQ-013, REQ-014, REQ-015 | EX-014, EX-015, EX-016, EX-030 |
| S9 | REQ-026 | EX-035, EX-036 |
| S10 | REQ-027, REQ-028 | EX-040, EX-044 |
| S11 | REQ-029 | EX-038 |
| S12 | REQ-030, REQ-031 | EX-039, EX-045 |
| S13 | REQ-032, REQ-033, REQ-034 | EX-037, EX-041, EX-042, EX-043 |
| S14 | REQ-017 | EX-017, EX-031, EX-032, EX-033 |
| S15 | REQ-016, REQ-022, REQ-023, REQ-024 | EX-020, EX-021, EX-022, EX-025 |
| S16 | REQ-018, REQ-019 | EX-018, EX-026, EX-034 |
| S17 | REQ-012（review）, REQ-021（review） | EX-013, EX-027 |
| S18 | REQ-001〜REQ-034（全要件の確認） | 全て |
| S19 | REQ-025（review） | EX-023 |

## Left to the implementer

- crate の中のモジュールの分け方と名前、型の名前
- コマンド文字列の解析を自前で書くか crate を使うか、正規表現の crate の選び方
- CLI の引数解析の crate、JSON の組み立ての内部構造
- Rust の edition と最低対応版
- テストをファイルにどう分けるか

## Stop conditions

- 要件が定めていない入力の種類、受け入れの境界、エラーの扱いを決める必要が生じた（仕様の問いなので brainstorm に戻す）
- 旧フックの守備範囲と IR の取り出し元が食い違い、どちらに合わせるか要件から読めない
- git の起動を含む判定が 100ms の目標を満たせない
- リポジトリに remote が無く、CI と Release を設定できない（S19 の前提。人が用意する）
- 実測コーパスのコマンドで、要件と実装のどちらが正しいか読めない食い違いが出た

## Test command

```sh
cargo test --workspace
```

## Out of scope

- U1 の 4 操作（git clean、mv・cp の上書き、sed -i、rsync --delete）
- dotfiles 側の配線の差し替えと旧フックの削除（A23）
- ドッグフーディングの運用と、公開の時期の判断（A6。S19 の人が行う）
- Windows ネイティブと macOS 向けのビルド

## Steps

### S1: ワークスペースと破壊的効果の抽出

- Purpose: Cargo の workspace と core の型を作り、コマンドから 3 種の破壊的効果と対象パスを取り出す
- Specification: docs/ir/judgment.md#REQ-001
- Prerequisites: cargo と rustc が使える
- May change: Cargo.toml, Cargo.lock, crates/guardian-core/**, crates/guardian-judge/**, crates/guardian-policy/**, src/**, .kotowari/config.yaml（tests.files の追加だけ）
- Done when: rm、rmdir、unlink、find の -delete と -exec rm、xargs、shred、リダイレクトの >、dd、truncate、mkfs、wipefs から効果が取り出され、引用・ヒアドキュメント・コマンド置換・sudo と doas・コマンド列（; と &&）・パイプの各段・bash -c・eval の内側が読まれ、M1 で扱わない 4 つの操作からは効果が出ないことがテストで確かめられている
- Shown by: test — EX-010、REQ-001 の取り出し元ごとのテスト
- Left to the implementer: モジュールと型の名前、解析を自前で書くか crate を使うか
- Stop and hand back if: 旧フックの守備範囲と IR の取り出し元が食い違い、どちらに合わせるか読めない

### S2: パスの解決

- Purpose: コマンド自身が値の確定するパスを解決し、glob の base を決め、未解決を区別する
- Specification: docs/ir/judgment.md#REQ-002
- Prerequisites: S1
- May change: crates/guardian-core/**
- Done when: 絶対・相対・~ の付いたパス・HOME と TMPDIR と PWD・リテラルの代入（&& の連結を含む）・リテラルの cd・mktemp が作ったパスが解決され、glob は最も外側のディレクトリが base になり、それ以外は未解決になることがテストで区別されている
- Shown by: test — REQ-002 の解決の種類ごとのテスト
- Left to the implementer: 解決の内部表現
- Stop and hand back if: 要件が定めない変数の種類を解決する必要が生じた

### S3: 一時領域と保護領域の分類と判定

- Purpose: 設定なしの既定で、一時領域と保護領域と unknown を分類し、allow / ask / block を返す
- Specification: docs/ir/judgment.md#REQ-003, docs/ir/judgment.md#REQ-005, docs/ir/judgment.md#REQ-006
- Prerequisites: S2
- May change: crates/guardian-core/**, crates/guardian-judge/**
- Done when: /tmp 配下は allow。"/" と一時領域のルートとホームディレクトリと作業ディレクトリとリポジトリのルートそれ自体は block。システムの領域とほかの利用者のホームと .git は、それ自体と配下が block。未解決は block。どれでもないものは ask。glob の base が保護領域に一致するときは保護と同じ判定。許可ルートの配下は allow でルートそれ自体は含まれないことがテストで確かめられている
- Shown by: test — EX-001, EX-003, EX-004, EX-024, EX-029
- Left to the implementer: 分類の内部表現
- Stop and hand back if: 分類の優先順が要件から読めない（保護ルートの配下と一時領域が重なる場合など）

### S4: git による分類

- Purpose: 作業ツリーの中のパスを git status の報告の有無で vcs と unknown に分け、git の起動を必要なときだけにする
- Specification: docs/ir/judgment.md#REQ-004, docs/ir/cli.md#REQ-020
- Prerequisites: S3
- May change: crates/guardian-judge/**
- Done when: 無視された生成物と未変更の追跡済みは allow、未追跡のファイルと配下に変更があるディレクトリは ask（理由に未追跡や未コミットの変更が出る）になり、作業ツリーの外のパスでは git が起動されず、git の失敗は ask に落ちることがテストで確かめられている
- Shown by: test — EX-002, EX-005, EX-019, EX-028
- Left to the implementer: git の呼び出し方と引数
- Stop and hand back if: git の無い環境を前提にできない、判定が 100ms の目標を満たせない

### S5: 対象集合が未知の効果

- Purpose: find、xargs、ループの削除を供給元の子の分類で判定する
- Specification: docs/ir/judgment.md#REQ-008
- Prerequisites: S4
- May change: crates/guardian-core/**, crates/guardian-judge/**
- Done when: 無視されたディレクトリへの find の削除は allow、未コミットの変更がある作業ツリーへの find と xargs の削除は ask、for ループの供給元がリテラルの一覧ならその分類で判定され、供給元が確定できないときは ask、システムの領域と .git の配下への find の削除は block になることがテストで確かめられている
- Shown by: test — EX-006, EX-007 と、ループと xargs のテスト
- Left to the implementer: 供給元の読み取り方
- Stop and hand back if: 供給元の種類が増えて「子の分類」では決められなくなった

### S6: symlink・合成・内部エラー

- Purpose: symlink の意味論、複数の効果の合成、内部エラーの扱いを実装する
- Specification: docs/ir/judgment.md#REQ-007, docs/ir/judgment.md#REQ-009, docs/ir/judgment.md#REQ-010
- Prerequisites: S5
- May change: crates/guardian-core/**, crates/guardian-judge/**
- Done when: リンクはリンクそれ自体の分類で判定され末尾スラッシュ付きだけリンク先を見ること、ハードリンクは 1 つのリンクを消すだけとして扱われること、複数の効果は最も重い判定になること、内部エラーと git の失敗が ask に落ちて block しないことがテストで確かめられている
- Shown by: test — EX-008, EX-009, EX-011
- Left to the implementer: 内部エラーの型
- Stop and hand back if: 内部エラーの扱いが要件と食い違う

### S7: 非 allow の文面

- Purpose: ask と block の文面を「何を・なぜ・代替」で組み立てる
- Specification: docs/ir/messages.md#REQ-011
- Prerequisites: S6
- May change: crates/guardian-policy/**
- Done when: 未追跡・未コミットの変更・未解決・管理外・保護領域それぞれの理由と、当てはまる代替（無いときはその旨）が 2 行から 4 行で出ることがテストで確かめられている
- Shown by: test — EX-012、理由の種類ごとのテスト
- Left to the implementer: 文面の組み立ての内部構造
- Stop and hand back if: 要件に無い種類の理由が必要になった

### S8: 設定のファイルと層・信頼・壊れたとき

- Purpose: TOML の 3 層の設定を読み、プロジェクト設定は締める方向だけ反映し、壊れていたら既定で続ける
- Specification: docs/ir/config.md#REQ-013, docs/ir/config.md#REQ-014, docs/ir/config.md#REQ-015, docs/ir/judgment.md#REQ-005, docs/ir/judgment.md#REQ-006
- Prerequisites: S3
- May change: crates/guardian-policy/**
- Done when: 利用者設定とプロジェクト設定が読まれ、リストは足し合わされ、スカラーは後のものが勝ち、未信頼のプロジェクト設定では保護ルートの追加と unknown の block への変更と ask/block のカスタムパターンの追加が反映され、許可ルートの追加とルールの無効化と緩める変更は無視されて警告が出て、自己信頼は効かず利用者設定の trusted_projects にあるときは緩める変更も反映され、壊れた TOML では組み込みの既定で判定が続き、設定で追加した保護ルートとその配下が block になり許可ルートの配下が allow でルートそれ自体は含まれず、git.enabled が false のときは git を起動せず作業ツリーの中のパスも vcs にしないことがテストで確かめられている
- Shown by: test — EX-014, EX-015, EX-016, EX-030 と、反映される変更・trusted_projects・設定したルート・git.enabled のテスト
- Left to the implementer: キーの内部表現、警告の文言
- Stop and hand back if: 要件に無いキーが必要になった、マージの規則が要件から読めない

### S9: ルールの意味論

- Purpose: 効果の取り出しの無効化と、正規表現のカスタムルールを実装する
- Specification: docs/ir/config.md#REQ-026
- Prerequisites: S8
- May change: crates/guardian-policy/**
- Done when: rules.disable の名前で無効にした効果は取り出されず、カスタムルールの一致が合成に加わり、allow を返すカスタムルールは利用者設定でのみ有効になることがテストで確かめられている
- Shown by: test — EX-035, EX-036
- Left to the implementer: パターンの内部表現
- Stop and hand back if: 正規表現の扱いが要件から読めない

### S10: 見張りの規則の形と語の照合

- Purpose: commands.guard の規則を読み、形を検証し、語と正規表現の照合を作る
- Specification: docs/ir/guards.md#REQ-027, docs/ir/guards.md#REQ-028
- Prerequisites: S8
- May change: crates/guardian-policy/**
- Done when: 形の誤りと空の reason と照合を 1 つも持たない規則がその規則だけ無効になり警告が出ること、語全体と /regex/ の照合、パス付きの起動とラッパー越しの起動への一致がテストで確かめられている
- Shown by: test — EX-040, EX-044
- Left to the implementer: 規則の内部表現、正規表現のコンパイル
- Stop and hand back if: 規則の形の検証が要件の列挙と合わない

### S11: 使い方の先頭一致

- Purpose: deny と for の語の並びの先頭一致を、オプションの読み飛ばし付きで作る
- Specification: docs/ir/guards.md#REQ-029
- Prerequisites: S10
- May change: crates/guardian-policy/**
- Done when: options-with-value の読み飛ばし・= を含む語・-- の扱いを経た先頭の語の並びで一致し、引数の中の語には当たらないことがテストで確かめられている
- Shown by: test — EX-038
- Left to the implementer: 読み飛ばしの実装
- Stop and hand back if: 読み飛ばしの規則が要件の列挙と合わない

### S12: フラグ・オプションの値・環境変数

- Purpose: deny-flags、deny-option-values、deny-env の照合と for による絞り込みを作る
- Specification: docs/ir/guards.md#REQ-030, docs/ir/guards.md#REQ-031
- Prerequisites: S11
- May change: crates/guardian-policy/**
- Done when: まとめ書きのフラグ・= 付きのフラグ・-- より前という範囲、オプションの値の照合、コマンド本文の NAME=value とラッパー越しの代入の照合、for に当たる起動だけへの適用がテストで確かめられ、フック自身の環境を見ないことが確かめられている
- Shown by: test — EX-039, EX-045
- Left to the implementer: フラグ解析の実装
- Stop and hand back if: deny-env の対象をコマンド本文に限る形で要件が表せない

### S13: only・合成・例の検証

- Purpose: only による通過の規則、見張りの verdict の合成、examples の読み込み時の検証を作る
- Specification: docs/ir/guards.md#REQ-032, docs/ir/guards.md#REQ-033, docs/ir/guards.md#REQ-034
- Prerequisites: S12
- May change: crates/guardian-policy/**
- Done when: only に当たらない起動がその規則の verdict になり、deny 系に当たった起動には only を当てず、only を持つ規則が複数あるときは規則ごとに当ててすべての only に当たる起動だけが一致せず、複数の規則の verdict が最悪値で合成され、合わない例と壊れた正規表現の規則だけが無効になって警告が出て、プロジェクト設定の規則も反映されることがテストで確かめられている
- Shown by: test — EX-037, EX-041, EX-042, EX-043
- Left to the implementer: 例の分割の実装
- Stop and hand back if: 例の分割がシェルの引用の規則と合わない、合成の順が要件から読めない

### S14: check の口

- Purpose: バイナリと check コマンドを作り、判定と JSON と終了コードを返す
- Specification: docs/ir/cli.md#REQ-017
- Prerequisites: S13
- May change: src/**
- Done when: check が判定を標準出力に出し、--format json で verdict と効果ごとの op・path・class・verdict・reason を出し、終了コードが allow は 0、ask は 1、block は 2、判定を出せない失敗は 3 になることがテストで確かめられている
- Shown by: test — EX-017, EX-031, EX-032, EX-033
- Left to the implementer: 引数解析の実装、JSON の桁と順序
- Stop and hand back if: 終了コードの割り当てが要件から読めない

### S15: フックの口

- Purpose: hook コマンドを作り、Claude Code と Codex の入出力の写像をする
- Specification: docs/ir/cli.md#REQ-016, docs/ir/agents.md#REQ-022, docs/ir/agents.md#REQ-023, docs/ir/agents.md#REQ-024
- Prerequisites: S14
- May change: src/**
- Done when: --agent claude で allow は何も返さず、ask は hookSpecificOutput の封筒に hookEventName を PreToolUse として permissionDecision に ask と理由を返し、block は deny と理由を返し、permission_mode が dontAsk か bypassPermissions なら ask を何も返さないこと、--agent codex で block だけ deny と理由を返しそれ以外は何も返さないこと、どちらも hookEventName は PreToolUse であること、Bash のコマンドを含まない入力では何もしないことがテストで確かめられている
- Shown by: test — EX-020, EX-021, EX-022, EX-025
- Left to the implementer: 入出力の JSON の組み立ての内部構造
- Stop and hand back if: 出力の封筒の形が調査済みのプロトコルと食い違う

### S16: 影実行とログ

- Purpose: 影実行でフックとして何も返さずログに残し、check は影実行でも判定を出す
- Specification: docs/ir/cli.md#REQ-018, docs/ir/cli.md#REQ-019
- Prerequisites: S15
- May change: src/**
- Done when: enforce が false のときフックが何も返さず、XDG_STATE_HOME の hook-guardian の下（無いときは ~/.local/state/hook-guardian）に所有者だけが読める権限で、時刻・判定・理由・対象パス・コマンド本文が 1 行ずつ書かれること、enforce が true のときはログを書かないこと、check は影実行でも判定を出すことがテストで確かめられている
- Shown by: test — EX-018, EX-026, EX-034
- Left to the implementer: ログのファイル名
- Stop and hand back if: ログの置き場が要件と実行環境で合わない

### S17: 文面と応答時間のレビュー

- Purpose: 文面の書き分けと応答時間の目標を、人が読んで確かめる
- Specification: docs/ir/messages.md#REQ-012, docs/ir/cli.md#REQ-021
- Prerequisites: S16
- May change: なし
- Done when: 代表的な 5 つの文面（拒否 2 件、ask 3 件）を読み、エージェントが次の一手を打てるか、利用者がその場で判断できるかを確かめた結果と、代表的な入力 100 件で判定の時間を計測して git の起動を含む場合でも 100ms 未満であることの結果が報告されている
- Shown by: external — 人が実際の文面を読み、代表的な入力を並べて計測し、100ms 未満と判断の材料の十分さを確かめる
- Left to the implementer: 計測の入力の選び方
- Stop and hand back if: レビューの結果が要件と合わない、100ms を満たせない

### S18: 全体の確認

- Purpose: 対象の全項目にテストの印があることと、検査とテストが通ることを示す
- Specification: docs/ir/judgment.md#REQ-001, docs/ir/judgment.md#REQ-002, docs/ir/judgment.md#REQ-003, docs/ir/judgment.md#REQ-004, docs/ir/judgment.md#REQ-005, docs/ir/judgment.md#REQ-006, docs/ir/judgment.md#REQ-007, docs/ir/judgment.md#REQ-008, docs/ir/judgment.md#REQ-009, docs/ir/judgment.md#REQ-010, docs/ir/messages.md#REQ-011, docs/ir/messages.md#REQ-012, docs/ir/config.md#REQ-013, docs/ir/config.md#REQ-014, docs/ir/config.md#REQ-015, docs/ir/config.md#REQ-026, docs/ir/cli.md#REQ-016, docs/ir/cli.md#REQ-017, docs/ir/cli.md#REQ-018, docs/ir/cli.md#REQ-019, docs/ir/cli.md#REQ-020, docs/ir/cli.md#REQ-021, docs/ir/agents.md#REQ-022, docs/ir/agents.md#REQ-023, docs/ir/agents.md#REQ-024, docs/ir/release.md#REQ-025, docs/ir/guards.md#REQ-027, docs/ir/guards.md#REQ-028, docs/ir/guards.md#REQ-029, docs/ir/guards.md#REQ-030, docs/ir/guards.md#REQ-031, docs/ir/guards.md#REQ-032, docs/ir/guards.md#REQ-033, docs/ir/guards.md#REQ-034
- Prerequisites: S17
- May change: なし
- Done when: review 以外の要件に印付きのテストがあり、review だけに属する例（EX-013、EX-023、EX-027）以外の例にも印付きのテストがあり、印の無いテストが無く、kotowari check がこのブランチで変えたファイルとこの計画の ID に error を出さず、fmt と clippy とテストが通る
- Shown by: check — `kotowari list | jq -r '.items[] | select(.kind == "requirement" and .verification != "review" and (.deferred | not) and .tests == []) | .id'` の出力が空であること、`kotowari list | jq -r '.items[] | select(.kind == "scenario" and .tests == []) | .id' | sort | tr '\n' ' '` の出力が EX-013 EX-023 EX-027 であること、`kotowari check --format json`、`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`
- Left to the implementer: none
- Stop and hand back if: この計画の対象外のファイルに対する kotowari check の error が新たに出た

### S19: 配布

- Purpose: Linux x86_64 向けの単一バイナリを GitHub Releases に置き、mise で導入できるようにする
- Specification: docs/ir/release.md#REQ-025
- Prerequisites: S18、GitHub のリポジトリが作られ remote が設定されている、CI の土台（.github/workflows）が置ける
- May change: .github/workflows/**, Cargo.toml（リリース用の設定だけ）
- Done when: リリースのページに Linux x86_64 向けの実行ファイルがあり、`mise use -g github:ba0918/hook-guardian` で導入でき、導入した `hook-guardian check` が動き、CI がビルドとテストを回している
- Shown by: external — 公開は人が行う（ドッグフーディングの後。A6）。人はリリースのページと、mise で導入した実行ファイルで `hook-guardian check` が動くことを確かめる
- Left to the implementer: ワークフローの分け方（テストとリリースを同じファイルにするか分けるか）、アセットの名前
- Stop and hand back if: remote が無い、ドッグフーディングが済んでいない、リリースの形式が mise の GitHub バックエンドと合わない
