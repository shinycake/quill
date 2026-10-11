//! The open chat's header menu (the vertical ellipsis). Which entries show,
//! and in which order, is decided in `quill::chat_header_menu`; this maps each
//! entry to its label and the existing flow it opens.

use super::app::QuillApp;
use super::navigation::NavigationAction;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::*;
use quill::chat_header_menu::{HeaderEntry, HeaderFacts, header_entries};
use quill::ids::ChatId;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    pub(super) fn chat_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let chat = session.and_then(|s| s.open_chat.and_then(|id| s.chats.get(&id.0)));
        let muted = chat.is_some_and(|c| c.is_muted());
        let archived = chat.is_some_and(|c| c.in_archive);
        let live = self.live.is_some();
        let owner = cx.entity().downgrade();
        let chat_id = chat.map(|c| c.id);
        let is_secret = chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
        let ttl_ready = chat.is_some_and(|c| {
            if is_secret {
                c.can_post()
            } else {
                // tdesktop's "Auto-Delete" item: private chats, basic groups
                // and supergroups/channels where the user can change info.
                quill::auto_delete::can_edit_regular_ttl(quill::auto_delete::TtlFacts {
                    kind: &c.kind,
                    can_change_info: c.can_change_info(),
                }) && c.supported()
            }
        });
        let discussion = chat_id.and_then(|id| session?.discussion_chat_id(id));
        // tdesktop hides "Export chat history" for chats with protected content.
        let exportable = live
            && chat_id.is_some_and(|id| session.is_some_and(|s| !s.chat_has_protected_content(id)));
        // "View as Topics" / "View as Messages" for a forum without tabs
        // (tdesktop `addViewAsTopics`), and "View as Chats" / "View as
        // Messages" for Saved Messages.
        let view_toggle: Option<&'static str> = chat_id.and_then(|id| {
            let session = session?;
            let as_topics = session.chat_views_as_topics(id);
            if session.is_saved_messages(id) {
                return Some(if as_topics {
                    "View as Messages"
                } else {
                    "View as Chats"
                });
            }
            (chat?.is_forum_chat()
                && !session.subsection_tabs_used_for(id)
                && session.open_topic.is_none())
            .then_some(if as_topics {
                "View as Messages"
            } else {
                "View as Topics"
            })
        });
        let supergroup_stats = chat.is_some_and(|c| match c.kind {
            ChatKind::Supergroup { supergroup_id, .. } => session
                .and_then(|s| s.supergroup_full_info(supergroup_id))
                .is_some_and(|info| info.can_get_statistics),
            _ => false,
        });
        let private_user = chat_id.and_then(|id| session?.private_chat_user_id(id));
        let entries = chat
            .map(|c| {
                let is_saved = session.is_some_and(|s| s.is_saved_messages(c.id));
                header_entries(&HeaderFacts {
                    kind: &c.kind,
                    is_saved_messages: is_saved,
                    is_self: private_user.is_some()
                        && private_user == session.and_then(|s| s.my_user_id),
                    is_bot: private_user.is_some_and(|u| session.is_some_and(|s| s.is_bot_user(u))),
                    live,
                    can_post: c.can_post(),
                    polls_allowed: quill::poll::chat_allows_polls(c.permissions.as_ref()),
                    can_be_reported: c.can_be_reported && live,
                    can_get_statistics: supergroup_stats,
                    exportable,
                    has_view_toggle: view_toggle.is_some(),
                    can_create_topic: live
                        && c.is_forum_chat()
                        && session.is_some_and(|s| s.chat_can_manage_topics(c.id)),
                    has_discussion: discussion.is_some(),
                    auto_delete_ready: ttl_ready,
                    gifts_ok: live
                        && (matches!(c.kind, ChatKind::Private { user_id } if user_id.0 > 0)
                            || matches!(
                                c.kind,
                                ChatKind::Supergroup {
                                    is_channel: true,
                                    ..
                                }
                            )),
                    new_window: self.can_open_chat_window(c),
                })
            })
            .unwrap_or_default();
        Button::new("chat-more-menu")
            .icon(gpui_kit::assets::IconName::EllipsisVertical)
            .ghost()
            .accessibility_label("Chat actions")
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                for entry in &entries {
                    let (label, action) =
                        entry_label(*entry, muted, archived, view_toggle, is_secret);
                    let owner = owner.clone();
                    let entry = *entry;
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| match action {
                            Some(action) => this.navigate(action, window, cx),
                            None => this.run_header_entry(entry, discussion, window, cx),
                        });
                    }));
                }
                menu
            })
            .into_any_element()
    }

    /// The entries that are not a plain `NavigationAction`.
    fn run_header_entry(
        &mut self,
        entry: HeaderEntry,
        discussion: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        match entry {
            HeaderEntry::ViewProfile(_) => {
                if let Some(target) = self
                    .session()
                    .and_then(|s| s.info_panel_target_for_chat(chat_id))
                {
                    self.open_info_panel_target(target, window, cx);
                }
            }
            HeaderEntry::Boost(_) => self.open_boost_panel(chat_id, window, cx),
            HeaderEntry::Statistics => {
                self.open_info_panel_target(InfoPanelTarget::Statistics(chat_id.0), window, cx);
            }
            HeaderEntry::CreatePoll => self.open_poll_dialog(window, cx),
            HeaderEntry::Wallpaper => self.open_chat_look_dialog(chat_id.0, cx),
            HeaderEntry::Report => self.open_message_report(chat_id, Vec::new(), cx),
            HeaderEntry::ViewDiscussion => {
                if let Some(discussion) = discussion {
                    self.select_listed_chat(ChatId(discussion), window, cx);
                }
            }
            HeaderEntry::AutoDelete => {
                self.notify.ttl_picker_open = true;
                self.notify.ttl_custom_open = false;
                self.notify.ttl_custom_secs = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .map(|c| c.message_auto_delete_time)
                    .filter(|secs| *secs > 0)
                    .unwrap_or(86_400);
            }
            HeaderEntry::CloseSecretChat => self.open_close_secret_chat_confirm(chat_id, cx),
            _ => {}
        }
        cx.notify();
    }
}

