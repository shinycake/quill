//! QuillShell, DialogKind/DialogBuilder, title bar, kit-dialog sync.

use super::app::{PaneMode, QuillApp};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
pub(super) fn title_bar(
    mode: PaneMode,
    live: bool,
    search_open: bool,
    chat_search_open: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let title = match mode {
        PaneMode::Synthetic => "Quill — synthetic chat",
        PaneMode::Connecting if live => "Quill — live TDLib",
        PaneMode::Connecting => "Quill — connecting",
        PaneMode::Ready if live => "Quill — chats",
        PaneMode::Ready => "Quill — chats (demo)",
    };
    let show_cycle = mode == PaneMode::Synthetic;
    // kit Phase 7: kit `TitleBar` — native-feel chrome (drag, double-click
    // zoom, Linux min/max/close, macOS traffic-light inset) in theme tokens,
    // replacing the hand-rolled 44px bar. The caption + action buttons ride
    // as its children.
    TitleBar::new()
        .on_close_window(|_, window, cx| {
            window.remove_window();
            cx.quit();
        })
        .child(div().font_semibold().text_sm().child(title))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .flex_none()
                .child(
                    Button::new("older")
                        .icon(IconName::ChevronsUp)
                        .ghost()
                        .tooltip("Load older messages")
                        .accessibility_label("Load older messages")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.load_older_action(cx);
                        })),
                )
                .when(mode == PaneMode::Ready, |this| {
                    this.child(
                        Button::new("search")
                            .icon(if search_open {
                                IconName::X
                            } else {
                                IconName::Search
                            })
                            .ghost()
                            .tooltip(if search_open {
                                "Close search"
                            } else {
                                "Search"
                            })
                            .accessibility_label(if search_open {
                                "Close search"
                            } else {
                                "Search"
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.search_is_open() {
                                    this.close_search_ui(window, cx);
                                } else {
                                    this.open_search_ui(window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("find-in-chat")
                            .icon(if chat_search_open {
                                IconName::X
                            } else {
                                IconName::TextSearch
                            })
                            .ghost()
                            .tooltip(if chat_search_open {
                                "Close find in chat"
                            } else {
                                "Find in chat"
                            })
                            .accessibility_label(if chat_search_open {
                                "Close find in chat"
                            } else {
                                "Find in chat"
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.chat_search_is_open() {
                                    this.close_chat_search_ui(window, cx);
                                } else {
                                    this.open_chat_search_ui(window, cx);
                                }
                            })),
                    )
                })
                .when(show_cycle, |this| {
                    this.child(
                        Button::new("cycle-auth")
                            .icon(IconName::RotateCcw)
                            .ghost()
                            .tooltip("Cycle auth state")
                            .accessibility_label("Cycle auth state")
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                    )
                }),
        )
}

// kit Phase 2 (redo): dialog/notification shell.
// Thin view between the kit [`Root`] and [`QuillApp`] that mounts the
// kit-managed dialog and notification layers.
//
// Why a shell: `window.open_dialog` builders read live [`QuillApp`]
// state through `Entity<QuillApp>`, which panics while `QuillApp` is
// leased — and `QuillApp` is leased for the whole of its own `render`.
// Building the layers here keeps every builder outside that lease.
// Re-render chaining needs no manual observe: `cx.notify()` on the app
// marks ancestor views (this shell) dirty, so the layers rebuild on
// every app notify and dialog content stays live.

/// kit Phase 2 (redo): every dialog migrated to `window.open_dialog`.
/// The shell's sync opens/closes the kit dialog to match the app-side
/// open flag; the flag remains the single source of truth so trigger
/// sites are untouched.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DialogKind {
    Scheduled,
    PaymentForm,
    PaymentReceipt,
    // Slice `parity:bots-payment-recurring`: the `starSubscriptions`
    // management dialog.
    Subscriptions,
    CallbackPassword,
    LoginUrlConfirm,
    /// `parity:platform-deep-links`: TDLib's deep-link info / error text.
    DeepLinkInfo,
    DeepLinkInvite,
    PollVoters,
    ArchiveSettings,
    ArchivedStickers,
    EmojiSets,
    ImportContacts,
    AddContact,
    EditProfile,
    GroupCallStart,
    FolderEditor,
    FolderDelete,
    FolderManage,
    CallConfirm,
    /// Swap prompt: incoming call while another call is active.
    CallSwap,
    NotificationDefaults,
    StorageUsage,
    TwoFa,
    Sessions,
    Websites,
    CreateChat,
    Member,
    Permissions,
    Username,
    Restrict,
    GroupConfirm,
    QuoteReply,
    ForumManage,
    CommentThread,
    Welcome,
    Appearance,
    /// Slice A9: account lifecycle (delete account + self-destruct TTL).
    AccountLifecycle,
    /// Slice parity:auth-multi-account (UI): the Accounts dialog (list /
    /// switch / add / remove accounts).
    Accounts,
    /// Slice G10: communities create dialog.
    CommunityCreate,
    /// Slice G10: communities hub dialog.
    CommunityHub,
    /// Slice parity:platform-shortcuts-reference: read-only keyboard
    /// shortcuts reference dialog.
    Shortcuts,
}

