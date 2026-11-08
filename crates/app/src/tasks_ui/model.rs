//! The queue model: lanes, marks, state sentences, lifecycle, timeline and the actions of each status.

use super::format::*;
use super::*;

// ---------------------------------------------------------------------------
// The queue: lanes, marks, the lifecycle track and the timeline
// ---------------------------------------------------------------------------

/// The list's sections, by who has the next move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// The person: review, test, a merge that stopped, a failure.
    NeedsYou,
    /// An agent: working, resolving a conflict, or queued behind another task.
    Working,
    /// Nobody yet.
    Drafts,
    /// Completed or discarded.
    Finished,
}

impl Lane {
    pub const ORDER: [Lane; 4] = [Lane::NeedsYou, Lane::Working, Lane::Drafts, Lane::Finished];

    pub fn title(self) -> &'static str {
        match self {
            Lane::NeedsYou => t!("Needs you"),
            Lane::Working => t!("Agents working"),
            Lane::Drafts => t!("Drafts"),
            Lane::Finished => t!("Finished"),
        }
    }

    pub fn of(status: TaskStatus) -> Lane {
        match status {
            TaskStatus::Review | TaskStatus::Validating | TaskStatus::Failed => Lane::NeedsYou,
            TaskStatus::Running | TaskStatus::Queued => Lane::Working,
            TaskStatus::Draft => Lane::Drafts,
            TaskStatus::Merged | TaskStatus::Discarded => Lane::Finished,
        }
    }
}

/// Since when the task waits for its next move (ms): what orders a lane.
pub fn waiting_since(task: &Task) -> u64 {
    match task.status {
        TaskStatus::Review | TaskStatus::Validating | TaskStatus::Failed => task.finished_ms.unwrap_or_else(|| last_activity(task)),
        TaskStatus::Running => task.started_ms.unwrap_or(task.created_ms),
        TaskStatus::Queued => task.queued_ms.unwrap_or(task.created_ms),
        TaskStatus::Draft => task.created_ms,
        TaskStatus::Merged | TaskStatus::Discarded => task.finished_ms.unwrap_or_else(|| last_activity(task)),
    }
}

/// The order of what needs you: decisions first (review, then test), then what went wrong (a merge that stopped, a
/// failure).
fn needs_rank(task: &Task) -> u8 {
    match mark_of(task) {
        Mark::Review => 0,
        Mark::Validate => 1,
        Mark::Stopped => 2,
        _ => 3,
    }
}

/// The open lanes (only those with tasks; `Finished` filtered by `filter`), in [`Lane::ORDER`]. What needs you goes by
/// [`needs_rank`] and then from the oldest wait (the first is the next to handle); what works, from the oldest (a
/// conflict being resolved first, the queue last); drafts and finished go from the newest.
pub fn lanes(tasks: &[Task], filter: HistoryFilter) -> Vec<(Lane, Vec<&Task>)> {
    Lane::ORDER
        .iter()
        .filter_map(|&lane| {
            let mut list: Vec<&Task> =
                tasks.iter().filter(|t| Lane::of(t.status) == lane && (lane != Lane::Finished || filter.matches(t.status))).collect();
            match lane {
                Lane::NeedsYou => list.sort_by_key(|t| (needs_rank(t), waiting_since(t), task_number(&t.id))),
                Lane::Working => {
                    list.sort_by_key(|t| (t.status == TaskStatus::Queued, !t.merge_approved, waiting_since(t), task_number(&t.id)))
                }
                Lane::Drafts | Lane::Finished => {
                    list.sort_by_key(|t| (std::cmp::Reverse(waiting_since(t)), std::cmp::Reverse(task_number(&t.id))))
                }
            }
            (!list.is_empty() || lane == Lane::Finished && tasks.iter().any(|t| Lane::of(t.status) == Lane::Finished))
                .then_some((lane, list))
        })
        .collect()
}

/// The 16 px state glyph of a row (one per state, see `ui/tasks.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Filled ring: your turn to review.
    Review,
    /// Ring with a check: merged, test it.
    Validate,
    /// Amber "!": the automatic merge stopped.
    Stopped,
    /// Red "!".
    Failed,
    /// Pulsing dot in the agent's colour.
    Running,
    /// Amber pulse: resolving a conflict, merges by itself.
    Conflict,
    /// Dashed ring.
    Queued,
    /// Hollow ring.
    Draft,
    /// Green check.
    Done,
    /// Faint ring.
    Discarded,
}

