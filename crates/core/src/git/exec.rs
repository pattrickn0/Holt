//! Running git: environment without prompts, timeouts, output limits and the `Repo` handle.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::process::{run_captured_raw, CapturedRaw, Exit};
use crate::t;

use super::parse::*;
use super::*;

// ---------- Execution ----------

/// Git state kept in the core.
#[derive(Default)]
pub(crate) struct GitState {
    /// Executable found (and when it was looked up: the search is redone after 30 s, to notice
    /// an installation made with the app open).
    pub(super) exe: Mutex<Option<(Instant, Option<PathBuf>)>>,
    /// One operation that writes to the repository at a time (git uses `index.lock`).
    pub(crate) op: Mutex<()>,
}

/// Repository found from the workspace.
pub(crate) struct Repo {
    pub(crate) git: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) git_dir: PathBuf,
    pub(super) prefix: String,
}

pub(crate) fn base_command(git: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(git);
    c.current_dir(cwd).args(["-c", "core.quotepath=false", "-c", "color.ui=false"]).args(args);
    for (k, v) in [
        ("LC_ALL", "C"),
        ("LANGUAGE", "C"),
        ("GIT_TERMINAL_PROMPT", "0"),
        // Git Credential Manager would open a login window and the command would stay stuck until
        // the timeout; with no saved credential, it fails right away (becomes `GitError::Auth`).
        ("GCM_INTERACTIVE", "never"),
        ("SSH_ASKPASS_REQUIRE", "never"),
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_MERGE_AUTOEDIT", "no"),
        ("GIT_PAGER", "cat"),
        ("GIT_EDITOR", "true"),
        ("GIT_SEQUENCE_EDITOR", "true"),
        ("GIT_LITERAL_PATHSPECS", "1"),
        // No TTY for the curses pinentry; with a graphical pinentry it uses DISPLAY/the OS session;
        // if it is a terminal, it fails immediately instead of waiting for the 120 s timeout.
        ("GPG_TTY", "not a tty"),
        ("PINENTRY_USER_DATA", "USE_CURSES=0"),
    ] {
        c.env(k, v);
    }
    for k in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX", "GIT_ASKPASS", "SSH_ASKPASS"] {
        c.env_remove(k);
    }
    c
}

pub(crate) fn run(cmd: Command, stdin: Option<Vec<u8>>, timeout: Duration) -> Result<CapturedRaw, GitError> {
    let out = run_captured_raw(cmd, stdin, timeout, None, None, MAX_OUT).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GitError::NotInstalled
        } else {
            GitError::Failed(t!("Failed to run git: {error}", error = e))
        }
    })?;
    match out.exit {
        Exit::Code(Some(0)) => Ok(out),
        Exit::TimedOut => Err(GitError::Timeout),
        _ => {
            // Some errors (e.g. "nothing to commit") come out on standard output.
            let err = String::from_utf8_lossy(&out.stderr);
            let err = if err.trim().is_empty() { String::from_utf8_lossy(&out.stdout) } else { err };
            Err(classify_error(&err))
        }
    }
}

/// Paths separated by NUL (for `--pathspec-from-file=- --pathspec-file-nul`).
pub(super) fn nul_paths(paths: &[String]) -> Result<Vec<u8>, GitError> {
    let mut buf = Vec::new();
    for p in paths {
        if !safe_rel(p) {
            return Err(GitError::Failed(t!("Invalid path: {path}", path = p)));
        }
        buf.extend_from_slice(p.as_bytes());
        buf.push(0);
    }
    Ok(buf)
}

impl Repo {
    pub(crate) fn cmd(&self, args: &[&str]) -> Command {
        base_command(&self.git, &self.root, args)
    }

    pub(crate) fn read(&self, args: &[&str]) -> Result<CapturedRaw, GitError> {
        run(self.cmd(args), None, READ_TIMEOUT)
    }

    pub(crate) fn write(&self, args: &[&str], stdin: Option<Vec<u8>>) -> Result<CapturedRaw, GitError> {
        run(self.cmd(args), stdin, WRITE_TIMEOUT)
    }

    pub(crate) fn state(&self) -> RepoState {
        let has = |p: &str| self.git_dir.join(p).exists();
        if has("rebase-merge") || has("rebase-apply") {
            RepoState::Rebasing
        } else if has("MERGE_HEAD") {
            RepoState::Merging
        } else if has("CHERRY_PICK_HEAD") {
            RepoState::CherryPicking
        } else if has("REVERT_HEAD") {
            RepoState::Reverting
        } else {
            RepoState::Normal
        }
    }

