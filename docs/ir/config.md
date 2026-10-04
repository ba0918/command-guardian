# 設定

設定の置き場、層、キー、プロジェクト設定の信頼、壊れたときの扱いを決める。判定は judgment.md が扱う。

## Requirements

### REQ-013: 設定のファイルと層

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A5, docs/decision/records/2026-09-30-hook-guardian-scope.md#A18, docs/decision/records/2026-10-01-rename.md#A1, docs/decision/records/2026-10-03-llm-advice-layer.md#A18, docs/decision/records/2026-10-04-auto-allow-ask.md#A3
- verification: unit

command-guardian は、利用者の設定を "XDG_CONFIG_HOME" が指すディレクトリの "command-guardian/config.toml"（"XDG_CONFIG_HOME" が無いときは "~/.config/command-guardian/config.toml"）から、プロジェクトの設定を作業中のディレクトリから上へ探して最初に見つかった ".command-guardian.toml" から読む。マージは組み込み、利用者、プロジェクトの順に行い、リストは足し合わせ、それ以外は後のものが勝つ。

助言の"advisor"節だけはadvisor/provider.mdの利用者限定規則に従い、プロジェクト層のマージ対象にしない。
"mode.defer_ask" も defer-ask.md の REQ-059 に従い、プロジェクト層のマージ対象にしない。

### REQ-014: プロジェクト設定の信頼

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A20, docs/decision/records/2026-10-03-llm-advice-layer.md#A18, docs/decision/records/2026-10-03-llm-advice-layer.md#A46, docs/decision/records/2026-10-04-auto-allow-ask.md#A3
- verification: unit

プロジェクトの設定は、既定では判定を厳しくする変更だけを反映する。反映するのは、保護ルートの追加、`unknown` の判定の `block` への変更、`ask` か `block` を返すパターンの追加であり、許可ルートの追加、ルールの無効化、判定を緩める変更は無視して警告を出す。利用者設定の "trusted_projects" にリポジトリのパスがあるときだけ、すべてを反映する。

助言の"advisor"節は例外であり、信頼済みプロジェクトでも全キーを無視して警告する。
"mode.defer_ask" も同じく、信頼済みプロジェクトでも無視して警告する。

### REQ-015: 壊れた設定

- kind: event_driven
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A5, docs/decision/records/2026-09-30-hook-guardian-scope.md#A45, docs/decision/records/2026-10-02-ir-friction-contracts.md#A1, docs/decision/records/2026-10-03-llm-advice-layer.md#A46, docs/decision/records/2026-10-03-llm-advice-layer.md#A60, docs/decision/records/2026-10-04-auto-allow-ask.md#A15
- verification: unit

設定が読めない、TOML構文が壊れている、または一般設定の型・値が誤っているとき、command-guardian は、その設定ファイル全体を不採用にして警告を出す。組み込みの既定と他の正常な設定層で判定を続け、壊れたファイルの正常部分は残さない。個々のguard規則の誤りはREQ-034の規則単位無効化を使う。commandsセクションやguard一覧自体の型の誤りは一般設定の誤りとして扱う。

プロジェクトの"advisor"要素はTOML構文解析後、助言固有の検証前に除去するため、その無視する型や値の不正で他の正常な保護規則を不採用にしない。
プロジェクトの"mode.defer_ask"も一般設定の検証前に除去し、同じく他の正常な保護規則を不採用にしない。
TOML構文エラーと読取失敗、実際に適用する一般設定の不正はこの例外に含めず、利用者ファイルのadvisorは検証対象とする。

### REQ-026: ルールの意味論

- kind: ubiquitous
- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A4, docs/decision/records/2026-09-30-hook-guardian-scope.md#A32
- verification: unit

command-guardian のルールは、名前を持つ判定の単位である。組み込みのルールは効果の取り出しの単位であり、"delete"、"truncate"、"format" の名前を持ち、"rules.disable" で無効化すると、その効果は取り出さない。カスタムのルールは名前、正規表現のパターン、判定を持ち、引用とヒアドキュメントを外したコマンド本文に照合し、一致した判定を合成に加える。allow を返すカスタムのルールは、利用者設定でのみ有効とする。

## Decision tables

### TBL-001: 設定のキー

