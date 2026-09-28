#[cfg(feature = "ui")]
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().skip(1).any(|a| a == "--connect-smoke") {
        std::process::exit(quill::connect_smoke::cli_exit_code());
    }

    #[cfg(feature = "ui")]
    {
        ui_main(&args);
    }

    #[cfg(not(feature = "ui"))]
    {
        eprintln!("quill: UI not compiled. Pass --connect-smoke, or rebuild with --features ui.");
        std::process::exit(2);
    }
}

#[cfg(feature = "ui")]
fn ui_main(args: &[String]) {
    use gpui_kit::*;

    // Parity slice 5: drop leftover viewer frame caches from previous runs
    // (abandoned extractions, unclean exits) before anything re-creates them.
    quill::video::sweep_stale_viewer_frame_caches();

    if let Some(demo) = parse_screenshot_demo(args) {
        run_screenshot_demo(demo);
        return;
    }

    let credentials = quill::credentials::load();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            ui::bind_keys(cx);
            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(20.), px(20.)),
                            size: size(px(1200.), px(740.)),
                        })),
                        app_id: Some("org.shinycake.quill".into()),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Quill".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view = cx.new(|cx| ui::QuillApp::new(window, cx, credentials.clone()));
                        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                    },
                )
                .expect("failed to open window");
            })
            .detach();
        });
}

