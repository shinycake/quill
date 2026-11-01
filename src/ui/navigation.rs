//! Navigation keeps chat rows in the sidebar and preferences in Settings.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::ui_driver::guard::{self, HardSurface};
use quill::lang::{keys, tr};
use quill::new_chat::NewChatKind;
use quill::settings_catalog::Page;
use quill::telegram::envelope::ChatKind;

#[derive(Clone, Copy, Debug)]
pub(super) enum NavigationAction {
    /// `parity:platform-new-window`: the open chat in a window of its own.
    NewWindow,
    Saved,
    MyProfile,
    MyStories,
    EmojiStatus,
    MyGroups,
    MyChannels,
    Contacts,
    Calls,
    NightMode,
    Secret,
    Downloads,
    Group,
    Supergroup,
    Channel,
    CommunityCreate,
    Communities,
    ArchivedStickers,
    Storage,
    Business,
    Subscriptions,
    Stars,
    Premium,
    MyGifts,
    ChatGifts,
    Gift,
    /// The send-gift dialog for the open chat's user or channel.
    SendGift,
    ChatMute,
    ChatArchive,
    ChatFolders,
    SharedMedia,
    ExportChat,
    ViewAsTopics,
    NewTopic,
    NewStory,
    Appearance,
    Privacy,
    TwoFa,
    Sessions,
    Websites,
    Passcode,
    Account,
    Accounts,
    Notifications,
    Profile,
    ContactsSettings,
    CallSettings,
    /// Settings → Language (`parity:appearance-localization`).
    Language,
    MarkRead,
    Settings,
    Archive,
    ArchiveToList,
}
impl NavigationAction {
    /// Settings boxes open over the settings window; everything else
    /// (a chat, a panel, a tab) needs the chats back on screen.
    fn stays_in_settings(self) -> bool {
        matches!(
            self,
            Self::ArchivedStickers
                | Self::Storage
                | Self::Business
                | Self::Subscriptions
                | Self::Stars
                | Self::Premium
                | Self::MyGifts
                | Self::NightMode
                | Self::Appearance
                | Self::Privacy
                | Self::TwoFa
                | Self::Sessions
                | Self::Websites
                | Self::Passcode
                | Self::Account
                | Self::Accounts
                | Self::Notifications
                | Self::Profile
                | Self::ContactsSettings
                | Self::CallSettings
                | Self::Language
                | Self::Settings
                | Self::ArchiveToList
        )
    }

    /// The surface agents never use that the action opens: every settings
    /// box, and the payment ones a chat opens.
    fn hard_surface(self) -> Option<HardSurface> {
        match self {
            Self::Stars
            | Self::Premium
            | Self::Subscriptions
            | Self::MyGifts
            | Self::Gift
            | Self::SendGift => Some(HardSurface::Payments),
            Self::Passcode => Some(HardSurface::Passcode),
            Self::TwoFa | Self::Websites => Some(HardSurface::Security),
            Self::Account | Self::Accounts => Some(HardSurface::Account),
            // Toggles that write a setting without opening Settings.
            Self::NightMode | Self::ArchiveToList => Some(HardSurface::Setting),
            other => other.stays_in_settings().then_some(HardSurface::Settings),
        }
    }
}

