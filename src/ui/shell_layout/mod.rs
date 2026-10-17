//! Main window columns: which of the chat list, the forum topic column, the
//! conversation and the side panels (info, downloads, shared media) are on
//! screen, and how wide each is, for the window's width.
//!
//! Telegram Desktop decides this in one place too
//! (`SessionController::computeColumnLayout`): below the chat list's
//! minimum plus the chat's minimum (`columnMinimalWidthLeft` +
//! `columnMinimalWidthMain`) the window shows one column, the list or the
//! chat; a third column only stays when the chat keeps its minimum beside
//! it, and otherwise the info section takes the chat's place. Thresholds:
//! docs/decisions/sweep-window-narrow-layout.md.
//!
//! [`shell_layout`] is a pure function of [`ShellInput`]; `QuillApp` reads
//! its inputs from state each frame (`view.rs`) and every caller (the
//! shell, the chat list's fit checks, the chat header's back button)
//! derives from it.
#![deny(clippy::indexing_slicing)]

mod view;

#[cfg(test)]
mod tests;

use quill::settings::MIN_SIDEBAR_WIDTH;
use quill::state::{FORUM_COLUMN_WIDTH, ForumColumn};

/// Narrowest the conversation gets beside other columns (tdesktop
/// `columnMinimalWidthMain`, `historyMinimalWidth`).
pub(crate) const CHAT_MIN_WIDTH: f32 = 380.;
/// Narrowest a side panel gets beside the conversation (tdesktop's
/// `columnMinimalWidthThird` is 292; Quill's panels are laid out for 300).
pub(crate) const PANEL_MIN_WIDTH: f32 = 280.;
/// The main window's minimum size, in interface pixels (tdesktop
/// `windowMinWidth`, `windowMinHeight`); interface zoom scales it when the
/// window opens (`interface_zoom`).
pub(crate) const WINDOW_MIN_WIDTH: f32 = 380.;
pub(crate) const WINDOW_MIN_HEIGHT: f32 = 480.;

/// Preferred widths of the side panels.
pub(crate) const INFO_PANEL_WIDTH: f32 = 300.;
pub(crate) const DOWNLOADS_PANEL_WIDTH: f32 = 300.;
pub(crate) const SHARED_MEDIA_PANEL_WIDTH: f32 = 320.;

/// What the layout depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellInput {
    /// The window's content width in interface pixels (zoom already
    /// applied: at 200% a 1000 px window is 500 wide here).
    pub(crate) width: f32,
    /// The folder rail left of the chat list (0 when the folders are on top).
    pub(crate) rail: f32,
    /// The chat list width the user dragged to.
    pub(crate) list_width: f32,
    pub(crate) chat_open: bool,
    /// Where the open forum's topic list goes (`Session::forum_column`).
    pub(crate) forum: ForumColumn,
    pub(crate) topic_open: bool,
    /// Summed preferred width of the open side panels, if any is open.
    pub(crate) panels: Option<f32>,
}

/// The forum topic column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForumPane {
    Hidden,
    /// Its own column; `chats_button` brings the chat list back (it is
    /// hidden while the topics replace it).
    Column {
        chats_button: bool,
    },
    /// One column: the topics fill the window, a back button leaves the
    /// forum.
    Fill,
}

/// The side panels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PanelPane {
    Hidden,
    /// Beside the conversation, this wide.
    Column(f32),
    /// In the conversation's place (not enough room beside it).
    Fill,
}

/// Which columns the main window shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellLayout {
    /// Too narrow for the list beside the chat: one of them at a time.
    pub(crate) one_column: bool,
    /// The chat list's width (without the folder rail), when shown.
    pub(crate) list: Option<f32>,
    pub(crate) forum: ForumPane,
    /// The conversation pane (the open chat, or the empty placeholder).
    pub(crate) chat: bool,
    pub(crate) panels: PanelPane,
}

