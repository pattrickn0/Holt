//! The commit history (log and detail).

use crate::{t, Core};

use super::parse::*;
use super::*;

impl Core {
    /// Lists commits of the current branch (paged by `skip` and `limit`). Returns `(commits, has_more)`.
    pub fn git_log(&self, skip: usize, limit: usize) -> Result<(Vec<GitCommit>, bool), GitError> {
        let repo = self.git_repo()?;
        if !repo.has_head() {
            return Ok((Vec::new(), false));
        }
        let limit = limit.clamp(1, 200);
        let fetch_n = format!("-n{}", limit + 1);
        let skip_arg = format!("--skip={skip}");
        let fmt = format!("--format={LOG_FORMAT}");
        let out = repo.read(&["log", "-z", &fetch_n, &skip_arg, &fmt])?;
        let mut list = parse_log(&out.stdout);
        let has_more = list.len() > limit;
        if has_more {
            list.truncate(limit);
        }
        Ok((list, has_more))
    }

    /// Detail of a commit: metadata, changed files and the whole diff.
    pub fn git_commit_detail(&self, hash: &str) -> Result<GitCommitDetail, GitError> {
        if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GitError::Failed(t!("Invalid commit hash").to_owned()));
        }
        let repo = self.git_repo()?;
        let meta_out = repo.read(&["show", "-s", "--format=%H%x00%h%x00%an%x00%ae%x00%cr%x00%B", hash])?;
        let meta_str = String::from_utf8_lossy(&meta_out.stdout);
        let fields: Vec<&str> = meta_str.splitn(6, '\0').collect();
        if fields.len() < 6 {
            return Err(GitError::Failed(t!("Could not read the commit data").to_owned()));
        }
        let files_out = repo.read(&["diff-tree", "--root", "-r", "--no-commit-id", "--name-status", "-z", hash])?;
        let mut files = Vec::new();
        let mut parts = files_out.stdout.split(|b| *b == 0).filter(|p| !p.is_empty());
        while let Some(st) = parts.next() {
            let st_s = String::from_utf8_lossy(st);
            let letter = st_s.chars().next().unwrap_or('M');
            if matches!(letter, 'R' | 'C') {
                let _old = parts.next();
            }
            if let Some(path) = parts.next() {
                files.push((letter, String::from_utf8_lossy(path).into_owned()));
            }
        }
        let diff_out = repo.read(&["show", "--format=", "--patch", "--no-color", "--no-ext-diff", "-M", hash])?;
        let cut = diff_out.stdout.len() > DIFF_MAX_BYTES;
        let bytes = &diff_out.stdout[..diff_out.stdout.len().min(DIFF_MAX_BYTES)];
        let mut diff = parse_diff(&String::from_utf8_lossy(bytes), DIFF_MAX_LINES);
        diff.truncated |= cut;
        Ok(GitCommitDetail {
            hash: fields[0].to_owned(),
            short_hash: fields[1].to_owned(),
            author: fields[2].to_owned(),
            email: fields[3].to_owned(),
            date: fields[4].to_owned(),
            message: fields[5].trim_end().to_owned(),
            files,
            diff,
        })
    }
}
