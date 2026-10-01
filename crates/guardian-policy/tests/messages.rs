//! S7: 非 allow の文面（REQ-011）。

use guardian_core::{Ask, Class, Op, ProtectedKind, Target, Why};
use guardian_parser::Failure;
use guardian_policy::message::{ask_message, ask_reason, non_allow_message};
use std::path::PathBuf;

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

// @kotowari[REQ-011, EX-012]
#[test]
fn req_011_protected_message_has_op_path_class_loss_and_alternative() {
    let text = non_allow_message(
        Op::Delete,
        &Target::Path {
            path: PathBuf::from("/etc/nginx"),
            dereference: false,
        },
        Class::Protected,
        &Why::Protected(ProtectedKind::SystemArea),
    );
    let ls = lines(&text);
    assert!((2..=4).contains(&ls.len()), "{text}");
    assert!(text.contains("Delete"), "{text}");
    assert!(text.contains("/etc/nginx"), "{text}");
    assert!(text.contains("protected"), "{text}");
    assert!(text.contains("cannot be recovered"), "{text}");
    assert!(text.contains("Alternative:"), "{text}");
}

// @kotowari[REQ-011]
#[test]
fn req_011_reason_and_alternative_per_kind() {
    let cases: Vec<(Why, &str, &str)> = vec![
        (Why::Untracked, "untracked", "commit"),
        (Why::Uncommitted, "uncommitted", "Commit"),
        (
            Why::Unresolved("$X".to_string()),
            "unresolved",
            "literal path",
        ),
        (Why::Unmanaged, "not managed by git", "Move it"),
        (Why::UnknownSource, "cannot be determined", "literal"),
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
    assert!(text.contains("No applicable alternative"), "{text}");
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

// @kotowari[REQ-011, REQ-039]
#[test]
fn req_011_self_caused_deaths_ask_with_the_internal_reason() {
    // 自分に帰せる死（panic、起動とプロトコルの失敗）の ask は、block ではなく
    // 理由に判定の内部で失敗した を出す（REQ-011・REQ-039・A23）。
    for failure in [Failure::Panic, Failure::Internal] {
        let asks = vec![Ask::Parse(failure.clone())];
        assert!(!asks[0].is_limit(), "{failure:?}");
        let reason = ask_reason(&asks);
        assert!(reason.contains("internal failure"), "{failure:?}: {reason}");
        let text = ask_message(&asks);
        assert!(text.contains("internal failure"), "{failure:?}: {text}");
        let ls = lines(&text);
        assert!((2..=4).contains(&ls.len()), "{failure:?}: {text}");
    }
}
