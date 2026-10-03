# 別枠の期限と既存の出口

機械判定後の助言処理を同一バイナリの子に隔離し、文脈取得を含む期限で打ち切り、既存CLIとhookの出口を維持する契約を扱う。
助言ログは context.md、質問と接続は provider.md が扱う。

## Requirements

### REQ-advisor-017: 文脈取得を含む単一の助言期限

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A14, docs/decision/records/2026-10-03-llm-advice-layer.md#A16, docs/decision/records/2026-10-03-llm-advice-layer.md#A31, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A47
- verification: unit

機械判定は既存の5秒の解析と判定予算を保ち、助言は機械判定終了直後から単調時計で設定期限を計る。
内部の期限通知、子起動、文脈取得とキャッシュ、秘密検査、符号化、送信、受信、検証をすべて同じ助言期限に含め、文脈取得のために期限外の待ちを追加しない。
時間を使った各処理は残期限を引き継ぎ、期限後の回答は採用しない。
既定2000msは追加処理全体の上限であってHTTPに必ず2秒を与える保証ではなく、機械判定の残時間によって短縮しない。
助言オフ時の代表入力の応答目標100ms未満を維持し、有効時はモデル待ちを含む別の計測にする。
機械判定の既存のfs syscallの強制中断を保証する意味には変更しない。

### REQ-advisor-018: 同一バイナリの子と強制打切り

- kind: event_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A8, docs/decision/records/2026-10-03-llm-advice-layer.md#A30, docs/decision/records/2026-10-03-llm-advice-layer.md#A31, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A54
- verification: unit

助言処理は同じ配布バイナリの専用子モードで実行し、親の直接モデル実行fallbackや外部実行プラグインを置かない。
親が助言期限を監視し、文脈取得や接続が停止しても期限到来時に通信を閉じ、子を終了させ、遅い応答を捨てて機械判定を返す。
子起動失敗、子の死亡、panic、通信失敗、不正応答はすべて助言失敗として機械判定を維持し、解析workerの入力由来blockへの死因写像を流用しない。
子の回収やログのために期限後に無期限の待ちを加えず、終了を確認できない場合は定型警告を残す。
子終了は既送信要求や課金を取り消す保証ではない。

### REQ-advisor-019: 長さと要求対応を検証する内部通信

- kind: ubiquitous
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A30, docs/decision/records/2026-10-03-llm-advice-layer.md#A31, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A55, docs/decision/records/2026-10-03-llm-advice-layer.md#A48, docs/decision/records/2026-10-03-llm-advice-layer.md#A62
- verification: unit

親子間は継承済みUnixソケットを使い、版1、16バイトの要求ごとのnonce、1バイトの種別、4バイトの大端長、UTF-8 JSON本文の枠をやり取りする。
通常のCLI引数を追加せず、内部子モードは起動時にdispatchし、通常の標準出力へ枠や診断を出さない。
要求の枠上限は設定送信量にローカル照合情報用65536バイトを足した量、応答の枠上限は65536バイトとし、足し算のオーバーフローを拒否する。
文脈の取得もこの入力枠上限を超えて無制限に蓄積せず、上限を満たす限定窓を取得できなければ取得失敗として文脈なしにする。
版、nonce、種別、長さ、JSONの重複キーと必須フィールドを検証し、末尾の余分な枠、途中EOF、上限超過、異なる要求の回答を不正通信として拒否する。
ローカルの識別情報や取得先を外部モデル本文へ流用しない。
内部通信の上限は外部本文の上限を緩めない。

### REQ-advisor-020: OpenCodeの外側期限との整合

- kind: event_driven
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A16, docs/decision/records/2026-10-03-llm-advice-layer.md#A34, docs/decision/records/2026-10-03-llm-advice-layer.md#A41, docs/decision/records/2026-10-03-llm-advice-layer.md#A47, docs/decision/records/2026-10-02-opencode-v2-hook.md#A4, docs/decision/records/2026-10-02-opencode-v2-hook.md#A6, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A14, docs/decision/records/2026-10-02-opencode-v2-hook.md#A21, docs/decision/records/2026-10-02-opencode-v2-hook.md#A22
- verification: unit

