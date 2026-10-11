//! Which entries the open chat's header menu (the vertical ellipsis) offers,
//! in Telegram Desktop's order. `Filler::fillHistoryActions` in
//! `window/window_peer_menu.cpp` runs, for the chat history: mute, new topic,
//! info, view as topics, manage, boost, create poll, set wallpaper, discussion,
//! export, report. The profile section adds statistics, auto-delete and gifts.
//! Quill's own entries (archive, folders, shared media, new window, close
//! secret chat) follow.

use crate::telegram::envelope::ChatKind;

/// One header-menu entry. The UI maps each to a label and a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderEntry {
    Mute,
    NewTopic,
    /// `lng_context_view_profile` / `_group` / `_channel`.
    ViewProfile(&'static str),
    ViewAsTopics,
    /// `lng_boost_group_button` / `lng_boost_channel_button`.
    Boost(&'static str),
    CreatePoll,
    /// `lng_chat_theme_wallpaper`.
    Wallpaper,
    ViewDiscussion,
    Export,
    Report,
    Archive,
    Folders,
    SharedMedia,
    Statistics,
    AutoDelete,
    ReceivedGifts,
    Gift,
    NewWindow,
    CloseSecretChat,
}

/// What the open chat looks like to the menu.
#[derive(Debug, Clone, Copy)]
pub struct HeaderFacts<'a> {
    pub kind: &'a ChatKind,
    pub is_saved_messages: bool,
    /// The private chat is the user's own (Saved Messages) chat.
    pub is_self: bool,
    pub is_bot: bool,
    pub live: bool,
    /// The composer is shown (the user may post here).
    pub can_post: bool,
    /// `chatPermissions.can_send_polls` is not false.
    pub polls_allowed: bool,
    pub can_be_reported: bool,
    /// `supergroupFullInfo.can_get_statistics`.
    pub can_get_statistics: bool,
    pub exportable: bool,
    pub has_view_toggle: bool,
    pub can_create_topic: bool,
    pub has_discussion: bool,
    pub auto_delete_ready: bool,
    pub gifts_ok: bool,
    pub new_window: bool,
}

/// tdesktop `canCreatePolls`: users only for oneself and bots, secret chats
/// never, groups and channels by the send permission.
pub fn can_create_poll(facts: &HeaderFacts<'_>) -> bool {
    match facts.kind {
        ChatKind::Private { .. } => facts.is_self || facts.is_bot,
        ChatKind::Secret { .. } | ChatKind::Unknown => false,
        ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. } => {
            facts.can_post && facts.polls_allowed
        }
    }
}

/// tdesktop `addThemeEdit`: a private chat with another user.
pub fn can_set_wallpaper(facts: &HeaderFacts<'_>) -> bool {
    matches!(facts.kind, ChatKind::Private { .. }) && !facts.is_self
}

