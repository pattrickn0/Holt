//! Chat view of an agent tab: the CLI runs in its structured mode and its events are rendered, instead of its
//! terminal screen being scraped.
//!
//! Claude Code, started with `-p --input-format stream-json --output-format stream-json
//! --include-partial-messages --verbose --permission-prompt-tool stdio`, reads one JSON object per line on stdin
//! (user messages, permission answers, interrupts) and writes one per line on stdout (see [`parse_line`]). This module
//! has three parts, all without interface:
//!
//! 1. [`parse_line`]: a stdout line → typed [`ChatEvent`]s. Checked against real recordings in `tests/data/`.
//! 2. [`Transcript`]: the conversation as a list of [`Item`]s, built by [`Transcript::apply`] from the events (and from
//!    the CLI's own transcript file when a saved conversation is resumed, [`Transcript::from_history`]).

mod acp;
mod backend;
mod codex;
mod commands;
mod events;
mod messages;
mod process;
#[cfg(test)]
mod tests;
mod transcript;

pub use backend::{capability, for_kind, Capability, ChatBackend, Interrupt, Launch, Parsed, Protocol};
pub use codex::patch_from_diff;
pub use commands::*;
pub use events::*;
pub(crate) use events::{
    agy_patch_of, blocks_of, clip, context_tokens, parse_agy_line, parse_claude_line, str_of, strings, tool_results, user_text,
};
pub(crate) use messages::error_response_json;
pub use messages::*;
pub use process::*;
pub use transcript::*;

/// Tool results longer than this are cut when stored (the interface shows a few lines anyway).
const RESULT_MAX_CHARS: usize = 8_000;
/// Items kept in a transcript; the oldest are dropped (a resumed conversation loads the tail only).
const ITEMS_MAX: usize = 1_500;
/// The text the CLI echoes when a turn is interrupted.
const INTERRUPTED: &str = "[Request interrupted by user";

/// CLIs that have a chat view: the ones whose backend says so.
pub fn supports(kind: &str) -> bool {
    capability(kind) == Capability::Chat
}

/// The CLI speaks ACP and goes through the generic backend (see `acp.rs`).
pub fn is_acp(kind: &str) -> bool {
    acp::backend_for(kind).is_some()
}