pub fn mark_of(task: &Task) -> Mark {
    match task.status {
        TaskStatus::Review if task.merge_stopped.is_some() => Mark::Stopped,
        TaskStatus::Review => Mark::Review,
        TaskStatus::Validating => Mark::Validate,
        TaskStatus::Failed => Mark::Failed,
        TaskStatus::Running if task.merge_approved => Mark::Conflict,
        TaskStatus::Running => Mark::Running,
        TaskStatus::Queued => Mark::Queued,
        TaskStatus::Draft => Mark::Draft,
        TaskStatus::Merged => Mark::Done,
        TaskStatus::Discarded => Mark::Discarded,
    }
}

/// The state word on a row (short: the row also carries the agent, the model and the diff).
pub fn state_word(task: &Task) -> &'static str {
    match mark_of(task) {
        Mark::Review => t!("Review"),
        Mark::Validate => t!("Test it"),
        Mark::Stopped => t!("Not merged"),
        Mark::Failed => t!("Failed"),
        Mark::Running => t!("Working"),
        Mark::Conflict => t!("Resolving a conflict"),
        Mark::Queued => t!("Queued"),
        Mark::Draft => t!("Draft"),
        Mark::Done => t!("Completed"),
        Mark::Discarded => t!("Discarded"),
    }
}

/// The state sentence at the top of the detail.
pub fn state_sentence(task: &Task) -> &'static str {
    match mark_of(task) {
        Mark::Review => t!("Waiting for your review"),
        Mark::Validate => t!("Merged · test it"),
        Mark::Stopped => t!("Not merged by itself"),
        Mark::Failed => t!("Failed"),
        Mark::Running => t!("Agent working"),
        Mark::Conflict => t!("Resolving a conflict"),
        Mark::Queued => t!("Queued"),
        Mark::Draft => t!("Draft"),
        Mark::Done => t!("Completed"),
        Mark::Discarded => t!("Discarded"),
    }
}

/// `1 h 24`, `12 min`, `<1 min`: how long a stage took.
pub fn duration_text(ms: u64) -> String {
    let min = ms / 60_000;
    match min {
        0 => t!("<1 min").to_owned(),
        1..=59 => t!("{n} min", n = min),
        _ => format!("{} h {:02}", min / 60, min % 60),
    }
}

/// Local `HH:MM` of a moment, or `dd/mm` when it was not today.
pub fn clock_text(at_ms: u64, now_ms: u64) -> String {
    use chrono::TimeZone;
    let (Some(at), Some(now)) =
        (chrono::Local.timestamp_millis_opt(at_ms as i64).single(), chrono::Local.timestamp_millis_opt(now_ms as i64).single())
    else {
        return String::new();
    };
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%d/%m").to_string()
    }
}

/// How a stop of the lifecycle track is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopState {
    Done,
    /// Where the task is; `Warn` / `Fail` colour it amber / red.
    Now,
    Warn,
    Fail,
    Later,
}

/// One of the five stops of the track: Created, Worked, Review, Merged, Validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub label: &'static str,
    pub state: StopState,
    /// The time under it (`12:31`, `1 h 24` for the work, empty when it has not happened).
    pub when: String,
}

