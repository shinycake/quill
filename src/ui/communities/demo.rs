//! `ready-communities`: the TDLib 1.8.70 community screens over an
//! injected session (no live Telegram). "Rustaceans" shows as one chat in
//! the list; "Design Guild" is a community the viewer owns, with pending
//! requests. Both have a photo.
//!
//! `QUILL_DEMO_COMMUNITY` opens a screen over it: `info` (the community
//! panel), `requests`, `pick` (Add to Community from a chat), `visibility`,
//! `confirm-add`, `remove`, `choose` (a chat to add), `hidden-chat`,
//! `ungroup`, `ban` (the owned-chats warning), `manage` (the channel's
//! Manage box with its community section), `manage-linked`, `menu` (the
//! collapsed row's menu with its Mute submenu open), `mute-for` or
//! `create` (Create Community from the picker: photo and name).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::demo::{demo_file_json, demo_media_allowlist, seed_ready_chats_session};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::fetch::Fetch;
use quill::state::{
    AddChatRequestList, InfoPanelTarget, ProfileChatsFetch, ProfileChatsKind, Session,
};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::CommunityAddChatRequest;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-communities",
        seed_communities_session,
        "screenshot demo — communities: collapsed row, info panel, add/remove/request boxes (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_communities),
];

/// Shown as one chat.
const RUSTACEANS: i64 = 900;
/// Owned, with pending requests.
const GUILD: i64 = 901;
/// The viewer's own channel, in no community yet.
const OWN_CHANNEL: i64 = 66;
/// A requested chat only its members can see.
const HIDDEN_REQUEST: i64 = 69;
const SUGGESTER: i64 = 501;
/// A bot the account owns, offered by the chooser (`getOwnedBots`).
const OWN_BOT: i64 = 502;
/// The communities' photo files.
const RUSTACEANS_PHOTO: i32 = 9001;
const GUILD_PHOTO: i32 = 9002;

/// A downloaded `chatPhotoInfo` from a fixture image.
fn photo_json(file_id: i32, image: &str) -> String {
    let path = demo_media_allowlist()
        .join("showcase")
        .join(image)
        .to_string_lossy()
        .into_owned();
    let file = demo_file_json(file_id, &path, true);
    format!(
        r#"{{"@type":"chatPhotoInfo","small":{file},"big":{file},"minithumbnail":null,"has_animation":false,"is_personal":false}}"#
    )
}

fn chat_json(id: i64, title: &str, channel: bool, unread: i32) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":{channel}}},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#
    )
}

fn supergroup_json(id: i64, channel: bool, members: i32, creator: bool) -> String {
    let status = if creator {
        r#"{"@type":"chatMemberStatusCreator","custom_title":"","is_anonymous":false,"is_member":true}"#
    } else {
        r#"{"@type":"chatMemberStatusMember"}"#
    };
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{id},"is_channel":{channel},"member_count":{members},"status":{status}}}}}"#
    )
}

/// Last message + main-list position; a higher `order` sorts first.
fn listed_json(id: i64, order: i64, date: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{id}01,"chat_id":{id},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]}}"#
    )
}

