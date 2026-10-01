//! S4: シェル起動の認識（REQ-035）。

use guardian_analysis::{shell_invocation, ShellInvocation};
use guardian_parser::{parse, Command, Word};

/// 入力の最初の単純コマンドの語。
fn words(command: &str) -> Vec<Word> {
    let outcome = parse(command);
    assert!(
        outcome.failures.is_empty(),
        "{command}: {:?}",
        outcome.failures
    );
    let item = outcome.script.items.first().expect("項目がある");
    match item.first.commands.first().expect("コマンドがある") {
        Command::Simple(simple) => simple.words.clone(),
        other => panic!("単純コマンドではない: {other:?}"),
    }
}

fn body(command: &str) -> ShellInvocation {
    shell_invocation(&words(command))
}

fn bash_body(command: &str) -> Option<Word> {
    match body(command) {
        ShellInvocation::BashLike { body } => body,
        other => panic!("bash 系ではない: {other:?}"),
    }
}

// @kotowari[REQ-035]
#[test]
fn req_035_bash_like_shells_with_c_read_the_body() {
    for program in ["bash", "sh", "dash", "zsh", "ksh", "mksh", "/bin/bash"] {
        let command = format!("{program} -c 'rm -rf /etc/x'");
        let word = bash_body(&command).expect("本体の語がある");
        assert_eq!(
            word.literal_value().as_deref(),
            Some("rm -rf /etc/x"),
            "{command}"
        );
    }
    // "-lc" のまとめ書きも "-c" として扱う。
    let word = bash_body("bash -lc 'rm -rf /etc/x'").expect("本体の語がある");
    assert_eq!(word.literal_value().as_deref(), Some("rm -rf /etc/x"));
    // 値にオプション名を取るものは、その値も読み飛ばす。
    let word = bash_body("bash -o pipefail -c 'rm -rf /etc/x'").expect("本体の語がある");
    assert_eq!(word.literal_value().as_deref(), Some("rm -rf /etc/x"));
    let word = bash_body("bash --rcfile /dev/null -c 'rm -rf /etc/x'").expect("本体の語がある");
    assert_eq!(word.literal_value().as_deref(), Some("rm -rf /etc/x"));
}

// @kotowari[REQ-035]
#[test]
fn req_035_shells_without_c_have_no_body() {
    for command in [
        "bash script.sh",
        "sh < file",
        "bash --version",
        "zsh -x script.zsh",
        // ファイル起動の後の `-c` はスクリプトへの引数。
        "bash script.sh -c 'rm -rf /etc/x'",
        // `--` の後はオプションとして読まない。
        "bash -- -c 'rm -rf /etc/x'",
    ] {
        let word = bash_body(command);
        assert!(word.is_none(), "{command}");
    }
}

// @kotowari[REQ-035]
#[test]
fn req_035_non_posix_shells_are_known_regardless_of_c() {
    for program in ["fish", "csh", "tcsh", "elvish", "xonsh", "nu", "pwsh"] {
        for command in [
            program.to_string(),
            format!("{program} -c 'rm -rf /etc/x'"),
            format!("{program} script.fish"),
        ] {
            assert_eq!(body(&command), ShellInvocation::NonPosix, "{command}");
        }
    }
}

// @kotowari[REQ-035]
#[test]
fn req_035_unknown_programs_are_not_shells() {
    for command in ["mytool -c 'rm -rf /etc/x'", "sudo -c 'x'", "echo bash -c"] {
        assert_eq!(body(command), ShellInvocation::Other, "{command}");
    }
}

// @kotowari[REQ-035]
#[test]
fn req_035_wrappers_are_stripped_before_recognition() {
    for command in [
        "sudo bash -c 'rm -rf /etc/x'",
        "doas bash -c 'rm -rf /etc/x'",
        "sudo -u root bash -c 'rm -rf /etc/x'",
        "sudo -u \"$(rm -rf /etc/x)\" bash -c 'rm -rf /tmp/y'",
        "doas -u root sh -c 'rm -rf /etc/x'",
    ] {
        assert!(bash_body(command).is_some(), "{command}");
    }
    assert_eq!(
        body("sudo fish -c 'rm -rf /etc/x'"),
        ShellInvocation::NonPosix
    );
}

// @kotowari[REQ-035]
#[test]
fn req_035_a_non_literal_body_is_returned_unread() {
    for command in ["bash -c \"$CMD\"", "sh -c $S"] {
        let word = bash_body(command).expect("本体の語がある");
        assert!(word.literal_value().is_none(), "{command}");
    }
}
