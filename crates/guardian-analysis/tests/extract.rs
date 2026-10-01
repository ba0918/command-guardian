//! S1: 破壊的効果の抽出（REQ-001）。

mod common;
use common::{analyze, extract_effects};
use guardian_core::{Effect, Env, Op, Target};
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

// @kotowari[REQ-001, REQ-037]
#[test]
fn req_001_prefix_assignment_substitutions_have_effects_with_a_program() {
    assert_eq!(
        effects("X=$(rm -rf /etc/x) true"),
        vec![Effect {
            op: Op::Delete,
            target: path("/etc/x")
        }]
    );
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

// @kotowari[REQ-001, REQ-008, REQ-009]
#[test]
fn req_001_find_and_xargs_rm_keep_fixed_targets_as_well_as_source_children() {
    for command in [
        "find /tmp/scratch -exec rm -rf /etc/x {} +",
        "find /tmp/scratch -type f | xargs rm -rf /etc/x",
    ] {
        let effects = effects(command);
        assert_eq!(effects.len(), 2, "{command}: {effects:?}");
        assert!(effects.contains(&Effect {
            op: Op::Delete,
            target: path("/etc/x"),
        }));
        assert!(effects.contains(&Effect {
            op: Op::Delete,
            target: Target::Children {
                base: PathBuf::from("/tmp/scratch"),
                dereference: false,
            },
        }));
    }
}

// @kotowari[REQ-001, REQ-002, REQ-008]
#[test]
fn req_008_execdir_relative_targets_use_the_source_directory_not_parent_cwd() {
    let children = Effect {
        op: Op::Delete,
        target: Target::Children {
            base: PathBuf::from("/tmp/scratch"),
            dereference: false,
        },
    };
    assert_eq!(
        effects("find /tmp/scratch -type f -execdir rm -f victim {} \\;"),
        vec![
            Effect {
                op: Op::Delete,
                target: Target::Unresolved("victim".into()),
            },
            children.clone(),
        ]
    );
    assert_eq!(
        effects("find /tmp/scratch -exec rm victim {} +"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/home/you/work/repo/victim")
            },
            children.clone()
        ]
    );
    assert_eq!(
        effects("find /tmp/scratch -execdir rm /etc/x {} +"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/etc/x")
            },
            children
        ]
    );
    assert!(effects("find /tmp/scratch -execdir rm ../outside {} +")
        .iter()
        .any(|effect| matches!(effect.target, Target::Unresolved(_))));
}

// @kotowari[REQ-001, REQ-002, REQ-008]
#[test]
fn req_002_execdir_fixed_relative_targets_cannot_be_absorbed_into_source_children() {
    for command in [
        "find /tmp -execdir rm -rf etc {} +",
        "find /tmp /var/tmp -execdir rm -rf etc {} +",
        "find /tmp -execdir rm -rf ./etc {} +",
        "find /tmp -execdir rm -rf etc/* {} +",
        "S=etc; find /tmp -execdir rm -rf \"$S\" {} +",
    ] {
        let result = effects(command);
        assert!(
            result
                .iter()
                .any(|effect| matches!(effect.target, Target::Unresolved(_))),
            "{command}: {result:?}"
        );
        assert!(
            result
                .iter()
                .any(|effect| matches!(effect.target, Target::Children { .. })),
            "{command}: {result:?}"
        );
    }
}

