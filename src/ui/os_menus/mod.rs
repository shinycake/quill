//! OS integration menus, after Telegram Desktop:
//!
//! - macOS Dock icon menu (`menu/menu_dock.cpp`, see [`dock`]).
//! - macOS Window menu items (`platform/mac/global_menu_mac.mm`, see
//!   [`window_menu`]).
//! - Windows taskbar jump list: tdesktop's one "Quit Telegram" task
//!   (`platform/win/integration_win.cpp`), here "Quit Quill". The task starts
//!   `quill --dock-action 0`, which the single-instance forwarding turns into
//!   a quit of the running Quill (`start_files::quit_requested`).
//! - Linux: the `.desktop` file's "Quit Quill" action (`quill --quit`), the
//!   same as tdesktop's `[Desktop Action quit]`.
//!
//! GPUI builds the macOS menus from data, so they are rebuilt when what they
//! show changes: a check on every applied batch compares the model built
//! from memory, and reads the account registry off the UI thread only when
//! that model moved, the registry was written, a background client was added
//! or removed, or the last read is old (see [`sync`]).
#![deny(clippy::indexing_slicing)]
// The Dock and Window menus are macOS-only; Windows uses part of this.
#![cfg_attr(not(target_os = "macos"), allow(dead_code, unused_imports))]

mod dock;
mod sync;
mod window_menu;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

pub(crate) use dock::{DockAccount, DockEntry, DockModel, DockPlayer, dock_entries};
pub(crate) use window_menu::{WindowMenuState, window_menu_items};

use super::app::QuillApp;
use super::navigation::NavigationAction;
use gpui_kit::*;
use quill::ids::AccountKey;
use quill::playlist::Step;
use quill::telegram::envelope::AuthorizationState;

// `MenuUnavailable` is bound to disabled items; nothing handles it, so GPUI
// shows the item greyed out.
actions!(
    quill_os_menu,
    [
        MenuUnavailable,
        WindowMenuContacts,
        WindowMenuAddContact,
        WindowMenuNewGroup,
        WindowMenuNewChannel,
        WindowMenuShowMain,
        DockToggleNotifications,
        DockPlayerPrevious,
        DockPlayerPlayPause,
        DockPlayerNext,
    ]
);

/// Dock menu: bring this account's chats up.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = quill_os_menu, no_json)]
pub struct DockOpenAccount {
    pub key: String,
}

/// The accounts registry as the Dock menu needs it, read off the UI thread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Registry {
    /// `(key, stored display name)` in registry order.
    pub(crate) accounts: Vec<(AccountKey, String)>,
    /// The account Quill connects (the one on screen).
    pub(crate) active: Option<AccountKey>,
}

fn read_registry() -> Registry {
    let Some(root) = quill::settings::safe_app_root() else {
        return Registry::default();
    };
    Registry {
        accounts: quill::settings::list_accounts(&root)
            .into_iter()
            .map(|record| (record.key, record.display_name))
            .collect(),
        active: Some(quill::settings::active_account(&root)),
    }
}

impl QuillApp {
    /// An account is signed in on screen and the passcode lock is off:
    /// tdesktop's "there is a session controller" check.
    pub(crate) fn os_menu_session_usable(&self) -> bool {
        matches!(self.current_auth(), AuthorizationState::Ready) && !self.account.passcode.locked
    }

    /// What the Dock menu shows now, given the registry.
    pub(crate) fn dock_model(&self, registry: &Registry) -> DockModel {
        let on_screen = self
            .session()
            .map(|session| session.account.clone())
            .or_else(|| registry.active.clone());
        let own_name = self
            .session()
            .and_then(|session| session.my_user_id)
            .and_then(|id| self.session().and_then(|session| session.user(id)))
            .map(|user| user.display_name());
        let accounts = registry
            .accounts
            .iter()
            .filter_map(|(key, stored)| {
                let signed_in = if on_screen.as_ref() == Some(key) {
                    matches!(self.current_auth(), AuthorizationState::Ready)
                } else {
                    self.background.client(key).is_some_and(|client| {
                        matches!(
                            client.status,
                            quill::notify_accounts::BackgroundStatus::Ready
                                | quill::notify_accounts::BackgroundStatus::Connecting
                        )
                    })
                };
                if !signed_in {
                    return None;
                }
                let name = if on_screen.as_ref() == Some(key) {
                    own_name.clone().unwrap_or_else(|| stored.clone())
                } else {
                    stored.clone()
                };
                Some(DockAccount {
                    key: key.clone(),
                    name,
                })
            })
            .collect();
        let player = self.active_playback_id().map(|_| {
            let (can_previous, can_next) = self.playlist_can_move();
            DockPlayer {
                paused: !self.playback.is_playing(),
                can_previous,
                can_next,
            }
        });
        DockModel {
            registered: registry.accounts.len(),
            accounts,
            locked: self.account.passcode.locked,
            notifications: self.desktop_notifications_enabled(),
            player,
        }
    }

