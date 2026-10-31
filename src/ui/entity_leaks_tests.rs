//! Closing a surface lets go of everything opening it made: each surface
//! opens and closes once (what stays then is a cache filled on first use),
//! then `CYCLES` more times, after which the app must hold no more
//! entities, subscriptions or running tasks than after the first close.
//! Every open and close is followed by a frame of each open window, a
//! second of test clock and another frame, so views made after the first
//! frame are made. Counts are read after a further minute of test clock,
//! so a task still running then never ends.
//! Counts come from gpui-pre's test-support hooks
//! (`docs/decisions/entity-leaks.md`).
use crate::ui::actions::CloseWindow;
use crate::ui::app::QuillApp;
use crate::ui::esc_stack::ESC_LAYERS;
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::PanelTab;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, Focusable, TestAppContext, VisualTestContext,
    Window, point, px, size,
};
use quill::ids::{ChatId, FileId, MessageId};
use quill::new_chat::NewChatKind;
use quill::settings_catalog::Page;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::{MessageContent, MessageSender, PhotoContent, PhotoSizeView};
use std::collections::BTreeMap;
use std::time::Duration;

/// Opens and closes after the first one.
const CYCLES: usize = 3;

/// How far the test clock runs after each open and close: past closing
/// animations and debounces.
const SETTLE: Duration = Duration::from_secs(1);

/// How far it runs before counting: a task or timer still alive after
/// this never ends, rather than not having finished yet. Only while the
/// surface is closed: a live call ticks through every frame of it.
const QUIET: Duration = Duration::from_secs(60);

/// Live entities by type, subscriptions by kind and running tasks by
/// where they were spawned.
type Counts = BTreeMap<String, usize>;

pub(in crate::ui) struct Harness {
    pub(in crate::ui) app: Entity<QuillApp>,
    main: AnyWindowHandle,
    pub(in crate::ui) vcx: VisualTestContext,
}

/// One surface: how the user opens and closes it.
pub(in crate::ui) struct Surface {
    pub(in crate::ui) name: &'static str,
    pub(in crate::ui) open: fn(&mut Harness),
    pub(in crate::ui) close: fn(&mut Harness),
}

pub(in crate::ui) fn new_app(cx: &mut TestAppContext, demo: &'static str) -> Harness {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot
        .borrow()
        .clone()
        .expect("the window builder fills the slot");
    let main: AnyWindowHandle = handle.into();
    let vcx = VisualTestContext::from_window(main, cx);
    let mut h = Harness { app, main, vcx };
    // Some demos start on a Settings page.
    h.act(|app, _, _| app.close_settings());
    h.settle();
    h
}

impl Harness {
    /// One display frame: run what is ready, then for every open window
    /// (call, stream, chat and Picture-in-Picture windows too) run its
    /// `on_next_frame` callbacks and draw it, as the frame loop does,
    /// redrawing only what changed.
    fn frame(&mut self) {
        self.vcx.run_until_parked();
        let windows = self.vcx.update(|_, cx| cx.windows());
        for handle in windows {
            // A window that closed in the meantime has nothing to draw.
            let _ = self.vcx.cx.update_window(handle, |_, window, cx| {
                window.simulate_next_frame(cx);
                window.draw(cx).clear(cx);
            });
        }
        self.vcx.run_until_parked();
    }

    /// A frame, [`SETTLE`] of test clock, and a frame: what opening made
    /// is drawn (also what loaded after the first frame), and what
    /// closing let go of is gone.
    pub(in crate::ui) fn settle(&mut self) {
        self.frame();
        self.vcx.executor().advance_clock(SETTLE);
        self.frame();
    }

    /// Change the app as a click or key would; [`Harness::settle`] draws it.
    pub(in crate::ui) fn act(
        &mut self,
        f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
    ) {
        let app = self.app.clone();
        self.vcx
            .update(|window, cx| app.update(cx, |app, cx| f(app, window, cx)));
        self.vcx.run_until_parked();
    }

    /// Escape, as the user presses it.
    pub(in crate::ui) fn escape(&mut self) {
        self.vcx.simulate_keystrokes("escape");
    }

