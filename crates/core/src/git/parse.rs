//! Types and pure parsers: the output of `git` is read here, tested without git installed.

use std::path::{Component, Path, PathBuf};

use crate::t;

use super::STATUS_MAX_ENTRIES;

// ---------- Types ----------

/// What git returned about the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    /// Repository root.
    pub root: PathBuf,
    /// The workspace relative to the root (`""` = the workspace is the root; `"workspace/"` = subfolder).
    pub prefix: String,
    /// Current branch (`None` = detached HEAD).
    pub branch: Option<String>,
    /// Current commit (`None` = repository with no commits).
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub state: RepoState,
    /// Paths relative to the root.
    pub entries: Vec<GitEntry>,
    /// How many stashes are saved in the repository.
    pub stash_count: u32,
    /// There were more than [`STATUS_MAX_ENTRIES`] changes.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepoState {
    #[default]
    Normal,
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Ordinary,
    Renamed,
    Copied,
    /// Merge conflict.
    Unmerged,
    Untracked,
}

/// A changed file. `index`/`worktree` are git's letters (`M`, `A`, `D`, `R`, `C`, `U`,
/// `?`, `.` = no change on that side).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitEntry {
    pub path: String,
    /// Previous name (renamed/copied).
    pub orig_path: Option<String>,
    pub index: char,
    pub worktree: char,
    pub kind: EntryKind,
}

impl GitEntry {
    /// Has a staged change.
    pub fn staged(&self) -> bool {
        !matches!(self.kind, EntryKind::Untracked | EntryKind::Unmerged) && self.index != '.'
    }

    /// Has an unstaged change in the working copy.
    pub fn unstaged(&self) -> bool {
        !matches!(self.kind, EntryKind::Untracked | EntryKind::Unmerged) && self.worktree != '.'
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBranch {
    /// `main`, `origin/feat`.
    pub name: String,
    pub remote: bool,
    pub upstream: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Add,
    Del,
    Context,
    /// `@@ -1,3 +1,4 @@`
    Hunk,
    /// Headers (`diff --git`, `index`, `---`, `+++`, `\ No newline…`).
    Meta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// Line number in the old / new file.
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitDiff {
    pub lines: Vec<DiffLine>,
    /// Cut at [`DIFF_MAX_LINES`]/[`DIFF_MAX_BYTES`].
    pub truncated: bool,
    pub binary: bool,
}

/// An entry of the stash stack (`stash@{index}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStash {
    pub index: usize,
    pub date: String,
    pub message: String,
}

/// A commit of the history (log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

/// Detail of a commit (metadata, changed files and diff).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitCommitDetail {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub message: String,
    /// `(letter, path)` of the files changed in the commit.
    pub files: Vec<(char, String)>,
    pub diff: GitDiff,
}

/// A conflict block (`<<<<<<<` … `=======` … `>>>>>>>`) inside a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictBlock {
    /// Starting line of the `<<<<<<<` marker (1-indexed).
    pub start_line: u32,
    pub ours_label: String,
    pub ours: Vec<String>,
    pub theirs_label: String,
    pub theirs: Vec<String>,
}

/// File in conflict with its blocks (or whole sides when there are no text markers).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitConflictFile {
    pub path: String,
    pub blocks: Vec<ConflictBlock>,
    pub binary: bool,
}

