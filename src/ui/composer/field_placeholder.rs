//! The compose field's placeholder follows the chat (tdesktop
//! `updateFieldPlaceholder`): "New message…" while editing, the bot
//! keyboard's own text, "Message for N Stars" in paid chats, "Message in
//! {topic}" in a forum, "Broadcast a message…" in channels, "Send
//! anonymously…" for an anonymous admin, "Off-thread message" in a bot that
//! makes its own topics. An inline bot's own hint is drawn after `@bot `
//! while that is all that is typed.

use super::*;
use gpui_kit::component::input::TextareaState;
use quill::composer::{ComposerPlaceholder, PlaceholderFacts, inline_bot_hint};
use quill::lang::{keys, tr};
use quill::state::InlineBotResolve;

impl QuillApp {
    /// What the open chat says about the field.
    fn placeholder_facts(&self) -> PlaceholderFacts {
        let Some(session) = self.session() else {
            return PlaceholderFacts::default();
        };
        let Some(chat) = session.open_chat else {
            return PlaceholderFacts::default();
        };
        let broadcast = session
            .chats
            .get(&chat.0)
            .is_some_and(|chat| chat.is_channel());
        let reply = self
            .composer_ui
            .reply()
            .filter(|reply| reply.chat_id == chat && reply.target_chat.is_none())
            .map(|reply| reply.message_id)
            .or_else(|| self.keyboard_reply_target());
        PlaceholderFacts {
            editing: self.composer_ui.pending_edit.is_some(),
            keyboard: self.keyboard_placeholder(),
            stars_per_message: session.paid_send_price(chat),
            topic: session.composer_topic(chat, reply),
            broadcast,
            silent: broadcast && self.composer_effective_silent(),
            anonymous_admin: !broadcast && session.chat_posts_anonymously(chat),
            bot_off_thread: session.bot_chat_off_thread(chat),
        }
    }

    /// The message of the bot keyboard that is on screen or asks for a reply:
    /// tdesktop counts it as the reply the next message goes to
    /// (`_kbReplyTo`).
    fn keyboard_reply_target(&self) -> Option<MessageId> {
        let (chat, message, keyboard) = self.open_chat_custom_keyboard()?;
        let hidden = self
            .message_ui
            .collapsed_keyboards
            .contains(&(chat.0, message.0));
        (!hidden || keyboard.force_reply).then_some(message)
    }

    /// The placeholder of the bot keyboard on screen, or of a force-reply
    /// request, which tdesktop shows even when the keyboard is hidden
    /// (`_kbShown || _keyboard->forceReply()`).
    fn keyboard_placeholder(&self) -> String {
        if let Some((chat, message, keyboard)) = self.open_chat_custom_keyboard() {
            let hidden = self
                .message_ui
                .collapsed_keyboards
                .contains(&(chat.0, message.0));
            return if hidden && !keyboard.force_reply {
                String::new()
            } else {
                keyboard.placeholder
            };
        }
        self.open_chat_force_reply()
            .map(|(_, _, placeholder)| placeholder)
            .unwrap_or_default()
    }

    /// Frame upkeep: put the placeholder the state asks for in the field.
    /// It is derived here, not at the event sites, because `set_placeholder`
    /// needs a `Window` and the Stars answer arrives in `poll_live`. Only a
    /// change of facts or language rebuilds the text.
    pub(in crate::ui) fn sync_composer_placeholder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let facts = self.placeholder_facts();
        // A new language pack hands out new strings, so the address of any
        // one of them tells the language changed.
        let language = tr(keys::MESSAGE_PH).as_ptr() as usize;
        let shown = (facts, language);
        if self.composer_ui.placeholder_shown.as_ref() == Some(&shown) {
            return;
        }
        let (facts, _) = shown.clone();
        self.composer_ui.placeholder_shown = Some(shown);
        let text = ComposerPlaceholder::for_facts(&facts).text();
        self.composer
            .update(cx, |input, cx| input.set_placeholder(text, window, cx));
    }

    /// The hint of the inline bot named at the start of the field, once the
    /// bot is resolved and has one (tdesktop `_inlineBot`).
    pub(in crate::ui) fn inline_bot_hint_text(&self, cx: &App) -> Option<String> {
        let session = self.session()?;
        let Some(InlineBotResolve::Resolved {
            username, user_id, ..
        }) = session.bots.inline_bot_resolve.as_ref()
        else {
            return None;
        };
        let user = session.users.get(user_id)?;
        if !user.is_bot {
            return None;
        }
        let text = self.composer.read(cx).value().to_string();
        inline_bot_hint(
            &text,
            self.composer_ui.pending_edit.is_some(),
            username,
            &user.inline_query_placeholder,
        )
        .map(str::to_string)
    }

    /// The inline bot's hint, painted over the field right after the typed
    /// `@bot` (tdesktop `setPlaceholder(text, username.size() + 2)`). The
    /// field itself only shows a placeholder while empty.
    pub(in crate::ui) fn inline_bot_hint_overlay(&self, cx: &App) -> Option<AnyElement> {
        let hint = self.inline_bot_hint_text(cx)?;
        let composer = self.composer.clone();
        let color: Hsla = cx.theme().muted_foreground;
        Some(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    paint_inline_hint(&composer, &hint, color, bounds, window, cx);
                },
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
    }
}

fn paint_inline_hint(
    composer: &Entity<TextareaState>,
    hint: &str,
    color: Hsla,
    clip: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let state = composer.read(cx);
    let text = state.value().to_string();
    let Some((last_start, last)) = text.char_indices().next_back() else {
        return;
    };
    let Some(rect) = state
        .range_to_rects(&(last_start..text.len()))
        .into_iter()
        .next_back()
    else {
        return;
    };
    // The field's own font and size.
    let (font, font_size) = state.text_style();
    let system = window.text_system().clone();
    let shape = |text: &str| {
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        system.shape_line(text.to_string().into(), font_size, &[run], None)
    };
    // Right after the name there is no separator yet: leave room for one.
    let gap = if last.is_whitespace() {
        px(0.)
    } else {
        shape(" ").width
    };
    let left = rect.right() + gap;
    let room = clip.right() - left;
    let mut line = shape(hint);
    if line.width > room {
        // Elide at the field edge, as lib_ui does for a placeholder.
        let mut kept: Vec<char> = hint.chars().collect();
        while !kept.is_empty() {
            kept.pop();
            let mut cut: String = kept.iter().collect();
            cut.push('…');
            line = shape(&cut);
            if line.width <= room {
                break;
            }
        }
        if line.width > room {
            return;
        }
    }
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        let _ = line.paint(
            point(left, rect.top()),
            rect.size.height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    });
}
