# LLM助言層の検証記録

機械検査、独立レビュー、修正後の確認を、対象候補と検証の限界を分けて記録する。

## 状態

CLIと三ホストの接続、修正後候補の機械検査、品質・仕様適合の独立レビューが完了した。
CLI、Claude、Codexへ専用子を接続し、OpenCodeのguardian入口では実FDの交渉成功時だけ助言を開始する。
ログ保存は専用子で最終回答の送信後に行い、親は結果を出力してから残期限で回収する。
独立レビューの指摘6件は修正後の差分レビューで解消を確認した。機械検査成功だけで合格としたものではない。
検証記録と実装者の変更照合記録の更新を独立担当が確認し、review.yamlを作成した。
実装を`f7f3b83de4c4bf34c3d42511508b828dbb84125b`、独立担当の記録を`c2a7b5f265bc3b2cdbbab278bedccd9cb6eadc6d`にコミットした。
後者のHEADでPROJECT.mdの両統合ゲートが成功し、変更照合は53件中53件を充足した。
同じHEADをmainへfast-forwardで統合した。
リリースの公開は行っていない。
実モデルの呼出し、現在のTypeSafe APIとの適合確認、モデル精度の評価は行っていない。
閾値0.9は正答率90%の証拠ではなく、jev-latestの将来のモデルも固定されない。

## 修正後候補の最終結果

比較元は`9c5a25884a2d430dfaafd56758f515a66992330e`のままである。
最終機械検査の202ファイルのmanifest SHA-256は`2a6b06c0b9b9a618a963e6c0ae9218dcd6b3d3d7344e059454b5e08156120486`で、配送準備前にも内容と権限の一致を確認した。
下表はその製品候補の証拠であり、後から更新する検証記録・変更照合記録の独立確認や最終コミットの統合ゲートとは分ける。

| 検査 | 結果 |
|---|---|
| GNU/musl Rust | 各414 passed、0 failed、0 ignored。fmt、両targetのclippy、release build、GNUのcheckとdebug buildも成功 |
| Bun typecheck、unit | 成功。30 pass、0 fail、101 assertions |
| 固定OpenCode 2.0.21 host | debug、GNU release、musl release、checkout外のGNU/musl配布物の5経路で各14 pass、0 fail |
| 配布物 | GNU/muslの包装と各SHA-256照合が成功。公開はしていない |
| kotowari check | exit 0、error 0、notice 2。既存のcli.mdとopencode.mdの行数通知のみ。changesの最終review検査とは別 |
| offの代表100入力 | debug binaryで10種類を各10回、実git起動20回。中央値3.584ms、最大7.378ms、100ms以上0件。浅い入力の観測値であり時間の保証ではない |
| 独立レビュー | 品質・仕様適合の2回目のフルレビューと後続差分レビューが収束し、6件すべて解消。新規未処理指摘なし |
| 固定質問の意味 | 承認済みA63/A64に基づく独立照合は対象範囲でPASS。質問・分類定義・送信データの意味の判断であり、実モデル品質や現在のTypeSafeサービス適合の証拠ではない |

機械検査の証拠一覧は`.agents/artifacts/llm-advisor-final-check-round3.json`、レビューの集約は`.agents/artifacts/reviews/llm-advisor.json`にある。
品質・仕様適合それぞれの`llm-advisor-*-full2.json`と後続の`llm-advisor-*-diff3.json`、質問の`llm-advisor-question-semantics.json`に判断根拠を残した。
これらはローカルの検証成果物であり、配布物には含めない。
作業ツリーの削除前に、成果物を`.agents/artifacts/llm-advisor-archive-c2a7b5f/`へ退避した。
最終コミットと独立照合の証拠は、その中の`llm-advisor-delivery-final.json`と`llm-advisor-delivery-review.json`にある。
ホストの出所と対応範囲、既知の限界は[文脈取得の検証記録](llm-advisor-context.md)の通りで、新しいホスト対応や人間由来の完全証明を追加していない。
以前のhost準備失敗の原因は未確定であり、今回の5経路成功から不安定さの解消を推定しない。

## 修正前候補の機械検査と独立レビュー（履歴）

