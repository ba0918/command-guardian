# command-guardian

シェルコマンドを実行する前に、削除などの破壊的な操作を調べます。command-guardianはRust製のCLIです。コマンド、対象パス、設定に基づいて`allow`、`ask`、`block`を返します。同じ実行ファイルで、エージェントのフックからの要求も処理します。

渡されたコマンドを実行することはありません。対象を分類するために、設定の読み込み、ファイルシステム上のパスの確認、Gitへの問い合わせを行います。

事故を減らすための道具であり、セキュリティサンドボックスではありません。`allow`でも、コマンドが安全だと証明されたわけではありません。実行するかどうかは呼び出し側で判断してください。エージェントと連携するには、ホスト側にフックを登録し、その判定が反映される必要があります。

## インストール

[mise](https://mise.jdx.dev/)経由のインストールを推奨します。

```sh
mise use -g github:ba0918/command-guardian
command-guardian --help
```

GitHub ReleasesからLinux x86_64用の実行ファイルをインストールします。
Unreleasedに記載した変更は公開済みリリースには含まれません。
利用するには、下の手順でソースからビルドしてください。

### ソースからのビルド

Linuxと、Cargoを含む新しい安定版のRustツールチェーンを使ってください。実行基盤にはUnixソケットを使い、プロセス隔離のテストはLinuxで行っています。Windowsネイティブ版とmacOS版は提供していません。

```sh
git clone https://github.com/ba0918/command-guardian.git
cd command-guardian
cargo build --release --locked
cargo install --path . --locked
command-guardian --help
```

`cargo install`はCargoのbinディレクトリに実行ファイルを配置します。そのディレクトリを`PATH`に含めてください。インストールせずに`./target/release/command-guardian`を実行することもできます。

## コマンドの判定

コマンド全体を引用符で囲み、1つの引数として渡してください。シングルクォートで囲むと、判定前に呼び出し元のシェルが変数を展開したり、コマンド置換を実行したりするのを防げます。

```sh
command-guardian check 'rm -rf /etc/nginx' --cwd /tmp
command-guardian check 'rm -rf /etc/nginx' --cwd /tmp --format json
command-guardian check 'S=/etc/nginx; rm -rf "$S"'
```

これらは解析する文字列を渡す例です。`rm`を実行することはありません。

`--cwd`の既定値は現在の作業ディレクトリです。`--format`には`text`か`json`を指定でき、既定値は`text`です。オプションとして解釈される文字列を判定するときは、その前に`--`を置いてください。

```sh
command-guardian check -- '--help'
command-guardian check --help
```

| 終了コード | 意味 |
|---|---|
| `0` | `allow`、またはヘルプの正常な表示 |
| `1` | `ask`。実行前にコマンドを確認してください |
| `2` | `block` |
| `3` | 判定を出せない失敗。CLI引数の誤りや出力の失敗など |

テキスト出力の先頭には判定を表示します。JSONには`verdict`、`effects`、`rules`、`message`、`reason`を含めます。設定の警告は標準エラーに出力します。

コマンド本文と、解釈するオプション名・format値はUTF-8である必要があります。Unixでは、`--cwd`に非UTF-8のバイトを含むOSパスを渡せます。ただし、JSONやテキストのパス表示から元のバイト列を完全には復元できません。

### 対象パスと判定の関係

既定の判定は次のとおりです。

| 対象の分類 | 判定 |
|---|---|
| 一時ディレクトリや、設定で許可したディレクトリの配下 | `allow` |
| Gitの作業ツリー内で、`git status --porcelain -uall`が変更を報告しないパス | `allow` |
| 保護対象のパス | `block` |
| パスは解決できたが、分類できない対象 | `ask` |
| 明記されたパスの値を解決できない対象 | `block` |

一時ディレクトリには`/tmp`、`/var/tmp`、`TMPDIR`を含めます。そのディレクトリ自体は、一時領域の規則では許可しません。保護対象には、システムディレクトリ、`.git`、設定で追加した保護ルートを含めます。現在の作業ツリー、作業ディレクトリ、ホームディレクトリのルートも保護します。

`find`や`xargs`による削除など、対象の集合が不明な場合は、供給元のディレクトリを特定できればそれを使って判定します。供給元も不明なら、既定では`ask`を返します。globはファイルシステム上で展開せず、基点となるディレクトリで分類します。`find`の`-name`などの絞り込みは、分類の範囲を狭めるためには使いません。

最終的には、最も厳しい判定を返します。`block`、`ask`、`allow`の順に優先します。正確な規則は[パスと判定の仕様](docs/ir/judgment.md)を参照してください。

## 設定

次の3層を順に読み込みます。

1. 組み込みの既定値。
2. `$XDG_CONFIG_HOME/command-guardian/config.toml`。`XDG_CONFIG_HOME`がなければ`$HOME/.config/command-guardian/config.toml`。
3. 作業ディレクトリから親へ探し、最初に見つかった`.command-guardian.toml`。

リストは追加し、単一の値は後の層で上書きします。利用者設定の相対パスは、設定ファイルのディレクトリを基準に解決します。プロジェクト設定ではGitの作業ツリーのルートを基準にします。作業ツリーがなければ、設定ファイルのディレクトリを基準にします。

次のプロジェクト設定では、データを保護し、push前の確認を求めます。

```toml
[paths]
protected_roots = ["data"]

[[commands.guard]]
program = "git"
reason = "Review the destination and commits before pushing."
verdict = "ask"
deny = [["push"]]

[commands.guard.examples]
deny = ["git push origin main"]
allow = ["git status"]
```

設定を読み込むときに、guard規則の例も検査します。
個別規則の形式が誤っている場合や、例が検査に通らない場合は、その規則を無効にして警告します。
`commands`節やguard一覧自体の型が不正な場合は、一般設定の誤りと同様に、そのファイル全体を不採用にして警告します。
他の正常な設定層は引き続き適用します。

プロジェクト設定は、既定では信頼しません。保護ルートや厳しい規則の追加は反映しますが、許可ルートの追加、組み込み規則の無効化、保護を緩める変更は反映しません。緩める変更も許可する場合は、利用者設定の`trusted_projects`にプロジェクトのパスを追加してください。設定を信頼できるプロジェクトだけを指定してください。カスタム規則の`allow`は、利用者設定からだけ受け付けます。

| 設定 | 既定値 | 用途 |
|---|---|---|
| `paths.allowed_roots` | 一時ディレクトリ | 保護対象の規則に従ったうえで、指定したルートの配下への操作を許可する |
| `paths.protected_roots` | 空 | 保護するディレクトリとその配下を追加する |
| `unknown.verdict` | `"ask"` | 分類できない対象を`ask`か`block`にする |
| `rules.disable` | 空 | `delete`、`truncate`、`format`の効果抽出を無効にする |
| `rules.custom` | 空 | 名前付きの正規表現規則でコマンド本文を照合する |
| `commands.guard` | 空 | プログラムの引数、フラグ、オプション値、環境変数への代入を照合する |
| `git.enabled` | `true` | Gitによる分類を有効にする |
| `mode.enforce` | `true` | ログだけを残すのではなく、フックの判定を返す |
| `trusted_projects` | 空 | 指定したプロジェクトで、通常は制限する設定も許可する |
| `advisor.mode` | `"off"` | 0.2.0から利用可能。助言の無効化、観測、厳しい判定の適用を選ぶ。利用者設定のみ |

効果を無効にすると、その効果は抽出しなくなります。警告の表示だけを隠す設定ではありません。詳しい契約は[設定](docs/ir/config.md)と[コマンドの見張り](docs/ir/guards.md)を参照してください。

## 任意のLLM助言

LLM助言はguardian 0.2.0から利用できます。
既定では無効です。
まず機械判定を変えずに助言を記録する`observe`で確認し、結果と外部送信の範囲を確認してから、明示的に`enforce`へ切り替えてください。

`$XDG_CONFIG_HOME/command-guardian/config.toml`に次を追加します。
XDGが未設定なら、`$HOME/.config/command-guardian/config.toml`を使います。

```toml
[advisor]
mode = "observe"
model = "jev-latest"
timeout_ms = 2000
max_request_bytes = 65536
intervention_threshold = 0.9
context_exchanges = 3
context_ttl_hours = 24
debug_text = false
```

`mode`以外は既定値です。
`mode`の既定値は`"off"`です。
`max_request_bytes`は、符号化した送信要求の上限です。
`context_exchanges = 0`では会話文脈を使いませんが、助言自体は無効になりません。
`context_ttl_hours`はキャッシュの期限、`debug_text`は要求本文のログを制御します。
助言は利用者設定からだけ設定できます。
プロジェクトの`[advisor]`節は、信頼済みプロジェクトでも警告して無視します。
利用者の助言設定が不正な場合は、利用者設定ファイル全体を不採用にしますが、他の正常な設定層は維持します。
認証には、guardianプロセスの環境変数`TYPESAFE_API_KEY`を使います。
認証値をTOMLやシェル履歴に残るコマンドへ書かないでください。

有効にすると、TypeSafe接続が判定ごとに最大1回、`https://api.typesafe.ai/v1/systemone`へHTTPS要求を送ります。
再試行はしません。
送る内容はコマンド、作業ディレクトリ、機械判定、取得できた範囲の会話文脈です。
文脈は既定で直近3往復までです。
アシスタントの返答は参照先の理解に使いますが、利用者の承認の根拠にはしません。
ツール結果、ファイル本文、会話全体、環境変数一覧は送りません。
秘密らしい値の検出、符号化の失敗、送信量の上限超過では、入力を切り詰めず送信を見送ります。
秘密の検出は完全ではありません。
外部事業者へ送れない入力を扱う場合は、有効にしないでください。

`enforce`は機械判定の`allow`と`ask`を厳しくできますが、`block`の解除や`ask`から`allow`への緩和はしません。
独立した有害な不可逆効果の確率が閾値以上なら`block`にします。
重大破壊の確率が閾値以上の場合は、確認済み指示が対象とすべての効果を含み、指示一致の確率も閾値以上の場合だけ`ask`とします。
それ以外は`block`にします。
文脈がないだけではモデル評価を見送りません。
タイムアウト、不正応答、通信失敗、判断不能では機械判定を維持し、新しい確認を増やしません。

`timeout_ms`は文脈取得、子の起動、検査、通信を含む助言の期限です。
機械判定の5秒とは別枠で、親プロセスが期限を強制します。
`intervention_threshold`はモデルが返す確率の閾値であり、正答率の保証ではありません。
`jev-latest`のモデルは提供側で変わる場合があります。
実モデルの精度と、現在のTypeSafeサービスとの適合は未検証です。

助言ログは`$XDG_STATE_HOME/command-guardian/advisor.jsonl`へ保存します。
XDGが未設定なら、`$HOME/.local/state/command-guardian/advisor.jsonl`を使います。
既定では判定、モデル、経過時間、失敗や見送りの分類などのメタデータだけを記録し、コマンドや会話の本文は残しません。
`debug_text = true`では、検出した秘密を伏せて要求本文も記録します。
本文入りのログは機密情報として扱ってください。
別フックでの取得が必要な経路では、限定した文脈を所有者だけがアクセスできるキャッシュに保存します。
既定の期限は24時間で、期限切れの項目は使わず、アクセス時に削除します。
ログとキャッシュは別なので、本文ログを無効にしても文脈キャッシュは無効になりません。
既存の影ログには、引き続きコマンド本文と対象パスを含めます。
`advisor.mode = "observe"`と`[mode] enforce = false`は別の設定です。

対応する文脈取得経路と検証の限界は、[助言の要求](docs/ir/advisor/policy.md)、[文脈取得の検証記録](docs/verification/llm-advisor-context.md)、[検証結果](docs/verification/llm-advisor.md)を参照してください。

## エージェントのフック

OpenCodeとの連携は、下の[OpenCode V2の導入手順](#opencode-v2)を参照してください。
ここからのフック設定はClaudeとCodex向けです。

実行前のフックから呼び出し、標準入力にJSON要求を1つ送ってください。

```sh
printf '%s\n' '{"tool_name":"Bash","tool_input":{"command":"rm -rf /etc/nginx"},"cwd":"/tmp"}' |
  command-guardian hook --agent claude
```

Codex用の出力には`--agent codex`を使います。現在は、どちらのモードも上の要求形式を受け付けます。判定するのは、`tool_name: "Bash"`で、`tool_input.command`が空でない要求だけです。`cwd`を省略すると、現在の作業ディレクトリを使います。

利用するエージェントが対応する方法で、フック設定にコマンドを登録してください。この実行ファイルは、フックの登録や、他のツール要求形式からの変換は行いません。連携を使う前に、ホストがこの形式の要求を送り、`hookSpecificOutput`を反映することを確認してください。

| 判定 | Claudeモード | Codexモード |
|---|---|---|
| `allow` | 出力なし | 出力なし |
| `ask` | `permissionDecision: "ask"` | 出力なし。確認はホストの承認フローに委ねる |
| `block` | `permissionDecision: "deny"` | `permissionDecision: "deny"` |

判定は`hookSpecificOutput`の中に、`hookEventName: "PreToolUse"`と理由を含めて返します。Claudeモードでは、`permission_mode`が`dontAsk`か`bypassPermissions`なら`ask`の出力を抑制します。`block`は抑制しません。

`hook`サブコマンドの終了コードは常に`0`です。ホストは終了コードではなく、JSONの判定を読み取る必要があります。入力が不正な場合や、エージェントを認識できない場合は、判定を返しません。判定できないときに必ず実行を拒否する仕組みではありません。[エージェントとの入出力仕様](docs/ir/agents.md)も参照してください。

### OpenCode V2

このプラグインはguardian 0.1.2から利用できます。
miseで本体を入れ、OpenCode V2 2.0.21に同じリリースタグのプラグインを登録してください。

```sh
mise use -g github:ba0918/command-guardian@0.2.0
opencode plugin add 'github:ba0918/command-guardian#v0.2.0::path:plugins/opencode'
```

別の版を使う場合は、両方のコマンドの`0.2.0`をその版に置き換えてください。
同じ版のタグにあるプラグインのサブディレクトリを指定するため、別のnpmパッケージは不要です。
0.1.2では登録後、プラグインに同じOpenCodeサーバーへの認証付き接続を設定してください。
0.1.3からは、接続オプションを省略すると自身のローカルバックグラウンドサービスへ自動接続します。
明示起動したサーバーには引き続き接続設定が必要です。
本体の導入やパッケージの登録だけでは、設定は完了しません。

対応環境はLinux x86_64またはWSLです。
Bashを明示的に設定し、コマンド、作業ディレクトリ、シェルを変更する他のフックを使わないでください。
接続設定と、代替手順となるアーカイブからの導入方法は[英語の導入ガイド](docs/opencode.md)を参照してください。
隔離した導入試験では、公開のGitパッケージ導入機能を使っています。
ローカルGitの完全なコミットIDと同じサブディレクトリ指定で確認しています。
公開済みのGitHubタグからこのプラグインを取得する動作は、まだ確認していません。

### 影実行モード

フックの判定を返さず、結果だけを記録するには、利用者設定に次を追加してください。

```toml
[mode]
enforce = false
```

この設定はフックの判定に適用します。`check`の判定や終了コードは変えません。影実行モードでは、判定ごとに1件を`$XDG_STATE_HOME/command-guardian/shadow.log`へ記録します。`XDG_STATE_HOME`がなければ`$HOME/.local/state/command-guardian/shadow.log`を使います。記録にはコマンド本文、対象パス、判定、理由、時刻を含めます。ログは機密情報として扱ってください。

ログには所有者だけがアクセスできる権限を設定します。専用の`command-guardian`ディレクトリや`shadow.log`がシンボリックリンクの場合は拒否します。ログが通常ファイルでない場合も拒否します。保存に失敗すると標準エラーに警告し、フックの判定を出力せず、終了コード`0`で終わります。親ディレクトリ内のすべてのシンボリックリンクや、ハードリンクを拒否する保証はありません。

## 対応範囲と制約

パーサはBashとPOSIX shの構文を対象にします。認識したシェルの`-c`起動や`eval`では、内側のコマンド本文も実行せずに読みます。値が分かる代入やパスは解決します。状態の変化が不確定な場合は、分岐やパイプの実行モードを選んで決めつけず、未解決のまま扱います。

抽出する効果には、`rm`、`rmdir`、`unlink`、`find`、`xargs`、`shred`による削除を含めます。出力リダイレクト、`dd`、`truncate`による切り詰めも対象です。フォーマットは、`mkfs`、`wipefs`、ブロックデバイスへの`dd`の書き込みから抽出します。

現在の実装では、`git clean`、`mv`や`cp`による上書き、`sed -i`、`rsync --delete`の効果は抽出しません。guard規則で特定の使い方を検出できますが、これらのプログラムの効果を網羅的に解析する機能は追加しません。外部のスクリプトやプログラムが、渡されたコマンド本文に現れない操作を行う場合もあります。

通常の解析失敗、内部エラー、Gitの失敗では、`ask`になる場合があります。入力に起因するリソース上限の超過では、`block`になる場合があります。上限はコマンド本文1 MiB、ネスト深度128、1判定あたり1,000回の解析、判定時間5秒です。ただし、ファイルシステムのすべてのシステムコールを5秒以内に中断できる保証はありません。

アクセス制御の境界として使わないでください。悪意あるコマンドへの対策を、この判定だけに任せないでください。詳しくは[パーサの動作と隔離](docs/ir/parser.md)、[パイプの状態](docs/ir/shell-state.md)を参照してください。

## 開発

### リポジトリの構成

| パス | 役割 |
|---|---|
| `src/` | CLI、環境入力、フックのプロトコル、影実行のログ |
| `crates/guardian-core/` | 共通の値と診断 |
| `crates/guardian-advisor/` | 助言の型、分類、文脈の上限、秘密の検出 |
| `crates/guardian-advisor-typesafe/` | TypeSafeへの要求の符号化、認証、HTTPS通信 |
| `crates/guardian-parser/` | シェルの構文解析と正規化した構文木 |
| `crates/guardian-analysis/` | 効果、プログラムの起動、パスの状態の解析 |
| `crates/guardian-judge/` | ファイルシステムの観測とパスの分類 |
| `crates/guardian-policy/` | 設定、guard規則、判定の合成、メッセージ |
| `crates/guardian-app/` | エンジン、設定の読み込み、隔離ワーカー、助言の期限、ローカル状態 |
| `tests/` | CLI、フック、設定、隔離の統合テスト |
| `docs/ir/` | kotowariで検査する要求と例 |
| `docs/decision/records/` | 仕様を決めた理由と承認の記録 |

### 検証の実行

`changes`コマンドを持つ[kotowari](https://github.com/ba0918/kotowari)の0.3.0以降をインストールし、`PATH`から使えるようにしてください。そのうえで次を実行します。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/kotowari-check.sh
cargo build --release --locked
```

実行ファイルの外部契約に絞って確認する場合は、次を実行してください。

```sh
cargo test -p command-guardian --locked --test cli --test hook --test shadow --test isolation
```

テストでは`HOME`とXDGディレクトリを隔離し、個人の設定を読んだり、個人のログへ書き込んだりしないようにしています。Gitのテスト用データは`CARGO_TARGET_TMPDIR`の下に置いてください。`/tmp`へ移すと分類が変わり、確認したい動作を正しくテストできなくなる場合があります。

本番の解析は、アプリのruntime sessionを通じ、同じ実行ファイルの子プロセスで行います。純粋なパーサと解析のテストには、浅い入力を使ってください。深くネストした入力、ワーカーの失敗、プロセス隔離は、ルートの実行ファイルでテストしてください。Rustのテストランナーをワーカーとして再起動しないでください。

### 実装と仕様を合わせて変更する

プロジェクトの規約と統合手順は[PROJECT.md](PROJECT.md)を読んでください。要求は`docs/ir/`にあります。現在、仕様と決定履歴の多くは日本語で書かれています。

ブランチを統合する前に、比較元と最終候補のコミットを確定してください。実装者と独立したレビュー担当が、それぞれ自分の変更照合記録を`.kotowari/changes/`へ書きます。両方の記録で、同じ候補と関連する仕様ファイルを対象にしてください。記録をコミットした後、上の製品チェックと次の仕様チェックを実行します。

```sh
# BASEに、ブランチの比較元として選んだ完全なコミットIDを設定してください。
HEAD_SHA=$(git rev-parse HEAD)
kotowari check --format json
kotowari changes --base "$BASE" --head "$HEAD_SHA" --phase review --format json
```

`kotowari check`は仕様の構造と参照を検査します。`kotowari changes`は、記録した判断が変更全体を網羅し、現在の内容に対応しているかを検査します。どちらも、意味の判断が正しいことを証明するものではありません。独立レビューで実装と要求を照合する必要があります。未解決の仕様判断を残したまま統合しないでください。実装者の記録をコピーして、レビュー担当の記録にしないでください。

レビュー後にコードや仕様の意味を変えた場合は、影響する記録を見直し、再レビューしてください。`kotowari changes`をpre-commitの必須チェックにしないでください。詳しい手順は[PROJECT.md](PROJECT.md#conventions-specific-to-this-project)を参照してください。

## ライセンス

[MITライセンス](LICENSE)で提供します。著作権表示はCopyright (c) 2026 ba0918です。