pub fn header_entries(facts: &HeaderFacts<'_>) -> Vec<HeaderEntry> {
    let mut out = vec![HeaderEntry::Mute];
    if facts.can_create_topic {
        out.push(HeaderEntry::NewTopic);
    }
    if let Some(label) = crate::chatlist_menu::profile_label(facts.kind, facts.is_saved_messages) {
        out.push(HeaderEntry::ViewProfile(label));
    }
    if facts.has_view_toggle {
        out.push(HeaderEntry::ViewAsTopics);
    }
    if let ChatKind::Supergroup { is_channel, .. } = facts.kind {
        out.push(HeaderEntry::Boost(if *is_channel {
            "Boost channel"
        } else {
            "Boost group"
        }));
    }
    if can_create_poll(facts) {
        out.push(HeaderEntry::CreatePoll);
    }
    if can_set_wallpaper(facts) {
        out.push(HeaderEntry::Wallpaper);
    }
    if facts.has_discussion {
        out.push(HeaderEntry::ViewDiscussion);
    }
    if facts.exportable {
        out.push(HeaderEntry::Export);
    }
    if facts.can_be_reported {
        out.push(HeaderEntry::Report);
    }
    out.push(HeaderEntry::Archive);
    out.push(HeaderEntry::Folders);
    if facts.live {
        out.push(HeaderEntry::SharedMedia);
    }
    if matches!(facts.kind, ChatKind::Supergroup { .. }) && facts.can_get_statistics {
        out.push(HeaderEntry::Statistics);
    }
    if facts.auto_delete_ready {
        out.push(HeaderEntry::AutoDelete);
    }
    if facts.gifts_ok {
        out.push(HeaderEntry::ReceivedGifts);
    }
    out.push(HeaderEntry::Gift);
    if facts.new_window {
        out.push(HeaderEntry::NewWindow);
    }
    if matches!(facts.kind, ChatKind::Secret { .. }) {
        out.push(HeaderEntry::CloseSecretChat);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        HeaderEntry as E, HeaderFacts, can_create_poll, can_set_wallpaper, header_entries,
    };
    use crate::ids::UserId;
    use crate::telegram::envelope::ChatKind;

    fn facts(kind: &ChatKind) -> HeaderFacts<'_> {
        HeaderFacts {
            kind,
            is_saved_messages: false,
            is_self: false,
            is_bot: false,
            live: true,
            can_post: true,
            polls_allowed: true,
            can_be_reported: false,
            can_get_statistics: false,
            exportable: false,
            has_view_toggle: false,
            can_create_topic: false,
            has_discussion: false,
            auto_delete_ready: false,
            gifts_ok: false,
            new_window: false,
        }
    }

    fn user() -> ChatKind {
        ChatKind::Private { user_id: UserId(5) }
    }

    fn supergroup(is_channel: bool) -> ChatKind {
        ChatKind::Supergroup {
            supergroup_id: 9,
            is_channel,
        }
    }

    #[test]
    fn private_chat_gets_wallpaper_but_no_poll_or_boost() {
        let kind = user();
        let entries = header_entries(&facts(&kind));
        assert!(entries.contains(&E::Wallpaper));
        assert!(entries.contains(&E::ViewProfile("View profile")));
        assert!(!entries.contains(&E::CreatePoll));
        assert!(!entries.iter().any(|e| matches!(e, E::Boost(_))));
    }

    #[test]
    fn bots_and_saved_messages_can_create_polls() {
        let kind = user();
        let mut f = facts(&kind);
        f.is_bot = true;
        assert!(can_create_poll(&f));
        f.is_bot = false;
        f.is_self = true;
        assert!(can_create_poll(&f));
        assert!(!can_set_wallpaper(&f));
    }

    #[test]
    fn groups_follow_the_poll_permission() {
        let kind = supergroup(false);
        let mut f = facts(&kind);
        assert!(can_create_poll(&f));
        f.polls_allowed = false;
        assert!(!can_create_poll(&f));
        f.polls_allowed = true;
        f.can_post = false;
        assert!(!can_create_poll(&f));
    }

    #[test]
    fn secret_chats_never_get_polls_or_wallpaper() {
        let kind = ChatKind::Secret {
            secret_chat_id: 3,
            user_id: UserId(5),
        };
        let f = facts(&kind);
        assert!(!can_create_poll(&f));
        assert!(!can_set_wallpaper(&f));
        assert!(header_entries(&f).contains(&E::CloseSecretChat));
    }

    #[test]
    fn boost_label_follows_the_supergroup_kind() {
        let channel = supergroup(true);
        let group = supergroup(false);
        assert!(header_entries(&facts(&channel)).contains(&E::Boost("Boost channel")));
        assert!(header_entries(&facts(&group)).contains(&E::Boost("Boost group")));
    }

    #[test]
    fn statistics_need_the_server_flag() {
        let kind = supergroup(true);
        let mut f = facts(&kind);
        assert!(!header_entries(&f).contains(&E::Statistics));
        f.can_get_statistics = true;
        assert!(header_entries(&f).contains(&E::Statistics));
        let basic = ChatKind::BasicGroup { basic_group_id: 1 };
        let mut f = facts(&basic);
        f.can_get_statistics = true;
        assert!(!header_entries(&f).contains(&E::Statistics));
    }

    #[test]
    fn order_follows_tdesktop() {
        let kind = supergroup(false);
        let mut f = facts(&kind);
        f.can_create_topic = true;
        f.has_view_toggle = true;
        f.has_discussion = true;
        f.exportable = true;
        f.can_be_reported = true;
        f.can_get_statistics = true;
        f.auto_delete_ready = true;
        f.new_window = true;
        assert_eq!(
            header_entries(&f),
            vec![
                E::Mute,
                E::NewTopic,
                E::ViewProfile("View group info"),
                E::ViewAsTopics,
                E::Boost("Boost group"),
                E::CreatePoll,
                E::ViewDiscussion,
                E::Export,
                E::Report,
                E::Archive,
                E::Folders,
                E::SharedMedia,
                E::Statistics,
                E::AutoDelete,
                E::Gift,
                E::NewWindow,
            ]
        );
    }

    #[test]
    fn report_and_shared_media_follow_their_facts() {
        let kind = user();
        let mut f = facts(&kind);
        f.live = false;
        let entries = header_entries(&f);
        assert!(!entries.contains(&E::Report));
        assert!(!entries.contains(&E::SharedMedia));
        f.can_be_reported = true;
        assert!(header_entries(&f).contains(&E::Report));
    }
}