/// Builder for one dialog kind: `(app, shell, dialog, cx) -> dialog`.
/// Runs during the shell's render (the app is not leased there), so
/// `app.update`/`app.read` are safe inside.
pub type DialogBuilder = fn(&Entity<QuillApp>, &Entity<QuillShell>, Dialog, &mut App) -> Dialog;

pub struct QuillShell {
    pub(super) app: Entity<QuillApp>,
    /// The kit dialog is modal: at most one is shown at a time.
    /// `None` means no kit dialog is currently open.
    pub(super) open_dialog: Option<DialogKind>,
}

impl QuillShell {
    pub fn new(app: Entity<QuillApp>) -> Self {
        Self {
            app,
            open_dialog: None,
        }
    }

    /// The app-side open flag for each dialog kind. Stays in sync with
    /// the render-time overlay conditions the hand-rolled dialogs used.
    fn dialog_is_open(app: &QuillApp, kind: DialogKind) -> bool {
        match kind {
            DialogKind::Scheduled => app.scheduled_dialog_open,
            DialogKind::PaymentForm => {
                app.payment_dialog.is_some()
                    && app
                        .session()
                        .is_some_and(|s| s.payment_form.is_some() || s.payment_form_loading)
            }
            DialogKind::PaymentReceipt => app.session().is_some_and(|s| s.payment_receipt_open),
            DialogKind::Subscriptions => app.session().is_some_and(|s| s.subscriptions_open),
            DialogKind::CallbackPassword => app.callback_password_dialog.is_some(),
            DialogKind::LoginUrlConfirm => app.login_url_confirm.is_some(),
            DialogKind::DeepLinkInfo => app.deep_link_dialog.is_some(),
            DialogKind::DeepLinkInvite => app.deep_link_invite.is_some(),
            DialogKind::PollVoters => app.poll_voters_dialog.is_some(),
            DialogKind::ArchivedStickers => app.sticker_settings_open,
            DialogKind::EmojiSets => app.session().is_some_and(|s| s.emoji.open),
            DialogKind::ArchiveSettings => app.session().is_some_and(|s| s.archive_settings_open),
            DialogKind::ImportContacts => app.import_contacts_dialog.is_some(),
            DialogKind::AddContact => app.add_contact_dialog.is_some(),
            DialogKind::EditProfile => app.edit_profile_dialog.is_some(),
            DialogKind::GroupCallStart => app.group_call_start_dialog.is_some(),
            DialogKind::FolderEditor => app.folder_editor.is_some(),
            DialogKind::FolderDelete => app.folder_delete_confirm.is_some(),
            DialogKind::FolderManage => app.folder_manage_open,
            DialogKind::CallConfirm => app.call_confirm.is_some(),
            DialogKind::CallSwap => app.session().is_some_and(|s| s.call_swap_pending.is_some()),
            DialogKind::NotificationDefaults => app.notification_defaults_open,
            DialogKind::StorageUsage => app.storage_usage_open,
            DialogKind::TwoFa => app.twofa_open,
            DialogKind::Sessions => app.sessions_open,
            DialogKind::Websites => app.websites_open,
            DialogKind::CreateChat => app.create_chat_dialog.is_some(),
            DialogKind::Member => app.member_dialog.is_some(),
            DialogKind::Permissions => app.permissions_dialog.is_some(),
            DialogKind::Username => app.username_dialog.is_some(),
            DialogKind::Restrict => app.restrict_dialog.is_some(),
            DialogKind::GroupConfirm => app.group_confirm_dialog.is_some(),
            DialogKind::QuoteReply => app.quote_reply_dialog.is_some(),
            DialogKind::ForumManage => app.forum_manage_dialog.is_some(),
            DialogKind::CommentThread => app.comment_thread_dialog.is_some(),
            DialogKind::Welcome => app.welcome_dialog.is_some(),
            DialogKind::Appearance => app.appearance_open,
            DialogKind::AccountLifecycle => app.account_lifecycle.open,
            DialogKind::Accounts => app.accounts_ui.open,
            // Slice G10: communities create + hub dialogs.
            DialogKind::CommunityCreate => app.community_ui.create_dialog.is_some(),
            DialogKind::CommunityHub => app.community_ui.hub_open,
            DialogKind::Shortcuts => app.shortcuts_open,
        }
    }

