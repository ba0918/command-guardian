# コマンドの契約

実行ファイル、"check" と "hook" の契約、ヘルプ、影実行、ログ、git の起動条件、応答時間を扱う。エージェントのプロトコルへの写像は agents.md、製品が生成する説明の言語は messages.md が扱う。

## Requirements

### REQ-016: 実行ファイルとコマンド

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A19, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35, docs/decision/records/2026-10-01-rename.md#A1, docs/decision/records/2026-10-02-opencode-v2-hook.md#A19
- verification: unit

M1 の成果物は、"command-guardian" という 1 つの実行ファイルであり、"hook" と "check" の 2 つのコマンドを持つ。"hook" は "--agent claude"、"--agent codex"、"--agent opencode" を引数に取る。OpenCode の連携は opencode.md が扱う。

### REQ-017: check の契約

- kind: event_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A19, docs/decision/records/2026-09-30-hook-guardian-scope.md#A27, docs/decision/records/2026-09-30-hook-guardian-scope.md#A34, docs/decision/records/2026-10-01-parser.md#A13, docs/decision/records/2026-10-02-ir-friction-contracts.md#A4
- verification: unit

"command-guardian check" が呼ばれたとき、command-guardian は、コマンド文字列と作業ディレクトリを受け取り、判定を標準出力に出す。既定は人が読む形式で、"--format json" のときは、"verdict" と "reason"（判定の理由）、効果ごとの "op"、"path"、"class"、"verdict"、"reason" を持つ JSON を出す。効果が無いときは、効果の一覧は空になる。終了コードは、"allow" が 0、"ask" が 1、"block" が 2、判定を出せない失敗が 3 にする。影実行のときも判定を出す。

コマンド本文と解釈するオプション名・format値はUTF-8に限定し、非UTF-8なら終了コード3で拒否する。cwdはUnixのOSパスとして受理し、非UTF-8でも引数の読取でpanicしない。

### REQ-018: 影実行

- kind: state_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-10-02-ir-friction-contracts.md#A2, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23, docs/decision/records/2026-10-02-opencode-v2-hook.md#A19, docs/decision/records/2026-10-03-llm-advice-layer.md#A45, docs/decision/records/2026-10-04-auto-allow-ask.md#A6
- verification: unit

"mode.enforce" が false のとき、command-guardian は、フックとして判定を返さず、判定と理由をログに書く。M1 では、影実行のときだけログに書く。

この保存条件は影ログを指す。
助言のメタデータログはadvisor/context.mdの別契約であり、影ログの本文方針を変更しない。
`ask委任` の記録は影実行時に限らず同じ保存先へ書き、defer-ask.md の REQ-062 に従う。

ログの保存場所がない、または保存できないときは、その記録を省略して標準エラーへ警告する。Claude Code と Codex の hook の標準出力は空で、終了コードは0にする。OpenCode の影実行の応答は REQ-048 が扱い、判定を返さず影実行と判定不能を区別する。

### REQ-019: 影実行のログ

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-10-02-ir-friction-contracts.md#A2
- verification: unit

影実行のログは、"XDG_STATE_HOME" が指すディレクトリの "command-guardian" の下（"XDG_STATE_HOME" が無いときは "~/.local/state/command-guardian"）に、所有者だけが読める権限で書く。1 行に、時刻、判定、理由、対象パス、コマンド本文を含める。

専用ディレクトリ"command-guardian"とログ末尾要素"shadow.log"がsymlinkの場合、およびログが通常ファイルでない場合は、保存先として拒否する。拒否・保存失敗時はREQ-018の警告と継続を使う。この拒否条件は、その親経路の全symlinkやhardlinkの拒否を保証しない。

### REQ-020: git の起動条件

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
- verification: unit

command-guardian は、git のコマンドを、対象のパスが git の作業ツリーの中にあるときだけ起動する。