`.agents/artifacts/llm-advisor-final-check.json`の候補SHA-256は`50c6f9f6c0ca4d095f41e5280240061be62d22e4330e189abeb5275f84dc9a99`である。
以下はこの修正前候補の証拠であり、修正後のソースの成功とは扱わない。

| 検査 | 結果 |
|---|---|
| GNU/musl Rust | 各412 passed、0 failed、0 ignored。fmt、両targetのclippy、release buildも成功 |
| Bun typecheck、unit | 成功。29 pass、0 fail |
| 固定OpenCode host | debug、GNU release、musl release、checkout外のGNU/musl配布物の5経路すべてで各14 pass、0 fail |
| kotowari check | exit 0、error 0、notice 2。既存のcli.mdとopencode.mdの行数通知のみ |
| offの代表100入力 | 10種類を各10回、実git起動20回。中央値3.389ms、最大6.574ms、100ms以上0件。観測値であり応答時間の保証ではない |
| 独立フルレビュー | 品質・仕様適合ともFAIL。重複をまとめた指摘4件。機械検査成功だけでは統合できない |
| 独立した質問の意味のレビュー | PASS。実際の固定質問・分類定義・符号化データを要求と反例へ照合。実モデル精度やサービス適合の判定ではない |

独立レビューの根拠は`.agents/artifacts/reviews/llm-advisor-quality.json`、`llm-advisor-conformance.json`、`llm-advisor-question-semantics.json`にある。
以前のhost準備失敗の原因は未確定であり、5経路の成功から不安定さの解消を推定しない。

## 初回修正後の確認（履歴）

JSONキーと入れ子の秘密、実CLIの通常・debugログ、介入後のCLI JSONとnative hook出力、換算境界と設定ファイル全体の不採用を対象に、REDで不具合を確認してから修正した。
対象試験のGREENと整形後の再実行は成功した。
修正後のGNU workspaceは414 passed、0 failed、0 ignored。fmt、clippy、kotowari check、release/debug buildも成功した。kotowariの通知は既存の行数通知2件のみである。
再ビルドした実CLIでも、最後の表現可能値18446744073709msは受理し、その次の18446744073710msと指摘時の18446744074710msは警告して利用者ファイル全体を不採用にすることを確認した。他の有効なプロジェクト層は維持した。
初回修正後の証拠一覧は`.agents/artifacts/llm-advisor-fixes-round1.json`にある。この時点ではmusl、5経路のhost、配布物、off時間測定は再実行していなかった。現在の結果は上の最終結果を参照する。
質問、分類定義、送信データの契約とIRは変更していない。以前の質問レビューのPASSは、その限定された対象についての判断である。

## 実装途中の実行結果（履歴）

この表は実装途中の各snapshotの観測を保持する。修正前候補の最終結果は上の表と区別する。

