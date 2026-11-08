//! Merge, rebase, conflict resolution and `git init`.

use crate::process::{run_captured_raw, Exit};
use crate::{t, Core};

use super::exec::{base_command, read_capped, run, summary, write_atomic};
use super::parse::*;
use super::*;

impl Core {
    // ---------- Merge and Rebase ----------

    /// Merges the branch `branch` into the current branch (`git merge --no-edit <branch>`).
    /// When it goes into conflict, returns `Ok` with a warning (the `Merging` state and the conflicts section
    /// appear in the status).
    pub fn git_merge(&self, branch: &str) -> Result<String, GitError> {
        if !valid_branch_name(branch) {
            return Err(GitError::Failed(t!("Invalid branch name: {name}", name = branch)));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.check_gpg_ready()?;
        let out = run_captured_raw(repo.cmd(&["merge", "--no-edit", branch]), None, WRITE_TIMEOUT, None, None, MAX_OUT)
            .map_err(|e| GitError::Failed(t!("Failed to run git: {error}", error = e)))?;
        match out.exit {
            Exit::Code(Some(0)) => Ok(summary(&out)),
            Exit::TimedOut => Err(GitError::Timeout),
            _ if repo.state() == RepoState::Merging => {
                Ok(t!("Merge with conflicts: resolve the files in conflict and click Continue.").to_owned())
            }
            _ => Err(classify_error(&String::from_utf8_lossy(&out.stderr))),
        }
    }

    /// Rebase of the current branch onto `branch` (`git rebase <branch>`).
    /// When it stops on a conflict, returns `Ok` with a warning (the `Rebasing` state appears at the top).
    pub fn git_rebase(&self, branch: &str) -> Result<String, GitError> {
        if !valid_branch_name(branch) {
            return Err(GitError::Failed(t!("Invalid branch name: {name}", name = branch)));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.check_gpg_ready()?;
        let out = run_captured_raw(repo.cmd(&["rebase", branch]), None, WRITE_TIMEOUT, None, None, MAX_OUT)
            .map_err(|e| GitError::Failed(t!("Failed to run git: {error}", error = e)))?;
        match out.exit {
            Exit::Code(Some(0)) => Ok(summary(&out)),
            Exit::TimedOut => Err(GitError::Timeout),
            _ if repo.state() == RepoState::Rebasing => {
                Ok(t!("Rebase paused on a conflict: resolve the conflicts and click Continue.").to_owned())
            }
            _ => Err(classify_error(&String::from_utf8_lossy(&out.stderr))),
        }
    }

    /// Continues the operation in progress (`merge --continue`, `rebase --continue`, `cherry-pick --continue`, `revert --continue`).
    pub fn git_continue_op(&self) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.check_gpg_ready()?;
        let sub = match repo.state() {
            RepoState::Merging => "merge",
            RepoState::Rebasing => "rebase",
            RepoState::CherryPicking => "cherry-pick",
            RepoState::Reverting => "revert",
            RepoState::Normal => return Err(GitError::Failed(t!("No operation in progress").to_owned())),
        };
        let out = run(repo.cmd(&[sub, "--continue"]), None, COMMIT_TIMEOUT)?;
        Ok(summary(&out))
    }

    /// Aborts the operation in progress (`merge --abort`, `rebase --abort`, `cherry-pick --abort`, `revert --abort`).
    pub fn git_abort_op(&self) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let sub = match repo.state() {
            RepoState::Merging => "merge",
            RepoState::Rebasing => "rebase",
            RepoState::CherryPicking => "cherry-pick",
            RepoState::Reverting => "revert",
            RepoState::Normal => return Err(GitError::Failed(t!("No operation in progress").to_owned())),
        };
        let out = repo.write(&[sub, "--abort"], None)?;
        Ok(summary(&out))
    }

    // ---------- Conflicts ----------

