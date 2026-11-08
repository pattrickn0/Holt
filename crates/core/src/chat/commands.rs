//! Slash commands only the terminal understands, and the per-tab chat/terminal choice.

use crate::Core;

use super::*;

pub const TERMINAL_ONLY_COMMANDS: &[&str] =
    &["config", "login", "logout", "theme", "terminal-setup", "vim", "ide", "resume", "statusline", "output-style", "keybindings"];

// ---------- Slash commands ----------

/// What the composer does with a line that starts with `/`: the name of the command, if it is a terminal-only one.
pub fn terminal_only_command(text: &str, from_cli: &[String]) -> Option<String> {
    let name = text.trim().strip_prefix('/')?.split_whitespace().next()?.to_lowercase();
    (TERMINAL_ONLY_COMMANDS.contains(&name.as_str()) || from_cli.contains(&name)).then_some(name)
}

// ---------- View of a tab ----------

impl Core {
    /// Whether the tab shows the chat view: the user's choice for this tab, else the default of its CLI.
    pub fn tab_is_chat(&self, project: &str, ident: &crate::Ident) -> bool {
        if !supports(&ident.kind) {
            return false;
        }
        let cfg = self.inner.config.read();
        match cfg.project(project).and_then(|p| p.tab_views.get(&ident.id())) {
            Some(choice) => choice != "terminal",
            None => cfg.chat_by_default,
        }
    }

    /// Saves the user's choice of view for the tab.
    pub fn set_tab_chat(&self, project: &str, id: &str, chat: bool) {
        self.update_project(project, |p| {
            p.tab_views.insert(id.to_owned(), if chat { "chat" } else { "terminal" }.to_owned());
        });
    }

    /// Reads the saved conversation of a tab from the CLI's own transcript, for a resumed chat to show it.
    pub fn chat_history(&self, project: &str, id: &str) -> Transcript {
        let Some(session) = self.tab_session(project, id) else { return Transcript::default() };
        let kind = crate::Ident::parse(id).map(|i| i.kind).unwrap_or_default();
        let Some(mut t) = for_kind(&kind).and_then(|b| b.history(&self.inner.home, &session)) else { return Transcript::default() };
        t.session = Some(session);
        t
    }
}
