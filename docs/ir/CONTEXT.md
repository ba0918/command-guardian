# Glossary

| Term | Meaning | Source |
|---|---|---|
| allow | 判定の 1 つ。ガードは口を出さず、コマンドを止めない | docs/decision/records/2026-09-30-hook-guardian-scope.md#A3 |
| ask | 判定の 1 つ。利用者に確認を求める。Codex にはこの出口がない | docs/decision/records/2026-09-30-hook-guardian-scope.md#A3, docs/decision/records/2026-09-30-hook-guardian-scope.md#A24 |
| block | 判定の 1 つ。コマンドを止め、エージェントに理由と代替を返す | docs/decision/records/2026-09-30-hook-guardian-scope.md#A3, docs/decision/records/2026-09-30-hook-guardian-scope.md#A26, docs/decision/records/2026-09-30-hook-guardian-scope.md#A28 |
| ephemeral | パスの分類の 1 つ。一時領域の中にあるか、コマンド自身が作ったパス。消えると戻せないものは無い | docs/decision/records/2026-09-30-hook-guardian-scope.md#A7, docs/decision/records/2026-09-30-hook-guardian-scope.md#A17 |
| vcs | パスの分類の 1 つ。git の作業ツリーの中にあり、そのパスを指定した "git status --porcelain -uall" が何も報告しないもの | docs/decision/records/2026-09-30-hook-guardian-scope.md#A8 |
| protected | パスの分類の 1 つ。システムの領域、ホームそれ自体、作業ディレクトリそれ自体、リポジトリのルート、.git、設定で追加した保護ルート | docs/decision/records/2026-09-30-hook-guardian-scope.md#A17 |
| unknown | パスの分類の 1 つ。上のどれでもなく、消えると戻せない恐れがあるもの | docs/decision/records/2026-09-30-hook-guardian-scope.md#A7 |
| M1 | この IR に書かれた要求の全体を満たす最初のリリース | docs/decision/records/2026-09-30-hook-guardian-scope.md#A6, docs/decision/records/2026-09-30-hook-guardian-scope.md#A23, docs/decision/records/2026-09-30-hook-guardian-scope.md#A41 |
| guardian-parser | コマンドの構文解析を担う crate。OSS のシェルパーサを背後に隠し、訪問済みの正規化した構文木だけを公開する | docs/decision/records/2026-10-01-parser.md#A6 |
| brush-parser | 構文解析に使う OSS のシェルパーサ。`guardian-parser` の背後に隠す | docs/decision/records/2026-10-01-parser.md#A2, docs/decision/records/2026-10-01-parser.md#A6, docs/decision/records/2026-10-01-parser.md#A11 |
