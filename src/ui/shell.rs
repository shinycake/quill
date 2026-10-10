//! QuillShell, the dialog registry (DialogKind, `register_dialogs!`), title
//! bar, kit-dialog sync.

use super::app::{PaneMode, QuillApp};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::{Dialog, DialogContent};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
pub(super) fn title_bar(
    mode: PaneMode,
    live: bool,
    search_open: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let _ = live;
    let _ = mode;
    let title = "Quill";
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
///
/// A kind is a name, not a closed enum: each dialog module declares its
/// own kinds with [`register_dialogs!`], which adds an associated const
/// (`DialogKind::Settings`, ...) and submits a [`DialogSpec`]. Two kinds
/// with the same name are a compile error (duplicate associated const).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DialogKind(&'static str);

impl DialogKind {
    /// Only [`register_dialogs!`] should call this.
    pub(crate) const fn named(name: &'static str) -> Self {
        Self(name)
    }
}

impl std::fmt::Debug for DialogKind {
    // Same output as the old `#[derive(Debug)]` enum: just the name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Builder for one dialog kind: `(app, shell, dialog, cx) -> dialog`.
/// Runs during the shell's render (the app is not leased there), so
/// `app.update`/`app.read` are safe inside.
pub type DialogBuilder = fn(&Entity<QuillApp>, &Entity<QuillShell>, Dialog, &mut App) -> Dialog;

/// One dialog the shell can show, collected at link time.
pub(crate) struct DialogSpec {
    kind: DialogKind,
    /// Lower wins: when several open flags are set at once, the spec
    /// with the smallest priority is the one shown. Every priority must
    /// be unique (checked by a test).
    priority: u32,
    /// The app-side open flag. The passcode lock is checked by the shell,
    /// not here.
    is_open: fn(&QuillApp) -> bool,
    build: DialogBuilder,
}

impl DialogSpec {
    pub(crate) const fn new(
        priority: u32,
        is_open: fn(&QuillApp) -> bool,
        build: DialogBuilder,
    ) -> Self {
        Self {
            // Filled in by `register_dialogs!` from the entry's name.
            kind: DialogKind::named(""),
            priority,
            is_open,
            build,
        }
    }

    /// Only [`register_dialogs!`] should call this.
    pub(crate) const fn kind(self, kind: DialogKind) -> Self {
        Self { kind, ..self }
    }
}

inventory::collect!(DialogSpec);

/// Declare one or more dialogs next to their builders. Each entry names
/// the kind (it becomes `DialogKind::<Name>`) and gives its spec:
///
/// ```ignore
/// crate::ui::shell::register_dialogs! {
///     /// Doc comment for `DialogKind::Example`.
///     Example => DialogSpec::new(
///         4150,
///         |app| app.example_dialog.is_some(),
///         QuillApp::build_example_dialog,
///     ),
/// }
/// ```
///
/// `DialogSpec` is in scope inside the macro, so callers need no import.
/// See docs/decisions/codex-refactor-2.md for picking a priority.
macro_rules! register_dialogs {
    ($(
        $(#[$meta:meta])*
        $kind:ident => $spec:expr
    ),+ $(,)?) => {
        $(
            impl $crate::ui::shell::DialogKind {
                $(#[$meta])*
                #[allow(non_upper_case_globals)]
                pub const $kind: Self = Self::named(stringify!($kind));
            }
            ::inventory::submit! {{
                use $crate::ui::shell::DialogSpec;
                $spec.kind($crate::ui::shell::DialogKind::$kind)
            }}
        )+
    };
}
pub(crate) use register_dialogs;

/// Every registered dialog, sorted by priority (highest priority first).
fn registry() -> &'static [&'static DialogSpec] {
    static SORTED: std::sync::OnceLock<Vec<&'static DialogSpec>> = std::sync::OnceLock::new();
    SORTED.get_or_init(|| {
        let mut all: Vec<_> = inventory::iter::<DialogSpec>.into_iter().collect();
        all.sort_by_key(|spec| spec.priority);
        all
    })
}

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

    /// The app-side open flag for one dialog kind. Stays in sync with
    /// the render-time overlay conditions the hand-rolled dialogs used.
    fn dialog_is_open(app: &QuillApp, kind: DialogKind) -> bool {
        // The lock screen covers everything: no dialog stays above it.
        if app.passcode_ui.locked {
            return false;
        }
        registry()
            .iter()
            .find(|r| r.kind == kind)
            .is_some_and(|r| (r.is_open)(app))
    }

    /// The open dialog that wins by priority (first open flag in
    /// registry order), or `None` when none is open or the app is locked.
    fn first_open(app: &QuillApp) -> Option<&'static DialogSpec> {
        if app.passcode_ui.locked {
            return None;
        }
        registry().iter().find(|r| (r.is_open)(app)).copied()
    }

    /// Keep the single kit dialog in sync with the app-side open flags.
    /// The kit dialog is modal, so at most one flag wins, by registry
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
        let winner = Self::first_open(app.read(cx));
        let first_open = winner.map(|r| r.kind);
        if self.open_dialog != first_open {
            // Close the currently shown dialog (if any) before opening
            // the next one, so a transition (e.g. folder manager ->
            // editor) never stacks two kit dialogs.
            if self.open_dialog.is_some() {
                window.close_dialog(cx);
            }
            self.open_dialog = first_open;
            if let Some(spec) = winner {
                let build = spec.build;
                let app_c = app.clone();
                let shell_c = shell.clone();
                window.open_dialog(cx, move |dialog, window, cx| {
                    // The kit binds Enter to Confirm (which closes the dialog);
                    // the hand-rolled dialogs had no dialog-level Enter behavior,
                    // so keep it disabled. A builder that wants Enter-to-confirm
                    // can set its own `on_ok`, which overrides this default.
                    build(&app_c, &shell_c, dialog.on_ok(|_, _, _| false), cx)
                        .max_w((window.viewport_size().width - px(48.)).max(px(240.)))
                        .max_h((window.viewport_size().height - px(48.)).max(px(120.)))
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
            .when(self.open_dialog.is_some(), |this| {
                // Kit dialog contents paint at priority 10. Occlude the app immediately below them.
                this.child(
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .id("dialog-hit-test-barrier")
                                .w(window.viewport_size().width)
                                .h(window.viewport_size().height)
                                .occlude(),
                        ),
                    )
                    .with_priority(9),
                )
            })
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

/// Bound every dialog body to the current window and keep its header/footer visible.
pub(crate) fn scrollable_dialog_content<F>(
    build: F,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent
where
    F: Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static,
{
    move |content, window, cx| {
        let available = (window.viewport_size().height - px(180.)).max(px(80.));
        let body = build(DialogContent::new(), window, cx);
        content.min_h_0().child(
            div()
                .id("dialog-body-scroll")
                .role(Role::ScrollView)
                .aria_label("Dialog content")
                .w_full()
                .max_h(available)
                .overflow_y_scroll()
                .child(body),
        )
    }
}

/// Plain GPUI text does not become an accessible name. Use this for every
/// dialog title so its visible heading is also available to screen readers.
pub(crate) fn dialog_title(title: impl Into<SharedString>) -> impl IntoElement {
    let title = title.into();
    div()
        .id("quill-dialog-title")
        .role(Role::Heading)
        .aria_label(title.clone())
        .child(title)
}

#[cfg(test)]
mod tests {
    // Not a glob: `gpui_kit::*` would shadow the built-in `#[test]`.
    use super::{DialogKind, registry};

    /// The `KINDS` priority list as it stood before dialogs registered
    /// themselves (docs/decisions/codex-refactor-2.md). Frozen: new
    /// dialogs do not go here, they only need a free priority number.
    const LEGACY_ORDER: &[DialogKind] = &[
        DialogKind::AccountNotice,
        DialogKind::Passcode,
        DialogKind::Scheduled,
        DialogKind::GroupCallStart,
        DialogKind::ArchiveSettings,
        DialogKind::Websites,
        DialogKind::Sessions,
        DialogKind::TwoFa,
        DialogKind::StorageUsage,
        DialogKind::NotificationDefaults,
        DialogKind::CallConfirm,
        DialogKind::CallSwap,
        DialogKind::FolderLimit,
        DialogKind::ArchiveHint,
        DialogKind::ChatExport,
        DialogKind::ClearCalls,
        DialogKind::FolderEditor,
        DialogKind::FolderDelete,
        DialogKind::FolderShare,
        DialogKind::FolderInvite,
        DialogKind::ChatLook,
        DialogKind::FolderNewChats,
        DialogKind::FolderManage,
        DialogKind::CallbackPassword,
        DialogKind::LoginUrlConfirm,
        DialogKind::RequestShare,
        DialogKind::DeepLinkInfo,
        DialogKind::DeepLinkInvite,
        DialogKind::DeepLinkShare,
        DialogKind::ProxyEdit,
        DialogKind::ProxyLink,
        DialogKind::ProxyList,
        DialogKind::OpenLink,
        DialogKind::FileOpenConfirm,
        DialogKind::PaymentForm,
        DialogKind::PaymentReceipt,
        DialogKind::Subscriptions,
        DialogKind::Stars,
        DialogKind::ReceivedGifts,
        DialogKind::PremiumFeatures,
        DialogKind::Marketplace,
        DialogKind::CreateChat,
        DialogKind::Member,
        DialogKind::Permissions,
        DialogKind::Username,
        DialogKind::Restrict,
        DialogKind::Ownership,
        DialogKind::GroupConfirm,
        DialogKind::GroupSettings,
        DialogKind::ArchivedStickers,
        DialogKind::EmojiSets,
        DialogKind::ForumManage,
        DialogKind::SavedTagName,
        DialogKind::FactCheck,
        DialogKind::PollVoters,
        DialogKind::MessageReport,
        DialogKind::StickerSet,
        DialogKind::Welcome,
        DialogKind::ImportContacts,
        DialogKind::EditProfile,
        DialogKind::ProfilePanel,
        DialogKind::AddContact,
        DialogKind::BlockBar,
        DialogKind::JoinRequests,
        DialogKind::Translate,
        DialogKind::Appearance,
        DialogKind::AccountLifecycle,
        DialogKind::CommunityCreate,
        DialogKind::CommunityHub,
        DialogKind::Accounts,
        DialogKind::JumpToDate,
        DialogKind::Shortcuts,
        DialogKind::Settings,
    ];

    #[test]
    fn registry_keeps_the_legacy_kinds_order() {
        // Every legacy kind is registered, and relative to each other they
        // keep the old order exactly. Dialogs added later may sit anywhere.
        let order: Vec<DialogKind> = registry()
            .iter()
            .map(|spec| spec.kind)
            .filter(|kind| LEGACY_ORDER.contains(kind))
            .collect();
        assert_eq!(order, LEGACY_ORDER);
    }

    #[test]
    fn dialog_priorities_and_names_are_unique() {
        let all = registry();
        for pair in all.windows(2) {
            assert!(
                pair[0].priority < pair[1].priority,
                "{:?} and {:?} share priority {}",
                pair[0].kind,
                pair[1].kind,
                pair[0].priority
            );
        }
        let mut names: Vec<&str> = all.iter().map(|r| r.kind.0).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len());
    }

    #[test]
    fn dialog_kind_debug_prints_the_bare_name() {
        assert_eq!(format!("{:?}", DialogKind::Settings), "Settings");
    }
}
