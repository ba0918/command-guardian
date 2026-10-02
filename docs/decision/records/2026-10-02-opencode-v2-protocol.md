# OpenCode hookのJSON表現

## Context

承認済みのOpenCode連携は影実行と判定不能を区別する応答を必要とする。
既存のClaude・Codex応答を変更せず、新しいagent専用のJSON表現を定める。
この表現は[委譲された実装詳細](./2026-10-02-opencode-v2-hook.md#D1)として実装担当が決める。

## Agreements

- A1 "hook --agent opencode" の入力はJSONオブジェクトの "command"、"cwd"、"shell" とし、全て文字列で、commandは空でなく、cwdとshellは絶対パスとする。shellの最終パス要素が "bash" のときだけBashとして判定する。
  - why: ホストが解決した起動入力をそのまま標準入力へ渡し、暗黙のcwdやshellの推測を避けるため
  - decided_by: 実装担当。既存D1の範囲
- A2 応答の "status" は "judged"、"shadow"、"unavailable" のいずれかとし、"reason" は文字列とする。設定を読めた応答は "mode": {"enforce": boolean} を含む。judgedだけが "verdict": "allow" | "ask" | "block" を含む。shadowは判定を含まず、設定を読めないunavailableはmodeを含まない。プロトコル終了コードは全て0とする。
  - why: 確定済みの影実行だけを強制から除外し、設定読込や入力の失敗を安全な判定と誤認しないため
  - decided_by: 実装担当。既存D1の範囲
- A3 pluginの接続設定は "serverUrl" と "passwordEnv" とする。後者は同一HTTPサーバーの環境変数名で、HTTP BasicのusernameはOpenCodeの "opencode" を使う。URL・認証を探索せず、欠落または接続失敗時は承認を要する実行を開始しない。
  - why: 公開plugin contextには承認要求のcreateがなく、公式clientと同一サーバーへの明示接続でnative createを使うため。隔離ホストで認証成功と失敗を観測した
  - decided_by: 実装担当。既存D1の範囲
- A4 native承認のactionは "command-guardian"、IDは公式Permission.ID.createで発行する。metadataに "guardianReason"、"cwd"、"shell" を渡し、saveは空にする。このactionのevaluate hookはnative denyを保持し、それ以外をaskにする。要求IDとsessionIDの双方が一致する返信だけを受け入れる。
  - why: 保存済み許可があっても各実行に要求を作り、native once・always・rejectと並行実行を隔離ホストで確認したため
  - decided_by: 実装担当。既存D1の範囲
- A5 実行前のtool transformは元executorを保持し、入力のcopyを判定と委譲に共用する。公開config.getで明示shellを取得し、cwdを字句的に絶対化する。公開shell create.beforeで実入力を照合し、AsyncLocalStorageで実行contextを対応付ける。
  - why: redirectだけの入力はnative権限判定を通らない場合があり、tool transformが必要。shell hookは単独ではsession/tool IDを持たないが、公開tool.list由来のexecutorからasync contextが保持されることを実機probeで観測したため
  - decided_by: 実装担当。既存D1の範囲
- A6 runtimeは公式plugin・client・schema 2.0.21を使い、Bun buildで依存を含むserver.jsへまとめる。配布manifestのversionはCargo.tomlから生成する。MITのOpenCodeとEffectのlicense noticeを同梱する。型だけのoptional theme peerはbundleのruntime入力に含めない。
  - why: checkout外のbundle登録・実行を確認し、build metafileの依存を限定したため。開発用node_modulesや登録時のnpm解決を配布前提にしない
  - decided_by: 実装担当。既存D1の範囲
- A7 検証用Bunは1.4.2、TypeScriptは5.9.3、Bun型は1.3.11に固定する。OpenCodeのLinux x64 CLIとCode Mode interpreterは公式npmの2.0.21を使い、lockfileのintegrityと起動時の版確認を行う。
  - why: 既設CLIに依存しないCIとモデルなしの公開executor検証を再現するため。Code Modeは検証依存で配布runtimeには含めない
  - decided_by: 実装担当。既存D1の範囲

## Grounds

- [公式2.0.21のplugin hooks](https://github.com/anomalyco/opencode/tree/v2.0.21/packages/plugin/src/promise)のtool transform/list、shell hook、permission evaluateを使う。内部moduleをruntimeからimportしない。
- [公式2.0.21のnoninteractive run](https://github.com/anomalyco/opencode/blob/v2.0.21/packages/cli/src/run/noninteractive.ts)はauto時にnative permissionへonceを返す。モデルを使うCLI run自体は今回実行していない。
- [公式2.0.21のSessionExecution](https://github.com/anomalyco/opencode/blob/v2.0.21/packages/core/src/session/execution.ts)はidle sessionへのinterruptをno-opにする。RPC fixtureのsignal cancellationは実機検証したが、idle sessionへのinterruptをactive session中断の証拠として扱わない。
- 判定・設定・影ログは既存Rust Engineを採用し、processはNode互換child_process、JSON・path・async contextは標準機能を採用した。承認待ちのID対応と取消状態だけをport付き部品として実装した。
