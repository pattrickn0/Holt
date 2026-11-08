//! The state the tab keeps between frames and the replies that come back from the core.

use super::*;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// What the tab shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TasksView {
    #[default]
    List,
    /// The task's detail (id).
    Detail(String),
    /// New task form.
    New,
}

/// Dangerous action waiting for the "yes" in the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Merge(String),
    Discard(String),
}

/// A lifecycle operation in progress (buttons off, spinner turning).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    pub id: String,
    pub label: &'static str,
}

/// Open menu of the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormMenu {
    Agent,
    Model,
}

/// "New task" form.
#[derive(Debug, Clone)]
pub struct NewForm {
    pub title: String,
    pub brief: String,
    /// Files attached to the request (absolute paths in `data/attachments`).
    pub attachments: Vec<PathBuf>,
    /// Agent (`claude`); empty until the form opens.
    pub kind: String,
    /// The current agent came from the automatic recommendation, not from a choice by the user.
    pub kind_is_auto: bool,
    /// `None` = "Default".
    pub model: Option<String>,
    /// `None` = "Default" (nothing goes to the CLI). Only applies where `models::effort_unavailable` does not object.
    pub effort: Option<Effort>,
    /// The size of the work; decides which models can handle it and, with the automatic agent, who is recommended.
    pub level: Level,
    /// The capacity notice for the chosen agent and model (the task is above what they reach); recalculated
    /// by `tasks_form_refresh`, not on every frame (it reads the tasks and the capacity table).
    pub capacity_note: Option<String>,
    /// Provider of the model menu step (`opencode`): `None` = the list of providers shows
    /// first. Only applies while the menu is open (OpenCode mixes several providers).
    pub provider: Option<String>,
    pub base: String,
    /// Write scope: one path per line (or separated by commas). Empty = no scope. See [`parse_scope`].
    pub scope: String,
    pub start_now: bool,
    pub error: Option<String>,
    /// Creating (the button is off).
    pub busy: bool,
    pub menu: Option<FormMenu>,
    /// "Branch and scope" is unfolded.
    pub more_open: bool,
    /// The title asks for keyboard focus on the next frame (the form just opened).
    pub focus_title: bool,
}

impl Default for NewForm {
    fn default() -> Self {
        Self {
            title: String::new(),
            brief: String::new(),
            attachments: Vec::new(),
            kind: String::new(),
            kind_is_auto: true,
            model: None,
            effort: None,
            level: Level::default(),
            capacity_note: None,
            provider: None,
            base: String::new(),
            scope: String::new(),
            start_now: true,
            error: None,
            busy: false,
            menu: None,
            more_open: false,
            focus_title: false,
        }
    }
}

/// State of the Tasks tab in the `Deck`.
pub struct TasksUi {
    pub view: TasksView,
    pub tasks: Vec<Task>,
    /// The first read has already arrived.
    pub loaded: bool,
    pub refreshing: bool,
    /// Another read was requested while one was in progress.
    pub(super) dirty: bool,
    pub history_filter: HistoryFilter,
    pub history_menu: bool,
    /// The pane the person picked in the detail (`None` = [`DetailTab::default_for`] the task).
    pub detail_tab: Option<DetailTab>,
    /// Timeline messages opened whole (`task id`, message time).
    pub open_messages: HashSet<(String, u64)>,
    pub confirm: Option<Confirm>,
    /// The feedback field of "Return" (in review) or "Reject" (awaiting validation) is open.
    pub return_open: bool,
    pub feedback: String,
    pub busy: Option<Busy>,
    /// Error of the last action (red strip in the detail) or, with `notice_info`, the notice of an action that worked.
    pub notice: Option<String>,
    /// The notice is only information (the merge went through, but there is something the user needs to know): neutral strip.
    pub notice_info: bool,
    /// The structured reason for the merge refusal, when there is a way out to offer on a button.
    pub notice_block: Option<MergeBlock>,
    /// The stash notice that came with the refusal (`MergeRefusal::stash_note`), shown after the explanation.
    pub notice_stash: Option<String>,
    /// The full list of files of the refusal is open ("See what is uncommitted").
    pub notice_files: bool,
    pub form: NewForm,
    /// Changes on every project switch: answers to requests from the previous project are discarded.
    pub generation: u64,
    /// DEBUGGING ONLY: a fake state that re-reads do not replace.
    pub debug_hold: bool,
}

impl Default for TasksUi {
    fn default() -> Self {
        Self {
            view: TasksView::List,
            tasks: Vec::new(),
            loaded: false,
            refreshing: false,
            dirty: false,
            history_filter: HistoryFilter::All,
            history_menu: false,
            detail_tab: None,
            open_messages: HashSet::new(),
            confirm: None,
            return_open: false,
            feedback: String::new(),
            busy: None,
            notice: None,
            notice_info: false,
            notice_block: None,
            notice_stash: None,
            notice_files: false,
            form: NewForm::default(),
            generation: 0,
            debug_hold: false,
        }
    }
}

impl TasksUi {
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn review_count(&self) -> usize {
        review_count(&self.tasks)
    }

    pub fn validating_count(&self) -> usize {
        validating_count(&self.tasks)
    }

    /// The number on the tab icon's badge: what waits for a human decision (review or validate).
    pub fn attention_count(&self) -> usize {
        self.review_count() + self.validating_count()
    }
}

/// What a lifecycle operation does.
#[derive(Debug, Clone)]
pub enum TaskOp {
    Start,
    Merge,
    /// "Stash my changes, merge and give them back" (`Core::task_merge_assisted` with the stash on).
    MergeStash,
    Validate,
    Discard,
    Return(String),
    Reject(String),
    /// "Cancel automatic merge" on a task whose conflict was handed to the agent.
    CancelAutoMerge,
}

impl TaskOp {
    pub(super) fn busy_label(&self) -> &'static str {
        match self {
            TaskOp::Start => t!("Starting…"),
            TaskOp::Merge => t!("Merging…"),
            TaskOp::MergeStash => t!("Stashing and merging…"),
            TaskOp::Validate => t!("Validating…"),
            TaskOp::Discard => t!("Discarding…"),
            TaskOp::Return(_) => t!("Returning…"),
            TaskOp::Reject(_) => t!("Rejecting…"),
            TaskOp::CancelAutoMerge => t!("Cancelling…"),
        }
    }
}

/// What the merge returns besides the task.
#[derive(Debug, Default)]
pub struct MergeExtra {
    pub block: Option<MergeBlock>,
    pub note: Option<String>,
}

/// Answers of the background calls.
pub enum TaskReply {
    List(Vec<Task>),
    Created {
        result: Box<Result<Task, TaskError>>,
        /// The result of starting right after (only with "Start now").
        started: Option<Box<Result<Task, TaskError>>>,
    },
    Done {
        id: String,
        op: TaskOp,
        result: Box<Result<Task, TaskError>>,
        /// Only on the merge: the reason for a refusal that has a way out and the notice about the stashed changes.
        merge: Box<MergeExtra>,
    },
}
