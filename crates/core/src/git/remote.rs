//! Commit, fetch, pull and push.

use crate::{t, Core};

use super::exec::{run, summary};
use super::parse::*;
use super::*;

impl Core {
    /// Commits what is staged. Returns `"<hash> <subject>"`.
    pub fn git_commit(&self, message: &str, amend: bool) -> Result<String, GitError> {
        if message.trim().is_empty() {
            return Err(GitError::Failed(t!("Write the commit message").to_owned()));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        repo.check_gpg_ready()?;
        let mut args = vec!["commit", "--cleanup=strip", "-F", "-"];
        if amend {
            args.push("--amend");
        }
        run(repo.cmd(&args), Some(message.as_bytes().to_vec()), COMMIT_TIMEOUT)?;
        let out = repo.read(&["log", "-1", "--format=%h %s"])?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// Message of the last commit (for "amend last commit" or a merge message).
    pub fn git_last_message(&self) -> Option<String> {
        let repo = self.git_repo().ok()?;
        let out = repo.read(&["log", "-1", "--format=%B"]).ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
    }

    pub fn git_fetch(&self) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let out = run(repo.net(&["fetch", "--all", "--prune"]), None, NET_TIMEOUT)?;
        Ok(summary(&out))
    }

    /// Pull only by fast-forward (without creating a merge by itself).
    pub fn git_pull(&self) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let out = run(repo.net(&["pull", "--ff-only", "--no-edit"]), None, NET_TIMEOUT)?;
        Ok(summary(&out))
    }

    /// Push. With no upstream, publishes the branch to the only remote (`push -u <remote> <branch>`).
    pub fn git_push(&self) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let st = parse_status_v2(&repo.read(&["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=no"])?.stdout);
        let out = if st.upstream.is_some() {
            run(repo.net(&["push"]), None, NET_TIMEOUT)?
        } else {
            let branch = st.branch.ok_or_else(|| GitError::Failed(t!("Detached HEAD: switch to a branch before pushing").to_owned()))?;
            let remotes = repo.remotes()?;
            let remote = match remotes.as_slice() {
                [] => return Err(GitError::Failed(t!("No remote configured (git remote add origin <url>)").to_owned())),
                [one] => one.clone(),
                many if many.iter().any(|r| r == "origin") => "origin".into(),
                _ => {
                    return Err(GitError::Failed(
                        t!("Several remotes and no upstream: publish from the terminal (git push -u <remote> <branch>)").to_owned(),
                    ))
                }
            };
            run(repo.net(&["push", "-u", &remote, &branch]), None, NET_TIMEOUT)?
        };
        Ok(summary(&out))
    }
}