/// Choice to resolve a block or a whole file in conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Ours,
    Theirs,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    NotInstalled,
    NotARepo,
    /// Git refuses the folder because it belongs to another user (`safe.directory`).
    Unsafe {
        path: String,
    },
    /// Missing `user.name`/`user.email` for the commit.
    NoIdentity,
    Timeout,
    /// The remote refused (it needs a pull first, for example).
    Rejected(String),
    /// Credential missing or refused.
    Auth(String),
    Failed(String),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::NotInstalled => write!(f, "{}", t!("Git was not found. Install it from https://git-scm.com and try again.")),
            GitError::NotARepo => write!(f, "{}", t!("The working folder is not a git repository.")),
            GitError::Unsafe { path } => write!(
                f,
                "{}",
                t!(
                    "Git does not trust this folder (it belongs to another user). To allow it, run: git config --global --add safe.directory \"{path}\"",
                    path
                )
            ),
            GitError::NoIdentity => write!(
                f,
                "{}",
                t!(
                    "Set your name and e-mail for the commit: git config --global user.name \"Your Name\" and git config --global user.email you@example.com"
                )
            ),
            GitError::Timeout => write!(f, "{}", t!("Git took too long and was interrupted.")),
            GitError::Rejected(m) => write!(f, "{}", t!("The remote refused: {message}", message = m)),
            GitError::Auth(m) => write!(f, "{}", t!("Authentication failed: {message}", message = m)),
            GitError::Failed(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for GitError {}

// ---------- Output parsing (pure functions) ----------

/// Reads `git status --porcelain=v2 -z --branch --show-stash`. `root`/`prefix`/`state` are up to the caller.
pub fn parse_status_v2(out: &[u8]) -> GitStatus {
    let mut st = GitStatus::default();
    let mut records = out.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        let line = String::from_utf8_lossy(rec);
        if let Some(h) = line.strip_prefix("# ") {
            let (key, val) = h.split_once(' ').unwrap_or((h, ""));
            match key {
                "branch.oid" => st.head = (val != "(initial)").then(|| val.to_owned()),
                "branch.head" => st.branch = (val != "(detached)").then(|| val.to_owned()),
                "branch.upstream" => st.upstream = Some(val.to_owned()),
                "branch.ab" => {
                    for part in val.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                "stash" => st.stash_count = val.parse().unwrap_or(0),
                _ => {}
            }
            continue;
        }
        let entry = match line.as_bytes().first() {
            Some(b'1') => {
                let f: Vec<&str> = line.splitn(9, ' ').collect();
                (f.len() == 9).then(|| xy_entry(f[1], f[8], None, EntryKind::Ordinary))
            }
            Some(b'2') => {
                let f: Vec<&str> = line.splitn(10, ' ').collect();
                // The previous name is the next record.
                let orig = records.next().map(|r| String::from_utf8_lossy(r).into_owned());
                let kind = if f.get(8).is_some_and(|s| s.starts_with('C')) { EntryKind::Copied } else { EntryKind::Renamed };
                (f.len() == 10).then(|| xy_entry(f[1], f[9], orig, kind))
            }
            Some(b'u') => {
                let f: Vec<&str> = line.splitn(11, ' ').collect();
                (f.len() == 11).then(|| xy_entry(f[1], f[10], None, EntryKind::Unmerged))
            }
            Some(b'?') => line.get(2..).map(|p| GitEntry {
                path: p.to_owned(),
                orig_path: None,
                index: '?',
                worktree: '?',
                kind: EntryKind::Untracked,
            }),
            _ => None,
        };
        if let Some(e) = entry {
            if st.entries.len() >= STATUS_MAX_ENTRIES {
                st.truncated = true;
            } else {
                st.entries.push(e);
            }
        }
    }
    st
}

fn xy_entry(xy: &str, path: &str, orig_path: Option<String>, kind: EntryKind) -> GitEntry {
    let mut c = xy.chars();
    let index = c.next().unwrap_or('.');
    let worktree = c.next().unwrap_or('.');
    GitEntry { path: path.to_owned(), orig_path, index, worktree, kind }
}

/// Format asked of `git for-each-ref` (NUL-separated fields, one ref per line).
pub const BRANCH_FORMAT: &str = "%(refname)%00%(refname:short)%00%(upstream:short)%00%(HEAD)";

/// Reads the output of `git for-each-ref --format=BRANCH_FORMAT refs/heads refs/remotes`.
pub fn parse_branches(out: &[u8]) -> Vec<GitBranch> {
    let text = String::from_utf8_lossy(out);
    let mut list = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\0').collect();
        if f.len() < 4 {
            continue;
        }
        let (full, short, upstream, head) = (f[0], f[1], f[2], f[3]);
        let remote = full.starts_with("refs/remotes/");
        // `origin/HEAD` points to another branch: it is not a useful choice.
        if remote && full.ends_with("/HEAD") {
            continue;
        }
        list.push(GitBranch {
            name: short.to_owned(),
            remote,
            upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
            current: head == "*",
        });
    }
    list
}