/// The label of an entry and, when an existing `NavigationAction` handles
/// it, that action.
fn entry_label(
    entry: HeaderEntry,
    muted: bool,
    archived: bool,
    view_toggle: Option<&'static str>,
    _is_secret: bool,
) -> (&'static str, Option<NavigationAction>) {
    use NavigationAction as N;
    match entry {
        HeaderEntry::Mute => (
            if muted {
                "Unmute notifications"
            } else {
                "Mute notifications"
            },
            Some(N::ChatMute),
        ),
        HeaderEntry::NewTopic => ("New Topic", Some(N::NewTopic)),
        HeaderEntry::ViewProfile(label) => (label, None),
        HeaderEntry::ViewAsTopics => (
            view_toggle.unwrap_or("View as Topics"),
            Some(N::ViewAsTopics),
        ),
        HeaderEntry::Boost(label) => (label, None),
        HeaderEntry::CreatePoll => ("Create poll", None),
        HeaderEntry::Wallpaper => ("Set wallpaper", None),
        HeaderEntry::ViewDiscussion => ("View discussion", None),
        HeaderEntry::Export => ("Export chat history", Some(N::ExportChat)),
        HeaderEntry::Report => ("Report", None),
        HeaderEntry::Archive => (
            if archived {
                "Unarchive chat"
            } else {
                "Archive chat"
            },
            Some(N::ChatArchive),
        ),
        HeaderEntry::Folders => ("Add to folder", Some(N::ChatFolders)),
        HeaderEntry::SharedMedia => ("Shared media", Some(N::SharedMedia)),
        HeaderEntry::Statistics => ("Statistics", None),
        HeaderEntry::AutoDelete => ("Auto-Delete", None),
        HeaderEntry::ReceivedGifts => ("Received gifts", Some(N::ChatGifts)),
        HeaderEntry::Gift => ("Send collectible gift", Some(N::Gift)),
        HeaderEntry::NewWindow => ("Open in new window", Some(N::NewWindow)),
        HeaderEntry::CloseSecretChat => ("Close secret chat", None),
    }
}