#[cfg(feature = "ui")]
fn parse_screenshot_demo(args: &[String]) -> Option<(ui::ScreenshotDemo, std::path::PathBuf)> {
    use std::path::PathBuf;
    use ui::ScreenshotDemo;

    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--screenshot-demo" {
            let kind = iter.next().map(String::as_str)?;
            let out = iter
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("docs/screenshots"));
            let demo = match kind {
                "need-tdjson" => ScreenshotDemo::NeedTdjson,
                "wait-phone" => ScreenshotDemo::WaitPhone,
                "wait-code" => ScreenshotDemo::WaitCode,
                "wait-password" => ScreenshotDemo::WaitPassword,
                "wait-qr" => ScreenshotDemo::WaitQr,
                "ready-chats" => ScreenshotDemo::ReadyChats,
                "ready-chats-composer" => ScreenshotDemo::ReadyChatsComposer,
                "ready-unread" => ScreenshotDemo::ReadyUnread,
                "ready-unread-read" => ScreenshotDemo::ReadyUnreadRead,
                "ready-media" => ScreenshotDemo::ReadyMedia,
                "ready-downloads" => ScreenshotDemo::ReadyDownloads,
                "ready-send-media" => ScreenshotDemo::ReadySendMedia,
                "ready-search" => ScreenshotDemo::ReadySearch,
                "ready-search-in-chat" => ScreenshotDemo::ReadySearchInChat,
                "ready-reply" => ScreenshotDemo::ReadyReply,
                "ready-edit-delete" => ScreenshotDemo::ReadyEditDelete,
                "ready-forward" => ScreenshotDemo::ReadyForward,
                "ready-reactions" => ScreenshotDemo::ReadyReactions,
                "ready-pin" => ScreenshotDemo::ReadyPin,
                "ready-mute-archive" => ScreenshotDemo::ReadyMuteArchive,
                "ready-chat-list" => ScreenshotDemo::ReadyChatListMenu,
                "ready-chat-list-2" => ScreenshotDemo::ReadyChatList,
                "ready-chat-list-3" => ScreenshotDemo::ReadyChatList3,
                "ready-chat-list-archive" => ScreenshotDemo::ReadyChatListArchive,
                "ready-chat-list-search" => ScreenshotDemo::ReadyChatListSearch,
                "ready-typing" => ScreenshotDemo::ReadyTyping,
                "ready-stickers" => ScreenshotDemo::ReadyStickers,
                "ready-voice" => ScreenshotDemo::ReadyVoice,
                "ready-link-preview" => ScreenshotDemo::ReadyLinkPreview,
                "ready-composer-preview" => ScreenshotDemo::ReadyComposerPreview,
                "ready-preview-cards" => ScreenshotDemo::ReadyPreviewCards,
                "ready-caption-position" => ScreenshotDemo::ReadyCaptionPosition,
                "ready-gifs" => ScreenshotDemo::ReadyGifs,
                "ready-video" => ScreenshotDemo::ReadyVideo,
                "ready-video-note" => ScreenshotDemo::ReadyVideoNote,
                "ready-audio" => ScreenshotDemo::ReadyAudio,
                "ready-video-send" => ScreenshotDemo::ReadyVideoSend,
                "ready-video-note-send" => ScreenshotDemo::ReadyVideoNoteSend,
                "ready-drafts" => ScreenshotDemo::ReadyDrafts,
                "ready-albums" => ScreenshotDemo::ReadyAlbums,
                "ready-sponsored" => ScreenshotDemo::ReadySponsored,
                "ready-channels" => ScreenshotDemo::ReadyChannels,
                "ready-channels-admin" => ScreenshotDemo::ReadyChannelsAdmin,
                "ready-channel-stats" => ScreenshotDemo::ReadyChannelStats,
                "ready-invite-links" => ScreenshotDemo::ReadyInviteLinks,
                "ready-admin-management" => ScreenshotDemo::ReadyAdminManagement,
                "ready-admin-log" => ScreenshotDemo::ReadyAdminLog,
                "ready-group-manage" => ScreenshotDemo::ReadyGroupManage,
                "ready-groups2" => ScreenshotDemo::ReadyGroups2,
                "ready-bot-chat" => ScreenshotDemo::ReadyBotChat,
                "ready-bot-keyboard" => ScreenshotDemo::ReadyBotKeyboard,
                "ready-bot-command-menu" => ScreenshotDemo::ReadyBotCommandMenu,
                "ready-bot-profile" => ScreenshotDemo::ReadyBotProfile,
                "ready-text-entities" => ScreenshotDemo::ReadyTextEntities,
                "ready-poll" => ScreenshotDemo::ReadyPoll,
                "ready-location" => ScreenshotDemo::ReadyLocation,
                "ready-dice" => ScreenshotDemo::ReadyDice,
                "ready-media-viewer" => ScreenshotDemo::ReadyMediaViewer,
                "ready-video-playback" => ScreenshotDemo::ReadyVideoPlayback,
                "ready-stories" => ScreenshotDemo::ReadyStories,
                "ready-story-post" => ScreenshotDemo::ReadyStoryPost,
                "ready-story-composer" => ScreenshotDemo::ReadyStoryComposer,
                "ready-seek-bars" => ScreenshotDemo::ReadySeekBars,
                "ready-forum-topics" => ScreenshotDemo::ReadyForumTopics,
                "ready-topic-post" => ScreenshotDemo::ReadyTopicPost,
                "ready-contacts" => ScreenshotDemo::ReadyContacts,
                "ready-folders" => ScreenshotDemo::ReadyFolders,
                "ready-folders-manage" => ScreenshotDemo::ReadyFoldersManage,
                "ready-chat-avatars" => ScreenshotDemo::ReadyChatAvatars,
                "ready-notification-sound" => ScreenshotDemo::ReadyNotificationSound,
                "ready-slow-mode" => ScreenshotDemo::ReadySlowMode,
                "ready-secret-chat" => ScreenshotDemo::ReadySecretChat,
                "ready-secret-picker" => ScreenshotDemo::ReadySecretPicker,
                "ready-secret-bot-alert" => ScreenshotDemo::ReadySecretBotAlert,
                "ready-storage-usage" => ScreenshotDemo::ReadyStorageUsage,
                "ready-2fa-manage" => ScreenshotDemo::Ready2faManage,
                "ready-recovery-email" => ScreenshotDemo::ReadyRecoveryEmail,
                "ready-key-verification" => ScreenshotDemo::ReadyKeyVerification,
                "ready-self-destruct" => ScreenshotDemo::ReadySelfDestruct,
                "ready-call" => ScreenshotDemo::ReadyCall,
                "ready-call-video" => ScreenshotDemo::ReadyCallVideo,
                "ready-call-devices" => ScreenshotDemo::ReadyCallDevices,
                "ready-call-reconnecting" => ScreenshotDemo::ReadyCallReconnecting,
                "ready-chat-ttl" => ScreenshotDemo::ReadyChatTtl,
                "ready-group-call" => ScreenshotDemo::ReadyGroupCall,
                "ready-group-call-invite" => ScreenshotDemo::ReadyGroupCallInvite,
                "ready-group-call-invitation" => ScreenshotDemo::ReadyGroupCallInvitation,
                "ready-group-call-manage" => ScreenshotDemo::ReadyGroupCallManage,
                "ready-group-call-scheduled" => ScreenshotDemo::ReadyGroupCallScheduled,
                "ready-calls-settings" => ScreenshotDemo::ReadyCallsSettings,
                "ready-rich-message" => ScreenshotDemo::ReadyRichMessage,
                "ready-rich-editor" => ScreenshotDemo::ReadyRichEditor,
                _ => {
                    eprintln!(
                        "unknown screenshot demo '{kind}' (expected need-tdjson|wait-phone|wait-code|wait-password|wait-qr|ready-chats|ready-chats-composer|ready-unread|ready-unread-read|ready-media|ready-downloads|ready-send-media|ready-search|ready-search-in-chat|ready-reply|ready-edit-delete|ready-forward|ready-reactions|ready-pin|ready-mute-archive|ready-chat-list|ready-typing|ready-stickers|ready-voice|ready-link-preview|ready-gifs|ready-video|ready-video-note|ready-video-send|ready-video-note-send|ready-drafts|ready-albums|ready-audio|ready-sponsored|ready-channels|ready-channels-admin|ready-channel-stats|ready-bot-chat|ready-bot-keyboard|ready-bot-command-menu|ready-bot-profile|ready-text-entities|ready-poll|ready-location|ready-dice|ready-media-viewer|ready-video-playback|ready-stories|ready-story-post|ready-story-composer|ready-seek-bars|ready-forum-topics|ready-topic-post|ready-contacts|ready-folders|ready-folders-manage|ready-chat-avatars|ready-notification-sound|ready-slow-mode|ready-secret-chat|ready-key-verification|ready-self-destruct|ready-call|ready-call-video|ready-call-devices|ready-chat-ttl|ready-group-call|ready-group-call-invite|ready-group-call-invitation|ready-group-call-manage|ready-calls-settings|ready-rich-message|ready-rich-editor|ready-admin-management|ready-admin-log|ready-secret-bot-alert|ready-storage-usage|ready-group-manage|ready-groups2|ready-2fa-manage|ready-recovery-email|ready-group-call-scheduled)"
                    );
                    std::process::exit(2);
                }
            };
            return Some((demo, out));
        }
    }
    None
}

