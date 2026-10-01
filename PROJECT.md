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
