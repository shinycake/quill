//! Navigation keeps chat rows in the sidebar and preferences in Settings.
use super::app::QuillApp;
use super::dialogs::CreateChatKind;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;

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
    Subscriptions,
    Gift,
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
}
impl QuillApp {
    pub(super) fn navigate(
        &mut self,
        action: NavigationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
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
        let owner = cx.entity().downgrade();
        Button::new("chat-more-menu")
            .label("⋯")
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
            .dropdown_menu(move |menu, _, _| {
                let owner = owner.clone();
                menu.item(PopupMenuItem::new("Send collectible gift").on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.navigate(NavigationAction::Gift, window, cx)
                        });
                    },
                ))
            })
            .into_any_element()
    }
    pub(super) fn main_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        Button::new("main-menu")
            .label("☰")
            .ghost()
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
            .title("Settings")
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