    fn dialog_builder(kind: DialogKind) -> DialogBuilder {
        match kind {
            DialogKind::Scheduled => QuillApp::build_scheduled_dialog,
            DialogKind::PaymentForm => QuillApp::build_payment_dialog,
            DialogKind::PaymentReceipt => QuillApp::build_payment_receipt_dialog,
            DialogKind::Subscriptions => QuillApp::build_subscriptions_dialog,
            DialogKind::CallbackPassword => QuillApp::build_callback_password_dialog,
            DialogKind::LoginUrlConfirm => QuillApp::build_login_url_confirm_dialog,
            DialogKind::DeepLinkInfo => QuillApp::build_deep_link_dialog,
            DialogKind::DeepLinkInvite => QuillApp::build_deep_link_invite_dialog,
            DialogKind::PollVoters => QuillApp::build_poll_voters_dialog,
            DialogKind::ArchiveSettings => QuillApp::build_archive_settings_dialog,
            DialogKind::ArchivedStickers => QuillApp::build_archived_stickers_dialog,
            DialogKind::EmojiSets => QuillApp::build_emoji_sets_dialog,
            DialogKind::ImportContacts => QuillApp::build_import_contacts_dialog,
            DialogKind::AddContact => QuillApp::build_add_contact_dialog,
            DialogKind::EditProfile => QuillApp::build_edit_profile_dialog,
            DialogKind::GroupCallStart => QuillApp::build_group_call_start_dialog,
            DialogKind::FolderEditor => QuillApp::build_folder_editor_dialog,
            DialogKind::FolderDelete => QuillApp::build_folder_delete_dialog,
            DialogKind::FolderManage => QuillApp::build_folder_manage_dialog,
            DialogKind::CallConfirm => QuillApp::build_call_confirm_dialog,
            DialogKind::CallSwap => QuillApp::build_call_swap_dialog,
            DialogKind::NotificationDefaults => QuillApp::build_notification_defaults_dialog,
            DialogKind::StorageUsage => QuillApp::build_storage_usage_dialog,
            DialogKind::TwoFa => QuillApp::build_twofa_dialog,
            DialogKind::Sessions => QuillApp::build_sessions_dialog,
            DialogKind::Websites => QuillApp::build_websites_dialog,
            DialogKind::CreateChat => QuillApp::build_create_chat_dialog,
            DialogKind::Member => QuillApp::build_member_dialog,
            DialogKind::Permissions => QuillApp::build_permissions_dialog,
            DialogKind::Username => QuillApp::build_username_dialog,
            DialogKind::Restrict => QuillApp::build_restrict_dialog,
            DialogKind::GroupConfirm => QuillApp::build_group_confirm_dialog,
            DialogKind::QuoteReply => QuillApp::build_quote_reply_dialog,
            DialogKind::ForumManage => QuillApp::build_forum_manage_dialog,
            DialogKind::CommentThread => QuillApp::build_comment_thread_dialog,
            DialogKind::Welcome => QuillApp::build_welcome_dialog,
            DialogKind::Appearance => QuillApp::build_appearance_dialog,
            DialogKind::AccountLifecycle => QuillApp::build_account_lifecycle_dialog,
            DialogKind::Accounts => QuillApp::build_accounts_dialog,
            // Slice G10: community builders live in dialogs/community.rs.
            DialogKind::CommunityCreate => community::build_create_community_dialog,
            DialogKind::CommunityHub => community::build_community_hub_dialog,
            DialogKind::Shortcuts => QuillApp::build_shortcuts_dialog,
        }
    }

