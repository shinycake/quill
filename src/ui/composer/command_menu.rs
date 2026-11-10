//! The bot command menu above the composer.

use super::*;
use quill::composer::{commands_with_username, complete_command_at};
use std::ops::Range;

/// The bot command list above the composer. Telegram Desktop opens the same
/// list two ways: typing a `/` token at the start of the field filters it,
/// and the bot's Menu button shows every command without touching the
/// field (`FieldAutocomplete::showFiltered(peer, "/")`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum CommandMenu {
    #[default]
    Closed,
    /// Follows the `/` token before the caret; rows filter by it, and a
    /// picked command replaces it.
    Typed { selected: usize },
    /// Opened by the Menu button: every row, and a picked command leaves
    /// the composer as it was. Any edit of the field closes it.
    Button { selected: usize },
}

impl CommandMenu {
    pub(in crate::ui) fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub(in crate::ui) fn selected(self) -> usize {
        match self {
            Self::Closed => 0,
            Self::Typed { selected } | Self::Button { selected } => selected,
        }
    }

    fn with_selected(self, selected: usize) -> Self {
        match self {
            Self::Closed => Self::Closed,
            Self::Typed { .. } => Self::Typed { selected },
            Self::Button { .. } => Self::Button { selected },
        }
    }
}

/// Where the composer stands when a row is picked: the typed `/` token the
/// command replaces, or nothing (the Menu button's list).
enum PickSource {
    Token(Range<usize>),
    Button,
}

impl QuillApp {
    /// The `/command` token before the caret: its byte range (from the `/`
    /// to the caret) and the text typed after the `/`. tdesktop opens the
    /// menu only for a command at the start of the field, and not over a
    /// selection.
    pub(in crate::ui) fn command_trigger(&self, cx: &App) -> Option<(Range<usize>, String)> {
        let input = self.composer.read(cx);
        let caret = input.selected_range();
        if !caret.is_empty() {
            return None;
        }
        let text = input.value();
        let token = command_menu_trigger(&text, caret.start)?;
        Some((token.range, token.query.to_string()))
    }