OpenCodeプラグインはguardian起動時の単調時計から元の6000msの外側期限Oを固定し、標準出力と別の継承FDで内部制御通信を行う。
guardianは利用者設定で助言有効と確認した場合だけ、機械判定前に当該起動の版1とnonceを持つ"budget_probe"を一度送り、送信直前のローカル単調時計Qを記録する。
プラグインは応答を作る時点でOまでの正の残時間をミリ秒へ切り捨て、"original_remaining_ms"として同じ版とnonceで返す。
guardianはQから100ms以内の有効なprobe応答だけを採用し、D0=Q+original_remaining_msを元の期限の保守的な下限とする。
この機械判定前の制御交換は元の6000msの外側期限内で行い、その期限を延長せず、会話やファイル内容の取得は行わない。
応答作成はQより後なので、通信遅延を含めるこの計算は元の期限を過大評価しない。
同一ホストの単調時計の経過時間を用い、別プロセスの時計の絶対値を比較せず、不正値と期限計算のオーバーフローを拒否する。
FDなし、旧相手、probeの拒否、EOF、不正応答、100ms以内に応答なしでは当該起動を交渉非対応とし、機械判定後の再probeや確認応答待ちを行わない。

機械判定終了をM、設定timeout_msをTとして、助言期限DA=M+Tは元の外側期限の残時間によって短縮しない。
交渉対応の起動でも、確認応答を待つ期限はN=min(M+100ms,D0-500ms,DA)とし、NがM以下なら交渉せず即座に既存形式の機械結果を配送する。
Nまでの送信と確認応答待ちだけを認め、版1、当該起動のnonce、"advisory_start"、Tを一度通知する。
プラグインは元の期限Oまで500ms超の余裕があり当該起動の有効な初回通知である場合だけ受理し、受信時からT+1000msの外側期限へ切り替えてから確認応答を返す。
guardianはN未満に有効な確認応答を受けた場合だけ子を起動し、交渉に使った時間も含むDAの残時間で文脈取得と最大1回のモデル処理を行う。
拒否、通知失敗、不正な確認応答、EOF、Nまでに確認応答なしでは再交渉せず、モデルを呼ばず機械結果を直ちに配送し、N以後の応答を捨てる。
プラグインだけが期限を切り替えた後に確認応答を失っても、guardianはこの見送り規則を使う。
交渉と配送のためにログや子回収を先に待たず、標準出力には最終JSON以外を足さない。

配送用の500msは機械結果を生成済みの場合に交渉が消費しない予約であり、適時のスケジューリングと500ms未満の配送を前提とする未計測の技術値である。
機械判定が想定の5秒を超えた場合も、その終了時のMで余裕を再検査し、余裕不足なら即座に機械結果を返すだけで助言を開始しない。
MがD0-500ms以後なら500msの配送余裕自体を確保できたとは扱わず、即時の返却を試みても到達を保証しない。
機械処理より前の起動や設定読込、強制中断できないfs syscall、スケジューラ停止、配送停止によってすでにOを超えた場合、機械結果の到達を保証せず、ホストのタイムアウトによるREQ-051の承認経路が残る。
助言オフではprobeを含む制御交換も追加取得も行わず、機械blockでは機械判定前に行った制御probe以外にadvisory_start、文脈取得、モデル呼び出しを行わず従来の外側期限を使う。
直接hookのFDなしでも助言を見送るだけで機械判定は利用できる。
成功時の設定期限、通知前の元の外側期限、無効応答時のREQ-051、中断時のREQ-053、ホストの自動承認を含むREQ-049とREQ-050の意味は維持する。
この制御IPCは技術草稿であり、実装と実測による妥当性の証拠ではない。

### REQ-advisor-021: 最終判定だけを既存の出口へ渡す

