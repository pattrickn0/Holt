//! The small pure functions behind what a row and the detail show: names, dates, counts and stats.

use super::*;

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Status name for the chip (capitalized, unlike `TaskStatus::label`).
pub fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft => t!("Draft"),
        TaskStatus::Queued => t!("Queued"),
        TaskStatus::Running => t!("In progress"),
        TaskStatus::Review => t!("In review"),
        TaskStatus::Validating => t!("Awaiting validation"),
        TaskStatus::Merged => t!("Completed"),
        TaskStatus::Discarded => t!("Discarded"),
        TaskStatus::Failed => t!("Failed"),
    }
}

/// Filter of the "Finished" lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryFilter {
    #[default]
    All,
    Done,
    Discarded,
}

impl HistoryFilter {
    pub const ALL: [HistoryFilter; 3] = [HistoryFilter::All, HistoryFilter::Done, HistoryFilter::Discarded];

    pub fn label(self) -> &'static str {
        match self {
            HistoryFilter::All => t!("Completed and discarded"),
            HistoryFilter::Done => t!("Completed only"),
            HistoryFilter::Discarded => t!("Discarded only"),
        }
    }

    pub fn matches(self, status: TaskStatus) -> bool {
        match self {
            HistoryFilter::All => matches!(status, TaskStatus::Merged | TaskStatus::Discarded),
            HistoryFilter::Done => status == TaskStatus::Merged,
            HistoryFilter::Discarded => status == TaskStatus::Discarded,
        }
    }
}

/// Short `dd/mm` date of the end (or last activity) for the history row.
pub fn task_date_ddmm(task: &Task) -> String {
    use chrono::TimeZone;
    let at_ms = task.finished_ms.unwrap_or_else(|| last_activity(task));
    match chrono::Local.timestamp_millis_opt(at_ms as i64) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => t.format("%d/%m").to_string(),
        chrono::LocalResult::None => String::new(),
    }
}

/// The last thing that happened to the task (ms): the last history event, or the creation.
pub fn last_activity(task: &Task) -> u64 {
    task.events.iter().map(|e| e.at_ms).max().unwrap_or(0).max(task.created_ms)
}

/// `t12` → 12 (0 if it is not a task id).
pub fn task_number(id: &str) -> u64 {
    id.strip_prefix('t').and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// How many tasks wait for the human's review.
pub fn review_count(tasks: &[Task]) -> usize {
    tasks.iter().filter(|t| t.status == TaskStatus::Review).count()
}

/// How many tasks, already merged, wait for the human's test.
pub fn validating_count(tasks: &[Task]) -> usize {
    tasks.iter().filter(|t| t.status == TaskStatus::Validating).count()
}

/// The `git diff --stat` summary: files, added lines and removed lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffSummary {
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
}

/// Reads the last line of `git diff --stat` ("3 files changed, 120 insertions(+), 8 deletions(-)").
/// "(no changes against the base)" (what the core stores when there are no differences; tasks saved by an
/// older version have the Portuguese "(nenhuma alteração em relação à base)") becomes zero files.
pub fn parse_diff_stat(stat: &str) -> Option<DiffSummary> {
    if stat.contains("nenhuma alteração") || stat.trim_start().starts_with("(no changes") {
        return Some(DiffSummary { files: 0, insertions: 0, deletions: 0 });
    }
    let line = stat.lines().rev().map(str::trim).find(|l| l.contains("changed"))?;
    let mut sum = DiffSummary { files: 0, insertions: 0, deletions: 0 };
    let mut found = false;
    for part in line.split(',') {
        let part = part.trim();
        let Some(n) = part.split_whitespace().next().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if part.contains("file") {
            sum.files = n;
            found = true;
        } else if part.contains("insertion") {
            sum.insertions = n;
        } else if part.contains("deletion") {
            sum.deletions = n;
        }
    }
    found.then_some(sum)
}

/// One line per file of `git diff --stat`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatLine {
    pub path: String,
    /// Changed lines (`412`), or `Bin 0 -> 1234 bytes` for a binary file.
    pub changes: String,
    /// The share of insertions among the changed lines (see [`stat_plus_share`]).
    pub plus: Option<f32>,
}

/// The files of `git diff --stat` (` path | 12 ++++--`); the total line is not included.
pub fn parse_stat_lines(stat: &str) -> Vec<StatLine> {
    stat.lines()
        .filter_map(|line| {
            let (path, rest) = line.rsplit_once('|')?;
            let path = path.trim();
            let rest = rest.trim();
            let changes = match rest.split_whitespace().next() {
                Some(n) if n.chars().all(|c| c.is_ascii_digit()) => n,
                _ => rest,
            };
            (!path.is_empty() && !changes.is_empty()).then(|| StatLine {
                path: path.to_owned(),
                changes: changes.to_owned(),
                plus: stat_plus_share(rest),
            })
        })
        .collect()
}

/// The +/− split of a file's line in `git diff --stat` (the `++++--` graph): the share of insertions, `None` for a binary
/// file or a line without a graph.
pub fn stat_plus_share(stat_line_rest: &str) -> Option<f32> {
    let graph = stat_line_rest.split_whitespace().nth(1)?;
    let plus = graph.chars().filter(|c| *c == '+').count();
    let minus = graph.chars().filter(|c| *c == '-').count();
    (plus + minus > 0 && plus + minus == graph.chars().count()).then(|| plus as f32 / (plus + minus) as f32)
}