/// Reads a unified diff (or a combined one, `diff --cc` / `@@@`). Cuts at `max_lines` lines (and each
/// line at 1000 characters). The trailing `\r` of a CRLF line is kept in `text`.
pub fn parse_diff(text: &str, max_lines: usize) -> GitDiff {
    let mut diff = GitDiff::default();
    let (mut old, mut new) = (0u32, 0u32);
    let mut in_hunk = false;
    // Number of column prefixes (1 for a normal `@@` diff, >= 2 for a combined `@@@` diff).
    let mut cols = 1usize;
    // Split only on `\n`: the `\r` of CRLF files stays in the text (the patch of a hunk needs it to
    // apply; whoever draws it removes the `\r`).
    let mut raws: Vec<&str> = text.split('\n').collect();
    if raws.last() == Some(&"") {
        raws.pop();
    }
    for raw in raws {
        if diff.lines.len() >= max_lines {
            diff.truncated = true;
            break;
        }
        let line: String = if raw.chars().count() > 1000 { raw.chars().take(1000).chain(['…']).collect() } else { raw.to_owned() };
        let (kind, o, n) = if raw.starts_with("diff --git ") || raw.starts_with("diff --cc ") || raw.starts_with("diff --combined ") {
            in_hunk = false;
            (DiffKind::Meta, None, None)
        } else if raw.starts_with("Binary files ") && raw.ends_with(" differ") {
            diff.binary = true;
            (DiffKind::Meta, None, None)
        } else if raw.starts_with("@@") {
            let ats = raw.bytes().take_while(|b| *b == b'@').count();
            let rest = &raw[ats..];
            in_hunk = true;
            cols = ats.saturating_sub(1).max(1);
            // `@@ -a[,b] +c[,d] @@` or `@@@ -a[,b] -c[,d] +e[,f] @@@`
            let start = |p: &str, sign: char| -> Option<u32> {
                p.strip_prefix(sign).and_then(|s| s.split(',').next()).and_then(|s| s.parse().ok())
            };
            let mut old_found = None;
            let mut new_found = None;
            for part in rest.split_whitespace() {
                if part.starts_with('@') {
                    break;
                }
                if old_found.is_none() {
                    old_found = start(part, '-');
                }
                if let Some(v) = start(part, '+') {
                    new_found = Some(v);
                }
            }
            old = old_found.unwrap_or(0);
            new = new_found.unwrap_or(0);
            (DiffKind::Hunk, None, None)
        } else if !in_hunk || raw.starts_with('\\') {
            (DiffKind::Meta, None, None)
        } else if cols == 1 {
            if raw.starts_with('+') {
                new += 1;
                (DiffKind::Add, None, Some(new - 1))
            } else if raw.starts_with('-') {
                old += 1;
                (DiffKind::Del, Some(old - 1), None)
            } else {
                old += 1;
                new += 1;
                (DiffKind::Context, Some(old - 1), Some(new - 1))
            }
        } else {
            // Combined diff (`diff --cc`): the first `cols` characters mark `+`, `-` or ` ` in each parent.
            let prefix: String = raw.chars().take(cols).collect();
            if prefix.contains('+') {
                new += 1;
                (DiffKind::Add, None, Some(new - 1))
            } else if prefix.contains('-') {
                let o = if prefix.starts_with('-') {
                    old += 1;
                    Some(old - 1)
                } else {
                    None
                };
                (DiffKind::Del, o, None)
            } else {
                old += 1;
                new += 1;
                (DiffKind::Context, Some(old - 1), Some(new - 1))
            }
        };
        diff.lines.push(DiffLine { kind, old: o, new: n, text: line });
    }
    diff
}