/// The lifecycle track of the detail: where the task is, with real times. A conflict being resolved, a queue, a failure
/// or a discard rename the stop where they happened.
pub fn lifecycle(task: &Task, now_ms: u64) -> [Stop; 5] {
    use StopState::*;
    let at = |ms: Option<u64>| ms.map(|m| clock_text(m, now_ms)).unwrap_or_default();
    let worked = task.started_ms.map(|s| duration_text(task.finished_ms.unwrap_or(now_ms).saturating_sub(s))).unwrap_or_default();
    let stop = |label, state, when: String| Stop { label, state, when };
    let created = stop(t!("Created"), Done, at(Some(task.created_ms)));
    let later = |label| stop(label, Later, String::new());
    match task.status {
        TaskStatus::Draft => [
            stop(t!("Created"), Now, at(Some(task.created_ms))),
            later(t!("Worked")),
            later(t!("Review")),
            later(t!("Merged")),
            later(t!("Validated")),
        ],
        TaskStatus::Queued => {
            [created, stop(t!("Queued"), Now, at(task.queued_ms)), later(t!("Review")), later(t!("Merged")), later(t!("Validated"))]
        }
        TaskStatus::Running if task.merge_approved => [
            created,
            stop(t!("Worked"), Done, at(task.started_ms)),
            stop(t!("Conflict"), Warn, at(Some(last_activity(task)))),
            later(t!("Merged")),
            later(t!("Validated")),
        ],
        TaskStatus::Running => {
            [created, stop(t!("Working"), Now, worked), later(t!("Review")), later(t!("Merged")), later(t!("Validated"))]
        }
        TaskStatus::Failed if task.report.is_some() => [
            created,
            stop(t!("Worked"), Done, worked),
            stop(t!("Failed"), Fail, at(Some(last_activity(task)))),
            later(t!("Merged")),
            later(t!("Validated")),
        ],
        TaskStatus::Failed => [
            created,
            stop(t!("Failed"), Fail, at(Some(last_activity(task)))),
            later(t!("Review")),
            later(t!("Merged")),
            later(t!("Validated")),
        ],
        TaskStatus::Review => [
            created,
            stop(t!("Worked"), Done, worked),
            stop(t!("Review"), if task.merge_stopped.is_some() { Warn } else { Now }, at(task.finished_ms)),
            later(t!("Merged")),
            later(t!("Validated")),
        ],
        TaskStatus::Validating => [
            created,
            stop(t!("Worked"), Done, String::new()),
            stop(t!("Review"), Done, String::new()),
            stop(t!("Merged"), Now, at(task.finished_ms)),
            later(t!("Validated")),
        ],
        TaskStatus::Merged => [
            created,
            stop(t!("Worked"), Done, String::new()),
            stop(t!("Review"), Done, String::new()),
            stop(t!("Merged"), Done, String::new()),
            stop(t!("Validated"), Done, at(task.finished_ms)),
        ],
        TaskStatus::Discarded => {
            // The stops it reached stay done; the discard takes the next one.
            let reached = if task.report.is_some() {
                3
            } else if task.started_ms.is_some() {
                2
            } else {
                1
            };
            let mut stops = [created, later(t!("Worked")), later(t!("Review")), later(t!("Merged")), later(t!("Validated"))];
            for s in stops.iter_mut().take(reached).skip(1) {
                s.state = Done;
            }
            if let Some(s) = stops.get_mut(reached) {
                *s = Stop { label: t!("Discarded"), state: Fail, when: at(task.finished_ms) };
            }
            stops
        }
    }
}

/// An item of the detail's timeline: an event of the task or a message between agents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Moment<'a> {
    Event(&'a TaskEvent),
    Message(&'a TaskMessage),
}

impl Moment<'_> {
    pub fn at_ms(&self) -> u64 {
        match self {
            Moment::Event(e) => e.at_ms,
            Moment::Message(m) => m.at_ms,
        }
    }
}

/// Events and messages in one stream, newest first (at equal times, the message after the event it caused).
pub fn timeline(task: &Task) -> Vec<Moment<'_>> {
    let mut list: Vec<Moment> = task.events.iter().map(Moment::Event).chain(task.messages.iter().map(Moment::Message)).collect();
    list.sort_by_key(|m| (std::cmp::Reverse(m.at_ms()), matches!(m, Moment::Event(_))));
    list
}

/// The agent's last move for a working row: "asked antigravity" / "antigravity answered", with when.
pub fn last_move(task: &Task) -> Option<(String, u64)> {
    let m = task.messages.iter().max_by_key(|m| m.at_ms)?;
    let me = |who: Option<&str>| who == Some(task.instance.as_str());
    let text = match m.kind {
        FeedKind::Ask if me(m.from.as_deref()) => t!("asked {who}", who = m.to.as_deref().unwrap_or("…")),
        FeedKind::Answer if me(m.to.as_deref()) => t!("{who} answered", who = m.from.as_deref().unwrap_or("…")),
        FeedKind::Error => t!("{who} failed", who = m.from.as_deref().unwrap_or("…")),
        _ => t!("{from} → {to}", from = m.from.as_deref().unwrap_or("…"), to = m.to.as_deref().unwrap_or("…")),
    };
    Some((text, m.at_ms))
}