impl QuillApp {
    pub(super) fn navigate(
        &mut self,
        action: NavigationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action
            .hard_surface()
            .is_some_and(guard::refuse_hard_if_injecting)
        {
            return;
        }
        if !action.stays_in_settings() {
            self.close_settings();
        }
        match action {
            NavigationAction::ChatMute => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat)
                    && self
                        .session()
                        .and_then(|s| s.chats.get(&chat.0))
                        .is_some_and(|c| c.is_muted())
                {
                    self.apply_chat_mute(chat, 0, cx);
                } else {
                    self.open_mute_menu(cx);
                }
            }
            NavigationAction::ChatArchive => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.toggle_archive(chat, cx);
                }
            }
            NavigationAction::NewWindow => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.open_chat_in_new_window(chat, window, cx);
                }
            }
            NavigationAction::ChatFolders => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.open_folder_menu(chat, cx);
                }
            }
            NavigationAction::SharedMedia => self.open_shared_media_ui(cx),
            NavigationAction::ViewAsTopics => {
                if let Some((chat_id, topics)) = self.session().and_then(|s| {
                    let id = s.open_chat?;
                    Some((id, s.chat_views_as_topics(id)))
                }) {
                    self.set_chat_view_as_topics_ui(chat_id, !topics, cx);
                }
            }
            NavigationAction::NewTopic => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.open_forum_topic_editor(chat, None, window, cx);
                }
            }
            NavigationAction::ExportChat => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.start_chat_export(chat, cx);
                }
            }
            NavigationAction::NewStory => self.open_story_composer(window, cx),
            NavigationAction::Saved => {
                self.open_saved_messages(window, cx);
            }
            NavigationAction::MyProfile => {
                if let Some(me) = self.session().and_then(|s| s.my_user_id) {
                    self.open_user_panel(me, window, cx);
                }
            }
            NavigationAction::MyStories => self.open_my_stories(window, cx),
            NavigationAction::EmojiStatus => self.open_emoji_status_picker(window, cx),
            NavigationAction::MyGroups => self.open_my_chats(false, cx),
            NavigationAction::MyChannels => self.open_my_chats(true, cx),
            NavigationAction::Contacts => self.open_contacts_tab(cx),
            NavigationAction::Calls => self.open_calls_tab(cx),
            NavigationAction::NightMode => {
                let dark = self.night_mode_on();
                let (theme, auto_night) = quill::main_menu::night_mode_toggle(dark);
                self.set_appearance(cx, |a| {
                    a.theme = theme;
                    a.auto_night = auto_night;
                });
            }
            NavigationAction::Secret => {
                self.share.new_secret_picker_open = !self.share.new_secret_picker_open;
                cx.notify();
            }
            NavigationAction::Downloads => {
                if let Some(live) = self.live.as_mut() {
                    let open = &mut live.driver.session.media.downloads_panel_open;
                    *open = !*open;
                } else if let Some(session) = self.demo_session.as_mut() {
                    session.media.downloads_panel_open = !session.media.downloads_panel_open;
                }
                cx.notify();
            }
            NavigationAction::Group => {
                self.open_new_chat(NewChatKind::Group, window, cx);
            }
            NavigationAction::Supergroup => {
                self.open_new_chat(NewChatKind::Megagroup, window, cx);
            }
            NavigationAction::Channel => {
                self.open_new_chat(NewChatKind::Channel, window, cx);
            }
            NavigationAction::CommunityCreate => {
                self.open_create_community_dialog(window, cx);
            }
            NavigationAction::Communities => {
                self.open_community_hub(cx);
            }
            NavigationAction::ArchivedStickers => {
                self.open_archived_stickers(cx);
            }
            NavigationAction::Storage => {
                self.open_data_storage(cx);
            }
            NavigationAction::Business => self.open_business(window, cx),
            NavigationAction::Subscriptions => {
                self.open_subscriptions(cx);
            }
            NavigationAction::Stars => self.open_stars(cx),
            NavigationAction::Premium => self.open_premium(cx),
            NavigationAction::MyGifts => self.open_my_gifts(cx),
            NavigationAction::ChatGifts => {
                let owner = self.session().and_then(|s| {
                    let chat = s.chats.get(&s.open_chat?.0)?;
                    match chat.kind {
                        ChatKind::Private { user_id } if user_id.0 > 0 => {
                            Some(quill::telegram::envelope::MessageSender::User {
                                user_id: user_id.0,
                            })
                        }
                        ChatKind::Supergroup {
                            is_channel: true, ..
                        } => Some(quill::telegram::envelope::MessageSender::Chat {
                            chat_id: chat.id.0,
                        }),
                        _ => None,
                    }
                });
                if let Some(owner) = owner {
                    self.open_gifts(owner, cx);
                }
            }
            NavigationAction::SendGift => self.open_send_gift_dialog(window, cx),
            NavigationAction::Gift => {
                if self.session().and_then(|s| s.open_chat).is_some() {
                    let busy = self
                        .session()
                        .and_then(|s| s.payments.marketplace_gift.as_ref())
                        .is_some_and(|g| g.loading || g.sending);
                    if !busy {
                        self.payments
                            .marketplace_name_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        self.payments
                            .marketplace_comment_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        self.payments.marketplace_private = true;
                        self.payments.marketplace_error = None;
                        if let Some(live) = self.live.as_mut() {
                            live.driver.session.payments.marketplace_gift = None;
                        } else if let Some(session) = self.demo_session.as_mut() {
                            session.payments.marketplace_gift = None;
                        }
                    }
                    self.payments.marketplace_open = true;
                } else {
                    self.connection.set_note(StatusNote::silent(tr(
                        keys::Q_NAVIGATION_OPEN_A_PRIVATE_CHAT_OR,
                    )));
                }
                cx.notify();
            }
            NavigationAction::Appearance => {
                self.open_settings_on(Page::Appearance);
                cx.notify();
            }
            NavigationAction::Privacy => {
                self.open_settings(None);
                self.show_settings_page(Page::Privacy, cx);
            }
            NavigationAction::TwoFa => {
                self.open_twofa(cx);
            }
            NavigationAction::Sessions => {
                self.open_sessions(cx);
            }
            NavigationAction::Passcode => self.open_passcode(cx),
            NavigationAction::Websites => {
                self.open_websites(cx);
            }
            NavigationAction::Account => {
                self.open_account_lifecycle(cx);
            }
            NavigationAction::Accounts => {
                self.open_accounts(cx);
            }
            NavigationAction::Settings => self.open_settings(None),
            NavigationAction::Notifications => self.open_notification_settings(cx),
            NavigationAction::Profile => self.open_edit_profile_dialog(window, cx),
            NavigationAction::ContactsSettings => self.open_settings(Some(Page::Account)),
            NavigationAction::CallSettings => self.open_settings(Some(Page::Calls)),
            NavigationAction::Language => self.open_language_box(window, cx),
            NavigationAction::MarkRead => self.mark_all_chats_as_read(false, cx),
            NavigationAction::Archive => self.open_archive_folder(cx),
            NavigationAction::ArchiveToList => self.toggle_archive_in_main_menu(cx),
        }
        cx.notify();
    }
    /// Whether a dark theme is in effect (the Night Mode switch).
    pub(super) fn night_mode_on(&self) -> bool {
        quill::main_menu::night_mode_on(
            self.settings
                .appearance_applied
                .as_ref()
                .map(|(mode, ..)| *mode == gpui_kit::component::theme::ThemeMode::Dark),
            self.appearance.theme,
        )
    }

    pub(super) fn main_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        let night = self.night_mode_on();
        let status_label = self.emoji_status_menu_label();
        // tdesktop `archiveInMainMenu`: the archive lives here instead of
        // on top of the chat list while there is something archived.
        let archive_in_menu = quill::chatlist_archive::show_in_main_menu(
            self.session().is_some_and(|s| s.has_archived_chats()),
            self.appearance.archive_in_main_menu,
        );
        let accounts_unread = self.other_accounts_unread();
        let button = Button::new("main-menu")
            .icon(gpui_kit::assets::IconName::Menu)
            .ghost()
            .tooltip(tr(keys::MAIN_MENU))
            .accessibility_label(tr(keys::MAIN_MENU))
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                for (label, action) in [
                    (tr(keys::MENU_MY_PROFILE), NavigationAction::MyProfile),
                    (tr(keys::MENU_MY_STORIES), NavigationAction::MyStories),
                    (status_label, NavigationAction::EmojiStatus),
                    (tr(keys::MENU_MY_GROUPS), NavigationAction::MyGroups),
                    (tr(keys::MENU_MY_CHANNELS), NavigationAction::MyChannels),
                    (tr(keys::MENU_CONTACTS), NavigationAction::Contacts),
                    (tr(keys::MENU_CALLS), NavigationAction::Calls),
                    (tr(keys::SAVED_MESSAGES), NavigationAction::Saved),
                    (tr(keys::ARCHIVED_NAME), NavigationAction::Archive),
                    (
                        tr(keys::CONTEXT_ARCHIVE_TO_LIST),
                        NavigationAction::ArchiveToList,
                    ),
                    (tr(keys::Q_NAVIGATION_NEW_STORY), NavigationAction::NewStory),
                    (tr(keys::CREATE_GROUP_TITLE), NavigationAction::Group),
                    (
                        tr(keys::Q_NAVIGATION_NEW_SUPERGROUP),
                        NavigationAction::Supergroup,
                    ),
                    (tr(keys::CREATE_CHANNEL_TITLE), NavigationAction::Channel),
                    (
                        tr(keys::Q_NAVIGATION_NEW_SECRET_CHAT),
                        NavigationAction::Secret,
                    ),
                    (
                        tr(keys::Q_NAVIGATION_COMMUNITIES),
                        NavigationAction::Communities,
                    ),
                    (
                        tr(keys::COMMUNITY_CREATE_TITLE),
                        NavigationAction::CommunityCreate,
                    ),
                    (tr(keys::DOWNLOADS_SECTION), NavigationAction::Downloads),
                    (tr(keys::CONTEXT_MARK_READ_ALL), NavigationAction::MarkRead),
                    (tr(keys::Q_NAVIGATION_ACCOUNTS), NavigationAction::Accounts),
                    (tr(keys::MENU_SETTINGS), NavigationAction::Settings),
                    (tr(keys::MENU_NIGHT_MODE), NavigationAction::NightMode),
                ] {
                    if matches!(
                        action,
                        NavigationAction::Archive | NavigationAction::ArchiveToList
                    ) && !archive_in_menu
                    {
                        continue;
                    }
                    let owner = owner.clone();
                    let mut item = PopupMenuItem::new(label);
                    if matches!(action, NavigationAction::NightMode) {
                        item = item.checked(night);
                    }
                    // The other accounts' unread counter (tdesktop's
                    // accounts toggle badge).
                    if let (NavigationAction::Accounts, Some(unread)) = (action, accounts_unread) {
                        item = PopupMenuItem::element(move |_, cx| {
                            super::background_accounts::chrome::accounts_menu_row(unread, cx)
                        });
                    }
                    menu = menu.item(item.on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.navigate(action, window, cx));
                    }));
                }
                menu
            })
            .into_any_element();
        self.with_other_accounts_dot(button)
    }
}

