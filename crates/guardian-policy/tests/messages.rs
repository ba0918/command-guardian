//! S7: 非 allow の文面（REQ-011）。

use guardian_core::{extract_effects, Class, Env, Op, ProtectedKind, Target, Why};
use guardian_policy::message::non_allow_message;
use std::path::PathBuf;

fn env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

// @kotowari[REQ-011, EX-012]
#[test]
fn req_011_protected_message_has_op_path_class_loss_and_alternative() {
    let effects = extract_effects("rm -rf /etc/nginx", &env());
    assert_eq!(effects.len(), 1);
    let e = &effects[0];
    let text = non_allow_message(
        e.op,
        &e.target,
        Class::Protected,
        &Why::Protected(ProtectedKind::SystemArea),
    );
    let ls = lines(&text);
    assert!((2..=4).contains(&ls.len()), "{text}");
    assert!(text.contains("削除"), "{text}");
    assert!(text.contains("/etc/nginx"), "{text}");
    assert!(text.contains("protected"), "{text}");
    assert!(text.contains("戻せません"), "{text}");
    assert!(text.contains("代替:"), "{text}");
}

// @kotowari[REQ-011]
#[test]
fn req_011_reason_and_alternative_per_kind() {
    let cases: Vec<(Why, &str, &str)> = vec![
        (Why::Untracked, "未追跡", "コミット"),
        (Why::Uncommitted, "未コミット", "コミット"),
        (
            Why::Unresolved("$X".to_string()),
            "解決できない",
            "リテラルのパス",
        ),
        (Why::Unmanaged, "管理外", "移してから消す"),
        (Why::UnknownSource, "確定できない", "リテラル"),
        (Why::GitFailed, "git", "git status"),
    ];
    for (why, reason_word, alternative_word) in cases {
        let target = Target::Path {
            path: PathBuf::from("/home/you/work/repo/x"),
            dereference: false,
        };
        let text = non_allow_message(Op::Delete, &target, Class::Unknown, &why);
        let ls = lines(&text);
        assert!((2..=4).contains(&ls.len()), "{text}");
        assert!(text.contains(reason_word), "{text}");
        assert!(text.contains(alternative_word), "{text}");
        assert!(text.contains("unknown"), "{text}");
    }
    // 保護領域に当てはまる代替が無いときは、無いと書く。
    let text = non_allow_message(
        Op::Delete,
        &Target::Path {
            path: PathBuf::from("/etc/nginx"),
            dereference: false,
        },
        Class::Protected,
        &Why::Protected(ProtectedKind::SystemArea),
    );
    assert!(text.contains("当てはまる代替はありません"), "{text}");
}

// @kotowari[REQ-011]
#[test]
fn req_011_messages_fit_two_to_four_lines_for_every_target_shape() {
    let targets = vec![
        Target::Path {
            path: PathBuf::from("/tmp/x"),
            dereference: false,
        },
        Target::GlobBase(PathBuf::from("/tmp/x")),
        Target::Children {
            base: PathBuf::from("/tmp/x"),
            dereference: false,
        },
        Target::Mktemp,
        Target::UnknownSource,
        Target::Unresolved("$X".to_string()),
    ];
    for t in targets {
        for op in [Op::Delete, Op::Truncate, Op::Format] {
            let text = non_allow_message(op, &t, Class::Unknown, &Why::Unmanaged);
            let ls = lines(&text);
            assert!((2..=4).contains(&ls.len()), "{text}");
        }
    }
}