impl ShellLayout {
    /// The chat header offers a way back to the chat list: one column with
    /// the conversation on screen.
    pub(crate) fn chat_back(&self) -> bool {
        self.one_column && self.chat
    }
}

/// Below this content width the window shows one column.
pub(crate) fn one_column_below(rail: f32) -> f32 {
    rail + MIN_SIDEBAR_WIDTH + CHAT_MIN_WIDTH
}

/// The widest the chat list may be dragged in a window this wide: the
/// conversation keeps [`CHAT_MIN_WIDTH`].
pub(crate) fn max_list_width(width: f32, rail: f32, forum_beside: bool) -> f32 {
    let forum = if forum_beside { FORUM_COLUMN_WIDTH } else { 0. };
    (width - rail - forum - CHAT_MIN_WIDTH).max(MIN_SIDEBAR_WIDTH)
}

/// The width below which this input shows one column: the list beside the
/// chat, or, while the topic column replaces the list, the topics beside
/// the chat, each leaving the conversation [`CHAT_MIN_WIDTH`].
pub(crate) fn input_one_column_below(input: ShellInput) -> f32 {
    let base = one_column_below(input.rail);
    if input.forum == ForumColumn::Replacing {
        base.max(FORUM_COLUMN_WIDTH + CHAT_MIN_WIDTH)
    } else {
        base
    }
}

impl ShellInput {
    /// Back from the conversation steps to the open forum's topic list
    /// first when the topics are a page of their own (one column), and
    /// only then to the chat list. Escape and the header's back button
    /// both follow this.
    pub(crate) fn back_to_topics(&self) -> bool {
        self.width < input_one_column_below(*self)
            && self.topic_open
            && self.forum != ForumColumn::Hidden
    }
}

pub(crate) fn shell_layout(input: ShellInput) -> ShellLayout {
    if input.width < input_one_column_below(input) {
        return one_column(input);
    }
    let (list, forum, forum_width) = match input.forum {
        ForumColumn::Hidden => (Some(list_width(input, false)), ForumPane::Hidden, 0.),
        ForumColumn::Beside => (
            Some(list_width(input, true)),
            ForumPane::Column {
                chats_button: false,
            },
            FORUM_COLUMN_WIDTH,
        ),
        ForumColumn::Replacing => (
            None,
            ForumPane::Column { chats_button: true },
            FORUM_COLUMN_WIDTH,
        ),
    };
    let list_column = list.map_or(0., |list| list + input.rail);
    let chat_area = input.width - list_column - forum_width;
    let panels = match input.panels {
        None => PanelPane::Hidden,
        Some(_) if chat_area < CHAT_MIN_WIDTH + PANEL_MIN_WIDTH => PanelPane::Fill,
        Some(preferred) => PanelPane::Column(preferred.min(chat_area - CHAT_MIN_WIDTH)),
    };
    ShellLayout {
        one_column: false,
        list,
        forum,
        chat: panels != PanelPane::Fill,
        panels,
    }
}

fn list_width(input: ShellInput, forum_beside: bool) -> f32 {
    input
        .list_width
        .min(max_list_width(input.width, input.rail, forum_beside))
        .max(MIN_SIDEBAR_WIDTH)
}

/// tdesktop's one-column mode: the panel, else the open chat (its topics
/// first when it is a forum), else the chat list, filling the window.
fn one_column(input: ShellInput) -> ShellLayout {
    let hidden = ShellLayout {
        one_column: true,
        list: None,
        forum: ForumPane::Hidden,
        chat: false,
        panels: PanelPane::Hidden,
    };
    if input.panels.is_some() {
        return ShellLayout {
            panels: PanelPane::Fill,
            ..hidden
        };
    }
    if !input.chat_open {
        return ShellLayout {
            list: Some((input.width - input.rail).max(0.)),
            ..hidden
        };
    }
    if input.forum != ForumColumn::Hidden && !input.topic_open {
        return ShellLayout {
            forum: ForumPane::Fill,
            ..hidden
        };
    }
    ShellLayout {
        chat: true,
        ..hidden
    }
}