// @kotowari[REQ-001, REQ-002]
#[test]
fn req_002_execdir_expanded_absolute_targets_remain_separate_from_source_children() {
    let result = effects("S=/etc/x; find /tmp -execdir rm -rf \"$S\" {} +");
    assert!(result.contains(&Effect {
        op: Op::Delete,
        target: path("/etc/x")
    }));
    assert!(
        !result
            .iter()
            .any(|effect| matches!(effect.target, Target::Unresolved(_))),
        "{result:?}"
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
            target: Target::Children {
                base: PathBuf::from("/tmp/scratch"),
                dereference: false,
            }
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
            target: Target::Children {
                base: PathBuf::from("/home/you/work/repo"),
                dereference: false,
            }
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
            target: Target::Children {
                base: PathBuf::from("/tmp/scratch"),
                dereference: false,
            }
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

// @kotowari[REQ-001, EX-010]
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

// @kotowari[REQ-001]
#[test]
fn req_001_dd_quoted_of_prefix_is_read() {
    // `of=` が引用やエスケープで分かれていても、dd が受け取る綴りで読む。
    for cmd in [
        "dd \"of=/tmp/scratch/img\"",
        "dd 'of=''/tmp/scratch/img'",
        "dd of\\=/tmp/scratch/img",
    ] {
        assert_eq!(
            effects(cmd),
            vec![Effect {
                op: Op::Truncate,
                target: path("/tmp/scratch/img")
            }],
            "{cmd}"
        );
    }
    // ブロックデバイスへの書き込みは形式の効果。
    assert_eq!(
        effects("dd \"of=/dev/sda\""),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sda")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_option_values_are_not_targets() {
    // shred の値付きオプション（-n/--iterations、-s/--size）は削除の対象ではない。
    assert_eq!(
        effects("shred -n 3 /tmp/scratch/x"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/x")
        }]
    );
    assert_eq!(
        effects("shred --iterations=3 /tmp/scratch/y"),
        vec![Effect {
            op: Op::Delete,
            target: path("/tmp/scratch/y")
        }]
    );
    // mkfs の -L/--label も形式の対象ではない。
    assert_eq!(
        effects("mkfs.ext4 -L mylabel /dev/sdb1"),
        vec![Effect {
            op: Op::Format,
            target: path("/dev/sdb1")
        }]
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_without_a_start_point_uses_cwd() {
    for cmd in ["find -name foo -delete", "find -maxdepth 2 -delete"] {
        assert_eq!(
            effects(cmd),
            vec![Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/home/you/work/repo"),
                    dereference: false,
                },
            }],
            "{cmd}"
        );
    }
    // 先頭の全体オプション（-L など）は読み飛ばす。
    assert_eq!(
        effects("find -L . -name foo -delete"),
        vec![Effect {
            op: Op::Delete,
            target: Target::Children {
                base: PathBuf::from("/home/you/work/repo"),
                dereference: false,
            },
        }]
    );
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_start_point_after_leading_options() {
    for cmd in [
        "find -- /etc -delete",
        "find -L -- /etc -delete",
        "find -O3 /etc -delete",
        // -D の値は起点ではない。
        "find -D search /etc -delete",
    ] {
        assert_eq!(
            effects(cmd),
            vec![Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/etc"),
                    dereference: false,
                },
            }],
            "{cmd}"
        );
    }
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_debug_help_does_not_search() {
    // -D help はデバッグ一覧を出して終わるので、-delete が付いていても削除しない。
    assert_eq!(effects("find -D help /etc -delete"), vec![]);
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_after_dashdash_uses_cwd() {
    // `--` は全体オプションの終わりだけを告げる。その次の語が述語の始まり
    // （-x、(、!）なら、起点は既定の "." になる。
    for cmd in [
        "find -- -delete",
        "find -- -name x -delete",
        "find -- ! -name x -delete",
        "find -- '(' -name x ')' -delete",
    ] {
        assert_eq!(
            effects(cmd),
            vec![Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/home/you/work/repo"),
                    dereference: false,
                },
            }],
            "{cmd}"
        );
    }
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_with_multiple_start_points_takes_each_children() {
    // 述語の前の語はいくつでも起点になる。起点ごとの子が対象になる。
    assert_eq!(
        effects("find /tmp/scratch /etc -delete"),
        vec![
            Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/tmp/scratch"),
                    dereference: false,
                },
            },
            Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/etc"),
                    dereference: false,
                },
            },
        ]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_substitution_in_an_option_value_is_read() {
    // 値付きオプションの値になっているコマンド置換も読む。値そのものは対象にしない。
    assert_eq!(
        effects("shred -n \"$(rm -rf /etc/x)\" /tmp/y"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/etc/x")
            },
            Effect {
                op: Op::Delete,
                target: path("/tmp/y")
            }
        ]
    );
    assert_eq!(
        effects("truncate -s \"$(rm -rf /etc/x)\" /tmp/y"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/etc/x")
            },
            Effect {
                op: Op::Truncate,
                target: path("/tmp/y")
            }
        ]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_substitution_in_a_find_argument_is_read() {
    // 起点でない find の引数の中のコマンド置換も読む。絞り込みの値は対象にしない。
    assert_eq!(
        effects("find /tmp/scratch -name \"$(rm -rf /etc/x)\" -delete"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/etc/x")
            },
            Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/tmp/scratch"),
                    dereference: false,
                },
            }
        ]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_substitution_in_a_dd_argument_is_read() {
    // of= 以外の dd の引数の中のコマンド置換も読む。
    assert_eq!(
        effects("dd if=$(rm -rf /etc/x) of=/tmp/y"),
        vec![
            Effect {
                op: Op::Delete,
                target: path("/etc/x")
            },
            Effect {
                op: Op::Truncate,
                target: path("/tmp/y")
            }
        ]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_inout_redirect_is_not_truncate() {
    // <> は読み書きの両方向のリダイレクトで、切り詰めではない。
    assert_eq!(effects("echo x <> /etc/x"), vec![]);
    // >| は切り詰めのまま。
    assert_eq!(
        effects("echo x >| /etc/x"),
        vec![Effect {
            op: Op::Truncate,
            target: path("/etc/x")
        }]
    );
}

// @kotowari[REQ-001]
#[test]
fn req_001_fd_less_duplicate_output_to_a_filename_truncates() {
    // `>& file` は `&> file` の別の綴りで、ファイルを切り詰める。
    assert_eq!(
        effects("echo x >& /etc/foo"),
        vec![Effect {
            op: Op::Truncate,
            target: path("/etc/foo")
        }]
    );
    // 数字の語はファイル記述子への複製で、切り詰めではない。
    assert_eq!(effects("echo x >&1"), vec![]);
    assert_eq!(effects("echo x >&-"), vec![]);
}

// @kotowari[REQ-005]
#[test]
fn req_005_parent_dir_at_the_root_is_clamped() {
    // ルートより上へ出る綴りは、ルートで止めて解決する。
    for cmd in ["rm -rf /../etc", "rm -rf /tmp/../../etc"] {
        assert_eq!(
            effects(cmd),
            vec![Effect {
                op: Op::Delete,
                target: path("/etc")
            }],
            "{cmd}"
        );
    }
}

// @kotowari[REQ-010]
#[test]
fn req_010_a_trailing_wrapper_option_is_not_a_command() {
    // 値の無い値付きオプションが末尾でも、panic せず効果なしになる。
    for cmd in [
        "sudo -u",
        "sudo -p",
        "sudo --user",
        "sudo -g",
        "doas -a",
        "doas -C",
    ] {
        assert_eq!(effects(cmd), vec![], "{cmd}");
    }
}

// @kotowari[EX-047]
#[test]
fn ex_047_substitution_in_a_wrapper_option_value_is_read() {
    // 値を取るオプションの値の中のコマンド置換も読む。
    assert_eq!(
        effects("sudo -u \"$(rm -rf /etc/x)\" true"),
        vec![Effect {
            op: Op::Delete,
            target: path("/etc/x")
        }]
    );
}

// @kotowari[EX-048]
#[test]
fn ex_048_shell_c_body_is_read_as_nested_syntax() {
    assert_eq!(
        effects("bash -c 'rm -rf /etc/x'"),
        vec![Effect {
            op: Op::Delete,
            target: path("/etc/x")
        }]
    );
}

// @kotowari[EX-054]
#[test]
fn ex_054_bundled_c_body_is_read() {
    assert_eq!(
        effects("bash -lc 'rm -rf /etc/x'"),
        vec![Effect {
            op: Op::Delete,
            target: path("/etc/x")
        }]
    );
}

// @kotowari[EX-055]
#[test]
fn ex_055_script_file_launch_is_not_read() {
    // 本体がコマンド文字列の外にある起動は読まない。
    assert_eq!(effects("bash script.sh"), vec![]);
    assert_eq!(effects("sh < file"), vec![]);
    // `-c` はオプションの並びの中にだけ現れる。ファイル起動の後の `-c` は
    // スクリプトへの引数で、本体ではない（REQ-035）。
    assert_eq!(effects("bash script.sh -c 'rm -rf /etc/x'"), vec![]);
}

// @kotowari[REQ-008]
#[test]
fn req_008_find_with_multiple_start_points_feeds_xargs() {
    // パイプの供給元にも起点の数だけの子の集合が流れる。
    assert_eq!(
        effects("find /tmp/scratch /etc -type f | xargs rm -f"),
        vec![
            Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/tmp/scratch"),
                    dereference: false,
                },
            },
            Effect {
                op: Op::Delete,
                target: Target::Children {
                    base: PathBuf::from("/etc"),
                    dereference: false,
                },
            },
        ]
    );
}

