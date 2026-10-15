//! The bot command menu above the composer.

use super::*;

impl QuillApp {
    /// Phase 3.3: recompute the `/` command menu from the composer text.
    /// Called on every composer event (`Change` path) and after
    /// programmatic `set_value` writes, which suppress `Change`. Opens
    /// when the text ends with a `/`-led token at a word boundary and the
    /// open chat's bot has commands; closes otherwise (non-bot chat, no
    /// commands, invalid trigger, empty composer).
    pub(in crate::ui) fn sync_command_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        // Bots slice: the `@bot` inline trigger owns the composer start —
        // the `/` menu never competes with the inline-results dropdown.
        let inline_active = quill::composer::inline_query_trigger(&text).is_some();
        let triggered = command_menu_trigger(&text).is_some() && !inline_active;
        let open_chat = self.session().and_then(|session| session.open_chat);
        if triggered {
            self.load_quick_replies_for_menu();
        }
        let has_items = open_chat.is_some_and(|chat_id| {
            self.session()
                .map(|session| !session.command_menu_items(chat_id).is_empty())
                .unwrap_or(false)
        });
        let open = triggered && has_items;
        if open == self.composer_ui.command_menu_open {
            return;
        }
        self.composer_ui.command_menu_open = open;
        // tdesktop starts below the "Edit Quick Replies" row.
        self.composer_ui.command_menu_selected =
            self.command_menu_state(cx).map_or(0, |(_, items)| {
                usize::from(
                    items
                        .first()
                        .is_some_and(|i| i.quick_reply == Some(quill::quick_replies::EDIT_ROW)),
                )
            });
        cx.notify();
    }

    /// Phase 3.3: Esc / blur / selection / chat-switch dismissal. Returns
    /// true when the menu was open (the key interceptor swallows the
    /// keystroke only then).
    pub(in crate::ui) fn close_command_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.composer_ui.command_menu_open {
            return false;
        }
        self.composer_ui.command_menu_open = false;
        self.composer_ui.command_menu_selected = 0;
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
        if !self.composer_ui.command_menu_open || rows == 0 {
            return false;
        }
        self.composer_ui.command_menu_selected = (self.composer_ui.command_menu_selected as i32
            + delta)
            .rem_euclid(rows as i32) as usize;
        cx.notify();
        true
    }

    /// Phase 3.3: current menu rows — (typed prefix, prefix-filtered
    /// items). `None` when the menu is closed, the composer has no `/`
    /// trigger, the open chat's bot has no commands, or nothing matches.
    pub(in crate::ui) fn command_menu_state(
        &self,
        cx: &Context<Self>,
    ) -> Option<(String, Vec<CommandMenuItem>)> {
        if !self.composer_ui.command_menu_open {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        let prefix = command_menu_trigger(&text)?;
        let chat_id = self.session()?.open_chat?;
        let items = self.session()?.command_menu_items(chat_id);
        if items.is_empty() {
            return None;
        }
        let filtered: Vec<CommandMenuItem> = filter_command_menu_items(&items, prefix)
            .into_iter()
            .cloned()
            .collect();
        if filtered.is_empty() {
            return None;
        }
        Some((
            prefix.to_string(),
            quill::quick_replies::with_edit_row(filtered),
        ))
    }

    /// Phase 3.3: Enter with the menu open picks the highlighted row.
    /// Returns true when Enter was consumed.
    pub(in crate::ui) fn pick_command_menu_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self
            .command_menu_state(cx)
            .map(|(_, items)| items)
            .unwrap_or_default();
        if !self.composer_ui.command_menu_open || items.is_empty() {
            return false;
        }
        let index = self.composer_ui.command_menu_selected.min(items.len() - 1);
        self.pick_command_menu_index(index, window, cx);
        true
    }

    /// Phase 3.3: tap / Enter pick. The partial `/`-token is replaced via
    /// the 3.1 insert helper, then the menu closes (the next `Change`
    /// reopens it if a trigger token remains).
    pub(in crate::ui) fn pick_command_menu_index(
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
        self.insert_command_menu_index(index, window, cx);
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
        if !self.composer_ui.command_menu_open || items.is_empty() {
            return false;
        }
        let index = self.composer_ui.command_menu_selected.min(items.len() - 1);
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
        let command = item.command.clone();
        let current = self.composer_markup(cx);
        let base = strip_command_menu_trigger(&current).unwrap_or(current.as_str());
        // Trailing space (tdesktop behavior): without it, the `/`-token
        // trigger still matches `/command`, the menu reopens on the next
        // Enter and consumes it in a no-op loop — Enter could never send.
        let next = format!(
            "{} ",
            quill::composer::insert_bot_command_text(base, &command).trim_end()
        );
        self.set_composer_markup(&next, window, cx);
        self.composer.update(cx, |input, cx| {
            let end = input.value().len();
            input.set_selected_range(end..end, cx);
        });
        self.close_command_menu(cx);
    }

    /// A clicked command is sent at once (Telegram Desktop's bot command
    /// list): the composer's `/`-token is dropped, any other text stays.
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
        let command = format!("/{}", item.command.trim_start_matches('/'));
        let current = self.composer_markup(cx);
        let rest = strip_command_menu_trigger(&current)
            .unwrap_or(current.as_str())
            .to_string();
        self.close_command_menu(cx);
        self.set_composer_markup(rest.trim_end(), window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.submit_composer(command, window, cx);
    }
}