    /// Window > Contacts / Add Contact / New Group / New Channel. tdesktop
    /// disables them without a usable session; a stale click is a no-op.
    pub(crate) fn run_window_menu(
        &mut self,
        command: window_menu::WindowCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use window_menu::WindowCommand;
        if command == WindowCommand::ShowMain {
            raise(window, cx);
            return;
        }
        if !self.os_menu_session_usable() {
            return;
        }
        raise(window, cx);
        match command {
            WindowCommand::Contacts => self.navigate(NavigationAction::Contacts, window, cx),
            WindowCommand::AddContact => self.open_new_contact_dialog(window, cx),
            WindowCommand::NewGroup => self.navigate(NavigationAction::Group, window, cx),
            WindowCommand::NewChannel => self.navigate(NavigationAction::Channel, window, cx),
            WindowCommand::FullScreen | WindowCommand::ShowMain => {}
        }
    }

    /// Dock > an account: tdesktop opens that account's window; Quill has
    /// one window, so it switches to the account and raises the window.
    pub(crate) fn open_dock_account(
        &mut self,
        key: &AccountKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        raise(window, cx);
        if self.session().map(|session| &session.account) != Some(key) {
            self.switch_account(key, "", window, cx);
        }
    }

    /// Dock > Disable / Enable notifications.
    pub(crate) fn toggle_dock_notifications(&mut self, cx: &mut Context<Self>) {
        let on = self.desktop_notifications_enabled();
        self.set_desktop_notifications(!on, cx);
    }

    /// Dock > Previous / Next / Pause / Resume.
    pub(crate) fn dock_player(&mut self, step: Option<Step>, cx: &mut Context<Self>) {
        if self.active_playback_id().is_none() {
            return;
        }
        match step {
            Some(step) => self.player_step(step, cx),
            None => self.toggle_active_playback(cx),
        }
    }
}

/// Show and focus the main window (tdesktop `activate()`).
fn raise(window: &mut Window, cx: &mut App) {
    let _ = super::window_control::set_visible(window, true);
    cx.activate(true);
    window.activate_window();
}

/// Run `f` on the app in its window, after the current dispatch.
fn on_app(
    cx: &mut App,
    handle: AnyWindowHandle,
    app: WeakEntity<QuillApp>,
    f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) {
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, cx| {
            let _ = app.update(cx, |this, cx| f(this, window, cx));
        });
    });
}

/// The `MenuItem`s for the Dock menu's rows.
pub(crate) fn dock_menu_items(entries: &[DockEntry]) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|entry| {
            let (action, enabled): (Box<dyn Action>, bool) = match entry {
                DockEntry::Separator => return MenuItem::separator(),
                DockEntry::ProfilesHeader => (Box::new(MenuUnavailable), false),
                DockEntry::Account { key, .. } => {
                    (Box::new(DockOpenAccount { key: key.0.clone() }), true)
                }
                DockEntry::ToggleNotifications { .. } => (Box::new(DockToggleNotifications), true),
                DockEntry::Previous { enabled } => (Box::new(DockPlayerPrevious), *enabled),
                DockEntry::PlayPause { .. } => (Box::new(DockPlayerPlayPause), true),
                DockEntry::Next { enabled } => (Box::new(DockPlayerNext), *enabled),
            };
            MenuItem::Action {
                name: entry.label().into(),
                action: if enabled {
                    action
                } else {
                    Box::new(MenuUnavailable)
                },
                os_action: None,
                checked: false,
                disabled: !enabled,
            }
        })
        .collect()
}

/// Register the menu actions and keep the macOS menus current. Demo windows
/// (screenshots, tests) leave the OS menus alone.
pub(crate) fn install(demo: bool, window: &mut Window, cx: &mut Context<QuillApp>) {
    #[cfg(target_os = "macos")]
    super::touch_bar::install(demo, window, cx);
    if demo {
        return;
    }
    register_actions(window.window_handle(), cx.entity().downgrade(), cx);
    #[cfg(target_os = "windows")]
    cx.set_dock_menu(vec![MenuItem::action(
        quill::lang::tr(quill::lang::keys::Q_MISC_QUIT_QUILL),
        super::actions::QuitApp,
    )]);
    #[cfg(target_os = "macos")]
    spawn_menu_sync(window, cx);
}