- source: docs/decision/records/2026-09-30-hook-guardian-scope.md#A4, docs/decision/records/2026-09-30-hook-guardian-scope.md#A9, docs/decision/records/2026-09-30-hook-guardian-scope.md#A18, docs/decision/records/2026-09-30-hook-guardian-scope.md#A20, docs/decision/records/2026-09-30-hook-guardian-scope.md#A22, docs/decision/records/2026-09-30-hook-guardian-scope.md#A31, docs/decision/records/2026-09-30-hook-guardian-scope.md#A32, docs/decision/records/2026-09-30-hook-guardian-scope.md#A42, docs/decision/records/2026-09-30-hook-guardian-scope.md#A47, docs/decision/records/2026-10-04-auto-allow-ask.md#A3, docs/decision/records/2026-10-04-auto-allow-ask.md#A1, docs/decision/records/2026-10-04-auto-allow-ask.md#A13

| キー | 既定 | 意味 |
|---|---|---|
| "paths.allowed_roots" | 一時領域のルート | 丸ごと消してよいルート |
| "paths.protected_roots" | 空 | 追加する保護ルート |
| "unknown.verdict" | "ask" | 分類できないときの判定 |
| "rules.disable" | 空 | 無効にする組み込みルールの名前 |
| "rules.custom" | 空 | 名前、正規表現のパターン、判定の一覧 |
| "commands.guard" | 空 | プログラムごとの使い方を止める規則の一覧（guards.md） |
| "git.enabled" | true | git による分類を使うかどうか |
| "mode.enforce" | true | 判定を返すか、影実行でログだけ残すか |
| "mode.defer_ask" | false | フックで ask を返さずエージェントの権限判断に委ねるか（利用者設定のみ。推奨しない） |
| "trusted_projects" | 空 | プロジェクト設定をすべて反映するリポジトリのパス |

## Examples

```gherkin
@id=EX-014 @about=REQ-014 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A20
Scenario: 信頼していないプロジェクト設定の緩和は無視する
  Given リポジトリの ".command-guardian.toml" が許可ルートに "/" を追加している
  When そのリポジトリの中で削除の効果を判定する
  Then 許可ルートの追加は無視され、警告が出る

@id=EX-015 @about=REQ-013 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A18
Scenario: 親ディレクトリのプロジェクト設定を読む
  Given 親ディレクトリに ".command-guardian.toml" がある
  When 子ディレクトリで判定する
  Then その設定がプロジェクトの層として読まれる

@id=EX-016 @about=REQ-015 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A5
Scenario: 壊れた設定では既定で続ける
  Given 利用者の設定が TOML として壊れている
  When 判定する
  Then 組み込みの既定で判定し、警告が出る

@id=EX-112 @about=REQ-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A60
Scenario: 規則の容器の型誤りはファイル全体を不採用にする
  Given TOML構文は正しいがcommands節が表ではないかcommands.guardが一覧ではない
  And そのファイルには正常な保護ルートの指定もある
  When 設定を読む
  Then 警告しそのファイル全体を不採用にして同じファイルの保護ルートも残さない
  And 組み込み既定と他の正常な設定層は維持する

@id=EX-113 @about=REQ-015 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A60,docs/decision/records/2026-09-30-hook-guardian-scope.md#A45
Scenario: 個別規則の不正で正常な設定まで捨てない
  Given commands節とguard一覧の型は正しく個別のguard一件だけが不正である
  And 同じファイルに正常な保護ルートと別の正常なguard規則がある
  When 設定を読む
  Then 不正な規則だけを無効にして警告する
  And 同じファイルの正常な保護ルートと別の規則と他の正常な設定層を維持する

@id=EX-030 @about=REQ-014 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A20
Scenario: プロジェクト設定の自己信頼は効かない
  Given プロジェクト設定の "trusted_projects" に自分自身のリポジトリのパスがある
  When そのリポジトリの中で削除の効果を判定する
  Then 自己信頼は無視され、利用者設定の "trusted_projects" だけが信頼を決める

@id=EX-035 @about=REQ-026 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A32
Scenario: 無効にした効果は取り出さない
  Given "rules.disable" に "truncate" がある
  When リダイレクトの ">" を含むコマンドを判定する
  Then 切り詰めの効果は取り出されない

@id=EX-036 @about=REQ-026 @source=docs/decision/records/2026-09-30-hook-guardian-scope.md#A32
Scenario: カスタムのルールの判定が合成に加わる
  Given カスタムのルールのパターンがコマンド本文に一致する
  When 判定する
  Then そのルールの判定が合成に加わる
```
