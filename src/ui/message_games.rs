//! Bot game cards: `messageGame` rendering, high-score panels, game sends.

use super::app::QuillApp;
use super::message_media::{MediaCorners, photo_attachment};
use super::message_text::rich_text_line;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::Session;
use quill::telegram::envelope::{GameContent, ParsedFile};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Slice bots-games: `messageGame` card — thumbnail (glyph when the game
/// carries no photo/animation sizes), title, text, description, and
/// Play / Scores actions. Play reuses the existing `press_game_button`
/// flow (`callbackQueryPayloadGame` → answer URL → OS browser).
#[allow(clippy::too_many_arguments)]
pub(super) fn game_card(
    chat_id: ChatId,
    message_id: MessageId,
    game: &GameContent,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    downloading: &HashSet<i32>,
    media_roots: &[PathBuf],
    revealed: &HashSet<(i64, u64, u64, bool)>,
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let mut card = div()
        .id(("game-card", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(accent())
        .bg(bg_canvas());
    if game.photo.sizes.is_empty() {
        card = card.child(div().text_2xl().child("🎮"));
    } else {
        card = card.child(div().mt_2().child(photo_attachment(
            row_id,
            &game.photo,
            files,
            downloading,
            media_roots,
            None,
            None,
            MediaCorners::small(),
            cx,
        )));
    }
    let title = game.title.trim();
    card = card.child(div().text_sm().font_semibold().child(if title.is_empty() {
        "Game".to_string()
    } else {
        title.to_string()
    }));
    if !game.text.text.is_empty() {
        card = card.child(rich_text_line(
            &game.text.text,
            &game.text.entities,
            (chat_id.0, row_id),
            false,
            revealed,
            font,
            // Game cards don't resolve custom emoji in this slice (text fallback).
            &HashMap::new(),
            cx,
        ));
    }
    if !game.description.is_empty() {
        card = card.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(game.description.clone()),
        );
    }
    let short_name = game.short_name.clone();
    let actions = div()
        .id(("game-actions", row_id))
        .flex()
        .gap_2()
        .child(
            Button::new(format!("game-play-{row_id}"))
                .label("▶ Play")
                .primary()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.press_game_button(chat_id, message_id, Some(short_name.clone()), cx);
                })),
        )
        .child(
            Button::new(format!("game-scores-{row_id}"))
                .label("Scores")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.press_scores_button(chat_id, message_id, cx);
                })),
        );
    card = card.child(actions);
    if let Some(panel) = session.and_then(|s| game_scores_panel(chat_id, message_id, s, cx)) {
        card = card.child(panel);
    }
    card.into_any_element()
}

/// Slice bots-games: the inline high-score panel under a game card.
/// Present in `Session::game_scores` = open; `None` = request in flight.
fn game_scores_panel(
    chat_id: ChatId,
    message_id: MessageId,
    session: &Session,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let row_id = message_id.0 as u64;
    let scores = session.game_scores.get(&(chat_id.0, message_id.0))?;
    let mut panel = div()
        .id(("game-scores-panel", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .mt_1()
        .p_2()
        .rounded_md()
        .bg(bg_subtle());
    match scores {
        None => {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading scores…"),
            );
        }
        Some(rows) if rows.is_empty() => {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No scores yet — be the first to play!"),
            );
        }
        Some(rows) => {
            for row in rows {
                let name = session
                    .users
                    .get(&row.user_id)
                    .map(|user| user.display_name())
                    .unwrap_or_else(|| format!("User {}", row.user_id));
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .child(div().w(px(28.)).child(format!("#{}", row.position)))
                        .child(div().flex_1().child(name))
                        .child(div().font_semibold().child(row.score.to_string())),
                );
            }
        }
    }
    Some(panel.into_any_element())
}

impl QuillApp {
    /// Slice bots-games: toggle the high-score panel; on open, fetch via
    /// `getGameHighScores` (schema 1.8.67, line 13174). `user_id` is the
    /// current user (the table range centers on them).
    pub(super) fn press_scores_button(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let key = (chat_id.0, message_id.0);
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.game_scores.contains_key(&key))
        {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.game_scores.remove(&key);
            }
            cx.notify();
            return;
        }
        let user_id = self
            .live
            .as_ref()
            .and_then(|live| live.driver.session.my_user_id);
        let Some(user_id) = user_id else {
            self.set_status_note("Couldn't load the scores.", cx);
            return;
        };
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.game_scores.insert(key, None);
            live.driver
                .send_game_high_scores(chat_id, message_id, user_id)
        });
        if !matches!(sent, Some(Ok(_))) {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.game_scores.remove(&key);
            }
            self.set_status_note("Couldn't reach Telegram; try again.", cx);
        }
        cx.notify();
    }

    /// Slice bots-games: send the bot's game to the open chat
    /// (`inputMessageGame`, schema 1.8.67, line 6156). Only called with
    /// short names from `Session::bot_games` — never invented ones.
    pub(super) fn press_send_game(
        &mut self,
        chat_id: ChatId,
        bot_user_id: i64,
        game_short_name: &str,
        cx: &mut Context<Self>,
    ) {
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .send_game_message(chat_id, bot_user_id, game_short_name)
        });
        if !matches!(sent, Some(Ok(_))) {
            self.set_status_note("Couldn't reach Telegram; try again.", cx);
        }
        cx.notify();
    }
}
