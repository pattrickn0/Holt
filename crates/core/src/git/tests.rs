//! Parsers and the error wording, without a repository.

use super::*;

fn z(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in parts {
        out.extend_from_slice(p.as_bytes());
        out.push(0);
    }
    out
}

#[test]
fn status_of_a_fresh_repository() {
    let st = parse_status_v2(&z(&["# branch.oid (initial)", "# branch.head main", "? new file.txt"]));
    assert_eq!((st.head, st.branch.as_deref(), st.upstream, st.ahead, st.behind), (None, Some("main"), None, 0, 0));
    assert_eq!(
        st.entries,
        [GitEntry { path: "new file.txt".into(), orig_path: None, index: '?', worktree: '?', kind: EntryKind::Untracked }]
    );
}

#[test]
fn status_with_upstream_renames_conflicts_and_spaces() {
    let out = z(&[
        "# branch.oid 1234567890abcdef",
        "# branch.head feat/x",
        "# branch.upstream origin/feat/x",
        "# branch.ab +2 -3",
        "# stash 4",
        "1 .M N... 100644 100644 100644 aaaa bbbb a b.txt",
        "1 A. N... 000000 100644 100644 0000 cccc naïve/ü.txt",
        "2 R. N... 100644 100644 100644 dddd eeee R100 new name.rs",
        "old name.rs",
        "u UU N... 100644 100644 100644 100644 h1 h2 h3 conflito.md",
        "? dir/",
    ]);
    let st = parse_status_v2(&out);
    assert_eq!(
        (st.head.as_deref(), st.branch.as_deref(), st.upstream.as_deref()),
        (Some("1234567890abcdef"), Some("feat/x"), Some("origin/feat/x"))
    );
    assert_eq!((st.ahead, st.behind, st.stash_count), (2, 3, 4));
    let e = &st.entries;
    assert_eq!(e.len(), 5);
    assert_eq!((e[0].path.as_str(), e[0].index, e[0].worktree), ("a b.txt", '.', 'M'));
    assert!(!e[0].staged() && e[0].unstaged());
    assert_eq!((e[1].path.as_str(), e[1].index), ("naïve/ü.txt", 'A'));
    assert!(e[1].staged() && !e[1].unstaged());
    assert_eq!((e[2].path.as_str(), e[2].orig_path.as_deref(), e[2].kind), ("new name.rs", Some("old name.rs"), EntryKind::Renamed));
    assert_eq!((e[3].path.as_str(), e[3].kind), ("conflito.md", EntryKind::Unmerged));
    assert!(!e[3].staged() && !e[3].unstaged());
    assert_eq!((e[4].path.as_str(), e[4].kind), ("dir/", EntryKind::Untracked));
}

#[test]
fn detached_head_and_invalid_utf8() {
    let mut out = z(&["# branch.oid abc", "# branch.head (detached)"]);
    out.extend_from_slice(b"? arq\xff.txt\0");
    let st = parse_status_v2(&out);
    assert_eq!(st.branch, None);
    assert_eq!(st.entries[0].path, "arq\u{fffd}.txt");
}

#[test]
fn too_many_entries_are_truncated() {
    let lines: Vec<String> = (0..STATUS_MAX_ENTRIES + 3).map(|i| format!("? f{i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let st = parse_status_v2(&z(&refs));
    assert_eq!(st.entries.len(), STATUS_MAX_ENTRIES);
    assert!(st.truncated);
}

#[test]
fn branches_skip_the_remote_head() {
    let out = "refs/heads/main\0main\0origin/main\0*\nrefs/heads/feat\0feat\0\0 \nrefs/remotes/origin/HEAD\0origin\0\0 \nrefs/remotes/origin/main\0origin/main\0\0 \n";
    let b = parse_branches(out.as_bytes());
    assert_eq!(
        b.iter().map(|b| (b.name.as_str(), b.remote, b.current)).collect::<Vec<_>>(),
        [("main", false, true), ("feat", false, false), ("origin/main", true, false)]
    );
    assert_eq!(b[0].upstream.as_deref(), Some("origin/main"));
    assert_eq!(b[1].upstream, None);
}

#[test]
fn diff_lines_are_numbered() {
    let text = "diff --git a/a.rs b/a.rs\nindex 1..2 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -10,3 +10,4 @@ fn x()\n ctx\n-old\n+new\n+more\n ctx2\n\\ No newline at end of file\n";
    let d = parse_diff(text, 100);
    let k: Vec<(DiffKind, Option<u32>, Option<u32>)> = d.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
    assert_eq!(
        k,
        [
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Hunk, None, None),
            (DiffKind::Context, Some(10), Some(10)),
            (DiffKind::Del, Some(11), None),
            (DiffKind::Add, None, Some(11)),
            (DiffKind::Add, None, Some(12)),
            (DiffKind::Context, Some(12), Some(13)),
            (DiffKind::Meta, None, None),
        ]
    );
    assert!(!d.binary && !d.truncated);
    let d = parse_diff("diff --git a/x b/x\nBinary files a/x and b/x differ\n", 100);
    assert!(d.binary);
    let d = parse_diff(text, 3);
    assert!(d.truncated && d.lines.len() == 3);
}

#[test]
fn combined_diff_cc_is_parsed_correctly() {
    let text = "diff --cc a.txt\nindex 111,222..000\n--- a/a.txt\n+++ b/a.txt\n@@@ -1,3 -1,3 +1,5 @@@\n  um\n++<<<<<<< HEAD\n +main\n++=======\n+ feat\n++>>>>>>> feat/x\n";
    let d = parse_diff(text, 100);
    let k: Vec<(DiffKind, Option<u32>, Option<u32>)> = d.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
    assert_eq!(
        k,
        [
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Meta, None, None),
            (DiffKind::Hunk, None, None),
            (DiffKind::Context, Some(1), Some(1)),
            (DiffKind::Add, None, Some(2)),
            (DiffKind::Add, None, Some(3)),
            (DiffKind::Add, None, Some(4)),
            (DiffKind::Add, None, Some(5)),
            (DiffKind::Add, None, Some(6)),
        ]
    );
    // A combined diff does not produce an individual hunk patch.
    assert!(extract_hunk_patch(&d, 0).is_none());
}

