//! Navigation keeps chat rows in the sidebar and preferences in Settings.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::CreateChatKind;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
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
    Proxy,
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
    ChatSettings,
    ContactsSettings,
    CallSettings,
    AdvancedSettings,
    /// Settings → Language (`parity:appearance-localization`).
    Language,
    MarkRead,
    Settings,
    Archive,
    ArchiveToList,
}
impl QuillApp {
    pub(super) fn navigate(
        &mut self,
        action: NavigationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            NavigationAction::EmojiStatus => self.open_emoji_status_picker(cx),
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
                self.open_create_chat_dialog(CreateChatKind::BasicGroup, window, cx);
            }
            NavigationAction::Supergroup => {
                self.open_create_chat_dialog(CreateChatKind::Supergroup, window, cx);
            }
            NavigationAction::Channel => {
                self.open_create_chat_dialog(CreateChatKind::Channel, window, cx);
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
            NavigationAction::Proxy => self.open_proxy_list(cx),
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
                    self.connection.status_note =
                        tr(keys::Q_NAVIGATION_OPEN_A_PRIVATE_CHAT_OR).into();
                }
                cx.notify();
            }
            NavigationAction::Appearance => {
                self.settings.appearance_open = true;
                cx.notify();
            }
            NavigationAction::Privacy => self.settings.page = Some("Privacy and security"),
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
            NavigationAction::Settings => self.settings.open = true,
            NavigationAction::Notifications => self.notify.notification_defaults_open = true,
            NavigationAction::Profile => self.open_edit_profile_dialog(window, cx),
            NavigationAction::ChatSettings => self.settings.page = Some("Chat settings"),
            NavigationAction::ContactsSettings => self.settings.page = Some("Contacts"),
            NavigationAction::CallSettings => self.settings.page = Some("Calls"),
            NavigationAction::AdvancedSettings => {
                self.settings.page = Some(super::advanced_ui::ADVANCED_PAGE)
            }
            NavigationAction::Language => self.open_language_box(window, cx),
            NavigationAction::MarkRead => self.mark_all_chats_as_read(false, cx),
            NavigationAction::Archive => self.open_archive_folder(cx),
            NavigationAction::ArchiveToList => self.toggle_archive_in_main_menu(cx),
        }
        cx.notify();
    }
    fn settings_link(
        &self,
        label: &'static str,
        action: NavigationAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(SharedString::from(format!("setting-{action:?}")))
            .label(label)
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.settings.open = false;
                this.settings.page = None;
                window.close_dialog(cx);
                this.navigate(action, window, cx);
            }))
            .into_any_element()
    }
    fn privacy_settings_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("privacy-rules")
                    .label(tr(keys::Q_NAVIGATION_PRIVACY_RULES_AND_BLOCKED_USERS))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.settings.open = false;
                        window.close_dialog(cx);
                        this.open_privacy(cx);
                    })),
            )
            .child(self.settings_link(
                tr(keys::SETTINGS_PASSCODE_TITLE),
                NavigationAction::Passcode,
                cx,
            ))
            .child(self.settings_link(
                tr(keys::SETTINGS_CLOUD_PASSWORD_START_TITLE),
                NavigationAction::TwoFa,
                cx,
            ))
            .child(self.settings_link(tr(keys::SETTINGS_LOGGED_IN), NavigationAction::Websites, cx))
            .child(self.settings_link(
                tr(keys::Q_NAVIGATION_ACCOUNT_DELETION_AND_INACTIVITY),
                NavigationAction::Account,
                cx,
            ))
            .into_any_element()
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
            self.session()
                .is_some_and(|s| !s.ordered_archived_chats().is_empty()),
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
    pub(super) fn build_settings_dialog(
        app: &Entity<Self>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        _: &mut App,
    ) -> Dialog {
        let app_c = app.clone();
        dialog
            .title(crate::ui::shell::dialog_title(tr(keys::MENU_SETTINGS)))
            .width(px(520.))
            .on_close(QuillShell::on_close_kind(
                app,
                shell,
                DialogKind::Settings,
                |this, _, cx| {
                    this.settings.open = false;
                    this.settings.page = None;
                    cx.notify();
                },
            ))
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, _, cx| {
                    let page = app_c.read(cx).settings.page;
                    if let Some(page) = page {
                        return content.child(app_c.update(cx, |this, cx| {
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    Button::new("settings-back")
                                        .label(tr(keys::Q_NAVIGATION_BACK_TO_SETTINGS))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.settings.page = this
                                                .settings
                                                .page
                                                .and_then(super::advanced_ui::parent_page);
                                            cx.notify();
                                        })),
                                )
                                .child(div().font_semibold().child(settings_page_title(page)))
                                .child(match page {
                                    "Chat settings" => div()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(this.media_settings_section(cx))
                                        .child(this.settings_link(
                                            tr(keys::Q_NAVIGATION_ARCHIVED_STICKERS),
                                            NavigationAction::ArchivedStickers,
                                            cx,
                                        ))
                                        .into_any_element(),
                                    "Contacts" => this.contacts_settings_section(cx),
                                    super::advanced_ui::ADVANCED_PAGE => {
                                        this.advanced_settings_page(cx)
                                    }
                                    super::advanced_ui::EXPERIMENTAL_PAGE => {
                                        this.experimental_settings_page(cx)
                                    }
                                    "Privacy and security" => this.privacy_settings_navigation(cx),
                                    super::settings_account_ui::ASK_QUESTION_PAGE => {
                                        this.ask_question_page(cx)
                                    }
                                    _ => {
                                        this.ensure_ptt_delay_slider(cx);
                                        this.call_settings_section(cx).into_any_element()
                                    }
                                })
                                .into_any_element()
                        }));
                    }
                    let mut list = div().flex().flex_col().gap_2();
                    let connection_type = app_c.update(cx, |this, _| this.connection_type_row());
                    for (label, action) in [
                        (tr(keys::SETTINGS_INFORMATION), NavigationAction::Profile),
                        (tr(keys::Q_NAVIGATION_ACCOUNTS), NavigationAction::Accounts),
                        (
                            tr(keys::SETTINGS_SECTION_NOTIFY),
                            NavigationAction::Notifications,
                        ),
                        (
                            tr(keys::Q_NAVIGATION_APPEARANCE),
                            NavigationAction::Appearance,
                        ),
                        (
                            tr(keys::SETTINGS_SECTION_CHAT_SETTINGS),
                            NavigationAction::ChatSettings,
                        ),
                        (
                            tr(keys::SETTINGS_SECTION_PRIVACY),
                            NavigationAction::Privacy,
                        ),
                        (tr(keys::Q_NAVIGATION_DEVICES), NavigationAction::Sessions),
                        (tr(keys::SETTINGS_DATA_STORAGE), NavigationAction::Storage),
                        (connection_type.as_str(), NavigationAction::Proxy),
                        (tr(keys::Q_NAVIGATION_STARS), NavigationAction::Stars),
                        (tr(keys::GIFT_STARS_TABS_MY), NavigationAction::MyGifts),
                        (tr(keys::PREMIUM_SUMMARY_TITLE), NavigationAction::Premium),
                        (tr(keys::BUSINESS_TITLE), NavigationAction::Business),
                        (
                            tr(keys::Q_NAVIGATION_STAR_SUBSCRIPTIONS),
                            NavigationAction::Subscriptions,
                        ),
                        (tr(keys::MENU_CONTACTS), NavigationAction::ContactsSettings),
                        (tr(keys::SETTINGS_CALLS), NavigationAction::CallSettings),
                        (
                            tr(keys::SETTINGS_ADVANCED),
                            NavigationAction::AdvancedSettings,
                        ),
                        (tr(keys::SETTINGS_LANGUAGE), NavigationAction::Language),
                    ] {
                        if matches!(action, NavigationAction::Stars) {
                            let app = app_c.clone();
                            let money = app_c.update(cx, |this, cx| {
                                this.money_settings_row(
                                    move |window, cx| {
                                        app.update(cx, |this, cx| {
                                            this.settings.open = false;
                                            window.close_dialog(cx);
                                            this.open_money(cx);
                                        })
                                    },
                                    cx,
                                )
                            });
                            list = list.children(money);
                        }
                        let app = app_c.clone();
                        list = list.child(
                            Button::new(SharedString::from(format!("settings-{action:?}")))
                                .label(label)
                                .ghost()
                                .on_click(move |_, window, cx| {
                                    app.update(cx, |this, cx| {
                                        if !matches!(
                                            action,
                                            NavigationAction::ChatSettings
                                                | NavigationAction::ContactsSettings
                                                | NavigationAction::CallSettings
                                                | NavigationAction::AdvancedSettings
                                                | NavigationAction::Privacy
                                        ) {
                                            this.settings.open = false;
                                            window.close_dialog(cx);
                                        }
                                        this.navigate(action, window, cx);
                                    });
                                }),
                        );
                    }
                    content
                        .children(app_c.update(cx, |this, cx| this.settings_phone_suggestion(cx)))
                        .child(list)
                        .child(app_c.update(cx, |this, cx| this.settings_help_footer(cx)))
                },
            ))
    }
}

