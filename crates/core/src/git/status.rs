//! Status, diff, stage, discard and hunks.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{t, Core};

use super::exec::{nul_paths, read_capped, run, Repo};
use super::parse::*;
use super::*;

impl Core {
    /// Path of the git executable (or `None`).
    pub(crate) fn git_exe(&self) -> Option<PathBuf> {
        let state = &self.inner.git;
        let mut cache = state.exe.lock();
        if let Some((at, exe)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(30) {
                return exe.clone();
            }
        }
        let mut exe = self.inner.agents.which(if cfg!(windows) { "git.exe" } else { "git" });
        if exe.is_none() && cfg!(windows) {
            exe = self.inner.agents.system_dir("ProgramFiles").map(|p| p.join("Git").join("cmd").join("git.exe")).filter(|p| p.is_file());
        }
        *cache = Some((Instant::now(), exe.clone()));
        exe
    }

    /// Finds the repository of the workspace.
    pub(super) fn git_repo(&self) -> Result<Repo, GitError> {
        self.git_repo_at(&self.workspace())
    }

    /// Finds the repository that contains the folder `dir` (the one of the tasks, which are per project and not only
    /// of the active project).
    pub(crate) fn git_repo_at(&self, dir: &Path) -> Result<Repo, GitError> {
        let git = self.git_exe().ok_or(GitError::NotInstalled)?;
        let ws = dir;
        if ws.as_os_str().is_empty() {
            return Err(GitError::NotARepo);
        }
        let out =
            run(base_command(&git, ws, &["rev-parse", "--show-toplevel", "--absolute-git-dir", "--show-prefix"]), None, READ_TIMEOUT)?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines = text.lines();
        let root = lines.next().map(PathBuf::from).ok_or(GitError::NotARepo)?;
        let git_dir = lines.next().map(PathBuf::from).unwrap_or_else(|| root.join(".git"));
        let prefix = lines.next().unwrap_or("").to_owned();
        Ok(Repo { git, root, git_dir, prefix })
    }

    /// Status of the workspace's repository.
    pub fn git_status(&self) -> Result<GitStatus, GitError> {
        let repo = self.git_repo()?;
        let out = repo.read(&["status", "--porcelain=v2", "-z", "--branch", "--show-stash", "--untracked-files=normal"])?;
        let mut st = parse_status_v2(&out.stdout);
        st.state = repo.state();
        st.prefix = repo.prefix.clone();
        st.root = repo.root;
        Ok(st)
    }