### REQ-021: 応答時間

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A16, docs/decision/records/2026-10-03-llm-advice-layer.md#A14, docs/decision/records/2026-10-03-llm-advice-layer.md#A16, docs/decision/records/2026-10-03-llm-advice-layer.md#A58
- verification: review
- how_to_verify: 助言オフで代表的な入力100件の応答時間をgitの起動も含めて計測し、測定値と100ms未満の目標の達成または未達を報告する。100ms以上の測定を達成や必須保証へ書き換えていないことをレビューする。助言有効時はモデル待ちを含む別の計測として記録する

助言オフ時の判定の応答は、代表的な入力で100ms未満を目標とするが、必須の応答保証ではない。
測定値と目標の達成または未達を報告し、100ms以上の測定を達成とせず、目標の未達だけを理由とする実行の自動遮断を追加しない。
助言有効時の待ちは機械判定の5秒予算とadvisor/runtime.mdの設定可能な別枠を区別し、モデル待ちを含む応答時間として別に計測する。

### REQ-044: ヘルプの表示と判定対象の境界

- kind: event_driven
- source: docs/decision/records/2026-10-02-cli-language-help.md#A3, docs/decision/records/2026-10-02-cli-language-help.md#A4
- verification: unit

トップレベル、"check"、"hook" でヘルプのオプション "--help" または "-h" が指定されたとき、command-guardian は、その入口のヘルプを標準出力へ表示し、終了コード 0 で終わる。ヘルプ表示では、設定読込、コマンド判定、標準入力の読取を行わない。"check" では "--" をオプションと判定対象の区切りとし、それ以降に渡す "--help" や "-h" はヘルプのオプションではなくコマンド文字列として扱う。

### REQ-045: 入口に応じたヘルプの内容

- kind: ubiquitous
- source: docs/decision/records/2026-10-02-cli-language-help.md#A3, docs/decision/records/2026-10-02-cli-language-help.md#A6, docs/decision/records/2026-09-30-hook-guardian-scope.md#A19, docs/decision/records/2026-09-30-hook-guardian-scope.md#A27, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35, docs/decision/records/2026-10-02-opencode-v2-hook.md#A19, docs/decision/records/2026-10-04-version-and-followups.md#A3
- verification: review
- how_to_verify: トップレベル、check、hook のヘルプを読み、用途、呼出し方、引数、オプション、使用例、終了コードが英語で説明され、各入口の契約と一致することを確かめる。check の cwd 省略時と format の既定の説明は実装と照合し、hook の終了コードを check の判定別終了コードと混同していないことを確かめる

ヘルプは、各入口に応じた用途、呼出し方、引数、オプション、使用例、終了コードを英語で示す。トップレベルでは "check" と "hook" の入口と、版を表示する "--version" と "-V" を案内する。"check" では判定するコマンド文字列、作業ディレクトリを指定する "--cwd"（省略時は現在の作業ディレクトリ）、出力形式を指定する "--format text|json"（既定は "text"）、判定別の終了コード 0、1、2 と判定を出せない失敗の 3 を説明する。"hook" では標準入力でフックの入力を受けること、"--agent claude|codex|opencode"、プロトコル上の終了コードは常に 0 であることを説明する。各入口で "--help" と "-h" を案内する。

### REQ-071: 版の表示

- kind: event_driven
- source: docs/decision/records/2026-10-04-version-and-followups.md#A1, docs/decision/records/2026-10-04-version-and-followups.md#A2
- verification: unit

トップレベルで "--version" または "-V" が指定されたとき、command-guardian は、標準出力へ "command-guardian <版>" の1行を出し、終了コード 0 で終わる。版は Cargo.toml の "[package].version" と同じ値である。版の表示では、設定読込と標準入力の読取を行わない。"check" や "hook" の後ろの "--version" は版の表示として扱わない。

## Examples

