# 助言用文脈の取得根拠

## 確認範囲

2026-10-03に公開資料と固定したOpenCode依存を確認した。
個人の会話、認証、実モデルは使用していない。
公開の入力スキーマと実行前の会話対応を区別する。以下に「未対応」とした経路は文脈なしとして扱う。文脈なしは助言全体の見送りではない。

| ホスト | 公開根拠と版 | セッション・発言・実行の対応 | 判定 |
|---|---|---|---|
| Claude Code | [公式hooks reference](https://code.claude.com/docs/en/hooks#common-input-fields)。隔離した2.1.288で生成 | UserPromptSubmitとPreToolUseのsession_id/prompt_idが一致。PreToolUseでtranscriptがまだ存在しない実行を観測。MessageDisplayは別のmessage_idとturn_idを持つ。resumeした二つ目のpromptでも当該promptとtool_useを対応付けた | 単一の親子連鎖を持つ通常経路の取得と必要な別hook間cacheを本番入口へ接続。subagent/parallelの生成確認は未完了 |
| Codex | [rust-v0.160.0のhook試験](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core/tests/suite/hooks.rs)、[UserPromptSubmit schema](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/hooks/schema/generated/user-prompt-submit.command.input.schema.json)。隔離した0.160.0で生成 | UserPromptSubmit、PreToolUse、Stopのsession_id/turn_idが一致。Stopの合成継続はUserPromptSubmitを再発行せず同じturnを使用。実行前snapshotに元userと合成HookPromptがあり、型とcontent_item_kindsで区別できる | exec由来の取得と版・session・turn・call照合を本番入口へ接続。他の合成入力の生成確認は未完了。直接取得で足りる経路にcacheを作らない |
| OpenCode V2 | 公式npmの@opencode/plugin、@opencode/schema、@opencode/client、@opencode/cli-linux-x64 2.0.21。リポジトリのbun.lockでintegrityを固定 | ToolContextはsessionID、messageID、call idを持つ。SessionMessageはuser、synthetic、assistantを別の種別として定義する。既存試験bridgeはmessageIDを独立に生成し、実際の会話中のassistant発言へ結び付けていないため取得の正例ではない | 未対応。文脈なし |
| 直接check・それ以外の版 | 検証済み会話元の契約なし | コマンドや環境から指示を創作しない | 文脈なし |

## 再確認手順

```sh
bun --version
bun install --frozen-lockfile --cwd plugins/opencode
plugins/opencode/node_modules/@opencode/cli-linux-x64/bin/opencode --version
```

今回の結果はBun 1.4.2、OpenCode v2.0.21だった。
版の確認と依存取得は会話対応の証拠ではない。
公開済みpackage内のdist/promise/tool.d.ts、schemaのdist/tool.d.tsとdist/session-message.jsで型を確認した。
既存のplugins/opencode/tests/integration/bridge/index.tsではSessionMessage.ID.create()で実行contextを作っている。
このbridgeが生成する架空の実行はshellの保護の試験には使えるが、hostの会話元の正例には使わない。

## 未確認事項

Claude Code 2.1.288とCodex 0.160.0で、隔離したHOME・XDGとローカルHTTP fixtureを使い、実ホストがhook入力を生成した。
実サービス、実認証、個人の会話は使っていない。
Claudeの最初のprobeはSSEのtool引数形式が不十分でPreToolUseを生成しなかった。input_json_deltaを送る形式へ直した後、通常のpermission modeと許可したprintfだけでPreToolUseを生成できた。
Claudeの3回目のprobeではUserPromptSubmitとPreToolUseの時点でtranscriptが存在せず、MessageDisplayとStopでは存在した。4回目ではPreToolUseの時点で既に存在した。非同期保存の時機に依存するため、常に欠落するとはしない。
正規化した生成入力はtests/fixtures/advisor/claude-2.1.288.jsonに置く。ローカルの保存パスと架空IDだけを置換し、実行前の欠落観測は別欄に分離した。
CodexのprobeはUserPromptSubmit、PreToolUse、Stop、PreToolUse、Stopの順に5イベントを生成し、loopback fixtureへ4要求を送った。二つ目のPreToolUseでも同じturn_idを使い、合成継続はuser roleでもHookPromptとして記録された。
Codexのhooks/listで当該fixtureのcurrentHashを取得し、fixtureのconfig.tomlへtrusted_hashを保存してから実行した。hook trustやリポジトリのフックを無効化していない。
公開CodexソースのライセンスはApache-2.0。probeは公開プロトコルに対する独自スクリプトであり、公開試験のコードを転載していない。
このprobeだけでは、すべての既知の合成入力、subagent、parallel実行、複数往復、実装の版照合を確認したとはしない。
手書きparser fixtureでこの不足を埋めない。
userというroleだけで、人間がその場で入力したことを証明したとは扱わない。
窓外の撤回、参照先、制約の完全性も保証しない。
対応済みと宣言する前に、残る照合条件と取得アダプターを試験する必要がある。
直接取得と別hook間の保存を区別し、必要な保存経路を裏付けるまでは専用の会話キャッシュを作らない。

## 複数ターンの追加確認

同じ隔離条件で二つのprint実行を行い、二つ目に`--resume`を指定した。
モデルの代わりにloopback SSEで無害なprintfと固定返答を返し、計4要求と8イベントを観測した。
二つ目のUserPromptSubmitのsnapshotは前の返答までを持ち、二つ目のPreToolUseでは新しいuser promptと当該tool_useを持っていた。
正規化した記録は`tests/fixtures/advisor/claude-2.1.288-multiple.json`に置く。
識別情報と親子連鎖を保ち、attachment本文、tool引数・結果本文は除いた。
UserPromptSubmit時点の連鎖は同fixtureの最初の16レコードに対応する。

取得ライブラリはversion、sessionId、uuid、parentUuid、promptId、tool_use id、roleを照合する。
壊れた連鎖、重複ID、isSidechain=true、isMeta=true、未知版は文脈なしにする。
userの文字列本文とassistantのtextを区別し、user配列のtool_resultを人間の発言へ昇格させない。
初回hookで版を持つ会話元を取得できなければ、確認状態を創作せず文脈なしにする。
cacheは同一session・prompt・保存元path・対応する会話元の版に限定する。
直接読めるPreToolUseではcacheを作らず、欠落時だけ検証済みcacheを読む。読めるが壊れた会話元を古いcacheで補わない。
保存安全性は同一UIDのデータを暗号学的に認証するものではなく、保存元の版と対応付けにもその限界がある。
この確認はsubagent、並列実行、すべての合成入力をホスト生成で網羅した証拠ではない。