    /// Diff of a file (`path` relative to the repository root). `staged`: what is staged
    /// (index × HEAD); otherwise, the working copy × index. Untracked file: the whole content.
    pub fn git_diff(&self, entry: &GitEntry, staged: bool) -> Result<GitDiff, GitError> {
        let repo = self.git_repo()?;
        if !safe_rel(&entry.path) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path = entry.path)));
        }
        if entry.kind == EntryKind::Untracked {
            let full = repo.root.join(&entry.path);
            if full.is_dir() {
                return Ok(GitDiff::default());
            }
            let bytes = read_capped(&full, DIFF_MAX_BYTES)
                .map_err(|e| GitError::Failed(t!("Failed to read {path}: {error}", path = entry.path, error = e)))?;
            let mut d = untracked_diff(&bytes.0, DIFF_MAX_LINES);
            d.truncated |= bytes.1;
            return Ok(d);
        }
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(&entry.path);
        if let Some(orig) = entry.orig_path.as_deref().filter(|o| safe_rel(o)) {
            args.push(orig);
        }
        let out = run(repo.cmd(&args), None, READ_TIMEOUT)?;
        let cut = out.stdout.len() > DIFF_MAX_BYTES;
        let bytes = &out.stdout[..out.stdout.len().min(DIFF_MAX_BYTES)];
        let mut d = parse_diff(&String::from_utf8_lossy(bytes), DIFF_MAX_LINES);
        d.truncated |= cut;
        Ok(d)
    }

    /// Stages the paths (relative to the root).
    pub fn git_stage(&self, paths: &[String]) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.write(&["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"], Some(nul_paths(paths)?))?;
        Ok(())
    }

    /// Stages every change in the repository.
    pub fn git_stage_all(&self) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.write(&["add", "-A"], None)?;
        Ok(())
    }

    /// Unstages. A renamed file takes the old name with it (otherwise the old name would stay
    /// staged as deleted).
    pub fn git_unstage(&self, paths: &[String]) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let stdin = nul_paths(paths)?;
        if repo.has_head() {
            let st = parse_status_v2(&repo.read(&["status", "--porcelain=v2", "-z", "--untracked-files=no"])?.stdout);
            let mut all = paths.to_vec();
            for e in st.entries.iter().filter(|e| paths.contains(&e.path)) {
                if let Some(orig) = e.orig_path.as_ref().filter(|o| !all.contains(o)) {
                    all.push(orig.clone());
                }
            }
            repo.write(&["restore", "--staged", "--pathspec-from-file=-", "--pathspec-file-nul"], Some(nul_paths(&all)?))?;
        } else {
            // With no commit at all there is no HEAD to restore from: it only leaves the index.
            repo.write(&["rm", "--cached", "-r", "-q", "--pathspec-from-file=-", "--pathspec-file-nul"], Some(stdin))?;
        }
        Ok(())
    }

    /// Unstages everything.
    pub fn git_unstage_all(&self) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        if repo.has_head() {
            repo.write(&["reset", "-q"], None)?;
        } else {
            // Empty index: nothing to unstage (without `--ignore-unmatch` git would fail with "did not match").
            repo.write(&["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--", "."], None)?;
        }
        Ok(())
    }

    /// Discards the changes of a file:
    /// - untracked: DELETES the file or folder (uses `-ffdq` on folders to also remove sub-repositories with an inner `.git`);
    /// - in conflict (`Unmerged`): restores to the `HEAD` version (or removes it if it was only added by the other side, e.g. `AU`/`DU`/`UU`);
    /// - ordinary tracked file: restores the working copy to what is in the index.
    pub fn git_discard(&self, entry: &GitEntry) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        if !safe_rel(&entry.path) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path = entry.path)));
        }
        let _op = self.inner.git.op.lock();
        match entry.kind {
            EntryKind::Untracked => {
                let is_dir = entry.path.ends_with('/') || repo.root.join(&entry.path).is_dir();
                let flags = if is_dir { "-ffdq" } else { "-fq" };
                repo.write(&["clean", flags, "--", &entry.path], None)?;
            }
            EntryKind::Unmerged => {
                // In a conflicted file, `restore --worktree` fails because there are multiple stages in the index.
                // If the file exists in HEAD, `restore --source=HEAD --staged --worktree` restores our clean side;
                // if it does not exist in HEAD (e.g. added by them `UA` or by both `AA`), we remove it from the index and from disk.
                let in_head = run(repo.cmd(&["cat-file", "-e", &format!("HEAD:{}", entry.path)]), None, READ_TIMEOUT).is_ok();
                if in_head {
                    repo.write(&["restore", "--source=HEAD", "--staged", "--worktree", "--", &entry.path], None)?;
                } else {
                    repo.write(&["rm", "-f", "-q", "--", &entry.path], None)?;
                }
            }
            _ => {
                repo.write(&["restore", "--worktree", "--", &entry.path], None)?;
            }
        }
        Ok(())
    }

    /// Applies the patch of a hunk to stage it, unstage it
    /// or discard it from the working copy.
    pub fn git_apply_hunk(&self, patch: &str, staged: bool, discard: bool) -> Result<(), GitError> {
        if patch.trim().is_empty() {
            return Err(GitError::Failed(t!("Empty hunk").to_owned()));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let args: &[&str] = match (discard, staged) {
            // Discard an unstaged hunk from the working copy.
            (true, _) => &["apply", "--reverse", "--recount", "-"],
            // Unstage a hunk that was staged.
            (false, true) => &["apply", "--cached", "--reverse", "--recount", "-"],
            // Stage a hunk from the working copy.
            (false, false) => &["apply", "--cached", "--recount", "-"],
        };
        repo.write(args, Some(patch.as_bytes().to_vec()))?;
        Ok(())
    }
}
