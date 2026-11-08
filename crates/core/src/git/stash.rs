//! Stash entries.

use crate::{t, Core};

use super::exec::{run, summary};
use super::parse::*;
use super::*;

impl Core {
    // ---------- Stash ----------

    /// Lists the repository's stashes.
    pub fn git_stash_list(&self) -> Result<Vec<GitStash>, GitError> {
        let repo = self.git_repo()?;
        let fmt = format!("--format={STASH_FORMAT}");
        let out = repo.read(&["stash", "list", &fmt])?;
        Ok(parse_stashes(&out.stdout))
    }

    /// Saves the current changes to the stash (`include_untracked`: includes untracked files `-u`).
    pub fn git_stash_push(&self, message: &str, include_untracked: bool) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
        let msg = message.trim();
        if !msg.is_empty() {
            args.push("-m");
            args.push(msg);
        }
        // With no paths of our own: `stash -u` calls `git clean` inside, and with literal pathspecs
        // it would leave the untracked files in place after saving them.
        let mut cmd = repo.cmd(&args);
        cmd.env_remove("GIT_LITERAL_PATHSPECS");
        let out = run(cmd, None, WRITE_TIMEOUT)?;
        let text = summary(&out);
        if text.to_lowercase().contains("no local changes to save") {
            return Err(GitError::Failed(t!("No local change to save in the stash").to_owned()));
        }
        Ok(text)
    }

    /// Full diff of a stash (`stash@{index}`), including untracked files saved in it.
    pub fn git_stash_diff(&self, index: usize) -> Result<GitDiff, GitError> {
        let repo = self.git_repo()?;
        let spec = format!("stash@{{{index}}}");
        let out = repo.read(&["stash", "show", "-p", "--no-color", "--no-ext-diff", &spec])?;
        let mut raw = out.stdout;
        // When the stash was saved with `--include-untracked`, the untracked files stay in the
        // third parent (`stash@{n}^3`).
        let untracked_spec = format!("{spec}^3");
        if let Ok(u) = repo.read(&["show", "--format=", "--patch", "--no-color", "--no-ext-diff", &untracked_spec]) {
            if !u.stdout.is_empty() {
                if !raw.is_empty() && !raw.ends_with(b"\n") {
                    raw.push(b'\n');
                }
                raw.extend_from_slice(&u.stdout);
            }
        }
        let cut = raw.len() > DIFF_MAX_BYTES;
        let bytes = &raw[..raw.len().min(DIFF_MAX_BYTES)];
        let mut d = parse_diff(&String::from_utf8_lossy(bytes), DIFF_MAX_LINES);
        d.truncated |= cut;
        Ok(d)
    }

    /// Applies a stash without deleting it (`git stash apply stash@{index}`).
    pub fn git_stash_apply(&self, index: usize) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let spec = format!("stash@{{{index}}}");
        let out = repo.write(&["stash", "apply", "-q", &spec], None)?;
        Ok(summary(&out))
    }

    /// Applies a stash and removes it from the list (`git stash pop stash@{index}`).
    pub fn git_stash_pop(&self, index: usize) -> Result<String, GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let spec = format!("stash@{{{index}}}");
        let out = repo.write(&["stash", "pop", "-q", &spec], None)?;
        Ok(summary(&out))
    }

    /// Deletes a stash (`git stash drop stash@{index}`).
    pub fn git_stash_drop(&self, index: usize) -> Result<(), GitError> {
        let repo = self.git_repo()?;
        let _op = self.inner.git.op.lock();
        let spec = format!("stash@{{{index}}}");
        repo.write(&["stash", "drop", "-q", &spec], None)?;
        Ok(())
    }

    // ---------- History (log) ----------
}