    /// Reads the conflict blocks (`<<<<<<<` … `>>>>>>>`) of a file in conflict.
    pub fn git_conflict_file(&self, path: &str) -> Result<GitConflictFile, GitError> {
        let repo = self.git_repo()?;
        if !safe_rel(path) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path)));
        }
        let full = repo.root.join(path);
        if !full.is_file() {
            return Ok(GitConflictFile { path: path.to_owned(), blocks: Vec::new(), binary: false });
        }
        let (bytes, _cut) =
            read_capped(&full, CONFLICT_MAX_BYTES).map_err(|e| GitError::Failed(t!("Failed to read {path}: {error}", path, error = e)))?;
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Ok(GitConflictFile { path: path.to_owned(), blocks: Vec::new(), binary: true });
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(GitConflictFile { path: path.to_owned(), blocks: parse_conflict_blocks(&text), binary: false })
    }

    /// Resolves the whole file in conflict by choosing our side (`Ours` → `--ours`), theirs
    /// (`Theirs` → `--theirs`) or both (`Both`, resolving every block of the file), and already
    /// stages it (`git add`) as resolved.
    pub fn git_resolve_conflict_file(&self, path: &str, choice: ConflictChoice) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        if !safe_rel(path) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path)));
        }
        let _op = self.inner.git.op.lock();
        match choice {
            ConflictChoice::Ours | ConflictChoice::Theirs => {
                let side = if choice == ConflictChoice::Ours { "--ours" } else { "--theirs" };
                // If the chosen side exists in the index (stage 2 = ours, 3 = theirs), `checkout <side>`
                // restores it; if the file was deleted on that side (e.g. a modify/delete conflict),
                // `git rm` applies the removal.
                let stage = if choice == ConflictChoice::Ours { "2" } else { "3" };
                let has_stage = repo
                    .read(&["ls-files", "-u", "--", path])
                    .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.split_whitespace().nth(2) == Some(stage)));
                if has_stage {
                    repo.write(&["checkout", side, "--", path], None)?;
                    repo.write(&["add", "--", path], None)?;
                } else {
                    repo.write(&["rm", "-f", "-q", "--", path], None)?;
                }
            }
            ConflictChoice::Both => {
                let full = repo.root.join(path);
                let text =
                    std::fs::read_to_string(&full).map_err(|e| GitError::Failed(t!("Failed to read {path}: {error}", path, error = e)))?;
                let mut cur = text;
                while let Some(next) = resolve_conflict_block_in_text(&cur, 0, ConflictChoice::Both) {
                    cur = next;
                }
                write_atomic(&full, cur.as_bytes())
                    .map_err(|e| GitError::Failed(t!("Failed to write {path}: {error}", path, error = e)))?;
                repo.write(&["add", "--", path], None)?;
            }
        }
        Ok(())
    }

    /// Resolves a single block (`block_index`, 0-indexed) of a file in conflict.
    pub fn git_resolve_conflict_block(&self, path: &str, block_index: usize, choice: ConflictChoice) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        if !safe_rel(path) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path)));
        }
        let _op = self.inner.git.op.lock();
        let full = repo.root.join(path);
        let text = std::fs::read_to_string(&full).map_err(|e| GitError::Failed(t!("Failed to read {path}: {error}", path, error = e)))?;
        let new_text = resolve_conflict_block_in_text(&text, block_index, choice)
            .ok_or_else(|| GitError::Failed(t!("Conflict block not found (the file changed)").to_owned()))?;
        write_atomic(&full, new_text.as_bytes()).map_err(|e| GitError::Failed(t!("Failed to write {path}: {error}", path, error = e)))?;
        Ok(())
    }

    /// `git init` in the workspace (never in the user's home folder).
    pub fn git_init(&self) -> Result<(), GitError> {
        let git = self.git_exe().ok_or(GitError::NotInstalled)?;
        let ws = self.workspace();
        if ws.as_os_str().is_empty() {
            return Err(GitError::Failed(t!("No project open").to_owned()));
        }
        if crate::util::same_path(&crate::util::absolutize(&ws), &crate::util::absolutize(&self.inner.home)) {
            return Err(GitError::Failed(t!("I do not create a repository in your home folder; choose a project folder").to_owned()));
        }
        run(base_command(&git, &ws, &["init", "-q"]), None, WRITE_TIMEOUT)?;
        Ok(())
    }
}