#[test]
fn extract_single_hunk_patch() {
    let text = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-um\n+UM\n dois\n@@ -10,2 +10,2 @@\n nove\n-dez\n+DEZ\n";
    let d = parse_diff(text, 100);
    let p0 = extract_hunk_patch(&d, 0).expect("hunk 0");
    assert!(p0.contains("@@ -1,2 +1,2 @@") && !p0.contains("@@ -10,2 +10,2 @@"));
    let p1 = extract_hunk_patch(&d, 1).expect("hunk 1");
    assert!(p1.contains("@@ -10,2 +10,2 @@") && !p1.contains("@@ -1,2 +1,2 @@"));
    assert!(extract_hunk_patch(&d, 2).is_none());
}

#[test]
fn conflict_blocks_are_parsed_and_resolved() {
    let content = "inicio\n<<<<<<< HEAD\nnosso 1\nnosso 2\n=======\ndeles 1\n>>>>>>> feat/x\nmeio\n<<<<<<< HEAD\nA\n||||||| base\nO\n=======\nB\n>>>>>>> feat/x\nfim\n";
    let blocks = parse_conflict_blocks(content);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].start_line, 2);
    assert_eq!(blocks[0].ours_label, "HEAD");
    assert_eq!(blocks[0].ours, ["nosso 1", "nosso 2"]);
    assert_eq!(blocks[0].theirs_label, "feat/x");
    assert_eq!(blocks[0].theirs, ["deles 1"]);
    assert_eq!(blocks[1].ours, ["A"]);
    assert_eq!(blocks[1].theirs, ["B"]);

    let after0 = resolve_conflict_block_in_text(content, 0, ConflictChoice::Ours).expect("resolve 0");
    assert_eq!(after0, "inicio\nnosso 1\nnosso 2\nmeio\n<<<<<<< HEAD\nA\n||||||| base\nO\n=======\nB\n>>>>>>> feat/x\nfim\n");
    let after1 = resolve_conflict_block_in_text(&after0, 0, ConflictChoice::Both).expect("resolve 1");
    assert_eq!(after1, "inicio\nnosso 1\nnosso 2\nmeio\nA\nB\nfim\n");
}

#[test]
fn stashes_and_log_are_parsed() {
    let st = parse_stashes(b"stash@{0}\x002 hours ago\0On main: rascunho\nstash@{1}\x003 days ago\0WIP on feat: x\n");
    assert_eq!(
        st,
        [
            GitStash { index: 0, date: "2 hours ago".into(), message: "On main: rascunho".into() },
            GitStash { index: 1, date: "3 days ago".into(), message: "WIP on feat: x".into() },
        ]
    );
    let log_bytes = b"aaaa1111\0aaaa\0Ana\x002 hours ago\0feat: x\0\nbbbb2222\0bbbb\0Bia\0yesterday\0fix: y\0";
    let commits = parse_log(log_bytes);
    assert_eq!(
        commits,
        [
            GitCommit {
                hash: "aaaa1111".into(),
                short_hash: "aaaa".into(),
                author: "Ana".into(),
                date: "2 hours ago".into(),
                subject: "feat: x".into()
            },
            GitCommit {
                hash: "bbbb2222".into(),
                short_hash: "bbbb".into(),
                author: "Bia".into(),
                date: "yesterday".into(),
                subject: "fix: y".into()
            },
        ]
    );
}

#[test]
fn untracked_files_show_every_line_or_binary() {
    let d = untracked_diff(b"a\nb\n", 100);
    assert_eq!(
        d.lines.iter().map(|l| (l.kind, l.new, l.text.as_str())).collect::<Vec<_>>(),
        [(DiffKind::Add, Some(1), "a"), (DiffKind::Add, Some(2), "b")]
    );
    assert!(untracked_diff(b"PK\0\x03", 100).binary);
}