| 検査 | 結果 |
|---|---|
| S2 RED: cargo test -p guardian-advisor --locked | 未実装のcombine、AssessmentなどのimportがE0432で失敗 |
| S2 GREEN: 同コマンド | 初回5 passed、0 failed。再開後はtrait試験1件とpolicy試験6件が成功 |
| S2再開 RED | 分布保持のgetterがなくE0599、traitと送信許可値がなくE0405/E0422で失敗 |
| S3 RED: cargo test -p command-guardian --locked --test app_config | 未実装のadvisor設定フィールドがなくE0609で失敗 |
| S3 GREEN: 同コマンド | 21 passed、0 failed。利用者既定・変更・不正、両信頼状態での除去、容器と個別規則の復旧単位を確認 |
| 検出力の確認 | 独立有害効果の候補をblockからaskへ一時変更すると、合成とobserveの2件が失敗。変更を戻した |
| REFACTOR | 整理は追加せず、修復後にworkspace全試験を再実行して成功 |
| cargo fmt --all --check | 成功 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 成功 |
| cargo test --workspace --locked | 成功 |
| cargo build --release --locked | 成功 |
| Bun typecheck、test:unit | 成功 |
| 通常のS1 commit | kotowariの未充足20要求・53例により失敗。hookは変更も迂回もしていない |
| Claude Code 2.1.288の隔離probe | 実CLIとloopback SSEからUserPromptSubmit・PreToolUse・MessageDisplay・Stopを生成。同一session/prompt対応と実行前のtranscript欠落を確認 |
| Codex 0.160.0の隔離probe | 実CLIから5イベントを生成。Stopの合成user継続は同じturnを使い、直接取得した記録では元userと区別できる |
| S4 RED: guardian-advisorのcontext試験 | bounded_context、ContentBlock、ContextMessage、RoleがなくE0432で失敗 |
| S4 RED: guardian-appのstate試験 | stateモジュールがなく失敗。符号化した枠全体の上限試験は6成功・1失敗で不備を検出 |
| S4 RED: guardian-appの取得試験 | 未実装APIで失敗。重複キー、発言順序、実行境界の重複もそれぞれ失敗を確認してから実装 |
| S4 GREEN・REFACTOR | guardian-advisorの4件、stateの7件、取得境界の6件が成功。workspaceは362成功・0失敗・0 ignored |
| S4の検出力 | 合成userの除外を一時無効にすると取得試験3件が失敗。復元後のworkspace試験が成功 |
| 前のsnapshotのkotowari check | 未充足9要求・27例、notice 2件。成功とは報告しない |
| S4追加 RED | Claudeのparser、cache、実ファイル取得の未実装API、Stateの型なし文脈、cache無効化APIで失敗 |
| S4追加 GREEN | 隔離ホストの複数ターン記録でrole・順序・実行前境界を照合。未知版・壊れた連鎖・重複ID・別promptを拒否。保存更新失敗後は古い窓を復元しない |
| S5 RED | 送信許可検査・providerのprepare・HTTP境界がなく失敗。引用符付きcredential名とUnicodeで符号化した名前の見落としも試験で検出 |
| S5 GREEN | 実際のJSON本文65536/65537境界、モデル・質問・全状態の検査、二質問の分離、重複応答・不正分布を検証。loopback HTTPでPOST一回、redirect追従なし、429再試行なし、期限・応答上限を確認 |
| S6通信枠 RED→GREEN | 未実装wireで失敗後、実Unix socketで版・nonce・種別・大端長・UTF-8・JSON・必須欄・余分な欄・末尾枠・EOFを検証 |
| S6専用子 RED→GREEN | worker import欠落で失敗後、実バイナリdispatch、認証欠落と不正枠の無出力を確認 |
| S6親 RED→GREEN | 起動と期限helper欠落で失敗後、実ソケットと独立fixture子で妥当回答、停止、死亡、遅い回答、起動失敗を検証 |
| S6取得期限 RED→GREEN | 取得依存境界の欠落で失敗後、期限後に文脈取得が戻ってもモデル未実行を確認 |
| S7 RED→GREEN | logとappend API欠落で失敗後、既定の本文除外、明示debugの伏字、実ファイルの権限とlink拒否を検証 |
| S8 Rust/Bun RED→GREEN | control module欠落で失敗後、元期限の保守的計算、500ms予約、初回通知、ack欠落と拒否を検証 |
| S8継承FD RED→GREEN | fd 3欠落でfixture子が失敗後、双方向通信と6秒を超える結果受領、中断後の再開禁止を確認 |
| S7配送順序 RED→GREEN | 未実装start_withで失敗後、回答後に停止する独立子を期限で打ち切り、既に得たblockを変更しないことを確認 |
| S7ログ接続 RED→GREEN | 実助言子が未接続log欄を拒否して失敗後、認証失敗の分類ログと本文なし警告を実ファイルで確認。親の起動失敗でログ未保存の警告が欠ける試験も修正 |
| S9入口 RED→GREEN | CLIとClaude/Codexでadvisor.jsonlが作られず失敗後、実バイナリの認証欠落、無出力allow、既存影ログを確認。秘密・サイズ・非UTF-8の試験は既存動作の追加観測でありREDとはしない |
| S8 guardian入口 RED→GREEN | 実FDにprobeが届かず失敗後、ack成功・欠落・拒否・off・blockを確認。各試行の最終JSONと終了まで3.31〜110.34msで、500ms未満だった |
| Claude cacheイベント RED→GREEN | 実UserPromptSubmitでcacheが作られず失敗後、専用子が確認済み限定窓だけを保存。判定や助言ログを出さない |
| 配布license RED→GREEN | 既存アーカイブにureqのlicenseがなく失敗後、target別の非dev依存とringの入れ子noticeを同梱。追跡された配布試験でも実行とlicenseを確認 |
| 高リスク回答の親とnative出力 | 実IPCの独立fixture子から妥当な分布を受け、親の合成と本番で使うCLI JSON・終了コード、Claude/Codex出力関数まで追跡。文脈なし重大破壊0.95はblock/2、0.89はallow/0、確認済みmatchedはask/1、独立有害効果はblock、observeはallow。Codexのaskは出力なし、blockだけdeny。fixtureのmatchedは指示範囲の意味の証拠にはしない |
| 取得停止の親の打切り | 実IPCを受けた独立fixture子をFIFOのopenで停止させ、2秒の助言期限で終了させた。取得開始を観測し、その後のモデル段階は未到達。機械allowとCLI JSON・終了コード0を維持。Rust取得アダプター自身を停止させた試験とは区別する |
| OpenCodeのnative出力 | 同じ実IPCと親の合成を通した高リスクReportを、本番で使うJSON組立てへ渡す。block/ask/observeをjudgedとして出し、影実行はshadowのままでverdictを返さない。独立fixture子での写像試験であり、実モデルや固定ホスト内の高リスク応答試験とは区別する |
| 最新のGNU/musl Rust試験 | 各412 passed、0 failed、0 ignored。両targetのclippyとrelease buildも成功。HTTPの140ms待ち試験が送信本文の準備も測っていたため、準備後から通信を測るよう修正。診断では準備94.84ms、通信36.78msだった。通信予算30msと試験上限140msは変えていない |
| 最新のBun typecheck、unit | 成功。29 pass、0 fail、95 assertions |
| 入口接続時点のOpenCode 2.0.21隔離host integration | debug、GNU release、musl releaseと両targetのcheckout外配布物で各14 pass、0 fail。実guardianの認証欠落経路も含む。GNUの一回は隔離serverの終了で1 failとなり、再実行では14件成功。原因は未確定 |
| native出力関数抽出後の固定host（過去の失敗） | debugとmuslは各14件成功。GNUの全体再実行は隔離serverの5秒以内の準備確認ができず13 pass・1 failとなり、失敗した試験の単独再実行は成功。GNUの二回目の全体再実行でも別の試験で同じ準備失敗が起きた。一括実行の一回は外側120秒の実行予算で中断した。原因は未確定。後続の修正前候補の5経路成功とは区別する |
| offの代表100入力 | 入口接続後、10種類を各10回。実git起動20回を含み、中央値3.33ms、最大7.28ms、100ms以上0件。未完了の部分候補の測定であり最終承認の証拠ではない |
| A64反映前のkotowari check | REQ-advisor-008とEX-advisor-015、016が未充足、notice 2件だった。後続の修正前候補ではerror 0となった |

