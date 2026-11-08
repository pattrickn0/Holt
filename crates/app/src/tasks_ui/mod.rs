//! State and actions of the right bar's "Tasks" tab (the painting lives in `ui/tasks.rs`).
//!
//! Every task call into the core runs git (create, start, merge, discard, diff), so it goes to a
//! thread (`Deck::spawn_bg`) and the answer comes back as `UiMsg::Tasks`. The list is re-read when
//! the tab opens, when the core says so (`CoreEvent::TasksChanged`) and after every action. The pure
//! functions that decide what the screen shows (groups, texts, actions per status) live here too, with tests.

use std::collections::HashSet;
use std::path::PathBuf;

use holt_core::capability::{self, Level};
use holt_core::models::{self, Effort};
use holt_core::{t, FeedKind, MergeBlock, Task, TaskError, TaskEvent, TaskMessage, TaskSpec, TaskStatus};

use crate::deck::{AttachTarget, Deck, UiMsg};

mod deck;
mod demo;
mod format;
mod model;
mod state;

pub use demo::*;
pub use format::*;
pub use model::*;
pub use state::*;