/// The heading of a Settings page; pages are identified by their English
/// title so deep links and demos keep working in any language.
fn settings_page_title(page: &'static str) -> &'static str {
    let key = match page {
        "Chat settings" => keys::SETTINGS_SECTION_CHAT_SETTINGS,
        "Contacts" => keys::MENU_CONTACTS,
        "Calls" => keys::SETTINGS_CALLS,
        "Privacy and security" => keys::SETTINGS_SECTION_PRIVACY,
        super::advanced_ui::ADVANCED_PAGE => keys::SETTINGS_ADVANCED,
        super::advanced_ui::EXPERIMENTAL_PAGE => keys::SETTINGS_EXPERIMENTAL,
        super::settings_account_ui::ASK_QUESTION_PAGE => keys::SETTINGS_ASK_QUESTION,
        _ => return page,
    };
    tr(key)
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
        let width = width.clamp(
            px(quill::settings::MIN_SIDEBAR_WIDTH),
            px(quill::settings::MAX_SIDEBAR_WIDTH),
        );
        if width != self.frame.sidebar_width {
            self.frame.sidebar_width = width;
            self.schedule_window_state_save(window, cx);
            cx.notify();
        }
    }

    /// Save the window geometry shortly after the last change (moves and
    /// resizes arrive as a stream).
    pub(super) fn schedule_window_state_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                this.save_window_state(window);
            });
        })
        .detach();
    }

    fn save_window_state(&self, window: &Window) {
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, false),
            WindowBounds::Maximized(bounds) => (bounds, true),
            // Fullscreen is not restored; keep the windowed geometry.
            WindowBounds::Fullscreen(bounds) => (bounds, false),
        };
        let state = quill::settings::WindowState {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            maximized,
            sidebar_width: f32::from(self.frame.sidebar_width),
        };
        // Demo windows run on an isolated app root; nothing to protect.
        if let Some(state) = state.sanitized() {
            let _ = quill::settings::save_window_state(&state);
        }
    }
}

crate::ui::shell::register_dialogs! {
    Settings => DialogSpec::new(
        7300,
        |app| app.settings.open,
        QuillApp::build_settings_dialog,
    ),
}