上記は各時点の実装途中の試験結果であり、統合ゲートの成功ではない。
修正前候補のcheckout外の配布試験は成功したが、その後の独立フルレビューはFAILだった。
TypeSafe接続は本番入口から専用子を介して呼び出し、試験では実認証を渡さず認証欠落まで確認した。
Claudeの取得とUserPromptSubmit/MessageDisplayのcache更新を本番hookへ接続した。
子の起動とdispatch、全体期限は部品試験が通っている。
助言ログの隔離と保存失敗時の警告、guardian入口からのOpenCode交渉を接続した。
symlink、hardlink、他プロセスのlockで保存できなくてもCLIのallowと終了コード0は変わらず、内容を含まない警告だけを出す。
高リスク回答を実IPCからCLIとClaude/Codexの共有native出力関数まで通す試験を追加した。
OpenCodeも共有native出力関数までの高リスク試験を追加した。固定ホストへ実モデルの高リスク応答を通した試験やRust取得アダプター自身を停止させる試験ではなく、承認済みの部品ごとの証拠を組み合わせている。
安全保存の試験は実Unixファイルによる期限・権限・link・サイズの境界の証拠であり、保存した本文が人間由来である証明ではない。
Codexの取得は0.160.0のexec由来の形式に限定し、未知版、別session/turn/call、既知の合成入力、壊れた順序、重複キー、上限超過は文脈なしにする。本番入口へ接続済みだが、他の合成入力まで生成確認したものではない。
限定窓へ撤回文を古い順で保持する試験だけでは、指示範囲の評価まで検証できないためEX-advisor-015/016のマークは付けていない。

