//! UI-side agent registry: colors/names (the renderer's COLORS/NAMES) and the menu lists
//! (`buildMenus`).

use std::collections::{HashMap, HashSet};

use egui::Color32;
use holt_core::agents::{COLOR_ANTIGRAVITY, COLOR_CLAUDE, COLOR_CODEX, COLOR_GEMINI, COLOR_QWEN, COLOR_SHELL};
use holt_core::{t, AgentInfo};

use crate::theme;

/// Label shown for the plain terminal (the original renderer always uses "PowerShell").
pub fn shell_label(core_label: Option<&str>) -> String {
    if cfg!(windows) {
        "PowerShell".into()
    } else {
        core_label.filter(|l| !l.is_empty()).unwrap_or("Terminal").to_owned()
    }
}

/// Menu item (new tab / empty state).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub kind: String,
    pub label: String,
    pub color: Color32,
}

/// State derived from the agent list.
#[derive(Debug, Default, Clone)]
pub struct Agents {
    pub loaded: bool,
    colors: HashMap<String, Color32>,
    names: HashMap<String, String>,
    local_models: HashMap<String, String>,
    custom: HashSet<String>,
    /// `availableAgents`
    pub available: Vec<AgentInfo>,
    /// "+" menu items (`renderNewTabMenu`).
    pub new_tab_menu: Vec<MenuItem>,
    /// Empty-state buttons (`renderEmptyStateActions`).
    pub empty_actions: Vec<MenuItem>,
}

impl Agents {
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of `buildMenus(agents)`.
    pub fn build(&mut self, agents: &[AgentInfo]) {
        self.colors.clear();
        self.names.clear();
        self.local_models.clear();
        self.custom.clear();
        let color_of = |a: &AgentInfo| theme::parse_hex_color(&a.color).unwrap_or(theme::SHELL);
        for a in agents {
            self.colors.insert(a.id.clone(), color_of(a));
            let name = if a.id == "shell" { shell_label(Some(&a.label)) } else { a.label.clone() };
            self.names.insert(a.id.clone(), name);
            if let Some(m) = &a.local_model {
                self.local_models.insert(a.id.clone(), m.clone());
            }
            if a.custom {
                self.custom.insert(a.id.clone());
            }
        }
        self.colors.entry("shell".into()).or_insert(theme::SHELL);
        let shell_core = agents.iter().find(|a| a.id == "shell");
        self.names.insert("shell".into(), shell_label(shell_core.map(|a| a.label.as_str())));

        self.available = agents.iter().filter(|a| a.installed || a.id == "shell").cloned().collect();
        let ai: Vec<&AgentInfo> = self.available.iter().filter(|a| a.id != "shell" && a.installed).collect();
        let shell_color = shell_core.map(color_of).unwrap_or(theme::SHELL);
        let shell_name = shell_label(shell_core.map(|a| a.label.as_str()));

        let display_label = |a: &AgentInfo| match &a.local_model {
            Some(m) if a.installed && !m.is_empty() => format!("{} · {m}", a.label),
            _ => a.label.clone(),
        };

        self.new_tab_menu = ai
            .iter()
            .map(|a| MenuItem { kind: a.id.clone(), label: display_label(a), color: color_of(a) })
            .chain(std::iter::once(MenuItem { kind: "shell".into(), label: shell_name.clone(), color: shell_color }))
            .collect();
        self.empty_actions = ai
            .iter()
            .map(|a| MenuItem { kind: a.id.clone(), label: t!("Open {agent}", agent = display_label(a)), color: color_of(a) })
            .chain(std::iter::once(MenuItem { kind: "shell".into(), label: t!("Open {agent}", agent = shell_name), color: shell_color }))
            .collect();
        self.loaded = true;
    }

    /// `true` when `kind` was manually added by the user (`Add CLI…`).
    pub fn is_custom(&self, kind: &str) -> bool {
        self.custom.contains(kind)
    }