    pub(super) fn has_head(&self) -> bool {
        run(self.cmd(&["rev-parse", "--verify", "-q", "HEAD"]), None, READ_TIMEOUT).is_ok()
    }

    /// Checks whether the current commit would use GPG signing (`commit.gpgsign = true`).
    pub(super) fn is_gpgsign_enabled(&self) -> bool {
        self.read(&["config", "--bool", "--get", "commit.gpgsign"]).is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
    }

    /// When `commit.gpgsign` is on, checks in an instant whether GPG can sign WITHOUT asking
    /// anything (`--pinentry-mode error`): with the key unlocked in `gpg-agent` it passes; with the
    /// key locked (or with no usable agent/pinentry) gpg fails right away, instead of letting the
    /// commit wait for the `COMMIT_TIMEOUT` for a passphrase prompt that nobody sees. Returns the
    /// explanation and does NOT touch the user's configuration. Only valid for OpenPGP signing.
    pub(crate) fn check_gpg_ready(&self) -> Result<(), GitError> {
        if !self.is_gpgsign_enabled() {
            return Ok(());
        }
        let cfg = |key: &str| {
            self.read(&["config", "--get", key])
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        if cfg("gpg.format").is_some_and(|f| f != "openpgp") {
            return Ok(());
        }
        let prog = cfg("gpg.openpgp.program").or_else(|| cfg("gpg.program")).unwrap_or_else(|| "gpg".into());
        let mut cmd = Command::new(&prog);
        cmd.current_dir(&self.root).args(["--batch", "--pinentry-mode", "error", "--detach-sign", "-o", "-"]);
        if let Some(k) = cfg("user.signingkey") {
            cmd.args(["-u", &k]);
        }
        cmd.env("LC_ALL", "C").env("GPG_TTY", "not a tty").env("PINENTRY_USER_DATA", "USE_CURSES=0");
        match run_captured_raw(cmd, Some(b"holt".to_vec()), Duration::from_secs(5), None, None, 64 << 10) {
            Ok(out) if matches!(out.exit, Exit::Code(Some(0))) => Ok(()),
            Ok(out) if matches!(out.exit, Exit::TimedOut) => {
                Err(classify_error("error: gpg failed to sign the data (no response from gpg-agent)"))
            }
            Ok(out) => Err(classify_error(&format!(
                "error: gpg failed to sign the data
{}",
                String::from_utf8_lossy(&out.stderr)
            ))),
            Err(_) => Err(classify_error("error: gpg failed to sign the data (gpg executable not found)")),
        }
    }

    /// Environment of the network operations: ssh with no prompt (otherwise it opens the terminal and hangs), unless
    /// the user has configured their own ssh.
    pub(super) fn net(&self, args: &[&str]) -> Command {
        let mut c = self.cmd(args);
        let user_ssh = std::env::var_os("GIT_SSH_COMMAND").is_some()
            || std::env::var_os("GIT_SSH").is_some()
            || self.read(&["config", "--get", "core.sshCommand"]).is_ok_and(|o| !o.stdout.is_empty());
        if !user_ssh {
            c.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
        c
    }

    pub(super) fn remotes(&self) -> Result<Vec<String>, GitError> {
        let out = self.read(&["remote"])?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect())
    }
}

/// Short text of what the command said (to show after pull/push).
pub(super) fn summary(out: &CapturedRaw) -> String {
    let all = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    crate::util::last_meaningful_lines(&all, 3)
}

/// Writes `bytes` to `path` atomically through a temporary file in the same folder.
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp");
    let tmp = parent.join(format!(".{name}.holt-tmp"));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        // On Windows, `rename` over an existing file sometimes requires removing it or trying again.
        let _ = std::fs::write(path, bytes);
        let _ = std::fs::remove_file(&tmp);
        if !path.exists() {
            return Err(e);
        }
    }
    Ok(())
}

/// Reads at most `max` bytes of the file. Returns (bytes, cut).
pub(super) fn read_capped(path: &Path, max: usize) -> std::io::Result<(Vec<u8>, bool)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    f.by_ref().take(max as u64 + 1).read_to_end(&mut buf)?;
    let cut = buf.len() > max;
    buf.truncate(max);
    Ok((buf, cut))
}
