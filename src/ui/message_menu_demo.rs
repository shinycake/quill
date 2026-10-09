//! `--screenshot-demo ready-message-menu`: the message context menu (and
//! its Report / sticker set / moderated delete dialogs) over injected
//! messages of every kind. `QUILL_DEMO_MENU=<scenario>` picks which; no
//! live Telegram is involved.

use super::app::QuillApp;
use super::demo::{
    demo_file_json, demo_media_allowlist, demo_thumb_png_path, seed_ready_chats_session,
};
use super::menu_states::MessageMenuState;
use super::message_menu_ui::{MessageMenuPage, ModerationOffer};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{
    Audience, MessageAudience, MessageReportFlow, MessageReportStage, Session, StickerSetView,
    StickerSetViewStage,
};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    AddedReaction, AddedReactionsPage, MessageActions, MessageReadDate, MessageSender,
    MessageViewer, ReactionType, ReportOption, StickerFormat, StickerItem,
};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The dialog a scenario shows over the history.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DemoDialog {
    None,
    ReportPick,
    ReportSub,
    ReportText,
    ReportDone,
    StickerSet,
    ModeratedDelete,
}

struct Scenario {
    chat_id: i64,
    message_id: i64,
    page: MessageMenuPage,
    dialog: DemoDialog,
    position: (f32, f32),
}

fn scenario_name() -> String {
    std::env::var("QUILL_DEMO_MENU").unwrap_or_else(|_| "photo".into())
}

fn scenario(name: &str) -> Scenario {
    let at = |chat_id, message_id, x, y| Scenario {
        chat_id,
        message_id,
        page: MessageMenuPage::Main,
        dialog: DemoDialog::None,
        position: (x, y),
    };
    match name {
        "document" => at(11, 902, 380., 260.),
        "downloading" => at(11, 903, 380., 300.),
        "video" => at(11, 904, 380., 260.),
        "gif" => at(11, 905, 380., 260.),
        "sticker" => at(11, 906, 380., 260.),
        "audio" => at(11, 907, 380., 300.),
        "audio-save-to" => Scenario {
            page: MessageMenuPage::SaveTo,
            ..at(11, 907, 380., 300.)
        },
        "voice-private" => at(11, 908, 380., 260.),
        "uploading" => at(11, 909, 380., 260.),
        "group" => at(31, 3101, 340., 240.),
        "group-audience" => Scenario {
            page: MessageMenuPage::Audience,
            ..at(31, 3101, 340., 200.)
        },
        "channel" => at(13, 1301, 340., 240.),
        "protected" => at(32, 3201, 340., 240.),
        "report-pick" => Scenario {
            dialog: DemoDialog::ReportPick,
            ..at(31, 3101, 340., 240.)
        },
        "report-sub" => Scenario {
            dialog: DemoDialog::ReportSub,
            ..at(31, 3101, 340., 240.)
        },
        "report-text" => Scenario {
            dialog: DemoDialog::ReportText,
            ..at(31, 3101, 340., 240.)
        },
        "report-done" => Scenario {
            dialog: DemoDialog::ReportDone,
            ..at(31, 3101, 340., 240.)
        },
        "sticker-set" => Scenario {
            dialog: DemoDialog::StickerSet,
            ..at(11, 906, 380., 260.)
        },
        "moderate" => Scenario {
            dialog: DemoDialog::ModeratedDelete,
            ..at(31, 3102, 340., 240.)
        },
        // "photo" and anything unknown.
        _ => at(11, 901, 380., 260.),
    }
}

fn actions() -> MessageActions {
    MessageActions {
        can_be_copied: true,
        can_be_deleted_only_for_self: true,
        can_be_forwarded: true,
        can_be_replied: true,
        can_be_saved: true,
        can_report_chat: true,
        can_get_link: true,
        ..MessageActions::default()
    }
}