    /// Local model detected for `kind` (`ollama list`, Mimo config, etc.).
    pub fn local_model(&self, kind: &str) -> Option<String> {
        self.local_models.get(kind).cloned()
    }

    /// `COLORS[kind]`.
    pub fn color(&self, kind: &str) -> Option<Color32> {
        self.colors.get(kind).copied()
    }

    /// Tab color: `COLORS[kind] || '#9aa0a6'`, with the official colors as a fallback when the
    /// backend has not listed the agent yet.
    pub fn tab_color(&self, kind: &str) -> Color32 {
        self.color(kind).unwrap_or_else(|| {
            let hex = match kind {
                "claude" => COLOR_CLAUDE,
                "antigravity" => COLOR_ANTIGRAVITY,
                "codex" => COLOR_CODEX,
                "gemini" => COLOR_GEMINI,
                "qwen" => COLOR_QWEN,
                _ => COLOR_SHELL,
            };
            theme::parse_hex_color(hex).unwrap_or(theme::SHELL)
        })
    }

    /// `NAMES[kind] || kind` (with the official labels as a fallback).
    pub fn name(&self, kind: &str) -> String {
        if let Some(n) = self.names.get(kind) {
            return n.clone();
        }
        match kind {
            "claude" => "Claude".into(),
            "antigravity" => "Antigravity".into(),
            "codex" => "Codex".into(),
            "shell" => shell_label(None),
            other => other.to_owned(),
        }
    }

    /// Restoration only opens tabs of known kinds (`if (COLORS[kind])`).
    pub fn knows(&self, kind: &str) -> bool {
        self.colors.contains_key(kind)
    }

    /// Default kind for Ctrl+T with no active tab.
    pub fn fallback_kind(&self) -> String {
        self.available.iter().find(|a| a.id != "shell").or(self.available.first()).map(|a| a.id.clone()).unwrap_or_else(|| "shell".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, label: &str, installed: bool, peer: bool) -> AgentInfo {
        AgentInfo { id: id.into(), label: label.into(), color: "#d97757".into(), installed, peer, ..Default::default() }
    }

    #[test]
    fn build_menus_and_names() {
        let mut a = Agents::new();
        a.build(&[
            agent("claude", "Claude", false, true),
            agent("codex", "Codex", false, true),
            agent("opencode", "OpenCode", true, true),
            AgentInfo {
                id: "ollama".into(),
                label: "Ollama".into(),
                color: "#38bdf8".into(),
                installed: true,
                peer: true,
                local_model: Some("qwen2.5-coder:7b".into()),
                ..Default::default()
            },
            AgentInfo {
                id: "my-cli".into(),
                label: "My CLI".into(),
                color: "#a3e635".into(),
                installed: true,
                peer: true,
                custom: true,
                command: Some("my-cli --chat".into()),
                local_model: None,
            },
            agent("shell", "Terminal", true, false),
        ]);
        let kinds: Vec<&str> = a.new_tab_menu.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(kinds, vec!["opencode", "ollama", "my-cli", "shell"]);
        assert_eq!(a.new_tab_menu[0].label, "OpenCode");
        assert_eq!(a.new_tab_menu[1].label, "Ollama · qwen2.5-coder:7b");
        assert_eq!(a.empty_actions[0].label, "Open OpenCode");
        assert_eq!(a.empty_actions[1].label, "Open Ollama · qwen2.5-coder:7b");
        assert!(a.knows("opencode") && a.knows("ollama") && a.knows("my-cli"));
        assert!(a.is_custom("my-cli") && !a.is_custom("ollama"));
        assert_eq!(a.local_model("ollama").as_deref(), Some("qwen2.5-coder:7b"));
        assert_eq!(a.name("opencode"), "OpenCode");
        assert_eq!(a.name("my-cli"), "My CLI");
        assert_eq!(a.name("unknown"), "unknown");
        assert_eq!(a.fallback_kind(), "opencode");
    }
}
