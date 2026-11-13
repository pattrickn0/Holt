//! Hierarchy of a project's instances (who is whose boss), stored in
//! `ProjectCfg.tab_parents` (instance id → boss id), and the project's Orchestrators (`ProjectCfg.orchestrators` /
//! `ProjectCfg.orchestrators_off`). Pure functions; the `Core` applies them.
//!
//! Two different things live here: any agent that calls `ask-agent <other>` (or creates a task) becomes the **boss**
//! of whoever it opened; an **Orchestrator** is a project-level role assigned automatically to eligible roots leading
//! a hierarchy (`boss == None` with at least one open subordinate) or overridden per agent by the user (never a task
//! agent `tN`), persisted and shown by the bridge, `ask-agent` and the UI.

use std::collections::{BTreeMap, BTreeSet};

/// Boss and crew of an instance, plus the project's Orchestrators.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Team {
    /// Who the instance answers to (`None` = the user).
    pub boss: Option<String>,
    /// Who answers to it, in `open` order. Having a crew does not make an intermediate boss an Orchestrator:
    /// only roots leading a hierarchy (or manually pinned agents) hold the role.
    pub crew: Vec<String>,
    /// The project's effective Orchestrators that are currently open, in `open` order.
    pub orchestrators: Vec<String>,
    /// The instance itself is an Orchestrator for this project (auto-promoted root or manually pinned).
    pub is_orchestrator: bool,
}