/// Builds a unified patch containing only the hunk (`hunk_index`, 0-indexed) of `diff`, ready for
/// `git apply`. Returns `None` if the diff is combined (`@@@`), truncated, binary or has no
/// `---`/`+++` header.
pub fn extract_hunk_patch(diff: &GitDiff, hunk_index: usize) -> Option<String> {
    if diff.binary || diff.truncated {
        return None;
    }
    let mut header = Vec::new();
    let mut hunks: Vec<Vec<&DiffLine>> = Vec::new();
    for line in &diff.lines {
        match line.kind {
            DiffKind::Meta if hunks.is_empty() => header.push(line.text.as_str()),
            DiffKind::Hunk => {
                // A combined diff (`@@@`) cannot be applied with `git apply`.
                if line.text.starts_with("@@@") {
                    return None;
                }
                hunks.push(vec![line]);
            }
            _ => {
                if let Some(cur) = hunks.last_mut() {
                    cur.push(line);
                }
            }
        }
    }
    let hunk = hunks.get(hunk_index)?;
    if !header.iter().any(|l| l.starts_with("--- ")) || !header.iter().any(|l| l.starts_with("+++ ")) {
        return None;
    }
    // A truncated line (> 1000 chars) would not produce a faithful patch.
    if hunk.iter().any(|l| l.text.ends_with('…') && l.text.chars().count() > 1000) {
        return None;
    }
    let mut out = String::new();
    for h in header {
        out.push_str(h);
        out.push('\n');
    }
    for l in hunk {
        out.push_str(&l.text);
        out.push('\n');
    }
    Some(out)
}

/// Format asked of `git stash list`.
pub const STASH_FORMAT: &str = "%gd%x00%cr%x00%gs";

/// Reads the output of `git stash list --format=STASH_FORMAT`.
pub fn parse_stashes(out: &[u8]) -> Vec<GitStash> {
    let text = String::from_utf8_lossy(out);
    let mut list = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.splitn(3, '\0').collect();
        if f.len() < 3 {
            continue;
        }
        let idx = f[0].strip_prefix("stash@{").and_then(|s| s.strip_suffix('}')).and_then(|s| s.parse().ok()).unwrap_or(list.len());
        list.push(GitStash { index: idx, date: f[1].to_owned(), message: f[2].to_owned() });
    }
    list
}

/// Format asked of `git log -z`.
pub const LOG_FORMAT: &str = "%H%x00%h%x00%an%x00%cr%x00%s";

/// Reads the output of `git log -z --format=LOG_FORMAT`.
pub fn parse_log(out: &[u8]) -> Vec<GitCommit> {
    let mut list = Vec::new();
    // With `-z` and `%x00`, all fields stay separated by NUL, five at a time.
    let fields: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    for chunk in fields.chunks(5) {
        if chunk.len() < 5 {
            break;
        }
        let hash = String::from_utf8_lossy(chunk[0]).trim_start_matches('\n').to_owned();
        if hash.is_empty() {
            continue;
        }
        list.push(GitCommit {
            hash,
            short_hash: String::from_utf8_lossy(chunk[1]).into_owned(),
            author: String::from_utf8_lossy(chunk[2]).into_owned(),
            date: String::from_utf8_lossy(chunk[3]).into_owned(),
            subject: String::from_utf8_lossy(chunk[4]).into_owned(),
        });
    }
    list
}

/// Finds the conflict blocks (`<<<<<<<` … `=======` … `>>>>>>>`) in a text.
pub fn parse_conflict_blocks(text: &str) -> Vec<ConflictBlock> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(ours_label) = lines[i].strip_prefix("<<<<<<<") else {
            i += 1;
            continue;
        };
        let start_line = i as u32 + 1;
        let ours_label = ours_label.trim().to_owned();
        i += 1;
        let mut ours = Vec::new();
        // Also accepts diff3 conflicts (`|||||||`), ignoring the base section between `|||||||` and `=======`.
        let mut in_base = false;
        while i < lines.len() && !lines[i].starts_with("=======") {
            if lines[i].starts_with("|||||||") {
                in_base = true;
            } else if !in_base {
                ours.push(lines[i].to_owned());
            }
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        i += 1; // skip `=======`
        let mut theirs = Vec::new();
        while i < lines.len() && !lines[i].starts_with(">>>>>>>") {
            theirs.push(lines[i].to_owned());
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let theirs_label = lines[i].strip_prefix(">>>>>>>").unwrap_or("").trim().to_owned();
        blocks.push(ConflictBlock { start_line, ours_label, ours, theirs_label, theirs });
        i += 1;
    }
    blocks
}

