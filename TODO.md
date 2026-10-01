# TODO

`m1-guard-cli`（コマンドの読み方の再実装）を main にマージした時点の残件。
詳細な記録は findings ファイル（`.agents/artifacts/reviews/m1-guard-cli.json`、git 管理外）にある。
このファイルはその写しとして、判断待ちと次の作業を並べたもの。

## 判断待ち（仕様）

- [ ] **#23 `/dev/null` などへのリダイレクト**: 切り詰め（truncate）として block するのは仕様どおりか。変えるならブレスト（現状: block）
- [ ] **#33 配列の代入 `a=(1 2)`**: 非リテラルのプログラム語として ask になる（`declare -a a=(1 2)` は ask にならない）。配列の値を読むか、ask のままとするか
- [ ] **FLAG-001（承認時）**: 照合を 3 回で打ち切った 6 件の修正内容を直接確認する

## 記録のみ（info・低優先）

- [ ] #13 `crates/guardian-policy/src/guard.rs` のコメントが存在しない REQ-044 を参照（A44 が正）
- [ ] #17 `find -files0-from FILE` を読まない
- [ ] #24 sudo/doas 以外のラッパー（exec・env・timeout・nice・su）を外さない
- [ ] #25 `rm -rf /` の文面が `//` と表示される
- [ ] 潜在ギャップ: parse スレッドを大きくすると here-doc 内の算術の数え漏れが露出する（現構成では子の死で block に見える）。数え上げの穴は未修正

## 検証・リリース

- [ ] **S17**: メッセージと遅延の体感レビュー（人間）
- [ ] **S19・配備**: リリース（Linux x86_64 の単体バイナリ、GitHub Releases + mise）と、dotfiles のフック差し替え（`block_dangerous_command.py` → `hook-guardian`）。git remote が無いため未実施
- [ ] 実測 355 コマンドのコーパスでの再判定（コーパスはリポジトリに無い）

## 将来の方向（決まったら小さな決定から）

- [ ] **LLM 助言層（2 層化）**: 機械判定の残渣（ask）を安価で高速なモデルに投げる。プラグ可能必須・既定オフ・利用者層の設定のみ・失敗時は機械判定に戻す・tighten-only を既定に。再利用候補は `toys-toolchain-guardrail` と `toys-core::Judge`
- [ ] 外部アクセス（curl・wget など）の扱い: 現状は効果の対象外（柵の範囲。強制は kakoi 側）。利用者設定の `[[commands.guard]]` で ask にはできる
- [ ] `$PWD/rm` のような既知変数経由のプログラム語を読むか（現状 ask。利用者から見た挙動は再実装前と同じ）

## 既知の境界（仕様どおり）

- 不活性な深い算術の入れ子は allow（危険な効果は深さに関係なく拾う）
- 読めない構文・非 POSIX シェルは ask、想定を超える入力は理由つき block（A22・A23）