/// `id` is the agent of a task (role `tN`, e.g. `claude@t3`): it lives only while the task does, so it cannot be an
/// Orchestrator.
pub fn is_task_agent(id: &str) -> bool {
    let role = id.split('@').nth(1).unwrap_or("");
    role.strip_prefix('t').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `true` if `id` can be promoted automatically to Orchestrator when leading a hierarchy (not a task agent `tN`
/// and not a plain `shell` terminal).
pub fn is_auto_orchestrator_eligible(id: &str) -> bool {
    !is_task_agent(id) && id.split('@').next().is_some_and(|k| !k.is_empty() && k != "shell")
}

/// `true` if `id` is an open, eligible root leading at least one open subordinate in `parents`.
pub fn is_auto_orchestrator(parents: &BTreeMap<String, String>, open: &[String], id: &str) -> bool {
    is_auto_orchestrator_eligible(id)
        && open.iter().any(|o| o == id)
        && !parents.get(id).is_some_and(|b| open.iter().any(|o| o == b))
        && open.iter().any(|o| o != id && parents.get(o.as_str()).is_some_and(|p| p == id))
}

/// Effective Orchestrators among `open` (in `open` order, deduplicated): manually `pinned` non-task instances,
/// plus eligible hierarchy roots (`is_auto_orchestrator`) not in `opted_out`.
pub fn effective_orchestrators(
    parents: &BTreeMap<String, String>,
    open: &[String],
    pinned: &BTreeSet<String>,
    opted_out: &BTreeSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for id in open {
        if out.iter().any(|existing| existing == id) {
            continue;
        }
        let is_orch = (!is_task_agent(id) && pinned.contains(id.as_str()))
            || (!opted_out.contains(id.as_str()) && is_auto_orchestrator(parents, open, id));
        if is_orch {
            out.push(id.clone());
        }
    }
    out
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

/// Boss and crew of `id`, considering only the open instances (`open`), and the project's effective Orchestrators
/// (`pinned` + auto-promoted hierarchy roots minus `opted_out`).
pub fn team_of_with(
    parents: &BTreeMap<String, String>,
    id: &str,
    open: &[String],
    pinned: &BTreeSet<String>,
    opted_out: &BTreeSet<String>,
) -> Team {
    let orchs = effective_orchestrators(parents, open, pinned, opted_out);
    Team {
        boss: parents.get(id).filter(|b| open.contains(b)).cloned(),
        crew: open.iter().filter(|o| parents.get(o.as_str()).is_some_and(|p| p == id)).cloned().collect(),
        is_orchestrator: orchs.iter().any(|o| o == id),
        orchestrators: orchs,
    }
}

/// Like [`team_of_with`], with no manual opt-outs (`orchestrators_off` empty).
pub fn team_of(parents: &BTreeMap<String, String>, id: &str, open: &[String], pinned: &BTreeSet<String>) -> Team {
    team_of_with(parents, id, open, pinned, &BTreeSet::new())
}

/// The instance `closed` left: removes it from both `pinned` and `opted_out`. `true` if either set changed.
pub fn close_orchestrator(pinned: &mut BTreeSet<String>, opted_out: &mut BTreeSet<String>, closed: &str) -> bool {
    pinned.remove(closed) | opted_out.remove(closed)
}

/// The instance `old` is now called `new`: migrates both `pinned` and `opted_out` (dropping if `new` is a task agent).
pub fn rename_orchestrator(pinned: &mut BTreeSet<String>, opted_out: &mut BTreeSet<String>, old: &str, new: &str) {
    let keep = !is_task_agent(new);
    if pinned.remove(old) && keep {
        pinned.insert(new.to_owned());
    }
    if opted_out.remove(old) && keep {
        opted_out.insert(new.to_owned());
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
        // `fechado` is not open, so `claude` is an open root leading `claude@dev` and `antigravity` → auto-Orchestrator.
        assert_eq!(
            team_of(&m, "claude", &open, &empty),
            Team { crew: ids(&["claude@dev", "antigravity"]), orchestrators: ids(&["claude"]), is_orchestrator: true, ..Team::default() }
        );
        assert_eq!(
            team_of(&m, "antigravity", &open, &empty),
            Team { boss: Some("claude".into()), orchestrators: ids(&["claude"]), ..Team::default() }
        );
    }

    #[test]
    fn intermediate_boss_is_not_orchestrator_while_roots_and_pinned_can_be_multiple() {
        // `antigravity` (root) -> `claude` (intermediate delegator) -> `claude@dev`.
        // `codex` (root) -> `codex@qa` (second independent root), plus `claude@dev` manually pinned.
        let m = map(&[("claude", "antigravity"), ("claude@dev", "claude"), ("codex@qa", "codex")]);
        let open = ids(&["antigravity", "claude", "claude@dev", "codex", "codex@qa"]);
        let pinned = set(&["claude@dev", "gone"]);
        let off = set(&[]);

        let mid = team_of_with(&m, "claude", &open, &pinned, &off);
        assert_eq!(mid.boss.as_deref(), Some("antigravity"));
        assert_eq!(mid.crew, ids(&["claude@dev"]));
        assert!(!mid.is_orchestrator, "an intermediate boss with crew does not auto-promote");
        assert_eq!(mid.orchestrators, ids(&["antigravity", "claude@dev", "codex"]));

        let root = team_of_with(&m, "antigravity", &open, &pinned, &off);
        assert!(root.is_orchestrator && root.boss.is_none());

        let pinned_leaf = team_of_with(&m, "claude@dev", &open, &pinned, &off);
        assert!(pinned_leaf.is_orchestrator, "manual pin survives having a boss");
    }

    #[test]
    fn auto_orchestrator_rules_and_overrides() {
        assert!(is_auto_orchestrator_eligible("claude"));
        assert!(is_auto_orchestrator_eligible("antigravity@lead@ana"));
        assert!(!is_auto_orchestrator_eligible("shell"));
        assert!(!is_auto_orchestrator_eligible("shell@dev"));
        assert!(!is_auto_orchestrator_eligible("claude@t1"));
        assert!(!is_auto_orchestrator_eligible(""));

        let m = map(&[("claude@dev", "claude"), ("shell@dev", "shell"), ("claude@t1", "claude"), ("leaf", "claude@t1")]);
        let open = ids(&["claude", "claude", "claude@dev", "shell", "shell@dev", "claude@t1", "leaf"]);
        assert!(is_auto_orchestrator(&m, &open, "claude"));
        assert!(!is_auto_orchestrator(&m, &open, "claude@dev"));
        assert!(!is_auto_orchestrator(&m, &open, "shell"), "shell never auto-promotes");
        assert!(!is_auto_orchestrator(&m, &open, "claude@t1"), "tN never auto-promotes");
        assert!(!is_auto_orchestrator(&m, &open, "missing"));

        // Deduplicated in `open` order; `orchestrators_off` suppresses auto; manual `pinned` still allows shell.
        assert_eq!(effective_orchestrators(&m, &open, &set(&["shell", "claude@t1"]), &set(&[])), ids(&["claude", "shell"]));
        assert_eq!(effective_orchestrators(&m, &open, &set(&[]), &set(&["claude"])), Vec::<String>::new());
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
        let mut pinned = set(&["claude", "codex"]);
        let mut off = set(&["antigravity"]);
        assert!(!close_orchestrator(&mut pinned, &mut off, "gemini"));
        rename_orchestrator(&mut pinned, &mut off, "gemini", "gemini@dev");
        assert_eq!((&pinned, &off), (&set(&["claude", "codex"]), &set(&["antigravity"])));

        rename_orchestrator(&mut pinned, &mut off, "claude", "claude@lead");
        rename_orchestrator(&mut pinned, &mut off, "antigravity", "antigravity@lead");
        assert_eq!(pinned, set(&["claude@lead", "codex"]));
        assert_eq!(off, set(&["antigravity@lead"]));

        rename_orchestrator(&mut pinned, &mut off, "claude@lead", "claude@t9");
        rename_orchestrator(&mut pinned, &mut off, "antigravity@lead", "antigravity@t2");
        assert_eq!(pinned, set(&["codex"]), "a task agent cannot hold the role");
        assert!(off.is_empty(), "opt-out also drops when renamed to a task agent");

        assert!(close_orchestrator(&mut pinned, &mut off, "codex"));
        assert!(pinned.is_empty());
        let mut off_only = set(&["claude"]);
        assert!(close_orchestrator(&mut pinned, &mut off_only, "claude"));
        assert!(off_only.is_empty());
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
