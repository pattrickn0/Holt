//! Hierarchy of a project's instances (who is whose boss), stored in
//! `ProjectCfg.tab_parents` (instance id → boss id), and the project's Orchestrators (`ProjectCfg.orchestrators`).
//! Pure functions; the `Core` applies them.
//!
//! Two different things live here: any agent that calls `ask-agent <other>` (or creates a task) becomes the **boss**
//! of whoever it opened, a temporary place in the tree; an **Orchestrator** is a project-level role the user
//! toggles per agent (never a task agent `tN`), persisted and shown by the bridge, `ask-agent` and the UI.

use std::collections::{BTreeMap, BTreeSet};

/// Boss and crew of an instance, plus the project's Orchestrators.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Team {
    /// Who the instance answers to (`None` = the user).
    pub boss: Option<String>,
    /// Who answers to it, in `open` order. Having a crew does not make an instance an Orchestrator: any agent that
    /// opened helpers has one.
    pub crew: Vec<String>,
    /// The project's Orchestrators that are currently open, in `open` order.
    pub orchestrators: Vec<String>,
    /// The instance itself is designated as an Orchestrator for this project.
    pub is_orchestrator: bool,
}

/// `id` is the agent of a task (role `tN`, e.g. `claude@t3`): it lives only while the task does, so it cannot be an
/// Orchestrator.
pub fn is_task_agent(id: &str) -> bool {
    let role = id.split('@').nth(1).unwrap_or("");
    role.strip_prefix('t').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Putting `child` under `parent` would create a cycle (`parent` is `child` itself or someone in its crew).
pub fn would_cycle(parents: &BTreeMap<String, String>, child: &str, parent: &str) -> bool {
    let mut cur = Some(parent);
    // Without a cycle the chain has at most `len + 1` ids before it ends; the limit covers a map already
    // corrupted with a cycle (it does not hang).
    for _ in 0..parents.len() + 2 {
        match cur {
            Some(id) if id == child => return true,
            Some(id) => cur = parents.get(id).map(String::as_str),
            None => return false,
        }
    }
    true
}

/// Boss and crew of `id`, considering only the open instances (`open`), and the project's `orchestrators` (only open ones).
pub fn team_of(parents: &BTreeMap<String, String>, id: &str, open: &[String], orchestrators: &BTreeSet<String>) -> Team {
    let open_orchs = open_orchestrators(orchestrators, open);
    Team {
        boss: parents.get(id).filter(|b| open.contains(b)).cloned(),
        crew: open.iter().filter(|o| parents.get(o.as_str()).is_some_and(|p| p == id)).cloned().collect(),
        is_orchestrator: open_orchs.iter().any(|o| o == id),
        orchestrators: open_orchs,
    }
}

/// The saved Orchestrators that are currently open instances, in `open` order.
pub fn open_orchestrators(orchestrators: &BTreeSet<String>, open: &[String]) -> Vec<String> {
    open.iter().filter(|id| orchestrators.contains(id.as_str())).cloned().collect()
}

/// The instance `closed` left: removes it from the project's Orchestrators if it was one. `true` if it changed.
pub fn close_orchestrator(orchestrators: &mut BTreeSet<String>, closed: &str) -> bool {
    orchestrators.remove(closed)
}

/// The instance `old` is now called `new`: its Orchestrator status follows it (and is dropped if the new id is a task agent).
pub fn rename_orchestrator(orchestrators: &mut BTreeSet<String>, old: &str, new: &str) {
    if orchestrators.remove(old) && !is_task_agent(new) {
        orchestrators.insert(new.to_owned());
    }
}

/// The instance `closed` left: its crew passes to its boss (or to the root). Returns who changed boss
/// (to be told), plus its own boss, if any: it lost a subordinate (and may have inherited the crew).
pub fn remove(parents: &mut BTreeMap<String, String>, closed: &str) -> Vec<String> {
    let boss = parents.remove(closed);
    let crew: Vec<String> = parents.iter().filter(|(_, p)| *p == closed).map(|(c, _)| c.clone()).collect();
    for c in &crew {
        match &boss {
            Some(b) => parents.insert(c.clone(), b.clone()),
            None => parents.remove(c),
        };
    }
    let mut touched = crew;
    touched.extend(boss);
    touched
}

/// The instance `old` is now called `new`.
pub fn rename(parents: &mut BTreeMap<String, String>, old: &str, new: &str) {
    if let Some(p) = parents.remove(old) {
        parents.insert(new.to_owned(), p);
    }
    for p in parents.values_mut() {
        if p == old {
            *p = new.to_owned();
        }
    }
}

/// Notice attached to the next message delivered to an instance whose place in the team changed.
pub fn notice(team: &Team) -> String {
    let boss = match &team.boss {
        Some(b) => format!("you answer to \"{b}\" (report to him with ask-agent --boss \"message\")"),
        None => "you answer directly to the user".to_owned(),
    };
    let crew = if team.crew.is_empty() { "no agents".to_owned() } else { team.crew.join(", ") };
    let orchestrator = if team.is_orchestrator {
        format!(" {}", orchestrator_note())
    } else if !team.orchestrators.is_empty() {
        format!(" Project orchestrators: {}.", team.orchestrators.join(", "))
    } else {
        String::new()
    };
    format!(
        "[Holt] Your place in the team changed: {boss}; your crew: {crew}. Delegate preferably to your crew.{orchestrator} \
Check it at any time with ask-agent --team."
    )
}

/// What an instance designated as a project Orchestrator is told (instructions and team notice): its software
/// role, against a regular agent that merely opened helpers.
pub fn orchestrator_note() -> &'static str {
    "You are an Orchestrator for this project: the user designated you in Holt to coordinate the project's agents and tasks. \
Plan the work, split it into tasks (ask-agent --task new), pick agents and models by quota (ask-agent --models), follow \
them up and review what they deliver; prefer delegating to doing the changes yourself. Any agent that opens helpers \
becomes their boss, but only designated Orchestrators hold the Orchestrator role (ask-agent --team shows it)."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(c, p)| (c.to_string(), p.to_string())).collect()
    }

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_cycle_is_refused() {
        let m = map(&[("claude@dev", "claude"), ("codex", "claude@dev")]);
        assert!(would_cycle(&m, "claude", "claude"), "boss of itself");
        assert!(would_cycle(&m, "claude", "codex"), "the boss would end up below its own subordinate");
        assert!(!would_cycle(&m, "codex", "claude"));
        assert!(!would_cycle(&m, "antigravity", "codex"));
        // A map already corrupted does not hang.
        assert!(would_cycle(&map(&[("a", "b"), ("b", "a")]), "c", "a"));
    }

    #[test]
    fn the_team_only_has_open_instances() {
        let m = map(&[("claude@dev", "claude"), ("antigravity", "claude"), ("claude", "fechado")]);
        let open = ids(&["claude", "claude@dev", "antigravity"]);
        let empty = BTreeSet::new();
        assert_eq!(team_of(&m, "claude", &open, &empty), Team { crew: ids(&["claude@dev", "antigravity"]), ..Team::default() });
        assert_eq!(team_of(&m, "antigravity", &open, &empty), Team { boss: Some("claude".into()), ..Team::default() });
    }

    #[test]
    fn orchestrators_are_distinct_from_a_boss_with_crew_and_can_be_multiple() {
        // `claude` opened helpers (it has a crew), while `codex` and `antigravity` are designated Orchestrators.
        let m = map(&[("claude@dev", "claude"), ("antigravity", "claude")]);
        let open = ids(&["claude", "claude@dev", "antigravity", "codex"]);
        let orchs = set(&["codex", "antigravity", "gone"]);
        let delegator = team_of(&m, "claude", &open, &orchs);
        assert_eq!(delegator.crew, ids(&["claude@dev", "antigravity"]));
        assert!(!delegator.is_orchestrator, "a crew does not make an Orchestrator");
        assert_eq!(delegator.orchestrators, ids(&["antigravity", "codex"]));

        let orch = team_of(&m, "codex", &open, &orchs);
        assert!(orch.is_orchestrator && orch.crew.is_empty() && orch.boss.is_none());
        assert_eq!(orch.orchestrators, ids(&["antigravity", "codex"]));
    }

    #[test]
    fn task_agents_are_recognized() {
        for id in ["claude@t1", "codex@t42", "claude@t3@ana"] {
            assert!(is_task_agent(id), "{id}");
        }
        for id in ["claude", "claude@dev", "claude@t", "claude@tx1", "claude@test", "t1", "claude@dev@t1"] {
            assert!(!is_task_agent(id), "{id}");
        }
    }

    #[test]
    fn orchestrators_follow_close_and_rename() {
        let mut o = set(&["claude", "codex"]);
        assert!(!close_orchestrator(&mut o, "antigravity"));
        rename_orchestrator(&mut o, "antigravity", "antigravity@dev");
        assert_eq!(o, set(&["claude", "codex"]));
        rename_orchestrator(&mut o, "claude", "claude@lead");
        assert_eq!(o, set(&["claude@lead", "codex"]));
        rename_orchestrator(&mut o, "claude@lead", "claude@t9");
        assert_eq!(o, set(&["codex"]), "a task agent cannot hold the role");
        assert!(close_orchestrator(&mut o, "codex"));
        assert!(o.is_empty());
    }

    #[test]
    fn the_notice_names_the_orchestrators() {
        let team = Team { orchestrators: ids(&["codex", "antigravity"]), ..Team::default() };
        assert!(notice(&team).contains("Project orchestrators: codex, antigravity."));
        let me = Team { orchestrators: ids(&["codex"]), is_orchestrator: true, ..Team::default() };
        assert!(notice(&me).contains("You are an Orchestrator for this project"));
        assert!(!notice(&Team::default()).contains("Orchestrator"));
    }

    #[test]
    fn closing_hands_the_crew_to_the_grandparent() {
        let mut m = map(&[("dev", "claude"), ("codex", "dev"), ("qa", "dev")]);
        let mut touched = remove(&mut m, "dev");
        touched.sort();
        assert_eq!(touched, ids(&["claude", "codex", "qa"]));
        assert_eq!(m, map(&[("codex", "claude"), ("qa", "claude")]));
        // Without a boss, the crew goes to the root.
        assert_eq!(remove(&mut m, "claude"), ids(&["codex", "qa"]));
        assert!(m.is_empty());
    }

    #[test]
    fn the_boss_that_loses_a_subordinate_without_crew_is_told() {
        let mut m = map(&[("dev", "claude"), ("qa", "claude")]);
        assert_eq!(remove(&mut m, "dev"), ids(&["claude"]));
        assert_eq!(m, map(&[("qa", "claude")]));
        // Without boss or crew, nobody changes place.
        assert!(remove(&mut m, "solto").is_empty());
    }

    #[test]
    fn renaming_keeps_the_tree() {
        let mut m = map(&[("dev", "claude"), ("codex", "dev")]);
        rename(&mut m, "dev", "dev@ana");
        assert_eq!(m, map(&[("dev@ana", "claude"), ("codex", "dev@ana")]));
    }
}