/// Open a real GPUI window in the requested demo state, linger so an external
/// capture (ffmpeg x11grab) can snap docs/screenshots/*.png, then quit.
#[cfg(feature = "ui")]
fn run_screenshot_demo(demo: (ui::ScreenshotDemo, std::path::PathBuf)) {
    use gpui_kit::*;
    use std::time::Duration;
    use ui::ScreenshotDemo;

    let (kind, out_dir) = demo;
    let _ = std::fs::create_dir_all(&out_dir);
    let marker = out_dir.join(match kind {
        ScreenshotDemo::NeedTdjson => ".quill-ready-need-tdjson",
        ScreenshotDemo::WaitPhone => ".quill-ready-wait-phone",
        ScreenshotDemo::WaitCode => ".quill-ready-wait-code",
        ScreenshotDemo::WaitPassword => ".quill-ready-wait-password",
        ScreenshotDemo::WaitQr => ".quill-ready-wait-qr",
        ScreenshotDemo::ReadyChats => ".quill-ready-ready-chats",
        ScreenshotDemo::ReadyChatsComposer => ".quill-ready-ready-chats-composer",
        ScreenshotDemo::ReadyUnread => ".quill-ready-ready-unread",
        ScreenshotDemo::ReadyUnreadRead => ".quill-ready-ready-unread-read",
        ScreenshotDemo::ReadyMedia => ".quill-ready-ready-media",
        ScreenshotDemo::ReadyDownloads => ".quill-ready-ready-downloads",
        ScreenshotDemo::ReadySendMedia => ".quill-ready-ready-send-media",
        ScreenshotDemo::ReadySearch => ".quill-ready-ready-search",
        ScreenshotDemo::ReadySearchInChat => ".quill-ready-ready-search-in-chat",
        ScreenshotDemo::ReadyReply => ".quill-ready-ready-reply",
        ScreenshotDemo::ReadyEditDelete => ".quill-ready-ready-edit-delete",
        ScreenshotDemo::ReadyForward => ".quill-ready-ready-forward",
        ScreenshotDemo::ReadyReactions => ".quill-ready-ready-reactions",
        ScreenshotDemo::ReadyPin => ".quill-ready-ready-pin",
        ScreenshotDemo::ReadyMuteArchive => ".quill-ready-ready-mute-archive",
        ScreenshotDemo::ReadyChatListMenu => ".quill-ready-ready-chat-list",
        ScreenshotDemo::ReadyChatList => ".quill-ready-ready-chat-list-2",
        ScreenshotDemo::ReadyChatList3 => ".quill-ready-ready-chat-list-3",
        ScreenshotDemo::ReadyChatListArchive => ".quill-ready-ready-chat-list-archive",
        ScreenshotDemo::ReadyChatListSearch => ".quill-ready-ready-chat-list-search",
        ScreenshotDemo::ReadyTyping => ".quill-ready-ready-typing",
        ScreenshotDemo::ReadyStickers => ".quill-ready-ready-stickers",
        ScreenshotDemo::ReadyVoice => ".quill-ready-ready-voice",
        ScreenshotDemo::ReadyLinkPreview => ".quill-ready-ready-link-preview",
        ScreenshotDemo::ReadyComposerPreview => ".quill-ready-ready-composer-preview",
        ScreenshotDemo::ReadyPreviewCards => ".quill-ready-ready-preview-cards",
        ScreenshotDemo::ReadyCaptionPosition => ".quill-ready-ready-caption-position",
        ScreenshotDemo::ReadyGifs => ".quill-ready-ready-gifs",
        ScreenshotDemo::ReadyVideo => ".quill-ready-ready-video",
        ScreenshotDemo::ReadyVideoNote => ".quill-ready-ready-video-note",
        ScreenshotDemo::ReadyAudio => ".quill-ready-ready-audio",
        ScreenshotDemo::ReadyVideoSend => ".quill-ready-ready-video-send",
        ScreenshotDemo::ReadyVideoNoteSend => ".quill-ready-ready-video-note-send",
        ScreenshotDemo::ReadyDrafts => ".quill-ready-ready-drafts",
        ScreenshotDemo::ReadyAlbums => ".quill-ready-ready-albums",
        ScreenshotDemo::ReadySponsored => ".quill-ready-ready-sponsored",
        ScreenshotDemo::ReadyChannels => ".quill-ready-ready-channels",
        ScreenshotDemo::ReadyChannelsAdmin => ".quill-ready-ready-channels-admin",
        ScreenshotDemo::ReadyChannelStats => ".quill-ready-ready-channel-stats",
        ScreenshotDemo::ReadyInviteLinks => ".quill-ready-ready-invite-links",
        ScreenshotDemo::ReadyAdminManagement => ".quill-ready-ready-admin-management",
        ScreenshotDemo::ReadyAdminLog => ".quill-ready-ready-admin-log",
        ScreenshotDemo::ReadyGroups2 => ".quill-ready-ready-groups2",
        ScreenshotDemo::ReadyGroupManage => ".quill-ready-ready-group-manage",
        ScreenshotDemo::ReadyBotChat => ".quill-ready-ready-bot-chat",
        ScreenshotDemo::ReadyBotKeyboard => ".quill-ready-ready-bot-keyboard",
        ScreenshotDemo::ReadyBotCommandMenu => ".quill-ready-ready-bot-command-menu",
        ScreenshotDemo::ReadyBotProfile => ".quill-ready-ready-bot-profile",
        ScreenshotDemo::ReadyTextEntities => ".quill-ready-ready-text-entities",
        ScreenshotDemo::ReadyPoll => ".quill-ready-ready-poll",
        ScreenshotDemo::ReadyLocation => ".quill-ready-ready-location",
        ScreenshotDemo::ReadyDice => ".quill-ready-ready-dice",
        ScreenshotDemo::ReadyMediaViewer => ".quill-ready-ready-media-viewer",
        ScreenshotDemo::ReadyVideoPlayback => ".quill-ready-ready-video-playback",
        ScreenshotDemo::ReadyStories => ".quill-ready-ready-stories",
        ScreenshotDemo::ReadyStoryPost => ".quill-ready-ready-story-post",
        ScreenshotDemo::ReadyStoryComposer => ".quill-ready-ready-story-composer",
        ScreenshotDemo::ReadySeekBars => ".quill-ready-ready-seek-bars",
        ScreenshotDemo::ReadyForumTopics => ".quill-ready-ready-forum-topics",
        ScreenshotDemo::ReadyTopicPost => ".quill-ready-ready-topic-post",
        ScreenshotDemo::ReadyContacts => ".quill-ready-ready-contacts",
        ScreenshotDemo::ReadyFolders => ".quill-ready-ready-folders",
        ScreenshotDemo::ReadyFoldersManage => ".quill-ready-ready-folders-manage",
        ScreenshotDemo::ReadyChatAvatars => ".quill-ready-ready-chat-avatars",
        ScreenshotDemo::ReadyNotificationSound => ".quill-ready-ready-notification-sound",
        ScreenshotDemo::ReadySlowMode => ".quill-ready-ready-slow-mode",
        ScreenshotDemo::ReadySecretChat => ".quill-ready-ready-secret-chat",
        ScreenshotDemo::ReadySecretPicker => ".quill-ready-ready-secret-picker",
        ScreenshotDemo::ReadySecretBotAlert => ".quill-ready-ready-secret-bot-alert",
        ScreenshotDemo::ReadyStorageUsage => ".quill-ready-ready-storage-usage",
        ScreenshotDemo::Ready2faManage => ".quill-ready-ready-2fa-manage",
        ScreenshotDemo::ReadyRecoveryEmail => ".quill-ready-ready-recovery-email",
        ScreenshotDemo::ReadyKeyVerification => ".quill-ready-ready-key-verification",
        ScreenshotDemo::ReadySelfDestruct => ".quill-ready-ready-self-destruct",
        ScreenshotDemo::ReadyCall => ".quill-ready-ready-call",
        ScreenshotDemo::ReadyCallVideo => ".quill-ready-ready-call-video",
        ScreenshotDemo::ReadyCallDevices => ".quill-ready-ready-call-devices",
        ScreenshotDemo::ReadyCallReconnecting => ".quill-ready-ready-call-reconnecting",
        ScreenshotDemo::ReadyChatTtl => ".quill-ready-ready-chat-ttl",
        ScreenshotDemo::ReadyGroupCall => ".quill-ready-ready-group-call",
        ScreenshotDemo::ReadyGroupCallInvite => ".quill-ready-ready-group-call-invite",
        ScreenshotDemo::ReadyGroupCallInvitation => ".quill-ready-ready-group-call-invitation",
        ScreenshotDemo::ReadyGroupCallManage => ".quill-ready-ready-group-call-manage",
        ScreenshotDemo::ReadyGroupCallScheduled => ".quill-ready-ready-group-call-scheduled",
        ScreenshotDemo::ReadyCallsSettings => ".quill-ready-ready-calls-settings",
        ScreenshotDemo::ReadyRichMessage => ".quill-ready-ready-rich-message",
        ScreenshotDemo::ReadyRichEditor => ".quill-ready-ready-rich-editor",
    });
    let _ = std::fs::remove_file(&marker);
    let marker_for_spawn = marker.clone();

    eprintln!("quill screenshot-demo: {kind:?} → {}", out_dir.display());

    // Demo-only window size override (`QUILL_DEMO_WINDOW_SIZE=1200x1100`)
    // for slices whose fixture needs more vertical room than the default
    // 1200x740 (e.g. ready-location's four rows). Unset = unchanged, so
    // existing captures are unaffected.
    let (demo_w, demo_h) = std::env::var("QUILL_DEMO_WINDOW_SIZE")
        .ok()
        .and_then(|value| {
            let (w, h) = value.split_once('x')?;
            let w: f32 = w.parse().ok()?;
            let h: f32 = h.parse().ok()?;
            (w > 0.0 && h > 0.0).then_some((w, h))
        })
        .unwrap_or((1200.0, 740.0));

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            ui::bind_keys(cx);
            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(20.), px(20.)),
                            size: size(px(demo_w), px(demo_h)),
                        })),
                        app_id: Some("org.shinycake.quill".into()),
                        titlebar: Some(TitlebarOptions {
                            title: Some(if kind == ScreenshotDemo::ReadyCallDevices {
                                "Quill — Call audio devices".into()
                            } else {
                                "Quill".into()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view =
                            cx.new(|cx| ui::QuillApp::new_with_demo(window, cx, None, Some(kind)));
                        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                    },
                )
                .expect("failed to open screenshot demo window");

                // Allow a couple of frames to paint, then signal the capture script.
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                let _ = std::fs::write(&marker_for_spawn, b"ready\n");
                cx.background_executor()
                    .timer(Duration::from_millis(3500))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}