/// Global handlers, so the items work from the Dock and the menu bar while
/// no Quill window is key.
pub(crate) fn register_actions(handle: AnyWindowHandle, app: WeakEntity<QuillApp>, cx: &mut App) {
    use window_menu::WindowCommand;
    let window_command = |command: WindowCommand| {
        let app = app.clone();
        move |cx: &mut App| {
            on_app(cx, handle, app.clone(), move |this, window, cx| {
                this.run_window_menu(command, window, cx);
            });
        }
    };
    let contacts = window_command(WindowCommand::Contacts);
    cx.on_action(move |_: &WindowMenuContacts, cx| contacts(cx));
    let add_contact = window_command(WindowCommand::AddContact);
    cx.on_action(move |_: &WindowMenuAddContact, cx| add_contact(cx));
    let new_group = window_command(WindowCommand::NewGroup);
    cx.on_action(move |_: &WindowMenuNewGroup, cx| new_group(cx));
    let new_channel = window_command(WindowCommand::NewChannel);
    cx.on_action(move |_: &WindowMenuNewChannel, cx| new_channel(cx));
    let show_main = window_command(WindowCommand::ShowMain);
    cx.on_action(move |_: &WindowMenuShowMain, cx| show_main(cx));
    let weak = app.clone();
    cx.on_action(move |action: &DockOpenAccount, cx| {
        let key = AccountKey(action.key.clone());
        on_app(cx, handle, weak.clone(), move |this, window, cx| {
            this.open_dock_account(&key, window, cx);
        });
    });
    let weak = app.clone();
    cx.on_action(move |_: &DockToggleNotifications, cx| {
        on_app(cx, handle, weak.clone(), |this, _, cx| {
            this.toggle_dock_notifications(cx)
        });
    });
    let player = |step: Option<Step>| {
        let app = app.clone();
        move |cx: &mut App| {
            on_app(cx, handle, app.clone(), move |this, _, cx| {
                this.dock_player(step, cx)
            });
        }
    };
    let previous = player(Some(Step::Previous));
    cx.on_action(move |_: &DockPlayerPrevious, cx| previous(cx));
    let play_pause = player(None);
    cx.on_action(move |_: &DockPlayerPlayPause, cx| play_pause(cx));
    let next = player(Some(Step::Next));
    cx.on_action(move |_: &DockPlayerNext, cx| next(cx));
    // Quit with no window key: the main window hidden by its close button
    // on a Mac (the window's own handler closes it first otherwise).
    cx.on_action(|_: &super::actions::QuitApp, cx| {
        use quill::agent::ui_driver::guard::{self, HardSurface};
        if !guard::refuse_hard_if_injecting(HardSurface::Window) {
            cx.quit();
        }
    });
}

/// macOS: rebuild the Dock menu and the Window menu when what they show
/// changes. The registry is read on a background thread.
#[cfg(target_os = "macos")]
fn spawn_menu_sync(window: &mut Window, cx: &mut Context<QuillApp>) {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    let main_active = Rc::new(Cell::new(window.is_window_active()));
    let observed = main_active.clone();
    cx.observe_window_activation(window, move |_, window, _| {
        observed.set(window.is_window_active());
        // The menus and the tray's minimized-window check follow it.
        quill::wake::CHROME.poke();
    })
    .detach();
    cx.spawn(async move |this, cx| {
        let mut sync = sync::MenuSync::default();
        loop {
            // Taken before the work: a poke during it runs the loop again.
            let seen = quill::wake::CHROME.epoch();
            let main_now = main_active.get();
            let writes = quill::settings::registry_writes();
            let built = |this: &QuillApp, registry: &Registry| {
                let state = WindowMenuState {
                    session_usable: this.os_menu_session_usable(),
                    main_active: main_now,
                    sidebar_hidden: this.frame.sidebar.hidden(),
                };
                (this.dock_model(registry), state)
            };
            // From memory first: most wakes (an applied batch) change
            // nothing the menus show, and then the disk is not read.
            let current = this
                .update(cx, |this, _| {
                    (built(this, sync.registry()), this.background.keys())
                })
                .ok();
            let clients = current
                .as_ref()
                .map(|(_, keys)| keys.clone())
                .unwrap_or_default();
            let stale = current.as_ref().is_none_or(|(current, keys)| {
                sync.needs_registry(std::time::Instant::now(), current, writes, keys)
            });
            if stale {
                let registry = cx
                    .background_executor()
                    .spawn(async { read_registry() })
                    .await;
                sync.set_registry(registry, std::time::Instant::now(), writes, clients);
            }
            let alive = this
                .update(cx, |this, cx| {
                    let (model, state) = built(this, sync.registry());
                    let (dock_changed, menu_changed) = sync.install(model.clone(), state);
                    if dock_changed {
                        cx.set_dock_menu(dock_menu_items(&dock_entries(&model)));
                    }
                    if menu_changed {
                        cx.set_menus(super::keybindings::app_menus_with(state));
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
            // Event-driven: applied updates, activation, lock changes,
            // notification toggles and player changes poke `CHROME`. The
            // timeout covers what nothing announces (the account registry
            // on disk).
            super::wake_wait::sleep_or_wake(
                cx.background_executor(),
                &quill::wake::CHROME,
                seen,
                Duration::from_secs(10),
            )
            .await;
            // A burst of pokes rebuilds the menus at most once a second.
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    })
    .detach();
}
