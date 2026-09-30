//! S1: 破壊的効果の抽出（REQ-001）。

use guardian_core::{extract_effects, Effect, Env, Op, Target};
use std::path::PathBuf;

fn env() -> Env {
    Env {
        home: Some(PathBuf::from("/home/you")),
        tmpdir: Some(PathBuf::from("/tmp")),
        cwd: Some(PathBuf::from("/home/you/work/repo")),
    }
}

fn path(p: &str) -> Target {
    Target::Path {
        path: PathBuf::from(p),
        dereference: false,
    }
}

fn effects(command: &str) -> Vec<Effect> {
    extract_effects(command, &env())
}

// @kotowari[REQ-001]
#[test]
fn req_001_rm_delete() {
    assert_eq!(
        effects("rm -rf /tmp/scratch/x"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/x")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_rmdir_unlink_shred_delete() {
    assert_eq!(
        effects("rmdir -p /tmp/scratch/dir"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/dir")
        }]
    );
    assert_eq!(
        effects("unlink /tmp/scratch/f"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/f")
        }]
    );
    assert_eq!(
        effects("shred -u /tmp/scratch/secret"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/secret")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_find_delete_takes_children_of_base() {
    assert_eq!(
        effects("find /tmp/scratch -delete"),
        vec![Effect {
            op: Op::Delete,
            target: Target::Children(PathBuf::from("/tmp/scratch"))
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_find_exec_rm_takes_children_of_base() {
    assert_eq!(
        effects("find . -name __pycache__ -exec rm -rf {} +"),
        vec![Effect {
            op: Op::Delete,
            target: Target::Children(PathBuf::from("/home/you/work/repo"))
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_xargs_rm_takes_source_children() {
    assert_eq!(
        effects("find /tmp/scratch -type f | xargs rm -f"),
        vec![Effect {
            op: Op::Delete,
            target: Target::Children(PathBuf::from("/tmp/scratch"))
        }]
    );
    assert_eq!(
        effects("xargs rm -f"),
        vec![Effect {
            op: Op::Delete,
            target: Target::UnknownSource
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_redirect_is_truncate() {
    assert_eq!(
        effects("echo log > /tmp/scratch/log"),
        vec![Effect {
            op: Op::Truncate,
            target: path("/tmp/scratch/log")
        }]
    );
    // >> は切り詰めではない。
    assert_eq!(effects("echo log >> /tmp/scratch/log"), vec![]);
}

// @kotowari[REQ-001]
#[test]
fn req_001_truncate_command() {
    assert_eq!(
        effects("truncate -s 0 /tmp/scratch/f"),
        vec![Effect {
            op: Op::Truncate,
            target: path("/tmp/scratch/f")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_dd_of_file_and_block_device() {
    assert_eq!(
        effects("dd if=/dev/zero of=/tmp/scratch/img bs=1M count=1"),
        vec![Effect {
            op: Op::Truncate,
            target: path("/tmp/scratch/img")
        }]
    );
    assert_eq!(
        effects("dd if=/dev/zero of=/dev/sdb bs=1M"),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sdb")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_mkfs_and_wipefs_are_format() {
    assert_eq!(
        effects("mkfs.ext4 /dev/sdb1"),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sdb1")
        }]
    );
    assert_eq!(
        effects("mkfs -t ext4 /dev/sdb1"),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sdb1")
        }]
    );
    assert_eq!(
        effects("wipefs -a /dev/sdb"),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sdb")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_quotes_and_operators_are_read() {
    assert_eq!(
        effects("S=/tmp/scratch; rm -rf \"$S\"; echo done && rm -f /tmp/scratch/b"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/tmp/scratch")
            },
            Effect {
                op: Op::Delete,
                target: path("/tmp/scratch/b")
            }
        ]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_heredoc_body_is_not_a_command() {
    let cmd = "rm -rf /tmp/scratch/x <<EOF\nrm -rf /etc\nEOF";
    assert_eq!(
        effects(cmd),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/x")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_command_substitution_is_read() {
    assert_eq!(
        effects("rm -rf $(mktemp -d)"),
        vec![Effect {
            op: Op::Delete,
            target: Target::Mktemp
        }]
    );
    assert_eq!(
        effects("echo $(rm -rf /tmp/scratch/y)"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/y")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_sudo_and_doas_wrappers_are_read() {
    assert_eq!(
        effects("sudo rm -rf /tmp/scratch/x"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/x")
        }]
    );
    assert_eq!(
        effects("sudo -u root rm -rf /tmp/scratch/y"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/y")
        }]
    );
    assert_eq!(
        effects("doas rm -rf /tmp/scratch/z"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/z")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_bash_c_and_eval_are_read() {
    assert_eq!(
        effects("bash -c 'rm -rf /tmp/scratch/x'"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/x")
        }]
    );
    assert_eq!(
        effects("sh -c \"rm -rf /tmp/scratch/y\""),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/y")
        }]
    );
    assert_eq!(
        effects("eval \"rm -rf /tmp/scratch/z\""),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/z")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_four_excluded_operations_have_no_effects() {
    assert_eq!(effects("git clean -fdx"), vec![]);
    assert_eq!(effects("mv /tmp/scratch/a /tmp/scratch/b"), vec![]);
    assert_eq!(effects("cp /tmp/scratch/a /tmp/scratch/b"), vec![]);
    assert_eq!(effects("sed -i 's/a/b/' /tmp/scratch/f"), vec![]);
    assert_eq!(
        effects("rsync --delete /tmp/scratch/a/ /tmp/scratch/b/"),
        vec![]
    );
}