    fn other_windows(&mut self) -> Vec<AnyWindowHandle> {
        let main = self.main.window_id();
        self.vcx
            .update(|_, cx| cx.windows())
            .into_iter()
            .filter(|handle| handle.window_id() != main)
            .collect()
    }

    /// Close every window but the main one, as its close button does.
    fn close_other_windows(&mut self) {
        for handle in self.other_windows() {
            self.vcx.update(|_, cx| {
                let _ = cx.update_window(handle, |_, window, cx| {
                    window.dispatch_action(Box::new(CloseWindow), cx);
                });
            });
        }
        self.vcx.run_until_parked();
    }

    /// What shows over the chats: kit dialogs, the layers Escape closes,
    /// the panels and menus that close themselves, Settings and other
    /// windows.
    fn shown(&mut self) -> Vec<String> {
        let mut shown: Vec<String> = self.app.read_with(&self.vcx, |app, _| {
            let dialogs = QuillShell::open_dialog_names(app).into_iter();
            let layers = ESC_LAYERS
                .iter()
                .filter(|layer| (layer.is_open)(app))
                .map(|layer| layer.name);
            let search = app.session().map(|session| &session.search);
            let others = [
                ("media-panel", app.pickers.media_panel.open),
                ("send-menu", app.composer_ui.send_menu.open.is_some()),
                ("search", search.is_some_and(|s| s.search.open)),
                (
                    "date-jump",
                    search.is_some_and(|s| s.history_calendar.is_some()),
                ),
                ("edit-profile", app.dialogs.edit_profile_dialog.is_some()),
            ];
            let others = others
                .into_iter()
                .filter(|(_, open)| *open)
                .map(|(name, _)| name);
            let mut shown: Vec<String> = dialogs
                .chain(layers)
                .chain(others)
                .map(String::from)
                .collect();
            if app.settings.open {
                shown.push(format!("settings {:?}", app.settings.page));
            }
            shown
        });
        if self.vcx.update(|window, cx| window.has_active_dialog(cx)) {
            shown.push("kit-dialog".into());
        }
        for handle in self.other_windows() {
            shown.push(format!("window {:?}", handle.window_id()));
        }
        shown
    }

    /// What is alive once [`QUIET`] has passed.
    fn counts(&mut self) -> Counts {
        self.vcx.executor().advance_clock(QUIET);
        self.frame();
        // An update frees what was released since the last one only as it
        // ends, so the counting update would still see it.
        self.vcx.update(|_, _| {});
        self.vcx.update(|window, cx| {
            let mut counts: Counts = cx
                .live_entity_counts()
                .into_iter()
                .map(|(name, n)| (name.to_string(), n))
                .collect();
            for (kind, n) in cx.live_subscription_counts() {
                counts.insert(format!("subscriptions: app {kind}"), n);
            }
            for (kind, n) in window.live_subscription_counts() {
                counts.insert(format!("subscriptions: main window {kind}"), n);
            }
            for (at, n) in cx.foreground_executor().live_tasks() {
                let at = format!(
                    "task spawned at {}:{}:{}",
                    at.file(),
                    at.line(),
                    at.column()
                );
                *counts.entry(at).or_default() += n;
            }
            counts
        })
    }
}

/// What `after` holds more of than `before`.
fn growth(before: &Counts, after: &Counts) -> Vec<String> {
    after
        .iter()
        .filter_map(|(name, n)| {
            let was = before.get(name).copied().unwrap_or(0);
            (*n > was).then(|| format!("{name} +{}", n - was))
        })
        .collect()
}