/// Which pane of the detail shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    /// The report and, under it, the changed files: what a review reads in one go.
    Report,
    Timeline,
    Request,
}

impl DetailTab {
    pub const ALL: [DetailTab; 3] = [DetailTab::Report, DetailTab::Timeline, DetailTab::Request];

    pub fn label(self) -> &'static str {
        match self {
            DetailTab::Report => t!("Report"),
            DetailTab::Timeline => t!("Timeline"),
            DetailTab::Request => t!("Request"),
        }
    }

    /// The pane a task opens on: the report once there is one, the timeline while an agent works, the request before.
    pub fn default_for(task: &Task) -> DetailTab {
        if task.report.as_deref().is_some_and(|r| !r.trim().is_empty()) {
            DetailTab::Report
        } else if task.started_ms.is_some() {
            DetailTab::Timeline
        } else {
            DetailTab::Request
        }
    }
}

/// `now`, `5 min ago`, `3 h ago`, `2 d ago` (a `then` in the future counts as now).
pub fn age_text(now_ms: u64, then_ms: u64) -> String {
    let secs = now_ms.saturating_sub(then_ms) / 1000;
    match secs {
        0..=59 => t!("now").to_owned(),
        60..=3599 => t!("{n} min ago", n = secs / 60),
        3600..=86_399 => t!("{n} h ago", n = secs / 3600),
        _ => t!("{n} d ago", n = secs / 86_400),
    }
}

/// `now`, `5 min`, `3 h`, `2 d`: the age on a row, where the room is short.
pub fn age_short(now_ms: u64, then_ms: u64) -> String {
    let secs = now_ms.saturating_sub(then_ms) / 1000;
    match secs {
        0..=59 => t!("now").to_owned(),
        60..=3599 => t!("{n} min", n = secs / 60),
        3600..=86_399 => t!("{n} h", n = secs / 3600),
        _ => t!("{n} d", n = secs / 86_400),
    }
}

/// What happened and when, for the card's second line (`ready 3 min ago`).
pub fn status_age(task: &Task, now_ms: u64) -> String {
    let finished = || task.finished_ms.unwrap_or_else(|| last_activity(task));
    let at = match task.status {
        TaskStatus::Draft => task.created_ms,
        TaskStatus::Queued => task.queued_ms.unwrap_or(task.created_ms),
        TaskStatus::Running => task.started_ms.unwrap_or(task.created_ms),
        TaskStatus::Failed => last_activity(task),
        TaskStatus::Review | TaskStatus::Validating | TaskStatus::Merged | TaskStatus::Discarded => finished(),
    };
    let age = age_text(now_ms, at);
    match task.status {
        TaskStatus::Draft => t!("created {age}", age),
        TaskStatus::Queued => t!("queued {age}", age),
        TaskStatus::Running => t!("started {age}", age),
        TaskStatus::Review => t!("ready {age}", age),
        TaskStatus::Failed => t!("failed {age}", age),
        TaskStatus::Validating => t!("merged {age}", age),
        TaskStatus::Merged => t!("completed {age}", age),
        TaskStatus::Discarded => t!("discarded {age}", age),
    }
}

/// LLM model to show (`opencode/glm-5.3` → `glm-5.3`); empty when it is the CLI's default.
pub fn model_text(task: &Task) -> Option<&str> {
    task.model.as_deref().map(models::short_model).filter(|m| !m.is_empty())
}

/// Effort to show next to the model ("high"); empty for the default. On Antigravity the effort already shows in the
/// model name (`…-high`), so it is not repeated.
pub fn effort_text(task: &Task) -> Option<&'static str> {
    let effort = task.effort?;
    let in_name = task.model.as_deref().is_some_and(|m| m.ends_with(&format!("-{}", effort.as_str())));
    (!in_name).then(|| effort.label())
}

