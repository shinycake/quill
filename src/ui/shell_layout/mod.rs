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

mod sidebar_view;
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
    /// Share of the list column (rail plus list) in the layout, `0..=1`:
    /// 1 shown, 0 hidden, between while it slides (`sidebar_reveal`).
    /// One column ignores it.
    pub(crate) docked: f32,
    /// Share of the side panel column in the layout, `0..=1`, while it
    /// slides in or out (`panel_reveal`). A panel filling the window in
    /// the chat's place ignores it.
    pub(crate) panels_shown: f32,
    /// Space between the window's edge and a card, and between cards
    /// (`ChromeMetrics::gutter`; 0 in Classic).
    pub(crate) gutter: f32,
    /// The widest the conversation column gets (`ChromeMetrics::chat_column`;
    /// `None` in Classic, where the conversation fills its pane).
    pub(crate) chat_column: Option<f32>,
    /// At this width and under the list is a drawer over the chat
    /// (`ChromeMetrics::drawer_to`; `None` in Classic).
    pub(crate) drawer_to: Option<f32>,
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
    /// A card over the conversation's right part, this wide: there is no
    /// room for it beside a full conversation column. Only with a
    /// `chat_column`; the conversation keeps its whole pane.
    Over(f32),
}

/// Where the chat list sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListDock {
    /// In its own column, beside the chat (or alone, in one column).
    Beside,
    /// A drawer over the chat, which keeps the window's whole width.
    Drawer,
}

/// Which columns the main window shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellLayout {
    /// Too narrow for the list beside the chat: one of them at a time.
    pub(crate) one_column: bool,
    /// The chat list's width (without the folder rail), when shown. The
    /// list keeps this width while it slides out of a hidden column.
    pub(crate) list: Option<f32>,
    /// Share of the list column on screen beside the chat (always 1 in
    /// one column).
    pub(crate) docked: f32,
    pub(crate) forum: ForumPane,
    /// The conversation pane (the open chat, or the empty placeholder).
    pub(crate) chat: bool,
    pub(crate) panels: PanelPane,
    /// Share of a [`PanelPane::Column`] on screen beside the chat (1 for
    /// the other panes).
    pub(crate) panels_shown: f32,
    pub(crate) list_dock: ListDock,
    /// The input's gutter: the list column and a docked panel own theirs.
    pub(crate) gutter: f32,
}

impl ShellLayout {
    /// The chat header offers a way back to the chat list: one column with
    /// the conversation on screen.
    pub(crate) fn chat_back(&self) -> bool {
        self.one_column && self.chat
    }

    /// Whether the list column can be hidden: only beside the chat.
    pub(crate) fn sidebar_toggle(&self) -> bool {
        !self.one_column
    }

    /// The list is a drawer over the chat.
    pub(crate) fn drawer(&self) -> bool {
        self.list_dock == ListDock::Drawer
    }

    /// The list column's width on screen: its left gutter, rail plus
    /// list, narrowed while it slides out of a hidden column. A drawer takes
    /// none.
    pub(crate) fn list_column(&self, rail: f32) -> f32 {
        self.list
            .map_or(0., |list| (self.gutter + list + rail) * self.docked)
    }

    /// The list column takes no room: hidden, or not laid out at all.
    pub(crate) fn list_column_hidden(&self) -> bool {
        self.list.is_some() && self.docked <= 0.
    }

    /// The chat list's resize edge: beside the chat, while the column is
    /// on screen.
    pub(crate) fn resize_handle(&self) -> bool {
        !self.one_column && !self.drawer() && self.list.is_some() && self.docked > 0.
    }

    /// The side panel column's width on screen: narrowed while it slides
    /// in or out, the panels keeping their full width inside it.
    pub(crate) fn panel_column(&self) -> f32 {
        match self.panels {
            PanelPane::Column(width) => (width + self.gutter) * self.panels_shown,
            PanelPane::Hidden | PanelPane::Fill | PanelPane::Over(_) => 0.,
        }
    }

    /// The space the conversation pane leaves at the window's left and
    /// right edges (the list, rail and topics before it; the side panels
    /// after it), where a toast centres over the conversation. Zero both
    /// sides when the pane fills the window.
    pub(crate) fn chat_insets(&self, rail: f32) -> (f32, f32) {
        if !self.chat {
            return (0., 0.);
        }
        let list = self.list_column(rail);
        let forum = forum_column(self.forum, self.gutter);
        (list + forum, self.panel_column())
    }
}

/// Below this content width the window shows one column.
pub(crate) fn one_column_below(rail: f32) -> f32 {
    rail + MIN_SIDEBAR_WIDTH + CHAT_MIN_WIDTH
}

/// The widest the chat list may be dragged in a window this wide: the
/// conversation keeps [`CHAT_MIN_WIDTH`], and every card its `gutter`.
pub(crate) fn max_list_width(width: f32, rail: f32, forum_beside: bool, gutter: f32) -> f32 {
    let forum = if forum_beside {
        gutter + FORUM_COLUMN_WIDTH
    } else {
        0.
    };
    (width - gutter - rail - forum - CHAT_MIN_WIDTH).max(MIN_SIDEBAR_WIDTH)
}