#[test]
fn auth_failures_from_git_and_gh_are_recognized() {
    for s in [
        "fatal: could not read Username for 'https://github.com': No such device or address",
        "remote: Invalid username or token. Password authentication is not supported for Git operations.\nfatal: Authentication failed for 'https://github.com/o/r.git/'",
        "fatal: unable to access 'https://github.com/o/r.git/': The requested URL returned error: 403",
        "fatal: unable to access 'https://github.com/o/r.git/': The requested URL returned error: 401",
        "The token in default is invalid.\nTo re-authenticate, run: gh auth login -h github.com",
        "gh: HTTP 401: Bad credentials (https://api.github.com/graphql)",
    ] {
        assert!(matches!(classify_error(s), GitError::Auth(_)), "{s}");
    }
    assert!(matches!(classify_error("error: pathspec 'x' did not match"), GitError::Failed(_)));
}

#[test]
fn signin_plan_by_platform() {
    let SigninPlan::Command(c) = signin_plan(false, true, false) else { panic!() };
    assert!(c.starts_with("gh auth login") && c.contains("-s workflow") && c.ends_with("gh auth setup-git"));
    assert_eq!(signin_plan(true, true, true), SigninPlan::Command("git credential-manager github login".into()));
    assert!(matches!(signin_plan(true, true, false), SigninPlan::Command(c) if c.starts_with("gh auth login")));
    assert!(matches!(signin_plan(false, false, true), SigninPlan::Install(_)));
}

#[test]
fn errors_are_recognized() {
    assert_eq!(classify_error("fatal: not a git repository (or any of the parent directories): .git"), GitError::NotARepo);
    assert_eq!(
        classify_error("fatal: detected dubious ownership in repository at '/srv/x'\nTo add an exception…"),
        GitError::Unsafe { path: "/srv/x".into() }
    );
    assert_eq!(classify_error("*** Please tell me who you are.\n\nRun\n"), GitError::NoIdentity);
    assert!(matches!(
        classify_error("fatal: could not read Username for 'https://github.com': terminal prompts disabled"),
        GitError::Auth(_)
    ));
    let GitError::Auth(ssh_msg) =
        classify_error("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.")
    else {
        panic!("ssh auth")
    };
    assert!(ssh_msg.contains("ssh-agent") && ssh_msg.contains("ssh-add") && ssh_msg.contains("Start-Service ssh-agent"), "{ssh_msg}");
    assert!(matches!(classify_error(" ! [rejected]        main -> main (fetch first)\nerror: failed to push"), GitError::Rejected(_)));
    assert!(matches!(classify_error("fatal: Not possible to fast-forward, aborting."), GitError::Rejected(_)));
    assert!(matches!(
        classify_error("fatal: Cannot prompt because user interactivity has been disabled.\nfatal: could not read Username"),
        GitError::Auth(_)
    ));
    let GitError::Failed(m) = classify_error("error: gpg failed to sign the data\nfatal: failed to write commit object") else {
        panic!("gpg")
    };
    assert!(m.contains("commit.gpgsign"), "{m}");
    assert_eq!(classify_error("error: pathspec 'x' did not match"), GitError::Failed("error: pathspec 'x' did not match".into()));
}

#[test]
fn branch_names_and_paths_are_checked() {
    for ok in ["main", "feat/x", "fix-1.2", "user/ana/test"] {
        assert!(valid_branch_name(ok), "{ok}");
    }
    for bad in ["", "-f", "a b", "a..b", "a~1", "a^", "x:y", "a.lock", "a/", "/a", ".hidden", "a/.b", "@", "a@{1}", "a\\b", "a*"] {
        assert!(!valid_branch_name(bad), "{bad:?}");
    }
    assert!(safe_rel("src/main.rs") && safe_rel("a b/naïve.txt") && safe_rel("./a") && safe_rel("dir/"));
    for bad in ["", "/etc/passwd", "/x", "../x", "a/../b", "a/../../x", "a\\..\\b", "\\\\server\\x", "\\x", "a\0b"] {
        assert!(!safe_rel(bad), "{bad:?}");
    }
    // Relative to the drive: `C:..\x` would leave the repository root.
    if cfg!(windows) {
        for bad in ["C:..\\x", "C:x", "C:\\x", "a:b"] {
            assert!(!safe_rel(bad), "{bad:?}");
        }
    }
}

#[test]
fn errors_are_explained_in_the_current_language() {
    assert!(GitError::NotInstalled.to_string().contains("git-scm.com"));
    assert!(GitError::Unsafe { path: "/x".into() }.to_string().contains("safe.directory \"/x\""));
    let pt = crate::i18n::with_lang(crate::i18n::Lang::PtBr, || GitError::NotARepo.to_string());
    assert_eq!(pt, "A pasta de trabalho não é um repositório git.");
}