```gherkin
@id=EX-017 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: check の終了コード
  When "command-guardian check 'rm -rf /tmp/scratch/x' --cwd /tmp/scratch" を実行する
  Then 判定は allow で、終了コードは 0 になる

@id=EX-018 @about=REQ-018 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A22,docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
Scenario: 影実行は止めずに記録する
  Given "mode.enforce" が false である
  When Claude CodeまたはCodexのhookで保護領域の削除を判定する
  Then 何も返さず、判定と理由がログに書かれる

@id=EX-019 @about=REQ-020 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A16
Scenario: 作業ツリーの外では git を起動しない
  Given 対象のパスが git の作業ツリーの外にある
  When 判定する
  Then git のコマンドは起動されない

@id=EX-025 @about=REQ-016 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A19,docs/decision/records/2026-09-30-hook-guardian-scope.md#A40
Scenario: 2 つのコマンドを受け付ける
  Given M1 の実行ファイルがある
  When "command-guardian check 'true' --cwd /tmp" を実行する
  And "command-guardian hook --agent claude" に "tool_name" が "Write" の入力を与える
  Then どちらもコマンドとして受け付けられる

@id=EX-026 @about=REQ-019 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A22
Scenario: ログの場所と権限
  Given 影実行で判定が記録される
  When ログのファイルを調べる
  Then 所有者だけが読める権限で、時刻、判定、理由、対象パス、コマンド本文が 1 行に入っている

@id=EX-027 @about=REQ-021 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A16,docs/decision/records/2026-10-03-llm-advice-layer.md#A58
Scenario: 応答時間の目標達成を測定値とともに報告する
  Given 助言はoffで代表的な入力100件がある
  When gitの起動を含む応答時間を計測し各測定値が100ms未満である
  Then 測定値と目標達成を報告しすべての応答の必須保証とは記さない

@id=EX-111 @about=REQ-021 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A16,docs/decision/records/2026-10-03-llm-advice-layer.md#A58
Scenario: 応答目標の未達を保証や達成へ書き換えない
  Given 助言はoffで代表的な入力100件の応答時間に120msの測定がある
  When 測定報告をレビューする
  Then 測定値と100ms未満の目標が未達であることを報告する
  And この測定を達成や必須保証とは記さず未達だけから実行を自動遮断しない

@id=EX-031 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A7,docs/decision/records/2026-09-30-hook-guardian-scope.md#A8,docs/decision/records/2026-09-30-hook-guardian-scope.md#A9,docs/decision/records/2026-09-30-hook-guardian-scope.md#A15,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: ask の終了コード
  Given "/home/you/work/repo" は一時領域ではない git の作業ツリーである
  And その作業ツリーの直下の "notes.txt" 自体が未追跡である
  When "command-guardian check 'rm notes.txt' --cwd /home/you/work/repo" を実行する
  Then 判定は ask で、終了コードは 1 になる

@id=EX-032 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: block の終了コード
  When "command-guardian check 'rm -rf /etc/nginx' --cwd /tmp/scratch" を実行する
  Then 判定は block で、終了コードは 2 になる

@id=EX-033 @about=REQ-017 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: 失敗の終了コード
  When "command-guardian check" をコマンド文字列なしで実行する
  Then 終了コードは 3 になる

@id=EX-034 @about=REQ-018 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A17,docs/decision/records/2026-09-30-hook-guardian-scope.md#A22,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: 影実行でも check は判定を出す
  Given "mode.enforce" が false である
  When "command-guardian check 'rm -rf /etc/nginx' --cwd /tmp/scratch" を実行する
  Then 判定は block として表示される

@id=EX-066 @about=REQ-044 @source=docs/decision/records/2026-10-02-cli-language-help.md#A3,docs/decision/records/2026-10-02-cli-language-help.md#A4
Scenario: すべての入口で長短のヘルプを表示する
  When "command-guardian --help" と "command-guardian -h" を実行する
  And "command-guardian check --help" と "command-guardian check -h" を実行する
  And "command-guardian hook --help" と "command-guardian hook -h" を実行する
  Then それぞれの入口のヘルプが標準出力に出る
  And すべて終了コード 0 で終わる

@id=EX-067 @about=REQ-044 @source=docs/decision/records/2026-10-02-cli-language-help.md#A4
Scenario: ヘルプは設定と標準入力に依存しない
  Given 利用者設定が壊れている
  And 標準入力は開いたままで、データをまだ渡していない
  When "command-guardian hook --help" を実行する
  Then 設定を読まず、設定の警告も出さない
  And 標準入力を読まず、コマンドを判定せずにヘルプを表示して終了コード 0 で終わる

@id=EX-068 @about=REQ-044 @source=docs/decision/records/2026-10-02-cli-language-help.md#A4
Scenario: 区切り以降のヘルプと同じ文字列は判定対象になる
  When "command-guardian check -- '--help'" を実行する
  And "command-guardian check -- '-h'" を実行する
  Then それぞれ "--help" と "-h" をコマンド文字列として判定する
  And ヘルプは表示しない

@id=EX-069 @about=REQ-045 @source=docs/decision/records/2026-10-02-cli-language-help.md#A3
Scenario: トップレベルから使い方を探せる
  When "command-guardian --help" を実行する
  Then 用途と "check" と "hook" の入口、ヘルプのオプション、使用例、終了コードの説明が英語で表示される

@id=EX-070 @about=REQ-045 @source=docs/decision/records/2026-10-02-cli-language-help.md#A3,docs/decision/records/2026-10-02-cli-language-help.md#A6,docs/decision/records/2026-09-30-hook-guardian-scope.md#A19,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: check の省略時の使い方と判定別終了コードを確認できる
  When "command-guardian check --help" を実行する
  Then コマンド文字列、"--cwd" と省略時の作業ディレクトリ、"--format text|json" と既定の "text"、ヘルプのオプション、使用例が英語で説明される
  And 終了コードは allow が 0、ask が 1、block が 2、判定を出せない失敗が 3 と説明される

@id=EX-071 @about=REQ-045 @source=docs/decision/records/2026-10-02-cli-language-help.md#A3,docs/decision/records/2026-10-02-cli-language-help.md#A6,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A35,docs/decision/records/2026-10-02-opencode-v2-hook.md#A19
Scenario: hook の終了コードを check と取り違えない
  When "command-guardian hook --help" を実行する
  Then 標準入力、"--agent claude|codex|opencode"、ヘルプのオプション、使用例が英語で説明される
  And 判定にかかわらず終了コードは 0 と説明される

@id=EX-076 @about=REQ-017 @source=docs/decision/records/2026-10-02-ir-friction-contracts.md#A4
Scenario: 非UTF-8のcwdを入力として受理する
  Given 存在する作業ディレクトリのOSパスに非UTF-8のバイトがある
  When そのcwdで"true"をcheckに渡す
  Then 判定はallowで終了コードは0になり、引数の読取でpanicしない

@id=EX-077 @about=REQ-017 @source=docs/decision/records/2026-10-02-ir-friction-contracts.md#A4
Scenario: 非UTF-8の本文とformatを拒否する
  When 本文またはformatに非UTF-8のバイトをcheckへ渡す
  Then 終了コードは3になる

@id=EX-154 @about=REQ-071 @source=docs/decision/records/2026-10-04-version-and-followups.md#A1
Scenario: 版を表示する
  When "command-guardian --version" と "command-guardian -V" を実行する
  Then どちらも標準出力に "command-guardian " と Cargo.toml の版を並べた1行が出て、終了コード 0 で終わる

@id=EX-155 @about=REQ-071 @source=docs/decision/records/2026-10-04-version-and-followups.md#A2
Scenario: サブコマンドの後ろの --version は版の表示にしない
  When "command-guardian hook --version" を実行する
  Then 版の1行は出ず、"hook" の入口として扱われる
```
