//! The `ready-ai-compose` variants for styles (`QUILL_DEMO_AI`): `editor`
//! (New Style with an icon and a name, Create pressed without
//! instructions), `edit` (Edit Style of the user's own Pirate), `icon` (the
//! Style Icon picker), `menu` (the Style tab with Pirate as the user's own
//! style, for a right click), `preview` (a shared style with its example),
//! `preview-loading` (its lookup still running), `preview-installed` (a
//! saved style: Remove Style), `limit` (the saved-style limit toast over
//! the box) and `link` (an `addstyle/` link card in the chat).

use super::demo::CREATOR;
use super::session_mut;
use crate::ui::app::QuillApp;
use gpui_kit::*;
use quill::ai_compose::{StyleExample, StyleLimitNotice, TextCompositionStyle};
use quill::ids::FileId;
use quill::state::{StyleLookup, StyleNotice};
use quill::telegram::envelope::{StickerFormat, StickerItem, StickerSetInfo};
use quill::text::{TextEntity, TextEntityKind};
use std::sync::atomic::Ordering;

/// The custom emoji the demo seed resolves (its still is file 61).
const EMOJI: i64 = 4242;
const EMOJI_FILE: FileId = FileId(61);
const ICON_SET: i64 = 990;

fn emoji_item(id: i64, set_id: i64, emoji: &str, file: FileId) -> StickerItem {
    StickerItem {
        custom_emoji_id: Some(id),
        id,
        set_id,
        emoji: emoji.into(),
        width: 100,
        height: 100,
        format: StickerFormat::Webp,
        file_id: file,
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

/// A shared style as the lookup returns it, with its English example.
fn shared_style(installed: bool) -> TextCompositionStyle {
    let source = "The launch moves to Thursday afternoon.";
    let result = "Hey all! The launch is moving to Thursday afternoon \u{1F680}";
    let bold = result.find("Thursday afternoon").unwrap_or(0);
    TextCompositionStyle {
        name: if installed { "pirate" } else { "cheerful" }.into(),
        custom_emoji_id: EMOJI,
        title: if installed { "Pirate" } else { "Cheerful" }.into(),
        is_custom: true,
        is_creator: false,
        install_count: 1284,
        prompt: String::new(),
        creator_user_id: CREATOR,
        english_example: Some(StyleExample {
            source_text: source.into(),
            source_entities: Vec::new(),
            result_text: result.into(),
            result_entities: vec![TextEntity {
                utf8_start: bold,
                utf8_end: bold + "Thursday afternoon".len(),
                kind: TextEntityKind::Bold,
            }],
        }),
    }
}

impl QuillApp {
    /// The style variants; `false` for any other variant.
    pub(super) fn demo_ai_styles(
        &mut self,
        variant: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match variant {
            "editor" | "edit" | "icon" | "menu" | "limit" => {}
            "preview" | "preview-loading" | "preview-installed" => {
                self.demo_ai_preview(variant, window, cx);
                return true;
            }
            "link" => {
                self.demo_ai_style_link();
                cx.notify();
                return true;
            }
            _ => return false,
        }
        self.demo_own_pirate();
        self.open_ai_compose(window, cx);
        self.ai_compose_update(window, cx, |ai| {
            ai.set_mode(quill::ai_compose::AiComposeMode::Style)
        });
        match variant {
            "editor" => {
                self.open_ai_style_editor(None, window, cx);
                if let Some(editor) = self.ai_style_editor_mut() {
                    editor.emoji_id = EMOJI;
                }
                self.ai_compose
                    .style_name
                    .update(cx, |state, cx| state.set_value("Shanty", window, cx));
                self.submit_ai_style_editor(window, cx);
            }
            "edit" | "icon" => {
                let pirate = self
                    .session()
                    .and_then(|s| s.messages.ai_compose.styles.find("pirate").cloned());
                self.open_ai_style_editor(pirate, window, cx);
                if variant == "icon" {
                    self.demo_seed_style_icons();
                    self.open_ai_style_icons(cx);
                }
            }
            "limit" => {
                if let Some(session) = session_mut(self) {
                    session.messages.ai_compose.style_notice =
                        Some(StyleNotice::Limit(StyleLimitNotice::Promo {
                            count: 5,
                            premium_count: 20,
                        }));
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }

    /// Pirate becomes the user's own style, with an emoji and instructions.
    fn demo_own_pirate(&mut self) {
        let Some(session) = session_mut(self) else {
            return;
        };
        let styles = &mut session.messages.ai_compose.styles;
        let Some(mut pirate) = styles.find("pirate").cloned() else {
            return;
        };
        pirate.is_creator = true;
        pirate.custom_emoji_id = EMOJI;
        pirate.prompt = "Write in a bold, nautical tone with light pirate slang.".into();
        let mut list = styles.styles().to_vec();
        for style in &mut list {
            if style.name == pirate.name {
                *style = pirate.clone();
            }
        }
        styles.set_server(list);
    }

    /// The default topic icons and one installed emoji set, for the icon
    /// picker. Cells without a local file show their emoji.
    fn demo_seed_style_icons(&mut self) {
        let Some(session) = session_mut(self) else {
            return;
        };
        session.threads.forum_topic_icons = (0..8)
            .map(|ix| emoji_item(5000 + ix, 0, "\u{2B50}", EMOJI_FILE))
            .collect();
        let set: Vec<StickerItem> = [
            "\u{1F3F4}\u{200D}\u{2620}\u{FE0F}",
            "\u{2693}",
            "\u{1F99C}",
            "\u{1F30A}",
            "\u{1F5FA}\u{FE0F}",
            "\u{1F4B0}",
            "\u{1F9ED}",
            "\u{1F3DD}\u{FE0F}",
            "\u{1F988}",
            "\u{1F419}",
        ]
        .iter()
        .enumerate()
        .map(|(ix, emoji)| emoji_item(6000 + ix as i64, ICON_SET, emoji, FileId(0)))
        .collect();
        session
            .media
            .media_library
            .set_stickers
            .insert(ICON_SET, set.clone());
        session.stickers.emoji.installed_sets = vec![StickerSetInfo {
            id: ICON_SET,
            title: "High Seas".into(),
            name: "high_seas".into(),
            size: i32::try_from(set.len()).unwrap_or(0),
            is_installed: true,
            is_official: false,
            is_owned: false,
            covers: Vec::new(),
            cover_files: Vec::new(),
            is_viewed: true,
        }];
    }

    /// The preview over the chat, answered as a live lookup would be
    /// (except `preview-loading`).
    pub(super) fn demo_ai_preview(
        &mut self,
        variant: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let installed = variant == "preview-installed";
        let style = shared_style(installed);
        if installed && let Some(session) = session_mut(self) {
            let mut list = session.messages.ai_compose.styles.styles().to_vec();
            for saved in &mut list {
                if saved.name == style.name {
                    *saved = style.clone();
                }
            }
            session.messages.ai_compose.styles.set_server(list);
        }
        self.open_ai_style_preview(style.name.clone(), window, cx);
        let token = self.ai_compose.preview.as_ref().map(|p| p.token);
        if variant != "preview-loading"
            && let (Some(token), Some(session)) = (token, session_mut(self))
        {
            session.messages.ai_compose.style_lookup = Some(StyleLookup {
                token,
                name: style.name.clone(),
                outcome: Some(Ok(style)),
            });
        }
        cx.notify();
    }

    /// A message in the open chat with an `addstyle/` link card.
    fn demo_ai_style_link(&mut self) {
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":140,"chat_id":11,"is_outgoing":false,"date":1790632400,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Try my style: t.me/addstyle/cheerful","entities":[{{"@type":"textEntity","offset":14,"length":22,"type":{{"@type":"textEntityTypeUrl"}}}}]}},"link_preview":{{"@type":"linkPreview","url":"https://t.me/addstyle/cheerful","display_url":"t.me/addstyle/cheerful","site_name":"Telegram","title":"Cheerful","description":{{"@type":"formattedText","text":"AI style for rewriting messages.","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeTextCompositionStyle","custom_emoji_id":"{EMOJI}"}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0}},"link_preview_options":null}}}}}}"#
        );
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let seq = &self.demo_ui.seq;
        let owned = quill::telegram::client::copy_and_parse(&json, seq, &sink);
        if let (Some(owned), Some(session)) = (owned, self.demo_session.as_mut()) {
            session.apply(owned);
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        }
    }
}