/// Opens and closes each surface; returns what each one left behind.
pub(in crate::ui) fn check(h: &mut Harness, surfaces: &[Surface]) -> Vec<String> {
    let mut failures = Vec::new();
    for surface in surfaces {
        let idle = h.shown();
        // Whether the surface opened and closed again.
        let cycle = |h: &mut Harness| -> Result<(), String> {
            (surface.open)(h);
            h.settle();
            if h.shown() == idle {
                return Err(format!("{} did not open", surface.name));
            }
            (surface.close)(h);
            h.settle();
            let left = h.shown();
            if left != idle {
                return Err(format!("{} did not close: {left:?}", surface.name));
            }
            Ok(())
        };
        if let Err(failure) = cycle(h) {
            failures.push(failure);
            continue;
        }
        let warm = h.counts();
        if let Err(failure) = (0..CYCLES).try_for_each(|_| cycle(h)) {
            failures.push(failure);
            continue;
        }
        let grown = growth(&warm, &h.counts());
        if !grown.is_empty() {
            failures.push(format!(
                "{} kept after {CYCLES} opens: {}",
                surface.name,
                grown.join(", ")
            ));
        }
    }
    failures
}

fn add_photo(h: &mut Harness, id: i64) {
    h.act(|app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("chat A's history");
        let mut message = history.messages.values().next().expect("a message").clone();
        message.id = MessageId(id);
        message.content = MessageContent::Photo(PhotoContent {
            caption: "A photo".to_string(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "m".into(),
                width: 100,
                height: 100,
                file_id: FileId(id as i32),
            }],
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(id, message);
        cx.notify();
    });
}

const PHOTO: i64 = 8901;