/// The width a forum topic column takes: its card and the gutter before it.
fn forum_column(pane: ForumPane, gutter: f32) -> f32 {
    match pane {
        ForumPane::Column { .. } => gutter + FORUM_COLUMN_WIDTH,
        ForumPane::Hidden | ForumPane::Fill => 0.,
    }
}

/// What of the chat stays visible beside an open drawer, to click on.
pub(crate) const DRAWER_CHAT_PEEK: f32 = 44.;

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
        !self.in_drawer_band()
            && self.width < input_one_column_below(*self)
            && self.topic_open
            && self.forum != ForumColumn::Hidden
    }
}

impl ShellInput {
    /// The window is in the drawer band: the list is a drawer over a chat
    /// that has the whole width.
    pub(crate) fn in_drawer_band(&self) -> bool {
        self.drawer_to.is_some_and(|to| self.width <= to)
    }
}

pub(crate) fn shell_layout(input: ShellInput) -> ShellLayout {
    // A window that has a drawer band (Floating) is wider than one column
    // by the time it is asked: at 600 and under it resolved to Classic's
    // metrics (`layout_style::resolve`), which have none.
    if input.in_drawer_band() {
        return drawer(input);
    }
    if input.width < input_one_column_below(input) {
        return one_column(input);
    }
    let gutter = input.gutter;
    let (list, forum, forum_width) = match input.forum {
        ForumColumn::Hidden => (Some(list_width(input, false)), ForumPane::Hidden, 0.),
        ForumColumn::Beside => (
            Some(list_width(input, true)),
            ForumPane::Column {
                chats_button: false,
            },
            gutter + FORUM_COLUMN_WIDTH,
        ),
        ForumColumn::Replacing => (
            None,
            ForumPane::Column { chats_button: true },
            gutter + FORUM_COLUMN_WIDTH,
        ),
    };
    let docked = input.docked.clamp(0., 1.);
    let list_column = list.map_or(0., |list| (gutter + list + input.rail) * docked);
    let chat_area = input.width - list_column - forum_width;
    let panels = match (input.panels, input.chat_column) {
        (None, _) => PanelPane::Hidden,
        // The panel docks only when a full conversation column still fits
        // beside it; otherwise it is a card over the chat.
        (Some(preferred), Some(column)) => {
            if chat_area - (preferred + gutter) >= column + 2. * gutter {
                PanelPane::Column(preferred)
            } else {
                PanelPane::Over(preferred)
            }
        }
        (Some(_), None) if chat_area < CHAT_MIN_WIDTH + PANEL_MIN_WIDTH => PanelPane::Fill,
        (Some(preferred), None) => PanelPane::Column(preferred.min(chat_area - CHAT_MIN_WIDTH)),
    };
    ShellLayout {
        one_column: false,
        list,
        docked,
        forum,
        chat: panels != PanelPane::Fill,
        panels,
        panels_shown: match panels {
            PanelPane::Column(_) => input.panels_shown.clamp(0., 1.),
            PanelPane::Hidden | PanelPane::Fill | PanelPane::Over(_) => 1.,
        },
        list_dock: ListDock::Beside,
        gutter,
    }
}

/// The drawer band (Web K's 601 to 925): the chat has the whole width, the
/// list is a drawer over it that keeps the width it was dragged to (less
/// room for a strip of chat beside it), the folder rail and the topic
/// column are not drawn, and a panel is a card over the chat.
fn drawer(input: ShellInput) -> ShellLayout {
    let room = input.width - 2. * input.gutter - DRAWER_CHAT_PEEK;
    ShellLayout {
        one_column: false,
        list: Some(input.list_width.min(room).max(0.)),
        docked: 0.,
        forum: ForumPane::Hidden,
        chat: true,
        panels: input.panels.map_or(PanelPane::Hidden, PanelPane::Over),
        panels_shown: 1.,
        list_dock: ListDock::Drawer,
        gutter: input.gutter,
    }
}

/// The conversation pane's width in `layout`: the window less the chat
/// list, topic column and side panels beside it, or the whole window in
/// one column.
pub(crate) fn chat_pane_width(input: ShellInput, layout: &ShellLayout) -> f32 {
    if layout.one_column {
        return input.width.max(0.);
    }
    let list = layout.list_column(input.rail);
    let forum = forum_column(layout.forum, layout.gutter);
    (input.width - list - forum - layout.panel_column()).max(0.)
}

fn list_width(input: ShellInput, forum_beside: bool) -> f32 {
    input
        .list_width
        .min(max_list_width(
            input.width,
            input.rail,
            forum_beside,
            input.gutter,
        ))
        .max(MIN_SIDEBAR_WIDTH)
}

/// tdesktop's one-column mode: the panel, else the open chat (its topics
/// first when it is a forum), else the chat list, filling the window.
fn one_column(input: ShellInput) -> ShellLayout {
    let hidden = ShellLayout {
        one_column: true,
        list: None,
        docked: 1.,
        forum: ForumPane::Hidden,
        chat: false,
        panels: PanelPane::Hidden,
        panels_shown: 1.,
        list_dock: ListDock::Beside,
        gutter: input.gutter,
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