// @kotowari[REQ-006, REQ-037]
#[test]
fn req_037_subscript_operand_is_read_in_textual_order() {
    // 添字はほかのオペランドより前の原文にある。順序が崩れると断片の位置が
    // 取れなくなり、効果が消えて ask に落ちる（REQ-006・REQ-037）。
    let analysis = analyze("rm -rf ${a[1]:-y}", &env());
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(
        analysis.effects,
        vec![Effect {
            op: Op::Delete,
            target: Target::Unresolved("${a[1]:-y}".to_string())
        }]
    );

    for cmd in ["echo ${a[1]:-y}", "echo ${a[$x]:-y}", "echo ${a[b]:-y}"] {
        let analysis = analyze(cmd, &env());
        assert!(
            analysis.diagnostics.is_empty(),
            "{cmd}: {:?}",
            analysis.diagnostics
        );
    }

    let analysis = analyze("rm -rf ${a[$(rm -rf /etc/x)]:-y}", &env());
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert!(
        analysis.effects.contains(&Effect {
            op: Op::Delete,
            target: path("/etc/x")
        }),
        "{:?}",
        analysis.effects
    );
}

// @kotowari[REQ-001, REQ-037]
#[test]
fn req_037_substitutions_in_expansion_operands_are_read() {
    // パラメータ展開のオペランド、算術、添字、配列の値の中の置換も読む。
    for cmd in [
        "echo ${X:-$(rm -rf /etc/x)}",
        "echo ${X:-${Y:-$(rm -rf /etc/x)}}",
        "echo $(( $(rm -rf /etc/x) + 1 ))",
        "(( $(rm -rf /etc/x) ))",
        "for ((i=0; i<$(rm -rf /etc/x); i++)); do :; done",
        "echo ${a[$(rm -rf /etc/x)]}",
        "declare -a a=($(rm -rf /etc/x))",
        // 二重引用の中に入れ子の置換が現れる綴り（REQ-037）。
        "echo ${X:-\"$(rm -rf /etc/x)\"}",
        "echo ${X:=\"$(rm -rf /etc/x)\"}",
        "X=abc; echo ${X#\"$(rm -rf /etc/x)\"}",
        "X=abc; echo ${X/zzz/\"$(rm -rf /etc/x)\"}",
        "echo $(( \"$(rm -rf /etc/x)\" + 1 ))",
        "(( \"$(rm -rf /etc/x)\" ))",
        "declare -a a=(\"$(rm -rf /etc/x)\")",
    ] {
        let analysis = analyze(cmd, &env());
        assert_eq!(
            analysis.effects,
            vec![Effect {
                op: Op::Delete,
                target: path("/etc/x")
            }],
            "{cmd}"
        );
        assert!(
            analysis.diagnostics.is_empty(),
            "{cmd}: {:?}",
            analysis.diagnostics
        );
    }
}
