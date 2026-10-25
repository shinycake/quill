//! Message reactions: the strip leading the message menu (expandable to
//! every reaction the chat allows) and the chips under messages.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::menu_states::MessageMenuState;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
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
    pub(super) fn open_message_menu(
        &mut self,
        menu: MessageMenuState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.message_ui.menu = Some(menu);
        self.message_ui.menu_ui.page = super::message_menu_ui::MessageMenuPage::Main;
        self.message_ui.menu_link = self.take_right_clicked_link(menu.position);
        self.message_ui.menu_item = self.take_right_clicked_item(menu.position);
        // Over a poll option the menu shows the option's page.
        if matches!(
            self.message_ui.menu_item,
            quill::composer::ReplyItem::PollOption(_)
        ) {
            self.message_ui.menu_ui.page = super::message_menu_ui::MessageMenuPage::PollOption;
        }
        self.message_ui.link_tooltip = None;
        // A right-click keeps the text selection; the menu then acts on it
        // (Telegram Desktop's Quote & Reply, Copy Selected Text).
        self.message_ui.menu_selection = super::selectable_text::selected_message_text(window, cx)
            .filter(|(key, _)| *key == (menu.chat_id.0, menu.message_id.0 as u64))
            .map(|(_, text)| text);
        self.message_ui.reactions_expanded = false;
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .fetch_message_reactions(menu.chat_id, menu.message_id);
            let _ = live
                .driver
                .fetch_message_menu_actions(menu.chat_id, menu.message_id);
            // A poll option has its own menu facts.
            if let quill::composer::ReplyItem::PollOption(option_id) = &self.message_ui.menu_item {
                let _ = live.driver.fetch_poll_option_properties(
                    menu.chat_id,
                    menu.message_id,
                    option_id,
                );
            }
            // Stickers and GIFs word their menu items by what the user has
            // installed, favorited and saved.
            let content = live
                .driver
                .session
                .histories
                .get(&menu.chat_id.0)
                .and_then(|h| h.messages.get(&menu.message_id.0))
                .map(|m| {
                    quill::telegram::envelope::effective_content(&m.content, m.ephemeral.as_ref())
                        .clone()
                });
            let packs = content
                .as_ref()
                .map(|content| live.driver.session.message_emoji_pack_ids(content))
                .unwrap_or_default();
            if let [only] = packs[..] {
                let _ = live.driver.fetch_emoji_pack_title(only);
            }
            match content {
                Some(quill::telegram::envelope::MessageContent::Sticker(_)) => {
                    let _ = live.driver.fetch_sticker_menu_facts();
                }
                Some(quill::telegram::envelope::MessageContent::Animation(_))
                    if !live.driver.session.stickers.gifs.loaded =>
                {
                    let _ = live.driver.show_saved_gifs();
                }
                // "Save to..." offers the profile only for songs that are
                // not in it yet.
                Some(quill::telegram::envelope::MessageContent::Audio(audio)) => {
                    let _ = live.driver.check_profile_audio(audio.file_id);
                }
                _ => {}
            }
        }
        cx.notify();
    }

    /// Escape closes the media viewer (after any menu over it); in video
    /// full screen it only leaves full screen first (tdesktop
    /// `handleKeyPress`, media_view_overlay_widget.cpp:7384).
    pub(super) fn close_media_viewer_on_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.viewer.state.is_open() {
            return false;
        }
        if self.viewer.extra.video_fullscreen.is_on() {
            self.viewer_leave_video_fullscreen();
            cx.notify();
            return true;
        }
        if self.collapse_viewer_caption_on_escape(cx) {
            return true;
        }
        self.close_media_viewer(cx);
        true
    }

    /// Escape closes an open message or chat context menu first.
    pub(super) fn close_context_menus(&mut self, cx: &mut Context<Self>) -> bool {
        if self.message_ui.menu.is_none() && self.chat_list.menu.is_none() {
            return false;
        }
        self.close_message_menu();
        self.chat_list.menu = None;
        cx.notify();
        true
    }

    /// Double-click on a message: react with the quick reaction
    /// (tdesktop `HistoryInner::toggleFavoriteReaction`); nothing when
    /// the message cannot take reactions or none is set.
    pub(super) fn quick_react(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(choice) = session.stickers.default_reaction.clone() else {
            return;
        };
        let can_react = session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0))
            .is_some_and(|m| m.can_react());
        if can_react {
            self.toggle_reaction(chat_id, message_id, choice, cx);
        }
    }

    /// Settings, "Quick reaction": the reactions Telegram offers
    /// (`updateActiveEmojiReactions`), the current one highlighted
    /// (tdesktop `Settings::AddReactionsSection`: pick the double-click
    /// reaction).
    pub(super) fn quick_reaction_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        const FALLBACK: [&str; 8] = ["👍", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🎉"];
        let session = self.session();
        let active: Vec<String> = session
            .map(|s| s.stickers.active_reactions.clone())
            .filter(|list| !list.is_empty())
            .unwrap_or_else(|| FALLBACK.iter().map(|e| e.to_string()).collect());
        let current = session.and_then(|s| s.stickers.default_reaction.clone());
        let muted = cx.theme().muted_foreground;
        let mut strip = div().flex().flex_wrap().gap_1().px_2();
        for (ix, emoji) in active.into_iter().take(24).enumerate() {
            let selected = current == Some(ReactionChoice::Emoji(emoji.clone()));
            let glyph = emoji_presentation(&emoji);
            let pick = emoji.clone();
            strip = strip.child(
                div()
                    .id(("quick-reaction", ix as u64))
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(px(20.))
                    .when(selected, |this| this.bg(accent().opacity(0.25)))
                    .hover(|style| style.bg(bg_subtle()))
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_MISC_QUICK_REACTION,
                        &[("glyph", &glyph.to_string())],
                    ))
                    .child(
                        super::emoji_sprites::glyph(
                            &glyph,
                            super::emoji_sprites::edge_for(px(20.)),
                        )
                        .unwrap_or_else(|| glyph.clone().into_any_element()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_default_reaction(pick.clone(), cx);
                    })),
            );
        }
        div()
            .id("quick-reaction")
            .flex()
            .flex_col()
            .gap_1()
            .py_1()
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .px_2()
                    .child(tr(keys::SETTINGS_CHAT_REACTIONS_TITLE)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .px_2()
                    .child(tr(keys::Q_MISC_USED_WHEN_DOUBLE_CLICK_IS)),
            )
            .child(strip)
            .into_any_element()
    }

    /// Settings: the reply and reaction buttons beside a hovered message
    /// (tdesktop "Reply button on messages" / "Reaction button on
    /// messages").
    pub(super) fn corner_button_switches(&self, cx: &mut Context<Self>) -> AnyElement {
        let (reply, react) = self.session().map_or((true, true), |s| {
            (
                s.settings.media_prefs.corner_reply,
                s.settings.media_prefs.corner_reaction,
            )
        });
        let row = || div().flex().items_center().gap_2().px_2().py_1();
        div()
            .flex()
            .flex_col()
            .child(
                row()
                    .child(
                        Switch::new("media-pref-corner-reply")
                            .checked(reply)
                            .accessibility_label(tr(keys::SETTINGS_CHAT_CORNER_REPLY))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.corner_reply = on, cx);
                            })),
                    )
                    .child(div().text_sm().child(tr(keys::SETTINGS_CHAT_CORNER_REPLY))),
            )
            .child(
                row()
                    .child(
                        Switch::new("media-pref-corner-reaction")
                            .checked(react)
                            .accessibility_label(tr(keys::SETTINGS_CHAT_CORNER_REACTION))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.corner_reaction = on, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::SETTINGS_CHAT_CORNER_REACTION)),
                    ),
            )
            .into_any_element()
    }

    /// Settings, "Quick reaction": make `emoji` the double-click reaction.
    pub(super) fn set_default_reaction(&mut self, emoji: String, cx: &mut Context<Self>) {
        let choice = ReactionChoice::Emoji(emoji);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_default_reaction(&choice);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.default_reaction = Some(choice);
        }
        cx.notify();
    }

    /// Add or remove the user's `choice` on a message.
    pub(super) fn toggle_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        choice: ReactionChoice,
        cx: &mut Context<Self>,
    ) {
        if let Some(feature) = self
            .session()
            .and_then(|s| s.react_premium_feature(chat_id, message_id, &choice))
        {
            self.show_premium_reaction_preview(feature, cx);
            return;
        }
        // The reaction flies up from the message when it is added.
        let adding = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .and_then(|h| h.messages.get(&message_id.0))
            .is_some_and(|m| !m.chosen_reaction(&choice));
        if adding && let ReactionChoice::Emoji(emoji) = &choice {
            self.history.reaction_fly = Some(super::history_fx::ReactionFly {
                chat_id: chat_id.0,
                message_id: message_id.0,
                glyph: emoji_presentation(emoji).into(),
                started: std::time::Instant::now(),
            });
        }
        if let Some(live) = self.live.as_mut() {
            self.connection.set_note(
                match live
                    .driver
                    .toggle_reaction_choice(chat_id, message_id, &choice)
                {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MISC_UPDATING_REACTION)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_REACTION)),
                },
            );
            cx.notify();
            return;
        }
        match choice {
            ReactionChoice::Emoji(emoji) => {
                self.toggle_emoji_reaction(chat_id, message_id, emoji, cx)
            }
            ReactionChoice::CustomEmoji(_) => {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_MISC_REACTION_UPDATED)));
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
        if let Some(live) = self.live.as_mut() {
            let result = live
                .driver
                .toggle_message_reaction(chat_id, message_id, &emoji);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_MISC_UPDATING_REACTION)),
                Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_REACTION)),
            });
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_reaction_toggle(chat_id, message_id, &emoji);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MISC_REACTION_UPDATED)));
            cx.notify();
        }
    }

    /// A downloaded still for custom emoji `id`, if resolved. Never animates
    /// and never asks for frame ticks.
    pub(super) fn custom_emoji_still(&self, id: i64) -> Option<std::path::PathBuf> {
        let item = self
            .session()?
            .stickers
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
            ReactionChoice::Emoji(emoji) => {
                match super::emoji_sprites::glyph(
                    &emoji_presentation(emoji),
                    super::emoji_sprites::edge_for(px(size)),
                ) {
                    Some(sprite) => sprite,
                    None => div()
                        .text_size(px(size))
                        .line_height(px(size + 4.))
                        .child(emoji_presentation(emoji))
                        .into_any_element(),
                }
            }
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
                            s.stickers
                                .emoji
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
            ReactionChoice::Emoji(emoji) => {
                tr_with(keys::Q_MISC_REACT, &[("emoji", &emoji.to_string())])
            }
            ReactionChoice::CustomEmoji(_) => tr(keys::Q_MISC_REACT_WITH_CUSTOM_EMOJI).to_string(),
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
                this.message_ui.menu = None;
                this.toggle_reaction(chat_id, message_id, choice.clone(), cx);
            }))
            .test_support()
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
    ) -> Option<AnyElement> {
        let session = self.session();
        let state = session.map(|s| &s.stickers.message_reaction_options);
        let options = state.and_then(|state| state.ready_for(chat_id, message_id));
        let Some(options) = options else {
            // Only a request in flight earns a placeholder (it keeps the
            // strip's height steady); no options, a failure or an empty
            // answer leave no strip, like tdesktop.
            if !state.is_some_and(|state| state.is_loading_for(chat_id, message_id)) {
                return None;
            }
            return Some(
                div()
                    .id("reaction-strip-loading")
                    .test_support()
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
                    .into_any_element(),
            );
        };
        let mut container = div()
            .id("reaction-strip")
            .test_support()
            .flex()
            .flex_col()
            .px_1()
            .py_1();
        if !self.message_ui.reactions_expanded {
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
            row = row.children(self.paid_picker_cell(chat_id, message_id, cx));
            let more = options.all().len() > options.top.len().min(7)
                || (options.allow_custom_emoji
                    && session.is_some_and(|s| s.premium_badges_shown()));
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
                        .aria_label(tr(keys::Q_MISC_SHOW_ALL_REACTIONS))
                        .child(Icon::new(gpui_kit::assets::IconName::ChevronDown).size(px(16.)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            // With custom emoji allowed, Telegram Desktop
                            // expands into the full selector (search, every
                            // reaction, custom packs); otherwise the grid.
                            let custom = this
                                .session()
                                .and_then(|s| {
                                    s.stickers
                                        .message_reaction_options
                                        .ready_for(chat_id, message_id)
                                })
                                .is_some_and(|o| o.allow_custom_emoji);
                            let position = this.message_ui.menu.map(|menu| menu.position);
                            match position {
                                Some(position) if custom => {
                                    this.open_reaction_selector(
                                        chat_id, message_id, position, window, cx,
                                    );
                                }
                                _ => {
                                    this.message_ui.reactions_expanded = true;
                                    cx.notify();
                                }
                            }
                        })),
                );
            }
            return Some(container.child(row).into_any_element());
        }
        let mut choices = options.all();
        if options.allow_custom_emoji
            && let Some(s) = session.filter(|s| s.my_is_premium())
        {
            for id in &s.settings.media_prefs.recent_custom_emoji_ids {
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
        Some(container.into_any_element())
    }
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "reaction_strip_state_tests.rs"]
mod reaction_strip_state_tests;