    /// Recompute the command menu from the composer text. Called on every
    /// composer event and after programmatic `set_value` writes, which
    /// suppress `Change`. A typed `/` token at the start of the field opens
    /// the filtered list when the open chat has commands; the Menu button's
    /// list stays until the field is edited (`end_button_command_menu`).
    pub(in crate::ui) fn sync_command_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value();
        // Bots slice: the `@bot` inline trigger owns the composer start —
        // the `/` menu never competes with the inline-results dropdown.
        let inline_active = quill::composer::inline_query_trigger(&text).is_some();
        // tdesktop turns command autocomplete off while a message is edited.
        let triggered =
            self.command_trigger(cx).is_some() && !inline_active && !self.chat_ui.is_editing();
        if triggered {
            self.load_quick_replies_for_menu();
        }
        let has_items = self.open_chat_has_command_rows();
        let next = match self.chat_ui.command_menu {
            CommandMenu::Button { .. } if !triggered && has_items && !self.chat_ui.is_editing() => {
                return;
            }
            CommandMenu::Typed { .. } if triggered && has_items => return,
            _ if triggered && has_items => CommandMenu::Typed { selected: 0 },
            _ => CommandMenu::Closed,
        };
        if next == self.chat_ui.command_menu {
            return;
        }
        self.chat_ui.command_menu = next;
        self.reset_command_menu_selection(cx);
        cx.notify();
    }

    fn open_chat_has_command_rows(&self) -> bool {
        let Some(chat_id) = self.viewed_chat() else {
            return false;
        };
        self.session()
            .is_some_and(|session| !session.command_menu_items(chat_id).is_empty())
    }

    /// tdesktop starts below the "Edit Quick Replies" row.
    fn reset_command_menu_selection(&mut self, cx: &Context<Self>) {
        let first = self.command_menu_state(cx).map_or(0, |(_, items)| {
            usize::from(
                items
                    .first()
                    .is_some_and(|i| i.quick_reply == Some(quill::quick_replies::EDIT_ROW)),
            )
        });
        self.chat_ui.command_menu = self.chat_ui.command_menu.with_selected(first);
    }

    /// The bot's Menu button: show every command above the composer, or hide
    /// the list when it is already showing (tdesktop hides the open
    /// autocomplete on the second press). The field keeps its text and
    /// gets focus, so Up/Down/Enter/Tab/Esc reach the list.
    pub(in crate::ui) fn toggle_button_command_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_command_menu(cx) {
            return;
        }
        if !self.open_chat_has_command_rows() {
            return;
        }
        self.load_quick_replies_for_menu();
        self.close_inline_results(cx);
        self.close_mention_menu(cx);
        self.close_suggest_menu(false, cx);
        self.chat_ui.command_menu = CommandMenu::Button { selected: 0 };
        self.reset_command_menu_selection(cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// An edit of the field closes the Menu button's list, as tdesktop's
    /// autocomplete re-filters from the field text on every change.
    pub(in crate::ui) fn end_button_command_menu(&mut self, cx: &mut Context<Self>) {
        if matches!(self.chat_ui.command_menu, CommandMenu::Button { .. }) {
            self.close_command_menu(cx);
        }
    }

    /// Phase 3.3: Esc / blur / selection / chat-switch dismissal. Returns
    /// true when the menu was open (the key interceptor swallows the
    /// keystroke only then).
    pub(in crate::ui) fn close_command_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.chat_ui.command_menu.is_open() {
            return false;
        }
        self.chat_ui.command_menu = CommandMenu::Closed;
        cx.notify();
        true
    }

    /// Phase 3.3: Up/Down highlight. Returns true when the menu consumed
    /// the key (open with rows to move between); the selection wraps.
    pub(in crate::ui) fn step_command_menu(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        let rows = self
            .command_menu_state(cx)
            .map(|(_, items)| items.len())
            .unwrap_or(0);
        if !self.chat_ui.command_menu.is_open() || rows == 0 {
            return false;
        }
        let selected =
            (self.chat_ui.command_menu.selected() as i32 + delta).rem_euclid(rows as i32) as usize;
        self.chat_ui.command_menu = self.chat_ui.command_menu.with_selected(selected);
        cx.notify();
        true
    }

    /// Phase 3.3: current menu rows — (typed prefix, prefix-filtered
    /// items). `None` when the menu is closed, a typed menu lost its `/`
    /// trigger, the open chat's bot has no commands, or nothing matches.
    /// The Menu button's list has no prefix and shows every row.
    pub(in crate::ui) fn command_menu_state(
        &self,
        cx: &Context<Self>,
    ) -> Option<(String, Vec<CommandMenuItem>)> {
        let prefix = match self.chat_ui.command_menu {
            CommandMenu::Closed => return None,
            CommandMenu::Typed { .. } => self.command_trigger(cx)?.1,
            CommandMenu::Button { .. } => String::new(),
        };
        let chat_id = self.viewed_chat()?;
        let items = self.session()?.command_menu_items(chat_id);
        if items.is_empty() {
            return None;
        }
        let filtered: Vec<CommandMenuItem> = filter_command_menu_items(&items, &prefix)
            .into_iter()
            .cloned()
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some((prefix, quill::quick_replies::with_edit_row(filtered)))
    }

    /// Enter with the menu open sends the highlighted command at once
    /// (tdesktop `FieldAutocomplete` `ByEnter`: `sendCommand`, then the
    /// field keeps the text after the caret); Tab inserts it instead.
    /// Returns true when Enter was consumed.
    pub(in crate::ui) fn pick_command_menu_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = self
            .command_menu_state(cx)
            .map(|(_, items)| items.len())
            .unwrap_or(0);
        if !self.chat_ui.command_menu.is_open() || rows == 0 {
            return false;
        }
        let index = self
            .chat_ui
            .command_menu
            .selected()
            .min(rows.saturating_sub(1));
        self.send_command_menu_index(index, window, cx);
        true
    }

    /// Tab: put the highlighted command in the composer instead of sending
    /// it (tdesktop `ByTab`). The "Edit Quick Replies" row still opens the
    /// settings. Returns true when Tab was consumed.
    pub(in crate::ui) fn insert_command_menu_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        if !self.chat_ui.command_menu.is_open() || items.is_empty() {
            return false;
        }
        let index = self.chat_ui.command_menu.selected().min(items.len() - 1);
        let Some(item) = items.get(index) else {
            return false;
        };
        match item.quick_reply {
            Some(quill::quick_replies::EDIT_ROW) => {
                self.send_quick_reply_row(quill::quick_replies::EDIT_ROW, window, cx)
            }
            _ => self.insert_command_menu_index(index, window, cx),
        }
        true
    }

    fn insert_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        let Some((source, prefix)) = self.command_pick_source(cx) else {
            return;
        };
        let command = item.command_text(commands_with_username(&items, &prefix));
        let range = match source {
            PickSource::Token(range) => range,
            // tdesktop's bot card: the command goes in front of the text.
            PickSource::Button => {
                self.close_command_menu(cx);
                self.insert_bot_command(&command, window, cx);
                self.composer
                    .update(cx, |input, cx| input.focus(window, cx));
                return;
            }
        };
        let text = self.composer.read(cx).value().to_string();
        let Some((insert, caret)) = complete_command_at(&text, range.clone(), &command) else {
            return;
        };
        // A range edit keeps the formatting around it. The trailing space
        // (tdesktop `insertTag`) moves the caret off the command, so the
        // menu does not reopen and swallow the next Enter.
        self.replace_composer_text(range, &insert, window, cx);
        self.composer
            .update(cx, |input, cx| input.set_selected_range(caret..caret, cx));
        self.close_command_menu(cx);
    }

    /// A clicked or Entered command is sent at once (Telegram Desktop's bot
    /// command list): the composer's `/`-token is dropped, any other text stays.
    pub(in crate::ui) fn send_command_menu_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        let Some(item) = items.get(index) else {
            return;
        };
        if let Some(shortcut_id) = item.quick_reply {
            self.send_quick_reply_row(shortcut_id, window, cx);
            return;
        }
        let Some((source, prefix)) = self.command_pick_source(cx) else {
            return;
        };
        let command = item.command_text(commands_with_username(&items, &prefix));
        self.close_command_menu(cx);
        match source {
            // The text after the caret stays in the field (tdesktop keeps
            // `getTextWithTagsPart(cursor)`), with the caret at its end.
            PickSource::Token(range) => {
                self.replace_composer_text(range, "", window, cx);
                self.composer.update(cx, |input, cx| {
                    let end = input.value().len();
                    input.set_selected_range(end..end, cx);
                    input.focus(window, cx);
                });
            }
            // The Menu button's list never wrote to the field.
            PickSource::Button => {
                self.composer
                    .update(cx, |input, cx| input.focus(window, cx));
            }
        }
        if self.send_menu_command(&command, cx) {
            // The send leaves the chat draft alone (`clear_draft` is off for
            // a menu command); the kept text is saved at once.
            if self.composer.read(cx).value().is_empty() {
                if let Some(chat_id) = self.viewed_chat() {
                    self.forget_local_draft(chat_id);
                }
            } else {
                self.note_open_draft(false, cx);
            }
            return;
        }
        // Demo and connecting panes have no live send: the composer's own
        // path reports it, then the kept text goes back.
        let rest = self.composer_markup(cx);
        self.submit_composer(command, window, cx);
        if !rest.is_empty() && self.composer.read(cx).value().is_empty() {
            self.set_composer_markup(&rest, window, cx);
            self.composer.update(cx, |input, cx| {
                let end = input.value().len();
                input.set_selected_range(end..end, cx);
            });
        }
    }
}

impl QuillApp {
    /// The pick's source and the prefix that decides `/command@bot`.
    fn command_pick_source(&self, cx: &Context<Self>) -> Option<(PickSource, String)> {
        match self.chat_ui.command_menu {
            CommandMenu::Closed => None,
            CommandMenu::Typed { .. } => {
                let (range, prefix) = self.command_trigger(cx)?;
                Some((PickSource::Token(range), prefix))
            }
            CommandMenu::Button { .. } => Some((PickSource::Button, String::new())),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_commands_tests.rs"]
mod group_commands_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "bot_menu_tests.rs"]
mod bot_menu_tests;
