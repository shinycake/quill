//! The compose field's placeholder follows the chat (tdesktop
//! `updateFieldPlaceholder`): "New message…" while editing, "Message for N
//! Stars" in paid chats, "Broadcast a message…" in channels, "Send
//! anonymously…" for an anonymous admin. An inline bot's own hint is drawn
//! after `@bot ` while that is all that is typed.

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
        PlaceholderFacts {
            editing: self.composer_ui.pending_edit.is_some(),
            stars_per_message: session.paid_send_price(chat),
            broadcast,
            silent: broadcast && self.composer_effective_silent(),
            anonymous_admin: !broadcast
                && session
                    .chats
                    .get(&chat.0)
                    .is_some_and(|chat| chat.my_is_anonymous_admin),
        }
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
        if self.composer_ui.placeholder_shown == Some((facts, language)) {
            return;
        }
        self.composer_ui.placeholder_shown = Some((facts, language));
        let text = ComposerPlaceholder::for_facts(facts).text();
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
    // The kit composer renders at `text_sm` (Input Size::Medium).
    let font_size = window.rem_size() * 0.875;
    let font = window.text_style().font();
    let run = |len: usize| TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let system = window.text_system().clone();
    let line = system.shape_line(hint.to_string().into(), font_size, &[run(hint.len())], None);
    // Right after the name there is no separator yet: leave room for one.
    let gap = if last.is_whitespace() {
        px(0.)
    } else {
        system
            .shape_line(" ".into(), font_size, &[run(1)], None)
            .width
    };
    let origin = point(rect.right() + gap, rect.top());
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        let _ = line.paint(origin, rect.size.height, TextAlign::Left, None, window, cx);
    });
}
