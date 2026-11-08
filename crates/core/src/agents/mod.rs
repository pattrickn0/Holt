//! Agent registry: resolution of the official executables, assembly of the interactive
//! command (PTY) and the bridge instructions (equivalent to `AGENTS`, `resolve*`, `peers`,

mod defs;
mod env;
mod hooks;
mod local;
mod resolve;
mod spawn;
mod types;

pub use defs::*;
pub use env::PATH_SEP;
pub use local::*;
pub use resolve::{agent_def, can_answer, can_answer_with, cannot_answer_error, command_can_answer, resolve_alias};
pub use types::*;

pub(crate) use env::agent_env;
pub(crate) use resolve::{cmd_safe_arg, Discovery, Registry, Resolved};
pub(crate) use spawn::{spawn_spec, spawn_spec_for};

#[cfg(test)]
mod tests;