- kind: invariant
- source: docs/decision/records/2026-10-03-llm-advice-layer.md#A8, docs/decision/records/2026-10-03-llm-advice-layer.md#A24, docs/decision/records/2026-10-03-llm-advice-layer.md#A30, docs/decision/records/2026-10-03-llm-advice-layer.md#A54, docs/decision/records/2026-09-30-hook-guardian-scope.md#A24, docs/decision/records/2026-09-30-hook-guardian-scope.md#A27, docs/decision/records/2026-09-30-hook-guardian-scope.md#A34, docs/decision/records/2026-09-30-hook-guardian-scope.md#A35, docs/decision/records/2026-10-02-opencode-v2-hook.md#A7, docs/decision/records/2026-10-02-opencode-v2-hook.md#A14, docs/decision/records/2026-10-02-opencode-v2-hook.md#A23
- verification: unit

追加判定の合成後に一度だけ最終判定を出し、checkの既存JSONと終了コード0、1、2、3、およびhookの既存封筒と終了コード0を維持する。
機械判定の効果ごとの一覧をモデル回答から創作せず、追加判定で厳しくした場合はguardianが作る理由を最終reasonへ含め、モデルの生の文章を理由として返さない。
Codexは最終allowとaskで無出力、blockだけ理由付きdenyとし、askをdenyに変えない。
Claude Codeの確認抑止モードとOpenCodeの影実行、判定不能、拒否、承認、自動承認、取消しの写像は既存契約に従う。
助言の失敗をホスト側のguardian全体の判定不能と混同せず、機械判定を返せた場合はその最終応答を使う。

## Examples