/// Drag payload for the chat list's resize edge.
#[derive(Clone, Copy)]
pub(super) struct SidebarResize;

impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // The drag has no preview; the column itself follows the pointer.
        div()
    }
}

impl QuillApp {
    /// The chat list's right edge: drag to resize, double-click to reset.
    pub(super) fn sidebar_resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sidebar-resize-handle")
            .relative()
            .w_0()
            .h_full()
            .flex_none()
            .child(
                div()
                    .id("sidebar-resize-hit")
                    .debug_selector(|| "sidebar-resize-hit".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-3.))
                    .w(px(6.))
                    .cursor_col_resize()
                    .hover(|style| style.bg(cx.theme().border))
                    .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| SidebarResize))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            this.set_sidebar_width(
                                px(quill::settings::DEFAULT_SIDEBAR_WIDTH),
                                window,
                                cx,
                            );
                        }
                    })),
            )
    }

    pub(super) fn set_sidebar_width(
        &mut self,
        width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The conversation keeps its minimum beside the list.
        let max = self
            .max_list_width()
            .min(px(quill::settings::MAX_SIDEBAR_WIDTH));
        let width = width.min(max).max(px(quill::settings::MIN_SIDEBAR_WIDTH));
        if width != self.frame.sidebar_width {
            self.frame.sidebar_width = width;
            self.schedule_window_state_save(window, cx);
            cx.notify();
        }
    }

    /// Save the window geometry shortly after the last change (moves and
    /// resizes arrive as a stream). The newest state is staged at once; the
    /// write itself runs on a background thread, and quitting flushes it
    /// ([`flush_settings_on_quit`]).
    pub(super) fn schedule_window_state_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stage_window_state(window);
        if self.frame.window_state_save_pending {
            return;
        }
        self.frame.window_state_save_pending = true;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(600))
                .await;
            let _ = this.update_in(cx, |this, window, _| {
                this.frame.window_state_save_pending = false;
                this.stage_window_state(window);
                quill::settings::save_staged_window_state();
            });
        })
        .detach();
    }

    fn stage_window_state(&self, window: &Window) {
        // Demo windows run on an isolated app root; nothing to protect.
        if let Some(state) = self.window_state(window).sanitized() {
            quill::settings::stage_window_state(state);
        }
    }

    /// What `window_state.json` gets for this window now.
    pub(super) fn window_state(&self, window: &Window) -> quill::settings::WindowState {
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, false),
            WindowBounds::Maximized(bounds) => (bounds, true),
            // Fullscreen is not restored; keep the windowed geometry.
            WindowBounds::Fullscreen(bounds) => (bounds, false),
        };
        quill::settings::WindowState {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            maximized,
            sidebar_width: f32::from(self.frame.sidebar_width),
            sidebar_hidden: self.frame.sidebar.hidden(),
        }
    }
}

/// Quitting writes the staged window state and every setting still queued
/// for the settings writer. Called once at startup, before any setting can
/// be saved.
pub(crate) fn flush_settings_on_quit(cx: &mut App) {
    after_windows_on_quit(cx, || {
        quill::settings::flush_window_state();
        quill::settings::flush_deferred_writes();
    });
}

/// Runs `work` at quit once GPUI has dropped every window, so a write
/// that stalls on a hung disk leaves no frozen window on screen.
fn after_windows_on_quit(cx: &mut App, work: impl FnOnce() + 'static) {
    let mut work = Some(work);
    cx.on_app_quit(move |_| {
        let work = work.take();
        async move { work.into_iter().for_each(|work| work()) }
    })
    .detach();
}

#[cfg(test)]
#[path = "navigation_quit_tests.rs"]
mod quit_tests;