/// Resolves a single conflict block (`block_index`, 0-indexed) by replacing the markers with the
/// choice (`Ours`, `Theirs` or `Both`), preserving the line-break style (`\r\n` or `\n`).
pub fn resolve_conflict_block_in_text(text: &str, block_index: usize, choice: ConflictChoice) -> Option<String> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing_nl = text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur_block = 0usize;
    let mut resolved = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("<<<<<<<") {
            let mut j = i + 1;
            let mut ours = Vec::new();
            let mut in_base = false;
            while j < lines.len() && !lines[j].starts_with("=======") {
                if lines[j].starts_with("|||||||") {
                    in_base = true;
                } else if !in_base {
                    ours.push(lines[j]);
                }
                j += 1;
            }
            if j < lines.len() {
                let mut k = j + 1;
                let mut theirs = Vec::new();
                while k < lines.len() && !lines[k].starts_with(">>>>>>>") {
                    theirs.push(lines[k]);
                    k += 1;
                }
                if k < lines.len() {
                    if cur_block == block_index {
                        match choice {
                            ConflictChoice::Ours => out.extend(ours.into_iter().map(str::to_owned)),
                            ConflictChoice::Theirs => out.extend(theirs.into_iter().map(str::to_owned)),
                            ConflictChoice::Both => {
                                out.extend(ours.into_iter().map(str::to_owned));
                                out.extend(theirs.into_iter().map(str::to_owned));
                            }
                        }
                        resolved = true;
                    } else {
                        for line in &lines[i..=k] {
                            out.push((*line).to_owned());
                        }
                    }
                    cur_block += 1;
                    i = k + 1;
                    continue;
                }
            }
        }
        out.push(lines[i].to_owned());
        i += 1;
    }
    if !resolved {
        return None;
    }
    let mut res = out.join(eol);
    if trailing_nl && !res.is_empty() {
        res.push_str(eol);
    }
    Some(res)
}

/// Diff of a not yet tracked file: every line as added.
pub fn untracked_diff(bytes: &[u8], max_lines: usize) -> GitDiff {
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return GitDiff { lines: Vec::new(), truncated: false, binary: true };
    }
    let text = String::from_utf8_lossy(bytes);
    let mut diff = GitDiff::default();
    for (i, raw) in text.lines().enumerate() {
        if i >= max_lines {
            diff.truncated = true;
            break;
        }
        let text: String = if raw.chars().count() > 1000 { raw.chars().take(1000).chain(['…']).collect() } else { raw.to_owned() };
        diff.lines.push(DiffLine { kind: DiffKind::Add, old: None, new: Some(i as u32 + 1), text });
    }
    diff
}

/// `true` if the (lowercased) stderr of git or `gh` says the HTTPS credential is missing, expired or refused.
fn is_auth_stderr(low: &str) -> bool {
    low.contains("authentication failed")
        || low.contains("could not read username")
        || low.contains("could not read password")
        || low.contains("terminal prompts disabled")
        || low.contains("user interactivity has been disabled")
        || low.contains("invalid username or token")
        || low.contains("invalid username or password")
        || low.contains("bad credentials")
        || low.contains("requested url returned error: 401")
        || low.contains("requested url returned error: 403")
        || low.contains("http 401")
        || (low.contains("token") && low.contains("is invalid"))
        || low.contains("gh auth login")
}

/// How the user can sign in to GitHub from inside Holt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigninPlan {
    /// Command to type in a new terminal tab; the tab closes itself (`exit`) only if it succeeded.
    Command(String),
    /// Nothing that can sign in is installed: explanation of how to get it.
    Install(String),
}

