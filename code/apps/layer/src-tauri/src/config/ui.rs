//! The app's look, its banners, and how its terminals behave: Settings →
//! Appearance, Notifications and Agents.

use serde::{Deserialize, Serialize};

use super::yes;

/// Presentation preferences. Terminal text scales separately from the chrome,
/// because xterm measures its own cell grid and a page zoom would fight it.
///
/// Every field defaults, the struct as a whole: one missing field without
/// its own default made the whole config unreadable, and the app started
/// with no tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub scale: f32,
    /// The app's colours (SET-5): a palette `THEMES` names, or "system" for
    /// the Mac's own appearance, followed as it changes. Nightshade's night
    /// by default, the studio's theme. "dark" and "light", the names before
    /// Nightshade, read as its night and day.
    #[serde(default = "crate::theme::night", deserialize_with = "crate::theme::saved_name")]
    pub theme: String,
    pub terminal_font_size: u16,
    /// The text of an agent's conversation over ACP (ACP-15), which is the
    /// app's own page and not a terminal, so neither of the others.
    pub conversation_font_size: u16,
    /// Put back the panes that were open when the app last closed, resuming
    /// each agent's conversation where its CLI can.
    #[serde(default = "yes")]
    pub restore_panes: bool,
    /// How many panes may run at once (PANE-2): 32, raised for someone who
    /// runs more agents than that, up to `pty::MAX_PANES`. A ceiling either
    /// way; `PtyManager::set_limit` keeps it between the two.
    pub pane_limit: usize,
    /// Answer Claude Code's workspace-trust dialog for folders this app made,
    /// so a new task or chat starts working instead of waiting on a question.
    #[serde(default = "yes")]
    pub trust_agent_dirs: bool,
    /// Move the ticket into progress when a task is started for it, so the
    /// board and this app do not disagree about what is being worked on.
    #[serde(default = "yes")]
    pub sync_jira_status: bool,
    /// A banner when a review is requested or a ticket is assigned, and only
    /// while the window is in the background. On by default: the first sweep
    /// after launch is silent, so turning the app on does not announce the
    /// queue that was already waiting.
    #[serde(default = "yes")]
    pub system_notifications: bool,
    /// A banner when an agent stops to wait on you — gone quiet, asking to
    /// trust its folder, out of budget, or exited — while the window is in the
    /// background, and the count of those waiting on the dock icon.
    #[serde(default = "yes")]
    pub notify_waiting_agents: bool,
    /// Let agents read the output of terminals in this app.
    ///
    /// Off by default, and deliberately: a shell's scrollback holds whatever
    /// has been typed and whatever printed, which can include tokens echoed by
    /// a failing script or a remote URL with credentials in it.
    #[serde(default)]
    pub agents_read_panes: bool,
    /// Keep the Mac from idle sleep while an agent is working (STATE-7).
    /// Off by default: on battery, an agent left working overnight would
    /// keep the machine up until it ran flat.
    #[serde(default)]
    pub keep_awake: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            scale: 1.0,
            theme: crate::theme::night(),
            terminal_font_size: 13,
            conversation_font_size: 14,
            restore_panes: true,
            pane_limit: crate::pty::PANE_LIMIT,
            trust_agent_dirs: true,
            sync_jira_status: true,
            system_notifications: true,
            notify_waiting_agents: true,
            agents_read_panes: false,
            keep_awake: false,
        }
    }
}