fn seed_communities_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    let seq = AtomicU64::new(session.last_seq);
    let community = |id: i64, name: &str, owner: bool, collapsed: bool, photo: String| {
        let status = if owner {
            "communityMemberStatusCreator"
        } else {
            "communityMemberStatusMember"
        };
        format!(
            r#"{{"@type":"updateCommunity","community":{{"@type":"community","id":{id},"have_access":true,"name":"{name}","photo":{photo},"date":1790000000,"status":{{"@type":"{status}"}},"permissions":{{"@type":"communityPermissions","can_edit_chat_list":false}},"are_chats_collapsed":{collapsed}}}}}"#
        )
    };
    let full_info = |id: i64, chats: &[(i64, bool, bool)], requests: i32| {
        let chats: Vec<String> = chats
            .iter()
            .map(|(chat_id, view, hidden)| {
                format!(
                    r#"{{"@type":"communityChat","chat_id":{chat_id},"can_view_history":{view},"is_hidden":{hidden}}}"#
                )
            })
            .collect();
        format!(
            r#"{{"@type":"updateCommunityFullInfo","community_id":{id},"community_full_info":{{"@type":"communityFullInfo","chats":[{}],"administrator_count":2,"banned_count":0,"add_chat_request_count":{requests}}}}}"#,
            chats.join(",")
        )
    };
    let jsons = [
        chat_json(61, "Rust Weekly", true, 3),
        chat_json(62, "Rust Help", false, 0),
        chat_json(63, "Rust Jobs", true, 0),
        chat_json(64, "Typeface Lab", false, 0),
        chat_json(65, "Color Club", false, 0),
        chat_json(OWN_CHANNEL, "Field Notes", true, 0),
        chat_json(67, "Grid Systems", false, 0),
        chat_json(HIDDEN_REQUEST, "Kerning Club", false, 0),
        chat_json(70, "Brush Talk", true, 0),
        supergroup_json(61, true, 12_400, false),
        supergroup_json(62, false, 860, false),
        supergroup_json(63, true, 2_100, false),
        supergroup_json(64, false, 48, false),
        supergroup_json(65, false, 310, false),
        supergroup_json(OWN_CHANNEL, true, 95, true),
        supergroup_json(67, false, 1_200, true),
        supergroup_json(HIDDEN_REQUEST, false, 37, false),
        supergroup_json(70, true, 5_300, false),
        listed_json(61, 28, 1790632000, "Issue 412 is out"),
        listed_json(62, 26, 1790631500, "Try a Box<dyn Trait> there"),
        listed_json(65, 24, 1790631000, "Palette night on Friday"),
        listed_json(OWN_CHANNEL, 22, 1790630000, "Sketches from the trip"),
        listed_json(67, 21, 1790629000, "Twelve columns or eight?"),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{SUGGESTER},"first_name":"Mira","last_name":"Lind","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":true,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{OWN_BOT},"first_name":"Palette Bot","last_name":"","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":true,"type":{{"@type":"userTypeBot","can_be_edited":true,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"is_inline":false,"inline_query_placeholder":"","need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}}}}"#
        ),
        community(
            RUSTACEANS,
            "Rustaceans",
            false,
            true,
            photo_json(RUSTACEANS_PHOTO, "av03.png"),
        ),
        community(
            GUILD,
            "Design Guild",
            true,
            false,
            photo_json(GUILD_PHOTO, "av11.png"),
        ),
        full_info(
            RUSTACEANS,
            &[(61, true, false), (62, true, false), (63, true, false)],
            0,
        ),
        full_info(
            GUILD,
            &[(65, true, false), (67, true, false), (64, false, true)],
            2,
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.communities.joined = Fetch::Ready(vec![RUSTACEANS, GUILD]);
    session.communities.owned_bots = Fetch::Ready(vec![OWN_BOT]);
    session.communities.add_chat_requests.insert(
        GUILD,
        Fetch::Ready(AddChatRequestList {
            requests: vec![
                CommunityAddChatRequest {
                    chat_id: 70,
                    is_hidden: false,
                    sender_user_id: SUGGESTER,
                    date: 1790630000,
                },
                CommunityAddChatRequest {
                    chat_id: HIDDEN_REQUEST,
                    is_hidden: true,
                    sender_user_id: SUGGESTER,
                    date: 1790629000,
                },
            ],
            total_count: 2,
            next_offset: None,
        }),
    );
    session.users_state.profile_chat_lists.insert(
        (ProfileChatsKind::SuitableCommunityChats, 0),
        ProfileChatsFetch::complete(vec![OWN_CHANNEL]),
    );
    session.open_chat(ChatId(11));
    session
}

impl QuillApp {
    fn demo_communities(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let step = std::env::var("QUILL_DEMO_COMMUNITY").unwrap_or_default();
        let open = match step.as_str() {
            "info" => {
                self.open_info_panel_target(InfoPanelTarget::Community(GUILD), window, cx);
                None
            }
            "requests" => {
                self.open_info_panel_target(InfoPanelTarget::Community(GUILD), window, cx);
                self.open_community_requests(GUILD, cx);
                None
            }
            "choose" => {
                self.open_info_panel_target(InfoPanelTarget::Community(GUILD), window, cx);
                self.open_choose_community_chat(GUILD, window, cx);
                None
            }
            "manage" | "manage-linked" => {
                let chat = if step == "manage" { OWN_CHANNEL } else { 67 };
                self.open_group_edit_box(ChatId(chat), window, cx);
                None
            }
            "menu" => {
                self.admin.community.flow.row_menu = Some(RowMenu {
                    community_id: RUSTACEANS,
                    position: point(px(180.), px(150.)),
                    submenu: Some(crate::ui::menu_states::ChatMenuSubmenu::Mute),
                });
                None
            }
            "create" => {
                self.open_new_community(OWN_CHANNEL, window, cx);
                None
            }
            "mute-for" => {
                self.open_community_mute_for(RUSTACEANS, window, cx);
                None
            }
            "pick" => Some(CommunityBox::Pick {
                chat_id: OWN_CHANNEL,
            }),
            "visibility" => Some(CommunityBox::Visibility {
                community_id: RUSTACEANS,
                chat_id: OWN_CHANNEL,
                hidden: false,
                back: Some(Back::Pick {
                    chat_id: OWN_CHANNEL,
                }),
            }),
            "confirm-add" => Some(CommunityBox::ConfirmAdd {
                community_id: RUSTACEANS,
                chat_id: OWN_CHANNEL,
                hidden: false,
            }),
            "remove" => Some(CommunityBox::ConfirmRemove {
                community_id: GUILD,
                chat_id: 67,
            }),
            "hidden-chat" => Some(CommunityBox::HiddenChat {
                community_id: GUILD,
                chat_id: HIDDEN_REQUEST,
                owner_user_id: SUGGESTER,
            }),
            "ungroup" => Some(CommunityBox::Ungroup {
                community_id: RUSTACEANS,
            }),
            "ban" => Some(CommunityBox::BanWarning {
                community_id: GUILD,
                member: CommunityMember::User(SUGGESTER),
                name: "Mira".into(),
                owned_chat_ids: vec![65],
            }),
            _ => None,
        };
        if let Some(open) = open {
            self.show_community_box(open, cx);
        }
    }
}
