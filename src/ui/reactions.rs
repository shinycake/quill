//! Message reactions: the strip leading the message menu (expandable to
//! every reaction the chat allows) and the chips under messages.

use super::app::QuillApp;
use super::menu_states::MessageMenuState;
use super::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{HistoryMessage, ReactionChoice, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Strip cell and glyph sizes (Telegram Desktop's reaction strip).
const STRIP_CELL: f32 = 34.;
const STRIP_GLYPH: f32 = 22.;
/// Columns of the expanded grid.
const GRID_COLS: usize = 8;
/// Custom emoji glyph size inside a chip under a message.
pub(super) const CHIP_GLYPH: f32 = 16.;

pub(super) fn apply_ready_reactions(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(RequestPurpose::AddMessageReaction, Some(ChatId(11)));
    let jsons = [
        format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#
            .to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// Open the message menu and ask which reactions the message may get.
    pub(super) fn open_message_menu(&mut self, menu: MessageMenuState, cx: &mut Context<Self>) {
        self.message_menu = Some(menu);
        self.reactions_expanded = false;
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .fetch_message_reactions(menu.chat_id, menu.message_id);
            let _ = live
                .driver
                .fetch_message_menu_actions(menu.chat_id, menu.message_id);
        }
        cx.notify();
    }

    /// Escape closes the media viewer (after any menu over it).
    pub(super) fn close_media_viewer_on_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.media_viewer.is_open() {
            return false;
        }
        self.close_media_viewer(cx);
        true
    }

    /// Escape closes an open message or chat context menu first.
    pub(super) fn close_context_menus(&mut self, cx: &mut Context<Self>) -> bool {
        if self.message_menu.is_none() && self.chat_menu.is_none() {
            return false;
        }
        self.message_menu = None;
        self.chat_menu = None;
        cx.notify();
        true
    }

    /// Add or remove the user's `choice` on a message.
    pub(super) fn toggle_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        choice: ReactionChoice,
        cx: &mut Context<Self>,
    ) {
        if let ReactionChoice::CustomEmoji(_) = choice
            && !self.session().is_some_and(|s| s.my_is_premium())
            && !self.session().is_some_and(|s| {
                s.histories
                    .get(&chat_id.0)
                    .and_then(|h| h.messages.get(&message_id.0))
                    .is_some_and(|m| m.chosen_reaction(&choice) || has_reaction(m, &choice))
            })
        {
            self.status_note = "Custom emoji reactions need Telegram Premium".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .toggle_reaction_choice(chat_id, message_id, &choice)
            {
                Ok(_) => "updating reaction…".into(),
                Err(_) => "could not update reaction".into(),
            };
            cx.notify();
            return;
        }
        match choice {
            ReactionChoice::Emoji(emoji) => {
                self.toggle_emoji_reaction(chat_id, message_id, emoji, cx)
            }
            ReactionChoice::CustomEmoji(_) => {
                self.status_note = "reaction updated".into();
                cx.notify();
            }
        }
    }

    pub(super) fn toggle_emoji_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: String,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_message_reaction(chat_id, message_id, &emoji);
            self.status_note = match result {
                Ok(_) => "updating reaction…".into(),
                Err(_) => "could not update reaction".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_reaction_toggle(chat_id, message_id, &emoji);
            self.status_note = "reaction updated".into();
            cx.notify();
        }
    }

    /// A downloaded still for custom emoji `id`, if resolved.
    fn custom_emoji_still(&self, id: i64) -> Option<std::path::PathBuf> {
        let item = self
            .session()?
            .emoji
            .custom_emoji_stickers
            .iter()
            .find(|item| item.custom_emoji_id == Some(id))?;
        self.panel_still(item)
    }

    /// The glyph of a reaction: the emoji, or the custom emoji's image
    /// (its fallback emoji until the file lands).
    pub(super) fn reaction_glyph(&self, choice: &ReactionChoice, size: f32) -> AnyElement {
        match choice {
            ReactionChoice::Emoji(emoji) => div()
                .text_size(px(size))
                .line_height(px(size + 4.))
                .child(emoji_presentation(emoji))
                .into_any_element(),
            ReactionChoice::CustomEmoji(id) => match self.custom_emoji_still(*id) {
                Some(path) => img(path)
                    .size(px(size))
                    .aspect_square()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => {
                    let fallback = self
                        .session()
                        .and_then(|s| {
                            s.emoji
                                .custom_emoji_stickers
                                .iter()
                                .find(|item| item.custom_emoji_id == Some(*id))
                                .map(|item| item.emoji.clone())
                        })
                        .filter(|emoji| !emoji.is_empty());
                    match fallback {
                        Some(emoji) => div()
                            .text_size(px(size * 0.9))
                            .child(emoji)
                            .into_any_element(),
                        None => div()
                            .size(px(size * 0.8))
                            .rounded_full()
                            .bg(bg_subtle())
                            .into_any_element(),
                    }
                }
            },
        }
    }

    fn reaction_cell(
        &self,
        id: SharedString,
        chat_id: ChatId,
        message_id: MessageId,
        choice: ReactionChoice,
        chosen: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = match &choice {
            ReactionChoice::Emoji(emoji) => format!("React {emoji}"),
            ReactionChoice::CustomEmoji(_) => "React with custom emoji".to_string(),
        };
        div()
            .id(id)
            .size(px(STRIP_CELL))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .when(chosen, |this| this.bg(accent().opacity(0.25)))
            .hover(|style| style.bg(bg_subtle()))
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .child(self.reaction_glyph(&choice, STRIP_GLYPH))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.message_menu = None;
                this.toggle_reaction(chat_id, message_id, choice.clone(), cx);
            }))
            .into_any_element()
    }

    /// The reaction strip leading the message menu. Collapsed: the chat's
    /// quick reactions and a chevron. Expanded: every allowed reaction
    /// (plus recently used custom emoji when the account can send them).
    pub(super) fn reaction_strip(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        message: &HistoryMessage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let options = session
            .and_then(|s| s.message_reaction_options.as_ref())
            .filter(|o| o.chat_id == chat_id && o.message_id == message_id);
        let Some(options) = options else {
            // Options are on their way: keep the strip's height steady.
            return div()
                .id("reaction-strip-loading")
                .h(px(STRIP_CELL + 8.))
                .mx_1()
                .mb_1()
                .flex()
                .items_center()
                .gap_1()
                .children((0..6).map(|ix| {
                    div()
                        .id(("reaction-skeleton", ix as u64))
                        .size(px(STRIP_CELL - 8.))
                        .mx(px(4.))
                        .rounded_full()
                        .bg(bg_subtle())
                }))
                .into_any_element();
        };
        let mut container = div()
            .id("reaction-strip")
            .flex()
            .flex_col()
            .px_1()
            .pt_1()
            .pb_1()
            .mb_1()
            .border_b_1()
            .border_color(bg_subtle());
        if !self.reactions_expanded {
            let mut row = div().flex().items_center().gap(px(2.));
            for (ix, choice) in options.top.iter().take(7).enumerate() {
                let chosen = message.chosen_reaction(choice);
                row = row.child(self.reaction_cell(
                    format!("reaction-strip-{ix}").into(),
                    chat_id,
                    message_id,
                    choice.clone(),
                    chosen,
                    cx,
                ));
            }
            let more = options.all().len() > options.top.len().min(7)
                || (options.allow_custom_emoji && session.is_some_and(|s| s.my_is_premium()));
            if more {
                row = row.child(
                    div()
                        .id("reaction-strip-expand")
                        .size(px(STRIP_CELL))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .text_color(text_menu())
                        .hover(|style| style.bg(bg_subtle()))
                        .role(gpui_kit::Role::Button)
                        .aria_label("Show all reactions")
                        .child(Icon::new(gpui_kit::assets::IconName::ChevronDown).size(px(16.)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reactions_expanded = true;
                            cx.notify();
                        })),
                );
            }
            return container.child(row).into_any_element();
        }
        let mut choices = options.all();
        if options.allow_custom_emoji
            && let Some(s) = session.filter(|s| s.my_is_premium())
        {
            for id in &s.media_prefs.recent_custom_emoji_ids {
                let choice = ReactionChoice::CustomEmoji(*id);
                if !choices.contains(&choice) {
                    choices.push(choice);
                }
            }
        }
        let mut grid = div()
            .id("reaction-grid")
            .flex()
            .flex_col()
            .gap(px(2.))
            .max_h(px(STRIP_CELL * 7.))
            .overflow_y_scroll();
        for (row_ix, chunk) in choices.chunks(GRID_COLS).enumerate() {
            let mut row = div().flex().gap(px(2.));
            for (col, choice) in chunk.iter().enumerate() {
                let chosen = message.chosen_reaction(choice);
                row = row.child(self.reaction_cell(
                    format!("reaction-grid-{}", row_ix * GRID_COLS + col).into(),
                    chat_id,
                    message_id,
                    choice.clone(),
                    chosen,
                    cx,
                ));
            }
            grid = grid.child(row);
        }
        container = container.child(grid);
        container.into_any_element()
    }
}

/// Whether `choice` is already among a message's reactions (anyone may
/// add one more of an existing custom-emoji reaction).
fn has_reaction(message: &HistoryMessage, choice: &ReactionChoice) -> bool {
    message
        .reaction_chips()
        .iter()
        .any(|chip| ReactionChoice::from_type(&chip.reaction_type).as_ref() == Some(choice))
}

/// TDLib names some reactions without the emoji variation selector ("❤");
/// without it they draw as monochrome text glyphs.
pub(super) fn emoji_presentation(emoji: &str) -> String {
    let mut chars = emoji.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if (c as u32) < 0x1F000 => format!("{c}\u{FE0F}"),
        _ => emoji.to_string(),
    }
}