pub(super) fn seed_ready_message_menu_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let apply = |session: &mut Session, value: serde_json::Value| {
        if let Some(owned) = copy_and_parse(&value.to_string(), &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let now = quill::local_time::now_unix();
    let date = |ago: i64| (now - ago) as i32;
    let thumb = demo_thumb_png_path();
    let voice_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let video_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let notes_path = demo_media_allowlist()
        .join("demo-notes.txt")
        .to_string_lossy()
        .into_owned();
    let file = |id: i32, path: &str, done: bool| -> serde_json::Value {
        serde_json::from_str(&demo_file_json(id, path, done)).unwrap()
    };
    let text = |body: &str| json!({"@type":"formattedText","text":body,"entities":[]});
    let message = |chat: i64, id: i64, out: bool, ago: i64, content: serde_json::Value| {
        json!({"@type":"updateNewMessage","message":{
            "id":id,"chat_id":chat,"is_outgoing":out,"date":date(ago),"content":content}})
    };
    // A short history in the open private chat: the seed's messages go.
    apply(
        &mut session,
        json!({"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}),
    );
    apply(
        &mut session,
        message(
            11,
            901,
            false,
            3600,
            json!({
            "@type":"messagePhoto",
            "photo":{"@type":"photo","has_stickers":false,"sizes":[
                {"@type":"photoSize","type":"m","photo":file(901, &thumb, true),"width":320,"height":240,"progressive_sizes":[]}]},
            "caption":text("Sunset from the roof"),"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            902,
            false,
            3000,
            json!({
            "@type":"messageDocument",
            "document":{"@type":"document","file_name":"Quarterly plan.pdf","mime_type":"application/pdf","document":file(902, &notes_path, true)},
            "caption":text("")}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            903,
            false,
            2400,
            json!({
            "@type":"messageDocument",
            "document":{"@type":"document","file_name":"Design archive.zip","mime_type":"application/zip","document":file(903, "", false)},
            "caption":text("")}),
        ),
    );
    session.downloading.insert(903);
    apply(
        &mut session,
        message(
            11,
            904,
            false,
            1800,
            json!({
            "@type":"messageVideo",
            "video":{"@type":"video","duration":12,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":file(904, &video_path, true)},
            "alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,
            "caption":text(""),"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            905,
            false,
            1500,
            json!({
            "@type":"messageAnimation",
            "animation":{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"cat.gif.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,
                "thumbnail":{"@type":"thumbnail","format":{"@type":"thumbnailFormatJpeg"},"width":240,"height":140,"file":file(9051, &thumb, true)},
                "animation":file(905, "", false)},
            "caption":text(""),"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            906,
            false,
            1200,
            json!({
            "@type":"messageSticker","is_premium":false,
            "sticker":{"@type":"sticker","id":"9006","set_id":"77","width":512,"height":512,"emoji":"😀","format":{"@type":"stickerFormatWebp"},
                "full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},
                "thumbnail":{"@type":"thumbnail","format":{"@type":"thumbnailFormatPng"},"width":128,"height":128,"file":file(9061, &thumb, true)},
                "sticker":file(906, &thumb, true)}}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            907,
            false,
            900,
            json!({
            "@type":"messageAudio",
            "audio":{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada Lovelace","file_name":"night.mp3","mime_type":"audio/mpeg",
                "album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[],"audio":file(907, &voice_path, true)},
            "caption":text("")}),
        ),
    );
    apply(
        &mut session,
        message(
            11,
            908,
            true,
            600,
            json!({
            "@type":"messageVoiceNote",
            "voice_note":{"@type":"voiceNote","duration":8,"waveform":quill::voice::waveform_base64(&[4, 16, 28, 12, 8, 20, 6, 18, 10, 24, 8, 14]),"mime_type":"audio/ogg","speech_recognition_result":null,"voice":file(908, &voice_path, true)},
            "caption":text(""),"is_listened":true}),
        ),
    );
    apply(
        &mut session,
        json!({"@type":"updateNewMessage","message":{
            "id":909,"chat_id":11,"is_outgoing":true,"date":date(60),
            "sending_state":{"@type":"messageSendingStatePending","sending_id":1},
            "content":{"@type":"messageDocument",
                "document":{"@type":"document","file_name":"Holiday photos.zip","mime_type":"application/zip","document":file(909, "", false)},
                "caption":text("")}}}),
    );
    session.stickers.installed_loaded = true;
    session.gifs.loaded = true;

    // A supergroup where the user moderates, with people who saw and
    // reacted.
    for (id, first, last) in [
        (8, "Alice", "Cohen"),
        (9, "Ben", "Levi"),
        (10, "Dana", "Katz"),
        (14, "Eli", "Mor"),
        (15, "Noa", "Bar"),
    ] {
        apply(
            &mut session,
            json!({"@type":"updateUser","user":{"id":id,"first_name":first,"last_name":last,"type":{"@type":"userTypeRegular"}}}),
        );
    }
    apply(
        &mut session,
        json!({"@type":"updateNewChat","chat":{"id":31,"title":"Design team","type":{"@type":"chatTypeSupergroup","supergroup_id":31,"is_channel":false},"unread_count":0}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateChatPosition","chat_id":31,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":31,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":true,"can_delete_messages":true}}}}),
    );
    let sender = |id: i64| json!({"@type":"messageSenderUser","user_id":id});
    let reactions = json!({"@type":"messageReactions","reactions":[
        {"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":3,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[sender(9), sender(10), sender(14)]},
        {"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[sender(15)]}],
        "are_tags":false,"paid_reactors":[],"can_get_added_reactions":true});
    apply(
        &mut session,
        json!({"@type":"updateNewMessage","message":{
            "id":3101,"chat_id":31,"sender_id":sender(8),"is_outgoing":false,"date":date(5400),
            "content":{"@type":"messageText","text":text("The new onboarding flow is ready for review. Please leave comments in the doc by Friday.")},
            "interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":reactions}}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateNewMessage","message":{
            "id":3102,"chat_id":31,"sender_id":sender(14),"is_outgoing":false,"date":date(4200),
            "content":{"@type":"messageText","text":text("Buy cheap followers now!! t.me/spam-bot")}}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateNewMessage","message":{
            "id":3103,"chat_id":31,"is_outgoing":true,"date":date(3000),
            "content":{"@type":"messageText","text":text("Thanks, taking a look now.")}}}),
    );

    // A channel post, and a chat that forbids copying.
    apply(
        &mut session,
        message(
            13,
            1301,
            false,
            7200,
            json!({"@type":"messageText","text":text("Release 1.4 is out: faster search, new themes and bug fixes.")}),
        ),
    );
    apply(
        &mut session,
        json!({"@type":"updateNewChat","chat":{"id":32,"title":"Private archive","type":{"@type":"chatTypeSupergroup","supergroup_id":32,"is_channel":false},"has_protected_content":true,"unread_count":0}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateChatPosition","chat_id":32,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false}}),
    );
    apply(
        &mut session,
        json!({"@type":"updateNewMessage","message":{
            "id":3201,"chat_id":32,"sender_id":sender(9),"is_outgoing":false,"date":date(3600),
            "content":{"@type":"messageText","text":text("Keep this one between us.")}}}),
    );

    let chosen = scenario(&scenario_name());
    let (chat, message_id) = (ChatId(chosen.chat_id), MessageId(chosen.message_id));
    session.open_chat(chat);

    // What TDLib's `messageProperties` would have said.
    let mut properties = actions();
    match chosen.chat_id {
        31 => {
            properties.can_get_viewers = true;
            properties.can_be_deleted_for_all_users = true;
            properties.can_report_supergroup_spam = true;
            properties.can_delete_reactions = true;
        }
        13 => {
            properties.can_get_link = true;
            properties.can_be_replied = false;
        }
        32 => {
            properties.can_be_copied = false;
            properties.can_be_forwarded = false;
            properties.can_be_saved = false;
        }
        _ => {}
    }
    if chosen.message_id == 908 {
        properties.can_get_read_date = true;
        properties.can_be_deleted_for_all_users = true;
    }
    session.message_menu_actions = Some((chat, message_id, properties));

    // Seen / reacted / read-date lookups that "arrived".
    if chosen.chat_id == 31 {
        let mut audience = MessageAudience::new(chat, message_id);
        audience.viewers = Audience::Ready(
            [8, 9, 10, 14, 15]
                .into_iter()
                .enumerate()
                .map(|(ix, user_id)| MessageViewer {
                    user_id,
                    view_date: date(4000 - ix as i64 * 500),
                })
                .collect(),
        );
        let reaction = |user: i64, emoji: &str, ago: i64| AddedReaction {
            reaction_type: ReactionType::emoji(emoji),
            sender: MessageSender::User { user_id: user },
            is_outgoing: false,
            date: date(ago),
        };
        audience.reactions = Audience::Ready(AddedReactionsPage {
            total_count: 4,
            reactions: vec![
                reaction(9, "👍", 3500),
                reaction(10, "👍", 3000),
                reaction(14, "👍", 90_000),
                reaction(15, "❤", 1200),
            ],
            next_offset: String::new(),
        });
        session.message_audience = Some(audience);
    }
    if chosen.message_id == 908 {
        let mut audience = MessageAudience::new(chat, message_id);
        audience.read_date = Audience::Ready(MessageReadDate::Read(date(240)));
        session.message_audience = Some(audience);
    }

    // The sticker's set is not installed yet; favorites are loaded.
    if chosen.message_id == 906 {
        session.stickers.sets.clear();
        session.stickers.favorites.clear();
    }

    // The reaction strip above the menu.
    {
        use quill::state::{MessageReactionOptions, ReactionChoice};
        session.message_reaction_options = Some(MessageReactionOptions {
            chat_id: chat,
            message_id,
            top: ["👍", "❤", "🔥", "😂", "😮", "🎉"]
                .into_iter()
                .map(|e| ReactionChoice::Emoji(e.to_string()))
                .collect(),
            recent: Vec::new(),
            popular: Vec::new(),
            allow_custom_emoji: false,
        });
    }

    // Dialog state.
    match chosen.dialog {
        DemoDialog::ReportPick
        | DemoDialog::ReportSub
        | DemoDialog::ReportText
        | DemoDialog::ReportDone => {
            let option = |id: &str, text: &str| ReportOption {
                id: id.into(),
                text: text.into(),
            };
            let reasons = vec![
                option("c3BhbQ==", "Spam"),
                option("dmlvbGVuY2U=", "Violence"),
                option("Y2hpbGQ=", "Child Abuse"),
                option("cG9ybg==", "Pornography"),
                option("ZHJ1Z3M=", "Illegal Drugs"),
                option("cGVyc29uYWw=", "Personal Details"),
                option("b3RoZXI=", "Other"),
            ];
            let mut flow = MessageReportFlow {
                chat_id: chat,
                message_ids: vec![message_id],
                stage: MessageReportStage::PickOption {
                    title: "Why are you reporting this message?".into(),
                    options: reasons.clone(),
                },
                trail: Vec::new(),
                previous: Vec::new(),
            };
            match chosen.dialog {
                DemoDialog::ReportSub => {
                    flow.trail.push("Violence".into());
                    flow.previous.push((
                        "Why are you reporting this message?".into(),
                        reasons.clone(),
                    ));
                    flow.stage = MessageReportStage::PickOption {
                        title: "Violence".into(),
                        options: vec![
                            option("Yw==", "Threats of violence"),
                            option("ZA==", "Graphic or disturbing content"),
                            option("ZQ==", "Glorifying violence"),
                        ],
                    };
                }
                DemoDialog::ReportText => {
                    flow.trail.push("Other".into());
                    flow.previous.push((
                        "Why are you reporting this message?".into(),
                        reasons.clone(),
                    ));
                    flow.stage = MessageReportStage::TextRequired {
                        option_id: "b3RoZXI=".into(),
                        is_optional: false,
                    };
                }
                DemoDialog::ReportDone => flow.stage = MessageReportStage::Reported,
                _ => {}
            }
            session.message_report = Some(flow);
        }
        DemoDialog::StickerSet => {
            let sticker = |id: i64| StickerItem {
                custom_emoji_id: None,
                id,
                set_id: 77,
                emoji: "😀".into(),
                width: 512,
                height: 512,
                format: StickerFormat::Webp,
                file_id: quill::ids::FileId(9100 + id as i32),
                thumb_file_id: Some(quill::ids::FileId(9061)),
                thumb_width: 128,
                thumb_height: 128,
                requires_premium: false,
            };
            session.sticker_set_view = Some(StickerSetView {
                set_id: 77,
                stage: StickerSetViewStage::Ready {
                    title: "Cozy Cats".into(),
                    installed: false,
                    stickers: (1..=10).map(sticker).collect(),
                },
                files_requested: true,
            });
        }
        _ => {}
    }
    session
}

impl QuillApp {
    /// Open the menu (or dialog) of the scenario `QUILL_DEMO_MENU` names.
    pub(super) fn demo_setup_message_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = scenario(&scenario_name());
        let (chat_id, message_id) = (ChatId(chosen.chat_id), MessageId(chosen.message_id));
        match chosen.dialog {
            DemoDialog::None => {
                self.message_menu = Some(MessageMenuState {
                    chat_id,
                    message_id,
                    position: point(px(chosen.position.0), px(chosen.position.1)),
                });
                self.message_menu_ui.page = chosen.page;
            }
            DemoDialog::ReportPick
            | DemoDialog::ReportSub
            | DemoDialog::ReportText
            | DemoDialog::ReportDone => self.message_menu_ui.report_open = true,
            DemoDialog::StickerSet => self.message_menu_ui.sticker_set_open = true,
            DemoDialog::ModeratedDelete => {
                let message = self.session().and_then(|s| {
                    s.histories
                        .get(&chat_id.0)?
                        .messages
                        .get(&message_id.0)
                        .cloned()
                });
                if let Some(message) = message {
                    let actions = self
                        .session()
                        .and_then(|s| s.message_menu_actions)
                        .map(|(_, _, a)| a);
                    let mut properties = actions.unwrap_or_default();
                    properties.can_be_deleted_for_all_users = true;
                    properties.can_report_supergroup_spam = true;
                    let offer: Option<ModerationOffer> =
                        self.moderation_offer(chat_id, &message, Some(properties));
                    if let Some(confirm) = quill::composer::DeleteConfirm::for_message(
                        chat_id, message_id, false, false,
                    ) {
                        // The kit dialog layer exists once the window has
                        // rendered; open the alert a moment after setup.
                        cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1200))
                                .await;
                            let _ = this.update_in(cx, |this, window, cx| {
                                this.open_delete_dialog_with(confirm, offer, window, cx);
                            });
                        })
                        .detach();
                    }
                }
            }
        }
        self.status_note = format!("screenshot demo — message menu: {}", scenario_name());
    }
}
