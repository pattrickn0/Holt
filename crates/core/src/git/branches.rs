//! Local and remote branches.

use crate::{t, Core};

use super::parse::*;
use super::*;

impl Core {
    /// Local and remote branches.
    pub fn git_branches(&self) -> Result<Vec<GitBranch>, GitError> {
        let repo = self.git_repo()?;
        let fmt = format!("--format={BRANCH_FORMAT}");
        let out = repo.read(&["for-each-ref", &fmt, "refs/heads", "refs/remotes"])?;
        Ok(parse_branches(&out.stdout))
    }

    /// Switches branch. A remote one (`origin/x`) becomes a local branch that tracks it; if the local one
    /// (`x`) already exists, it only switches to it.
    pub fn git_checkout(&self, branch: &GitBranch) -> Result<(), GitError> {
        if !valid_branch_name(&branch.name) {
            return Err(GitError::Failed(t!("Invalid branch name: {name}", name = branch.name)));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        if branch.remote {
            // Local name: what comes after the remote (the longest one that matches; remotes may have `/`).
            let remotes = repo.remotes()?;
            let local = remotes
                .iter()
                .filter_map(|r| branch.name.strip_prefix(r.as_str())?.strip_prefix('/').map(|l| (r.len(), l)))
                .max_by_key(|(len, _)| *len)
                .map(|(_, l)| l)
                .or_else(|| branch.name.split_once('/').map(|(_, l)| l))
                .filter(|l| valid_branch_name(l));
            let exists = |l: &str| run(repo.cmd(&["rev-parse", "--verify", "-q", &format!("refs/heads/{l}")]), None, READ_TIMEOUT).is_ok();
            match local {
                Some(l) if exists(l) => repo.write(&["switch", l], None)?,
                _ => repo.write(&["switch", "--track", &branch.name], None)?,
            };
        } else {
            repo.write(&["switch", &branch.name], None)?;
        }
        Ok(())
    }

    /// Creates a branch from HEAD (and switches to it, if `switch`).
    pub fn git_create_branch(&self, name: &str, switch: bool) -> Result<(), GitError> {
        if !valid_branch_name(name) {
            return Err(GitError::Failed(t!("Invalid branch name: {name}", name)));
        }
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        if switch {
            repo.write(&["switch", "-c", name], None)?;
        } else {
            repo.write(&["branch", name], None)?;
        }
        Ok(())
    }
}
