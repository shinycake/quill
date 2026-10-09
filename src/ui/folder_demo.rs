//! Screenshot fixtures for shareable folders, folder icons, recommended
//! folders and the vertical folder column (injected through the reducer,
//! no live Telegram).

use super::folders::apply_ready_folders;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChatFolderInviteLink, ChatFolderInviteLinkInfo, ChatFolderSpec, RecommendedChatFolder,
};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Recommended folders as `getRecommendedChatFolders` would answer.
pub(super) fn apply_demo_recommended_folders(session: &mut Session) {
    let all_types = ChatFolderSpec {
        include_contacts: true,
        include_non_contacts: true,
        include_bots: true,
        include_groups: true,
        include_channels: true,
        ..ChatFolderSpec::default()
    };
    session.recommended_folders = Some(vec![
        RecommendedChatFolder {
            spec: ChatFolderSpec {
                name: "Unread".into(),
                icon_name: Some("Unread".into()),
                exclude_read: true,
                ..all_types.clone()
            },
            description: "Chats with new messages".into(),
        },
        RecommendedChatFolder {
            spec: ChatFolderSpec {
                name: "Personal".into(),
                icon_name: Some("Private".into()),
                include_contacts: true,
                include_non_contacts: true,
                ..ChatFolderSpec::default()
            },
            description: "Chats with people".into(),
        },
    ]);
}

/// Four folders with distinct icons, nicer chat names, and a shareable
/// "Work" folder with two invite links.
pub(super) fn apply_ready_folders_share(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_folders(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let name = |id: i32, text: &str, icon: &str, shareable: bool| {
        format!(
            r#"{{"@type":"chatFolderInfo","id":{id},"name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"animate_custom_emoji":false}},"icon":{{"@type":"chatFolderIcon","name":"{icon}"}},"color_id":-1,"is_shareable":{shareable},"has_my_invite_links":{shareable}}}"#
        )
    };
    let folders = format!(
        r#"{{"@type":"updateChatFolders","chat_folders":[{},{},{},{}],"main_chat_list_position":0,"are_tags_enabled":false}}"#,
        name(1, "Work", "Work", true),
        name(2, "News", "Channels", false),
        name(3, "Family", "Home", false),
        name(4, "Travel", "Travel", false),
    );
    let titles = [
        (11, "Maya Chen"),
        (12, "Studio standup"),
        (13, "Design Weekly"),
        (14, "Product crew"),
        (16, "Design Weekly chat"),
    ];
    for json in std::iter::once(folders).chain(titles.iter().map(|(id, title)| {
        format!(r#"{{"@type":"updateChatTitle","chat_id":{id},"title":"{title}"}}"#)
    })) {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.folder_specs.insert(
        1,
        ChatFolderSpec {
            name: "Work".into(),
            icon_name: Some("Work".into()),
            included_chat_ids: vec![13, 14, 16],
            ..ChatFolderSpec::default()
        },
    );
    session.folder_link_chats.insert(1, vec![13, 14, 16]);
    session.folder_invite_links.insert(
        1,
        vec![
            ChatFolderInviteLink {
                invite_link: "https://t.me/addlist/Xk3pQ9aBn2".into(),
                name: "Design team".into(),
                chat_ids: vec![13, 16],
            },
            ChatFolderInviteLink {
                invite_link: "https://t.me/addlist/Zr7mLw2Dq8".into(),
                name: String::new(),
                chat_ids: vec![14],
            },
        ],
    );
    apply_demo_recommended_folders(session);
}

/// What `checkChatFolderInviteLink` answers for a folder the user does not
/// have yet: two chats to join and one already joined.
pub(super) fn apply_ready_folder_invite(session: &mut Session, link: &str) {
    session.folder_invite_link = Some(link.to_string());
    session.folder_invite_info = Some(ChatFolderInviteLinkInfo {
        folder: quill::telegram::envelope::ChatFolderInfo {
            id: 0,
            name: "Design".into(),
            icon_name: "Palette".into(),
            color_id: -1,
            is_shareable: true,
            has_my_invite_links: false,
        },
        missing_chat_ids: vec![13, 16],
        added_chat_ids: vec![14],
    });
}