/// "sonnet · high", "high", "sonnet" or nothing: what goes with the agent on the card and in the task detail.
pub fn model_effort_text(task: &Task) -> Option<String> {
    let parts: Vec<&str> = [model_text(task), effort_text(task)].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The scope typed in the form: one path per line or separated by commas; empty lines disappear. The
/// normalization and the refusal of what is not a path (wildcards, absolute paths, `..`) belong to the core, when the task is created.
pub fn parse_scope(text: &str) -> Vec<String> {
    text.split(['\n', ',']).map(str::trim).filter(|p| !p.is_empty()).map(str::to_owned).collect()
}

/// Buttons of a task's detail. The order is the one on screen (the first button is the main action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAct {
    Start,
    Retry,
    Merge,
    Return,
    Validate,
    Reject,
    GoToTab,
    ReopenTab,
    Discard,
}

impl TaskAct {
    pub fn label(self, task: &Task) -> String {
        match self {
            TaskAct::Start => t!("Start").to_owned(),
            TaskAct::Retry => t!("Try again").to_owned(),
            TaskAct::Merge => t!("Merge into {base}", base = task.base),
            TaskAct::Return => t!("Return").to_owned(),
            TaskAct::Validate => t!("Validate").to_owned(),
            TaskAct::Reject => t!("Reject").to_owned(),
            TaskAct::GoToTab => t!("Go to tab").to_owned(),
            TaskAct::ReopenTab => t!("Reopen tab").to_owned(),
            TaskAct::Discard => t!("Discard").to_owned(),
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            TaskAct::Start => t!("Creates the worktree and the branch, opens the agent and hands over the request"),
            TaskAct::Retry => t!("Tries again what failed (start, or merge if the agent already delivered the report)"),
            TaskAct::Merge => {
                t!("Merges the task branch into the base (git merge --no-ff); the agent and the worktree stay until you validate")
            }
            TaskAct::Return => t!("Sends the task back to the agent with a request for changes"),
            TaskAct::Validate => t!("You tested it and it is right: completes the task and deletes the worktree and the branch"),
            TaskAct::Reject => t!("You tested it and it failed: sends the task back to the same agent, with the reason"),
            TaskAct::GoToTab => t!("Shows the terminal of this task's agent"),
            TaskAct::ReopenTab => t!("Opens the agent's tab again, in the terminal of the task's worktree"),
            TaskAct::Discard => t!("Discards the task: deletes the worktree and the branch (without merging)"),
        }
    }

    /// Asks for confirmation inside the panel before running.
    pub fn needs_confirm(self, task: &Task) -> bool {
        match self {
            TaskAct::Discard | TaskAct::Merge => true,
            // Trying again with a report is a merge.
            TaskAct::Retry => task.report.is_some(),
            _ => false,
        }
    }
}

/// The buttons of each status. `tab_open`: the agent's tab is open (otherwise, "Reopen tab").
pub fn actions_for(task: &Task, tab_open: bool) -> Vec<TaskAct> {
    let tab = if tab_open { TaskAct::GoToTab } else { TaskAct::ReopenTab };
    match task.status {
        TaskStatus::Draft => vec![TaskAct::Start, TaskAct::Discard],
        // Starts by itself when the scope frees up; no tab or worktree yet.
        TaskStatus::Queued => vec![TaskAct::Discard],
        TaskStatus::Running => vec![tab, TaskAct::Discard],
        TaskStatus::Review => vec![TaskAct::Merge, TaskAct::Return, tab, TaskAct::Discard],
        TaskStatus::Validating => vec![TaskAct::Validate, TaskAct::Reject, tab, TaskAct::Discard],
        // With a report, the merge failed (in an earlier version): it can be sent back to the agent as in review.
        TaskStatus::Failed if task.report.is_some() => vec![TaskAct::Retry, TaskAct::Return, tab, TaskAct::Discard],
        TaskStatus::Failed => vec![TaskAct::Retry, TaskAct::Discard],
        TaskStatus::Merged | TaskStatus::Discarded => Vec::new(),
    }
}

/// Awaiting validation or completed, with the merge commit stored: the diff is what the merge brought
/// (the branch is already the base itself, and `base...branch` would be empty).
pub fn shows_merged_diff(task: &Task) -> bool {
    matches!(task.status, TaskStatus::Validating | TaskStatus::Merged) && task.merge_commit.as_deref().is_some_and(|h| !h.is_empty())
}