```gherkin
@id=EX-advisor-033 @about=REQ-advisor-017 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A16,docs/decision/records/2026-10-03-llm-advice-layer.md#A41
Scenario: 機械判定と助言の期限は別である
  Given 機械判定に4秒を使い助言期限は2000msである
  When 助言処理を始める
  Then 機械判定の残り1秒で助言期限を切らず文脈取得からの2秒で打ち切る

@id=EX-advisor-034 @about=REQ-advisor-017 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A41
Scenario: 文脈取得が期限を使い切る
  Given 文脈の取得が応答せず助言期限が来た
  When 助言処理を終了する
  Then モデルを追加で呼ばず機械判定を返す

@id=EX-advisor-035 @about=REQ-advisor-018 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A31,docs/decision/records/2026-10-03-llm-advice-layer.md#A54
Scenario: 期限を守る子の回答を採用する
  Given 子が期限内に妥当なAssessmentを返した
  When 親が回答を受け取る
  Then 親が合成を行い子の通信と寿命を終了する

@id=EX-advisor-036 @about=REQ-advisor-018 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A8,docs/decision/records/2026-10-03-llm-advice-layer.md#A31
Scenario: timeoutを無視する子でも親は待ち続けない
  Given 子の接続実装が停止したかpanicした
  When 助言期限または子の死亡を検知する
  Then 子を打ち切り機械判定を返し新しいblockやaskを作らない

@id=EX-advisor-037 @about=REQ-advisor-019 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A41
Scenario: 正しい要求への枠だけを受け取る
  Given 版とnonceと長さが当該要求に一致する1つの回答枠がある
  When 親が期限内に復号する
  Then 共通のAssessment検証へ進む

@id=EX-advisor-038 @about=REQ-advisor-019 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A41,docs/decision/records/2026-10-03-llm-advice-layer.md#A55,docs/decision/records/2026-10-03-llm-advice-layer.md#A8
Scenario: 古い回答や巨大な宣言長は拒否する
  Given 枠のnonceが違うか宣言長が上限を超えるかJSONが重複キーを持つ
  When 枠を読む
  Then 不正通信とし巨大な長さを確保せず機械判定を維持する

@id=EX-advisor-039 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47,docs/decision/records/2026-10-03-llm-advice-layer.md#A41
Scenario: 長い設定をOpenCodeの固定6秒で切らない
  Given プラグインの起動を0msとして元の期限Oは6000ms、probeで求めたD0は5980ms、設定Tは10000msである
  When 機械判定が5000msで終了し5050msにプラグインが通知を受理し5075msにguardianが有効な確認応答を得る
  Then Nは5100ms、DAは15000ms、プラグインの外側期限は16050msになる
  And 子はDAまでの残り9925ms以内で処理し機械判定の残時間でTを縮めない
  And 標準出力には最終JSON以外を出さない

@id=EX-advisor-040 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47
Scenario: 確認応答を待ち続けて機械結果を失わない
  Given Oは6000ms、D0は5980ms、Tは10000msであり対応プラグインからの確認応答が返らない
  When 機械判定が5000msで終了し通知後のNである5100msに達する
  Then モデルを呼ばず再交渉もせず機械結果を直ちに配送する
  And 500ms未満の配送と適時のスケジューリングの条件で元の外側期限前に機械結果を取得できる

@id=EX-advisor-045 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47
Scenario: probeの配送遅延で元の残時間を水増ししない
  Given 同じ起動のguardianが100msにprobeを送りプラグインが120msに残時間5880msを応答した
  When guardianが150msに版とnonceの正しいprobe応答を受け取る
  Then D0は受信時に5880msを足した6030msではなくQに足した5980msになる

@id=EX-advisor-046 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47
Scenario: FDのない旧相手やprobe失敗では機械判定後に待たない
  Given FDがないか旧プラグインがprobeに100ms以内の有効な応答を返さない
  When 機械判定が元の外側期限内に終了する
  Then advisory_startと追加取得とモデル呼び出しを行わず機械結果を直ちに配送する

@id=EX-advisor-047 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47,docs/decision/records/2026-10-02-opencode-v2-hook.md#A14
Scenario: 拒否や失った確認応答は助言実行に変わらない
  Given 対応プラグインが通知を拒否するか外側期限の切替え後に確認応答の通信が切れた
  When guardianが拒否またはEOFを検知する
  Then Nを待ち切ることなく機械結果を配送し子を起動しない
  And N以後の確認応答や中断後の応答で実行を再開しない

@id=EX-advisor-048 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47,docs/decision/records/2026-10-02-opencode-v2-hook.md#A6,docs/decision/records/2026-10-02-opencode-v2-hook.md#A7
Scenario: 機械処理自体が遅れた場合に到達を保証しない
  Given D0は5980msであるが先行するfs syscallにより機械判定の終了Mは6100msになった
  When guardianが機械結果を生成できる
  Then 助言交渉を開始せず機械結果の配送を試みる
  And 元の6000ms期限でホストがすでにタイムアウトした場合はREQ-051の経路を維持する

@id=EX-advisor-049 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47
Scenario: オフと機械blockは追加取得を起動しない
  Given 助言がoffか機械判定がblockである
  When 対応OpenCode起動を処理する
  Then advisory_startと文脈取得とモデル呼び出しを行わない
  And offでは機械判定前のprobeも行わない

@id=EX-advisor-050 @about=REQ-advisor-020 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A47
Scenario: 機械終了が予約境界を越えたら交渉しない
  Given 元の期限Oは6000ms、D0は5980ms、機械判定終了Mは5500msである
  When guardianがNを計算する
  Then Nは5480msでM以下となるため確認応答を待たず機械結果を即時配送する
  And 500msの配送余裕が確保できたとは主張しない

@id=EX-advisor-041 @about=REQ-advisor-021 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A24,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27
Scenario: Codexの重大操作確認は既存の無出力である
  Given 指示に一致する重大操作の最終判定はaskである
  When Codex hookの応答を作る
  Then 標準出力は空で終了コード0になりCodex本来の設定へ委ねる

@id=EX-advisor-042 @about=REQ-advisor-021 @source=docs/decision/records/2026-10-03-llm-advice-layer.md#A8,docs/decision/records/2026-10-03-llm-advice-layer.md#A30,docs/decision/records/2026-10-03-llm-advice-layer.md#A54,docs/decision/records/2026-09-30-hook-guardian-scope.md#A27,docs/decision/records/2026-09-30-hook-guardian-scope.md#A34
Scenario: 助言失敗をCLI失敗へ変えない
  Given 機械判定はallowで助言だけが失敗した
  When checkのJSON応答を作る
  Then 既存形式の最終allowを一度だけ出し終了コード0にする
  And 助言失敗の生の通信本文をreasonや標準出力へ混ぜない
```
