//! Navigation keeps chat rows in the sidebar and preferences in Settings.
use super::app::QuillApp;
use super::dialogs::CreateChatKind;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::envelope::ChatKind;

#[derive(Clone, Copy)]
pub(super) enum NavigationAction {
    Saved,
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
    Subscriptions,
    Gift,
    ChatMute,
    ChatArchive,
    ChatFolders,
    SharedMedia,
    ExportChat,
    NewStory,
    Appearance,
    Privacy,
    TwoFa,
    Sessions,
    Websites,
    Account,
    Accounts,
    Notifications,
    Profile,
    ChatSettings,
    ContactsSettings,
    CallSettings,
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
            NavigationAction::ChatFolders => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.open_folder_menu(chat, cx);
                }
            }
            NavigationAction::SharedMedia => self.open_shared_media_ui(cx),
            NavigationAction::ExportChat => {
                if let Some(chat) = self.session().and_then(|s| s.open_chat) {
                    self.start_chat_export(chat, cx);
                }
            }
            NavigationAction::NewStory => self.open_story_composer(window, cx),
            NavigationAction::Saved => {
                self.open_saved_messages(window, cx);
            }
            NavigationAction::Secret => {
                self.new_secret_picker_open = !self.new_secret_picker_open;
                cx.notify();
            }
            NavigationAction::Downloads => {
                if let Some(live) = self.live.as_mut() {
                    let open = &mut live.driver.session.downloads_panel_open;
                    *open = !*open;
                } else if let Some(session) = self.demo_session.as_mut() {
                    session.downloads_panel_open = !session.downloads_panel_open;
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
            NavigationAction::Subscriptions => {
                self.open_subscriptions(cx);
            }
            NavigationAction::Gift => {
                if self.session().and_then(|s| s.open_chat).is_some() {
                    let busy = self
                        .session()
                        .and_then(|s| s.marketplace_gift.as_ref())
                        .is_some_and(|g| g.loading || g.sending);
                    if !busy {
                        self.marketplace_name_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        self.marketplace_comment_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        self.marketplace_private = true;
                        self.marketplace_error = None;
                        if let Some(live) = self.live.as_mut() {
                            live.driver.session.marketplace_gift = None;
                        } else if let Some(session) = self.demo_session.as_mut() {
                            session.marketplace_gift = None;
                        }
                    }
                    self.marketplace_open = true;
                } else {
                    self.status_note =
                        "Open a private chat or channel to choose the gift recipient.".into();
                }
                cx.notify();
            }
            NavigationAction::Appearance => {
                self.appearance_open = true;
                cx.notify();
            }
            NavigationAction::Privacy => self.settings_page = Some("Privacy and security"),
            NavigationAction::TwoFa => {
                self.open_twofa(cx);
            }
            NavigationAction::Sessions => {
                self.open_sessions(cx);
            }
            NavigationAction::Websites => {
                self.open_websites(cx);
            }
            NavigationAction::Account => {
                self.open_account_lifecycle(cx);
            }
            NavigationAction::Accounts => {
                self.open_accounts(cx);
            }
            NavigationAction::Settings => self.settings_open = true,
            NavigationAction::Notifications => self.notification_defaults_open = true,
            NavigationAction::Profile => self.open_edit_profile_dialog(window, cx),
            NavigationAction::ChatSettings => self.settings_page = Some("Chat settings"),
            NavigationAction::ContactsSettings => self.settings_page = Some("Contacts"),
            NavigationAction::CallSettings => self.settings_page = Some("Calls"),
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
        Button::new(SharedString::from(format!("setting-{label}")))
            .label(label)
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.settings_open = false;
                this.settings_page = None;
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
                    .label("Privacy rules and blocked users")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.settings_open = false;
                        window.close_dialog(cx);
                        this.open_privacy(cx);
                    })),
            )
            .child(self.settings_link("Two-step verification", NavigationAction::TwoFa, cx))
            .child(self.settings_link("Connected websites", NavigationAction::Websites, cx))
            .child(self.settings_link(
                "Account deletion and inactivity",
                NavigationAction::Account,
                cx,
            ))
            .into_any_element()
    }
    pub(super) fn chat_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let chat = self
            .session()
            .and_then(|s| s.open_chat.and_then(|id| s.chats.get(&id.0)));
        let muted = chat.is_some_and(|c| c.is_muted());
        let archived = chat.is_some_and(|c| c.in_archive);
        let live = self.live.is_some();
        let owner = cx.entity().downgrade();
        // Chat-kind actions live in this menu, as in Telegram Desktop's
        // top-bar menu: a secret chat's auto-delete timer and close, and
        // a channel's discussion group.
        let chat_id = chat.map(|c| c.id);
        let is_secret = chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
        let ttl_ready = is_secret && chat.is_some_and(|c| c.can_post());
        let discussion = chat_id.and_then(|id| self.session()?.discussion_chat_id(id));
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
                for (label, action, visible) in [
                    (
                        if muted {
                            "Unmute notifications"
                        } else {
                            "Mute notifications"
                        },
                        NavigationAction::ChatMute,
                        true,
                    ),
                    (
                        if archived {
                            "Unarchive chat"
                        } else {
                            "Archive chat"
                        },
                        NavigationAction::ChatArchive,
                        true,
                    ),
                    ("Add to folder", NavigationAction::ChatFolders, true),
                    ("Shared media", NavigationAction::SharedMedia, live),
                    ("Export chat history", NavigationAction::ExportChat, live),
                    ("Send collectible gift", NavigationAction::Gift, true),
                ] {
                    if !visible {
                        continue;
                    }
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.navigate(action, window, cx));
                    }));
                }
                if let Some(discussion) = discussion {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new("View discussion").on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.select_listed_chat(ChatId(discussion), window, cx);
                            });
                        },
                    ));
                }
                if ttl_ready {
                    let owner = owner.clone();
                    menu =
                        menu.item(PopupMenuItem::new("Auto-Delete").on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.ttl_picker_open = true;
                                cx.notify();
                            });
                        }));
                }
                if let Some(chat_id) = chat_id.filter(|_| is_secret) {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new("Close secret chat").on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.open_close_secret_chat_confirm(chat_id, cx);
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }
    pub(super) fn main_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        // tdesktop `archiveInMainMenu`: the archive lives here instead of
        // on top of the chat list while there is something archived.
        let archive_in_menu = quill::chatlist_archive::show_in_main_menu(
            self.session()
                .is_some_and(|s| !s.ordered_archived_chats().is_empty()),
            self.appearance.archive_in_main_menu,
        );
        Button::new("main-menu")
            .icon(gpui_kit::assets::IconName::Menu)
            .ghost()
            .tooltip("Menu")
            .accessibility_label("Main menu")
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
                    ("Saved Messages", NavigationAction::Saved),
                    ("Archived chats", NavigationAction::Archive),
                    ("Move archive to chat list", NavigationAction::ArchiveToList),
                    ("New story", NavigationAction::NewStory),
                    ("New group", NavigationAction::Group),
                    ("New supergroup", NavigationAction::Supergroup),
                    ("New channel", NavigationAction::Channel),
                    ("New secret chat", NavigationAction::Secret),
                    ("Communities", NavigationAction::Communities),
                    ("New community", NavigationAction::CommunityCreate),
                    ("Downloads", NavigationAction::Downloads),
                    ("Mark all as read", NavigationAction::MarkRead),
                    ("Settings", NavigationAction::Settings),
                ] {
                    if matches!(
                        action,
                        NavigationAction::Archive | NavigationAction::ArchiveToList
                    ) && !archive_in_menu
                    {
                        continue;
                    }
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.navigate(action, window, cx));
                    }));
                }
                menu
            })
            .into_any_element()
    }
    pub(super) fn build_settings_dialog(
        app: &Entity<Self>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        _: &mut App,
    ) -> Dialog {
        let app_c = app.clone();
        dialog
            .title(crate::ui::shell::dialog_title("Settings"))
            .width(px(520.))
            .on_close(QuillShell::on_close_kind(
                app,
                shell,
                DialogKind::Settings,
                |this, _, cx| {
                    this.settings_open = false;
                    this.settings_page = None;
                    cx.notify();
                },
            ))
            .content(crate::ui::shell::scrollable_dialog_content(
                move |content, _, cx| {
                    let page = app_c.read(cx).settings_page;
                    if let Some(page) = page {
                        return content.child(app_c.update(cx, |this, cx| {
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    Button::new("settings-back")
                                        .label("Back to Settings")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.settings_page = None;
                                            cx.notify();
                                        })),
                                )
                                .child(div().font_semibold().child(page))
                                .child(match page {
                                    "Chat settings" => div()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(this.media_settings_section(cx))
                                        .child(this.settings_link(
                                            "Archived stickers",
                                            NavigationAction::ArchivedStickers,
                                            cx,
                                        ))
                                        .into_any_element(),
                                    "Contacts" => this.contacts_settings_section(cx),
                                    "Privacy and security" => this.privacy_settings_navigation(cx),
                                    _ => this.call_settings_section(cx).into_any_element(),
                                })
                                .into_any_element()
                        }));
                    }
                    let mut list = div().flex().flex_col().gap_2();
                    for (label, action) in [
                        ("Edit profile", NavigationAction::Profile),
                        ("Accounts", NavigationAction::Accounts),
                        ("Notifications and sounds", NavigationAction::Notifications),
                        ("Appearance", NavigationAction::Appearance),
                        ("Chat settings", NavigationAction::ChatSettings),
                        ("Privacy and security", NavigationAction::Privacy),
                        ("Devices", NavigationAction::Sessions),
                        ("Data and storage", NavigationAction::Storage),
                        ("Proxy", NavigationAction::Proxy),
                        ("Star subscriptions", NavigationAction::Subscriptions),
                        ("Contacts", NavigationAction::ContactsSettings),
                        ("Calls", NavigationAction::CallSettings),
                    ] {
                        let app = app_c.clone();
                        list = list.child(
                            Button::new(SharedString::from(format!("settings-{label}")))
                                .label(label)
                                .ghost()
                                .on_click(move |_, window, cx| {
                                    app.update(cx, |this, cx| {
                                        if !matches!(
                                            action,
                                            NavigationAction::ChatSettings
                                                | NavigationAction::ContactsSettings
                                                | NavigationAction::CallSettings
                                                | NavigationAction::Privacy
                                        ) {
                                            this.settings_open = false;
                                            window.close_dialog(cx);
                                        }
                                        this.navigate(action, window, cx);
                                    });
                                }),
                        );
                    }
                    content.child(list)
                },
            ))
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
        if width != self.sidebar_width {
            self.sidebar_width = width;
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
        if self.window_state_save_pending {
            return;
        }
        self.window_state_save_pending = true;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(600))
                .await;
            let _ = this.update_in(cx, |this, window, _| {
                this.window_state_save_pending = false;
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
            sidebar_width: f32::from(self.sidebar_width),
        };
        // Demo windows run on an isolated app root; nothing to protect.
        if let Some(state) = state.sanitized() {
            let _ = quill::settings::save_window_state(&state);
        }
    }
}
