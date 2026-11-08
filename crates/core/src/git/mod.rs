//! Module root for git: the types and parsers (`parse`), the process layer (`exec`) and the
//! operations, one file per area of the interface.

mod branches;
mod exec;
mod history;
mod merge;
mod parse;
mod remote;
mod stash;
mod status;

pub(crate) use exec::{base_command, run, GitState, Repo};
pub use parse::*;

use std::time::Duration;

/// Fast reads (status, diff, branches).
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Local operations that write (stage, checkout…).
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// Commit (hooks and GPG signing can take a while).
pub(crate) const COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
/// Network (fetch, pull, push).
const NET_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum output kept from a command (the diff of a huge file is cut).
pub(crate) const MAX_OUT: usize = 4 << 20;
/// Diff shown: at most this many bytes and lines.
pub const DIFF_MAX_BYTES: usize = 1 << 20;
pub const DIFF_MAX_LINES: usize = 10_000;
/// Status entries shown.
pub const STATUS_MAX_ENTRIES: usize = 5000;
/// Maximum size of a file read for conflict resolution.
pub const CONFLICT_MAX_BYTES: usize = 2 << 20;

#[cfg(test)]
mod tests;
