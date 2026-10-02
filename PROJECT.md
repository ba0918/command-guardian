# Project Context

## What this is

command-guardian は Bash コマンドの破壊的な効果を取り出し、対象パスと設定から allow、ask、block を返す実行前の見張りである。
CLI の check とエージェント用の hook を、同じ実行ファイルで提供する。
実行するコマンドを遮断する権限 sandbox ではない。

## Stack and layout

Rust 2021 の Cargo workspace。
現在の実行基盤は Unix のソケットを使い、プロセス隔離の試験は Linux で行う。
仕様は `docs/ir/`、決定は `docs/decision/records/` に置く。

| クレート | 責務 | 通常の内部依存 |
|---|---|---|
| guardian-core | 共通の値と診断 | なし |
| guardian-parser | brush-parser の隠蔽、正規化 AST、純粋な構文解析と引用除去 | core |
| guardian-analysis | Context を保持する意味解析、効果と Invocation の共有走査 | core、parser |
| guardian-judge | パスの観測、分類、期限付き git 実行 | core |
| guardian-policy | 設定の解釈とマージ、規則、最悪値の合成、文面 | core |
| guardian-app | 設定の探索と読込、worker と session、判定の組立て | 上記5クレート |
| command-guardian | CLI、hook の写像、環境取得、影ログ | app、core、policy |

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/kotowari-check.sh
cargo build --release --locked
```

実バイナリの外部契約を絞って確認する場合:

```sh
cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation
```

通常の利用例:

```sh
cargo run --locked -- check 'rm -rf /etc/x' --cwd /tmp --format json
```

## Conventions specific to this project

### 変更とIRの照合

kotowari 0.3.0以降の、"changes" コマンドを持つ版を使う。
これはIRを自動生成する機能ではなく、Gitの変更と判断記録の対応・鮮度を検査する。
今回の導入より前の変更を、自動で仕様適合とみなすものではない。

1. 呼出元がブランチ全体の比較元と候補先をGitから確定し、完全なコミットIDを記録する。記録ファイルの自己申告から比較元を選ばない。
2. 実装者が ".kotowari/changes/implementation.yaml" を作る。既存要求内なら要求と関連IR、仕様の穴を埋めるなら根拠・判断者・決定記録と必要なIRを残す。実装をそのまま正しい仕様として取り込まない。
3. 実装と別のコンテキストのレビュー担当が根拠・意味・承認範囲を確認し、同じ変更と実装者が挙げた全IRを含む ".kotowari/changes/review.yaml" を自分で作る。役割ラベルだけを変えてコピーしない。
4. 両記録をコミットし、呼出元が最終HEADを再確定する。下記の両検査と既存の製品チェックが成功した場合だけ統合する。未処理の仕様判断をdeferredにしたまま統合しない。

```sh
BASE=<呼出元がGitから確定したブランチ全体の比較元の完全なID>
HEAD_SHA=$(git rev-parse HEAD)
kotowari check --format json
kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format json
```

コード・IR・決定の意味の変更、rebase、cherry-pick、並行統合で最終記録が無効になったら、両記録を除いて該当entry全体を照合し直し、独立レビュー後に作り直す。
記録は上記2ファイルに現在の比較だけを保持し、過去分はGit履歴で読む。
中間コミットとpre-commitフックには "changes" を要求しない。
任意の実装者自己検査には "kotowari changes --base HEAD --staged --phase implementation" を使えるが、独立レビューの代わりにはならない。
"status" のcompleteだけでは変更照合の完了とは扱わない。
CIはmainへのpushと全PRで実行する。PRはイベントbaseと実headのmerge-baseを使い、合成mergeコミットを検査しない。pushはイベントbefore/afterを使い、ゼロbeforeと履歴不足は失敗させる。

### CIとpush前の検査

CIとlefthookのpre-pushでfmt、gnu/muslのclippy・全テスト・release build、kotowari check/changes reviewを実行する。
ローカルの前提はrustfmt、clippy、両Rustターゲット、musl-tools、kotowari 0.3.0以降。クローンごとに "lefthook install" でフックを有効にする。
push前に "git fetch origin main" を行う。作業ブランチはorigin/mainとのmerge-base、mainは送出先の旧SHAで照合する。比較元不明、未コミット変更、HEAD以外の送出は停止する。
CIとフックは開発運用の設定であり、製品IRへ要求を追加しない。

### リリース

版の正本は "Cargo.toml" の "[package].version"。
CHANGELOGを "## [VERSION] - YYYY-MM-DD" に昇格し、対応する比較リンクを添えてmainへ統合すると、全検査成功後に未公開のvVERSIONを自動公開する。Unreleasedの間は公開しない。
GitHub ReleasesにはLinux x86_64 muslバイナリとLICENSEのtar.gz、SHA256を置く。タグは検査したcommitへ付け、公開済みタグは移動・再利用しない。

### 製品の実装と試験

- 本番の入力由来の解析は app の session を通す。main の先頭で同一バイナリの子を dispatch する。親での直接解析 fallback は置かない。
- 純粋な parser と analysis の試験には浅い入力を使う。子の死亡と深い入力は root の実バイナリ試験へ置く。libtest の main を worker として再起動しない。
- git fixture は `CARGO_TARGET_TMPDIR` に置く。target を `/tmp` へ移すと一時領域の分類に変わるため、検証時に移さない。
- CLI と hook の試験では fixture の HOME と XDG を使い、実利用者の設定と影ログへ書き込まない。

## Constraints

単一バイナリ、CLI の終了コード、設定の三層と信頼条件、JSON、hook、影ログの外部契約を維持する。
構文のサイズは1 MiB、深さは128段、一判定の構文解析は1000回、判定時間は5秒が上限である。
設定例の検証には一要求の隔離上限を適用し、設定全体への累積1000回/5秒の受理上限は設けない。
git の待ちと出力の取得を判定の残時間で切るが、全 fs syscall を5秒で強制中断する保証はない。
代表的な浅い入力の応答目標は100ms未満で、病的な深い入力とは別に計測する。

## Glossary

- **JudgmentSession**：一判定の間だけ runtime を借用し、解析回数と経過時間を管理する値。
- **ValidationSession**：設定の読込中だけ例を隔離して解析する値。判定の累積予算は持たない。
- **CommandFacts**：同じ共有走査で得た効果、見かけの起動情報、診断。
- **ObservedPath**：一度解決したパス。設定ルートと既定分類で共有し、表示用の元 Target とは分ける。