const MAIN_WINDOW: &[Surface] = &[
    Surface {
        name: "emoji panel",
        open: |h| h.act(|app, _, cx| app.open_media_panel(PanelTab::Emoji, cx)),
        close: |h| {
            h.act(|app, _, cx| {
                app.close_media_panel(cx);
            })
        },
    },
    Surface {
        name: "sticker panel",
        open: |h| h.act(|app, _, cx| app.open_media_panel(PanelTab::Stickers, cx)),
        close: |h| {
            h.act(|app, _, cx| {
                app.close_media_panel(cx);
            })
        },
    },
    Surface {
        name: "GIF panel",
        open: |h| h.act(|app, _, cx| app.open_media_panel(PanelTab::Gifs, cx)),
        close: |h| {
            h.act(|app, _, cx| {
                app.close_media_panel(cx);
            })
        },
    },
    Surface {
        name: "reaction picker",
        open: |h| {
            h.act(|app, window, cx| {
                let at = point(px(500.), px(200.));
                app.open_reaction_selector(ChatId(11), MessageId(101), at, window, cx)
            })
        },
        close: |h| h.escape(),
    },
    Surface {
        name: "message menu",
        open: |h| {
            h.act(|app, _, cx| {
                app.message_ui.menu = Some(crate::ui::menu_states::MessageMenuState {
                    chat_id: ChatId(11),
                    message_id: MessageId(101),
                    position: point(px(500.), px(200.)),
                });
                cx.notify();
            })
        },
        close: |h| h.escape(),
    },
    Surface {
        name: "chat menu",
        open: |h| {
            h.act(|app, _, cx| {
                let at = point(px(40.), px(80.));
                app.chat_list.menu =
                    Some(crate::ui::menu_states::ChatMenuState::at(ChatId(11), at));
                cx.notify();
            })
        },
        close: |h| h.escape(),
    },
    Surface {
        name: "send menu",
        open: |h| {
            h.act(|app, window, cx| app.open_send_menu(point(px(900.), px(700.)), true, window, cx))
        },
        close: |h| h.act(|app, _, cx| app.close_send_menu(cx)),
    },
    Surface {
        name: "send effects",
        open: |h| {
            h.act(|app, window, cx| {
                app.open_send_menu(point(px(900.), px(700.)), true, window, cx);
                app.expand_send_effects(window, cx);
            })
        },
        close: |h| h.act(|app, _, cx| app.close_send_menu(cx)),
    },
    Surface {
        name: "search",
        open: |h| h.act(|app, window, cx| app.open_search_ui(window, cx)),
        close: |h| h.act(|app, window, cx| app.close_search_ui(window, cx)),
    },
    Surface {
        name: "search in chat",
        open: |h| h.act(|app, window, cx| app.open_chat_search_ui(window, cx)),
        close: |h| h.act(|app, window, cx| app.close_chat_search_ui(window, cx)),
    },
    Surface {
        name: "poll box",
        open: |h| h.act(|app, window, cx| app.open_poll_dialog(window, cx)),
        close: |h| h.act(|app, _, cx| app.close_poll_dialog(cx)),
    },
    Surface {
        name: "new group",
        open: |h| h.act(|app, window, cx| app.open_new_chat(NewChatKind::Group, window, cx)),
        close: |h| h.act(|app, _, cx| app.close_new_chat(cx)),
    },
    Surface {
        name: "new channel",
        open: |h| h.act(|app, window, cx| app.open_new_chat(NewChatKind::Channel, window, cx)),
        close: |h| h.act(|app, _, cx| app.close_new_chat(cx)),
    },
    Surface {
        name: "edit profile",
        open: |h| {
            h.act(|app, window, cx| {
                // The demo has no account user of its own.
                let session = app.demo_session.as_mut().expect("demo session");
                session.my_user_id.get_or_insert(777);
                app.open_edit_profile_dialog(window, cx)
            })
        },
        close: |h| h.act(|app, _, cx| app.close_edit_profile_dialog(cx)),
    },
    Surface {
        name: "forward box",
        open: |h| {
            h.act(|app, window, cx| {
                app.share.pending_forward =
                    quill::composer::ForwardDraft::from_message(ChatId(11), MessageId(101), false);
                app.open_forward_picker(window, cx);
            })
        },
        // Cancels the forward: the picker and the draft it was for.
        close: |h| h.act(|app, window, cx| app.clear_forward(window, cx)),
    },
    Surface {
        name: "chat theme box",
        open: |h| h.act(|app, _, cx| app.open_chat_look_dialog(11, cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "clear history box",
        open: |h| {
            h.act(|app, window, cx| {
                // The demo chat allows no clearing until it says so.
                let session = app.demo_session.as_mut().expect("demo session");
                let chat = session.chats.get_mut(&11).expect("chat A");
                chat.can_be_deleted_only_for_self = true;
                app.open_clear_history_box(ChatId(11), window, cx)
            })
        },
        close: |h| h.escape(),
    },
    Surface {
        name: "channel edit box",
        open: |h| h.act(|app, window, cx| app.open_group_edit_box(ChatId(13), window, cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "invite link box",
        open: |h| h.act(|app, window, cx| app.open_invite_link_box(ChatId(13), None, window, cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "emoji status picker",
        open: |h| h.act(|app, window, cx| app.open_emoji_status_picker(window, cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "jump to date",
        open: |h| h.act(|app, _, cx| app.open_jump_date_ui(cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "new folder",
        open: |h| h.act(|app, window, cx| app.open_folder_create(window, cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "shared media",
        open: |h| h.act(|app, _, cx| app.open_shared_media_ui(cx)),
        close: |h| h.escape(),
    },
    Surface {
        name: "user profile",
        open: |h| {
            h.act(|app, window, cx| {
                app.open_info_panel_target(InfoPanelTarget::User(11), window, cx)
            })
        },
        close: |h| h.act(|app, _, cx| app.close_info_panel(cx)),
    },
    Surface {
        name: "channel info",
        open: |h| {
            h.act(|app, window, cx| {
                app.open_info_panel_target(InfoPanelTarget::Supergroup(13), window, cx)
            })
        },
        close: |h| h.act(|app, _, cx| app.close_info_panel(cx)),
    },
    Surface {
        name: "profile from an avatar",
        open: |h| {
            h.act(|app, window, cx| {
                app.open_avatar_profile(MessageSender::User { user_id: 11 }, window, cx)
            })
        },
        close: |h| h.act(|app, _, cx| app.close_profile_modal(cx)),
    },
    Surface {
        name: "media viewer",
        open: |h| h.act(|app, _, cx| app.open_media_viewer(ChatId(11), MessageId(PHOTO), cx)),
        close: |h| h.act(|app, _, cx| app.close_media_viewer(cx)),
    },
    Surface {
        name: "chat window",
        open: |h| h.act(|app, window, cx| app.open_chat_in_new_window(ChatId(12), window, cx)),
        close: |h| h.close_other_windows(),
    },
];

#[gpui_kit::test]
fn closing_a_surface_of_the_main_window_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-chats");
    add_photo(&mut h, PHOTO);
    let failures = check(&mut h, MAIN_WINDOW);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A view the poll box below keeps by mistake.
struct KeptView;

/// A global nothing sets: observing it only registers the observer.
struct Unset;
impl gpui_kit::Global for Unset {}

thread_local! {
    static KEPT: std::cell::RefCell<Vec<Entity<KeptView>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The check names each kind of leak and each row that does not open or
/// close. The leaky row keeps a view in something that outlives the box,
/// detaches subscriptions onto the app (a global, quitting) and onto the
/// window, and detaches a task that never ends.
#[gpui_kit::test]
fn a_surface_that_keeps_what_it_made_is_named(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-chats");
    let failures = check(
        &mut h,
        &[
            Surface {
                name: "leaky poll box",
                open: |h| {
                    h.act(|app, window, cx| {
                        app.open_poll_dialog(window, cx);
                        let kept = cx.new(|_| KeptView);
                        KEPT.with(|kept_views| kept_views.borrow_mut().push(kept));
                        cx.observe_global::<Unset>(|_, _| {}).detach();
                        cx.on_app_quit(|_, _| async {}).detach();
                        cx.observe_window_activation(window, |_, _, _| {}).detach();
                        cx.spawn(async |_, _| std::future::pending::<()>().await)
                            .detach();
                    })
                },
                close: |h| h.act(|app, _, cx| app.close_poll_dialog(cx)),
            },
            Surface {
                name: "box that never opens",
                open: |_| {},
                close: |_| {},
            },
            Surface {
                name: "box that stays open",
                open: |h| h.act(|app, window, cx| app.open_poll_dialog(window, cx)),
                close: |_| {},
            },
        ],
    );
    KEPT.with(|kept_views| kept_views.borrow_mut().clear());
    let [leaky, never_opens, stays_open] = failures.as_slice() else {
        panic!("three failing rows: {failures:?}");
    };
    for kept in [
        "entity_leaks_tests::KeptView +3",
        "subscriptions: app global_observers +3",
        "subscriptions: app quit_observers +3",
        "subscriptions: main window activation_observers +3",
        "task spawned at src/ui/entity_leaks_tests.rs:",
    ] {
        assert!(leaky.contains(kept), "{leaky} names {kept}");
    }
    assert_eq!(never_opens, "box that never opens did not open");
    assert!(
        stays_open.starts_with("box that stays open did not close: [")
            && stays_open.contains(r#""poll-dialog""#),
        "{stays_open}"
    );
}

#[gpui_kit::test]
fn leaving_a_settings_page_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    // The System page asks `xdg-mime` for the `tg:` handler; the test
    // answers instead, so no process starts.
    struct PinnedLinkHandler;
    impl Drop for PinnedLinkHandler {
        fn drop(&mut self) {
            quill::link_handler::demo_pin(None);
        }
    }
    quill::link_handler::demo_pin(Some(quill::link_handler::LinkHandlerState::Unset));
    let _pinned = PinnedLinkHandler;
    let mut h = new_app(cx, "ready-chats");
    let mut failures = Vec::new();
    // One table row per page: `Surface` takes plain functions.
    macro_rules! pages {
        ($($page:ident),+ $(,)?) => {
            [$(Surface {
                name: concat!("settings: ", stringify!($page)),
                open: |h| {
                    h.act(|app, _, cx| {
                        app.open_settings(Some(Page::$page));
                        cx.notify();
                    });
                    let page = h.app.read_with(&h.vcx, |app, _| app.settings.page);
                    assert_eq!(page, Some(Page::$page), "the row opens its page");
                },
                close: |h| h.act(|app, _, cx| {
                    app.close_settings();
                    cx.notify();
                }),
            }),+]
        };
    }
    let pages = pages!(
        Account,
        Notifications,
        Privacy,
        Devices,
        AgentAccess,
        Chats,
        Appearance,
        Folders,
        DataStorage,
        Connection,
        Calls,
        Power,
        Language,
        Shortcuts,
        System,
        Telegram,
        About,
        Advanced,
    );
    assert_eq!(pages.len(), Page::ALL.len(), "every page has a row");
    failures.extend(check(&mut h, &pages));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[gpui_kit::test]
fn switching_between_chats_lets_go_of_the_chats_left(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-chats");
    let chats: Vec<ChatId> = h.app.read_with(&h.vcx, |app, _| {
        let session = app.session().expect("a signed-in demo");
        session.chats.keys().map(|id| ChatId(*id)).collect()
    });
    assert!(chats.len() >= 3, "the demo lists several chats");
    let walk = |h: &mut Harness, chats: &[ChatId]| {
        for chat in chats.iter().copied().chain([ChatId(11)]) {
            h.act(|app, window, cx| app.select_listed_chat(chat, window, cx));
            h.settle();
        }
    };
    walk(&mut h, &chats);
    let warm = h.counts();
    for _ in 0..CYCLES {
        walk(&mut h, &chats);
    }
    let grown = growth(&warm, &h.counts());
    assert!(grown.is_empty(), "kept: {}", grown.join(", "));
}

#[gpui_kit::test]
fn closing_the_story_viewer_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-story-viewer");
    // The demo starts with Leo's story open.
    h.act(|app, _, cx| app.close_story_viewer(cx));
    h.settle();
    let failures = check(
        &mut h,
        &[Surface {
            name: "story viewer",
            open: |h| {
                h.act(|app, _, cx| {
                    let scope = quill::story_viewer::StoryOpenScope::Tray;
                    app.open_story_viewer(ChatId(105), 3, scope, cx)
                })
            },
            close: |h| h.act(|app, _, cx| app.close_story_viewer(cx)),
        }],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[gpui_kit::test]
fn closing_the_call_window_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-video-call");
    h.act(|app, _, cx| app.sync_call_window(cx));
    let call = h.app.read_with(&h.vcx, |app, _| {
        let session = app.session().expect("a signed-in demo");
        session.calls.active_call.clone().expect("the demo's call")
    });
    // The call ends (its window closes) and starts again.
    thread_local! {
        static CALL: std::cell::RefCell<Option<quill::state::ActiveCall>> =
            const { std::cell::RefCell::new(None) };
    }
    CALL.with(|slot| *slot.borrow_mut() = Some(call));
    let end = |h: &mut Harness| {
        h.act(|app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.calls.active_call = None;
                session.calls.summary = None;
            }
            app.sync_call_window(cx);
        })
    };
    end(&mut h);
    h.settle();
    let failures = check(
        &mut h,
        &[Surface {
            name: "call window",
            open: |h| {
                h.act(|app, _, cx| {
                    if let Some(session) = app.demo_session.as_mut() {
                        session.calls.active_call = CALL.with(|slot| slot.borrow().clone());
                    }
                    app.calls.window_closed_by_user = None;
                    app.sync_call_window(cx);
                })
            },
            close: |h| {
                h.act(|app, _, cx| {
                    if let Some(session) = app.demo_session.as_mut() {
                        session.calls.active_call = None;
                        session.calls.summary = None;
                    }
                    app.sync_call_window(cx);
                })
            },
        }],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[gpui_kit::test]
fn closing_the_live_stream_window_lets_go_of_what_it_made(cx: &mut TestAppContext) {
    let mut h = new_app(cx, "ready-channels");
    let failures = check(
        &mut h,
        &[Surface {
            name: "live stream window",
            open: |h| {
                h.act(|app, _, cx| {
                    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                        app.demo_ui.sink.clone();
                    let session = app.demo_session.as_mut().expect("demo session");
                    crate::ui::group_call_stream_layout::demo::seed_rtmp_stream(
                        session,
                        quill::calls::engine::StreamState::Live,
                        &sink,
                        &app.demo_ui.seq,
                    );
                    app.group_call.window_closed_by_user = None;
                    cx.notify();
                })
            },
            close: |h| {
                h.act(|app, _, cx| {
                    app.leave_active_group_call(cx);
                    app.sync_group_call_window(cx);
                })
            },
        }],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