/// The diff can be viewed: the branch exists (the task was started and has not finished) or, after the merge, its
/// commit is stored.
pub fn can_show_diff(task: &Task) -> bool {
    shows_merged_diff(task)
        || (task.started_ms.is_some() && matches!(task.status, TaskStatus::Running | TaskStatus::Review | TaskStatus::Failed))
}

/// The label of the button that opens the diff.
pub fn diff_button_label(task: &Task) -> &'static str {
    if shows_merged_diff(task) {
        t!("View what was merged")
    } else {
        t!("View full diff")
    }
}

/// The tooltip of the button that opens the diff.
pub fn diff_button_tip(task: &Task) -> &'static str {
    if shows_merged_diff(task) {
        t!("Opens the diff of the merge commit: what the task brought into the base")
    } else {
        t!("Opens the git diff of the task branch against the base")
    }
}

/// The title of the diff screen.
pub fn diff_title(task: &Task) -> String {
    if shows_merged_diff(task) {
        t!("{id} · what the merge brought into {base}", id = task.id, base = task.base)
    } else {
        t!("{id} · changes against {base}", id = task.id, base = task.base)
    }
}

/// The short hash (7 characters) of the last merge commit, if the task has already been merged.
pub fn merge_short(task: &Task) -> Option<&str> {
    task.merge_commit.as_deref().filter(|h| !h.is_empty()).map(|h| h.get(..7).unwrap_or(h))
}

/// `branch → base`, with the short merge hash next to it when there is one.
pub fn branch_line(task: &Task) -> String {
    match merge_short(task) {
        Some(h) => format!("{} → {} · {h}", task.branch, task.base),
        None => format!("{} → {}", task.branch, task.base),
    }
}

/// Replaces the task with the same id in the list (or appends it).
pub fn upsert(tasks: &mut Vec<Task>, task: Task) {
    match tasks.iter_mut().find(|t| t.id == task.id) {
        Some(slot) => *slot = task,
        None => tasks.push(task),
    }
}

/// Local date and time of a history event (`17/10 14:32`).
pub fn event_time(at_ms: u64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(at_ms as i64) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => t.format("%d/%m %H:%M").to_string(),
        chrono::LocalResult::None => String::new(),
    }
}

/// Text cut at `max` characters, with an ellipsis and the notice (reports and requests can have 200 thousand).
pub fn clip_text(text: &str, max: usize) -> std::borrow::Cow<'_, str> {
    match text.char_indices().nth(max) {
        Some((i, _)) => {
            let notice = t!("[… text cut on screen: {shown} of {total} characters]", shown = max, total = text.chars().count());
            std::borrow::Cow::Owned(format!("{}\n\n{notice}", &text[..i]))
        }
        None => std::borrow::Cow::Borrowed(text),
    }
}

/// A collapsed message shows at most this much (in characters); a click opens it whole.
pub const MESSAGE_PREVIEW_CHARS: usize = 280;

/// Label of the message kind in the timeline.
pub fn message_kind_label(kind: FeedKind) -> &'static str {
    match kind {
        FeedKind::Ask => t!("question"),
        FeedKind::Answer => t!("answer"),
        FeedKind::Error => t!("error"),
        FeedKind::User => t!("you"),
        FeedKind::System => t!("notice"),
        FeedKind::Thinking => t!("progress"),
    }
}

/// The collapsed text of the message: the first lines, up to [`MESSAGE_PREVIEW_CHARS`], with an ellipsis. The second value
/// says whether something was left out (that is, whether there is something to open).
pub fn message_preview(text: &str) -> (String, bool) {
    let text = text.trim();
    let lines: Vec<&str> = text.lines().collect();
    let by_lines = lines.len() > 4;
    let head = if by_lines { lines[..4].join("\n") } else { text.to_owned() };
    match head.char_indices().nth(MESSAGE_PREVIEW_CHARS) {
        Some((i, _)) => (format!("{}…", head[..i].trim_end()), true),
        None if by_lines => (format!("{}…", head.trim_end()), true),
        None => (head, false),
    }
}