    /// All dialog kinds in a fixed order (matches the old overlay
    /// priority: first open flag wins when several are set).
    const KINDS: &[DialogKind] = &[
        DialogKind::Scheduled,
        DialogKind::GroupCallStart,
        DialogKind::ArchiveSettings,
        DialogKind::Websites,
        DialogKind::Sessions,
        DialogKind::TwoFa,
        DialogKind::StorageUsage,
        DialogKind::NotificationDefaults,
        DialogKind::CallConfirm,
        // Swap prompt is call-urgent: same priority band as CallConfirm.
        DialogKind::CallSwap,
        DialogKind::FolderEditor,
        DialogKind::FolderDelete,
        DialogKind::FolderManage,
        DialogKind::CallbackPassword,
        DialogKind::LoginUrlConfirm,
        // `parity:platform-deep-links`: link info sits with the other
        // low-priority informational dialogs.
        DialogKind::DeepLinkInfo,
        DialogKind::DeepLinkInvite,
        DialogKind::PaymentForm,
        DialogKind::PaymentReceipt,
        DialogKind::Subscriptions,
        DialogKind::CreateChat,
        DialogKind::Member,
        DialogKind::Permissions,
        DialogKind::Username,
        DialogKind::Restrict,
        DialogKind::GroupConfirm,
        DialogKind::ArchivedStickers,
        DialogKind::EmojiSets,
        DialogKind::QuoteReply,
        DialogKind::ForumManage,
        DialogKind::CommentThread,
        DialogKind::PollVoters,
        DialogKind::Welcome,
        DialogKind::ImportContacts,
        DialogKind::EditProfile,
        DialogKind::AddContact,
        DialogKind::Appearance,
        DialogKind::AccountLifecycle,
        // Slice G10: communities dialogs render last (lowest priority).
        DialogKind::CommunityCreate,
        DialogKind::CommunityHub,
        // Slice parity:auth-multi-account (UI): accounts sit with the
        // other settings-level dialogs (lowest priority band).
        DialogKind::Accounts,
        // Slice parity:platform-shortcuts-reference: informational, lowest
        // priority.
        DialogKind::Shortcuts,
    ];

    /// Keep the single kit dialog in sync with the app-side open flags.
    /// The kit dialog is modal, so at most one flag wins, by `KINDS`
    /// priority. A flag cleared without a kit close (e.g. an action
    /// button that only clears state) closes the kit dialog on the next
    /// render; a flag set while another dialog shows closes the old one
    /// first, so dialogs never stack.
    ///
    /// Runs inside the shell's render: the app is not leased here, so
    /// `open_dialog`/`close_dialog` (which lease only `Root`) are safe.
    fn sync_kit_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let shell = cx.entity();
        // Snapshot the flags once; the app is not leased here.
        let first_open = {
            let app_ref = app.read(cx);
            Self::KINDS
                .iter()
                .find(|kind| Self::dialog_is_open(app_ref, **kind))
                .copied()
        };
        if self.open_dialog != first_open {
            // Close the currently shown dialog (if any) before opening
            // the next one, so a transition (e.g. folder manager ->
            // editor) never stacks two kit dialogs.
            if self.open_dialog.is_some() {
                window.close_dialog(cx);
            }
            self.open_dialog = first_open;
            if let Some(kind) = first_open {
                let build = Self::dialog_builder(kind);
                let app_c = app.clone();
                let shell_c = shell.clone();
                window.open_dialog(cx, move |dialog, _window, cx| {
                    // The kit binds Enter to Confirm (which closes the dialog);
                    // the hand-rolled dialogs had no dialog-level Enter behavior,
                    // so keep it disabled. A builder that wants Enter-to-confirm
                    // can set its own `on_ok`, which overrides this default.
                    build(&app_c, &shell_c, dialog.on_ok(|_, _, _| false), cx)
                });
            }
        }
    }

    /// Shared `Dialog::on_close` for every migrated dialog: the kit
    /// already closed the dialog (Esc / backdrop / ✕), so this clears
    /// the app-side open flag and drops the tracked kind — it must NOT
    /// call `close_dialog` again.
    pub(crate) fn on_close_kind(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        kind: DialogKind,
        clear: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + Clone + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + Clone + 'static {
        let app = app.clone();
        let shell = shell.clone();
        move |_, window, cx: &mut App| {
            app.update(cx, |this, cx| clear(this, window, cx));
            shell.update(cx, |shell, _| {
                if shell.open_dialog == Some(kind) {
                    shell.open_dialog = None;
                }
            });
        }
    }
}

impl Render for QuillShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_kit_dialogs(window, cx);
        div()
            .size_full()
            .child(self.app.clone())
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl QuillApp {
    /// Slice CL2: the archive auto-settings dialog — the three
    /// schema-backed toggles (schema 1.8.67, line 3512), with TGX's
    /// labels (`SettingsArchiveChatListController`). TGX's fourth
    /// "archive as folder" appearance toggle is a client-side display
    /// preference and stays out of scope.
    /// kit Phase 2 (redo): explicit in-dialog actions (Cancel, Close,
    /// successful submits) must close the kit dialog immediately rather
    /// than waiting for the shell sync. The kit's `close_dialog` does not
    /// run `on_close`, so the action itself clears the app-side open flag
    /// first; this closes the kit dialog only when the flag is actually
    /// cleared — validation failures and non-closing actions keep the
    /// dialog open.
    pub(crate) fn close_kit_dialog_if_done(
        &self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !QuillShell::dialog_is_open(self, kind) {
            window.close_dialog(cx);
        }
    }
}