/// Chooses the sign-in route. Windows prefers Git Credential Manager, then `gh`; elsewhere `gh`.
pub fn signin_plan(windows: bool, gh: bool, gcm: bool) -> SigninPlan {
    const GH: &str = "gh auth login -h github.com --git-protocol https --web -s workflow && gh auth setup-git";
    if windows && gcm {
        SigninPlan::Command("git credential-manager github login".to_owned())
    } else if gh {
        // PowerShell 5.1 has no `&&`.
        SigninPlan::Command(if windows { GH.replace(" && ", "; ") } else { GH.to_owned() })
    } else if windows {
        SigninPlan::Install(
            t!("Install the GitHub CLI (winget install GitHub.cli) or Git Credential Manager, then try again. Or set a credential helper: git config --global credential.helper manager")
                .to_owned(),
        )
    } else {
        SigninPlan::Install(
            t!("Install the GitHub CLI (sudo zypper/dnf/apt install gh) and try again. Or set a credential helper: git config --global credential.helper store")
                .to_owned(),
        )
    }
}

/// Turns the stderr of a command that failed into a meaningful error (the messages come in
/// English because of `LC_ALL=C`).
pub fn classify_error(stderr: &str) -> GitError {
    let s = stderr.trim();
    let low = s.to_lowercase();
    if low.contains("not a git repository") {
        return GitError::NotARepo;
    }
    if low.contains("dubious ownership") {
        let path = s.split("repository at '").nth(1).and_then(|r| r.split('\'').next()).unwrap_or("").to_owned();
        return GitError::Unsafe { path };
    }
    if low.contains("please tell me who you are") || low.contains("empty ident") || low.contains("unable to auto-detect email") {
        return GitError::NoIdentity;
    }
    let short = crate::util::last_meaningful_lines(s, 4);
    if low.contains("permission denied (publickey") || low.contains("sign_and_send_pubkey") || low.contains("no mutual signature algorithm")
    {
        return GitError::Auth(t!(
            "{output}\nIf your SSH key has a passphrase, Holt cannot ask for it: load the key into ssh-agent and try again.\n• Windows (PowerShell; the first time as administrator): Set-Service ssh-agent -StartupType Manual; Start-Service ssh-agent; ssh-add\n• Linux/macOS: eval \"$(ssh-agent -s)\" && ssh-add",
            output = short
        ));
    }
    if is_auth_stderr(&low) {
        return GitError::Auth(short);
    }
    if low.contains("host key verification failed") {
        return GitError::Auth(short);
    }
    // `commit.gpgsign` with a locked key or with no agent/pinentry that can ask for the passphrase.
    if low.contains("gpg failed to sign") || low.contains("no pinentry") || low.contains("inappropriate ioctl for device") {
        return GitError::Failed(t!(
            "The commit was not made: the GPG signature (commit.gpgsign) needs the key unlocked, and Holt cannot open the passphrase prompt. Unlock the key in a terminal tab (echo test | gpg --clearsign) and try again right after; the passphrase stays in gpg-agent for a few minutes. If you do not want to sign in this repository: git config commit.gpgsign false\n{output}",
            output = short
        ));
    }
    if low.contains("[rejected]")
        || low.contains("non-fast-forward")
        || low.contains("fetch first")
        || low.contains("not possible to fast-forward")
        || low.contains("diverging branches")
    {
        return GitError::Rejected(short);
    }
    GitError::Failed(if short.is_empty() { t!("git failed with no message").to_owned() } else { short })
}

/// Acceptable branch name (a safe subset of the rules of `git check-ref-format`).
pub fn valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && name != "@"
        && !name.contains("..")
        && !name.contains("@{")
        && !name.contains("//")
        && !name.chars().any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
        && !name.split('/').any(|part| part.starts_with('.'))
}

/// Safe relative path to pass to git (no `..`, not absolute, no NUL). Accepts only ordinary
/// components (and `.`); on Windows, no `:` (`C:..\x` is relative to the drive and leaves the root).
pub fn safe_rel(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\0')
        && !(cfg!(windows) && path.contains(':'))
        && !path.starts_with(['/', '\\'])
        && !path.split(['/', '\\']).any(|p| p == "..")
        && Path::new(path).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}