## 承認された検証方法

EX-advisor-003は限定DELETEを語だけで重大破壊とする質問にしないこと、EX-advisor-004は名前から価値や承認を仮定しないこと、EX-advisor-053は分類定義が独立有害効果を優先することを要求する。
初回の返却後、利用者がA63でこの3例を独立した質問レビューの対象にすることを承認した。
REQ-advisor-002はreviewとなり、REQ-advisor-004の固定質問・データ分離、003の合成、013の分布検証は機械試験のままである。
承認された変更はpolicy.md、助言の決定記録、実装計画、新しい計画決定記録の4文書だけであり、実装者が要求の意味を追加変更したものではない。

今回の固定質問はその意味を表す文章として記述したが、入力を変えても質問が変わらない試験は、文章の意味まで検証しない。
偽のAssessmentを渡す合成試験も、質問が実際にどの分類をモデルへ求めるかの証拠にはしない。
実モデルを呼んでこの不足を埋めることは委譲範囲外であり、モデル精度を構造試験と混同する。
そのため、この3例を既存の構造・合成試験へ便宜上マークしていない。

前回の証拠条件に関する返却理由は解消済みであり、新しい仕様判断を求めているわけではない。
実際に送る固定質問と分類定義の独立レビューは完了し、対象範囲内でPASSだった。REQ-advisor-002、005、008と関連レビュー専用例の意味を照合した結果であり、製品候補全体のレビューや実モデル精度の未評価とは分ける。
キャッシュを必要とする経路が未到達だとした再開時の返却は、公開producerと実ホストの隔離probeによって撤回した。追加の利用者判断やIR改訂は求めていない。
実装途中の未コミットsnapshotや修正前候補の成功を、最終候補の承認の代わりにはしない。現在の結果と残る変更照合は冒頭に記す。

## 接続と依存の確認範囲

TypeSafeの公開API・choice文書を読み、state/model/questionsとchoice応答の形式を照合した。
SDKの推奨retryは採用せず、承認済み仕様のretry 0を保った。
接続はureq 3.4.2とrustlsを使い、gzipとproxy環境取得を無効にし、公開コンストラクターの接続先は固定HTTPSだけである。
loopback URLとHTTP許可は接続クレート内の非公開transportを組み立てる試験だけで使い、guardianの設定やCLIには出していない。
通常の接続クレートは認証値を内部で取得し、本文とAssessmentへは入れない。
依存取得はofflineで成功し、継承したlockfileの既存package版を削除・変更していない。
新規33packageのlicense表記をmetadataで確認し、ureqのMIT本文も確認した。外部ソースコードはコピーしていない。
配布scriptはCargoのlocked/offline/target別graphから非dev依存を辿り、LICENSE・COPYRIGHT・NOTICEを同梱する。ringの入れ子licenseも含む。
MIT OR Apache-2.0を宣言しlicense本文をpackageに含めない既存依存はApache-2.0を選び、ureqが同梱する標準本文を使う。その選択をNOTICEに明記し、それ以外のlicense本文欠落は包装を停止する。

独立担当に渡す実装パスは、固定質問の`crates/guardian-advisor/src/lib.rs`、分類ラベルと共通検証の同ファイル、実際の要求を組み立てる`crates/guardian-advisor-typesafe/src/lib.rs`である。
分類の定義は固定質問に含め、choiceのcriteriaは既知ラベルだけを持つ。
実際の送信本文ではrole、相対順序、確認状態、除外した参照情報と窓の限界を分離した。
この形式と質問の意味について実装者自身がレビュー合格を宣言したものではない。
