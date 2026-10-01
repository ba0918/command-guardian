//! レビューの入力コーパス。grammar.rs と、実行ファイルを通す不変テスト
//! （tests/isolation.rs）が共有する。

/// 文法要素のコーパス（REQ-037・A9）。
pub const CORPUS: &[&str] = &[
    // コマンド列・パイプ・and/or
    "echo a b c; pwd && ls || true; false & echo done",
    "find . -name '*.rs' -delete | xargs rm -f",
    "echo one\necho two\n",
    // 単純コマンド・代入・引用
    "a=1 b=2 cmd arg1 arg2",
    "IFS=, read a b <<< \"x\"",
    "echo \"quoted 'text'\" 'single \"text\"'",
    "echo foo\\ bar a\\*b",
    "echo ~; echo ~root; echo ~+/x",
    "echo {a,b,c}",
    // 複合構文
    "if true; then rm -rf /etc/x; elif false; then echo y; else echo z; fi",
    "while read -r line; do echo \"$line\"; done < input.txt",
    "until false; do echo x; done",
    "for i in a b c; do echo \"$i\"; done",
    "for ((i=0; i<3; i++)); do echo \"$i\"; done",
    "case \"$x\" in a|b) echo ab;; c) echo c;;& esac",
    "{ echo a; echo b; }",
    "( cd /tmp && rm -rf . )",
    "f() { echo \"$1\"; }; f x",
    "coproc NAME { echo hi; }",
    "time rm -rf /etc/x",
    "! false",
    // リダイレクト
    "cat < in.txt > out.txt >> log.txt 2>&1 <> rw.txt >| clobber.txt",
    "cat &> both.txt && cat &>> append.txt",
    "cat <<EOF\n$(rm -rf /etc/x)\nEOF",
    "cat <<-'EOF'\n\t$HOME\n\tEOF",
    "cat <<'EOF'\n$(rm -rf /etc/x)\nEOF",
    "cat <<< \"here $(echo x)\"",
    // 置換・算術・展開
    "echo $(date) `whoami` $((1+2)) ${HOME} $x",
    "echo \"pre $(rm -rf /etc/x) post\"",
    "echo <(ls) >(cat)",
    "[[ -n \"$(echo x)\" && -f /tmp/x ]]",
    "((1+2))",
    // 配列
    "a=(x y z); echo \"${a[0]}\"",
    // 走査漏れの事例として知られている位置
    "bash -c 'rm -rf /etc/x' extra args",
    "sudo -u \"$(rm -rf /etc/x)\" true",
    "sudo -u root bash -lc 'rm -rf /etc/x'",
    // 入れ子の置換
    "echo $(echo $(echo $(pwd)))",
    // 展開のオペランド・算術・配列の中の置換（走査漏れの位置）
    "echo ${X:-$(rm -rf /etc/x)}",
    "echo ${X:-${Y:-$(rm -rf /etc/x)}}",
    "echo $(( $(rm -rf /etc/x) + 1 ))",
    "(( $(rm -rf /etc/x) ))",
    "for ((i=0; i<$(rm -rf /etc/x); i++)); do :; done",
    "echo ${a[$(rm -rf /etc/x)]}",
    "echo ${a[$(rm -rf /etc/x)]:-y}",
    "a=($(rm -rf /etc/x))",
    // 二重引用の中の置換（同じ位置の別の綴り）
    "echo ${X:-\"$(rm -rf /etc/x)\"}",
    "echo ${X:=\"$(rm -rf /etc/x)\"}",
    "echo $(( \"$(rm -rf /etc/x)\" + 1 ))",
    "a=(\"$(rm -rf /etc/x)\")",
];
