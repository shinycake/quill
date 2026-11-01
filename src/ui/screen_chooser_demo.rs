//! Screenshot demo of screen sharing in a voice chat (no live Telegram, no
//! real capture): you share a window that is hidden, so your tile is
//! paused, and the chooser is open over four made-up sources.
//! `QUILL_DEMO_SCREEN_DENIED=1` shows the privacy box instead;
//! `QUILL_DEMO_SCREEN_AUDIO=1` adds Windows' "Share System Audio" box,
//! ticked for the running share. `QUILL_DEMO_CALL_CROWD=<people>` adds that
//! many listeners and opens the member list scrolled down.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::calls::demo_feed::{DemoFeed, demo_feed};
use super::screen_share_state::ScreenFor;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::capture::screen::{
    PreviewSlot, ScreenAccess, ScreenBackend, ScreenCapture, ScreenOptions, ScreenSource, SourceId,
    SourceKind, Thumbnail,
};
use quill::calls::capture::{CaptureError, FrameSink, VideoFrame as CaptureFrame};
use quill::calls::live_video::LiveKey;
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::{GroupCallVideoInfo, MessageSender, ParsedGroupCallParticipant};
use std::sync::Arc;

register_demos![
    DemoSpec::chats(
        "ready-call-screen-share",
        "screenshot demo — screen source chooser and a paused screen tile"
    )
    .setup(QuillApp::demo_call_screen_share)
    .timing(2500, 4000),
];

impl QuillApp {
    /// `QUILL_DEMO_THEME=high-contrast`: the call demos switch the app to
    /// the high-contrast theme (the generic demo prefs only know dark).
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn demo_high_contrast(&mut self, cx: &mut Context<Self>) {
        if std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast") {
            self.appearance.theme = quill::settings::ThemeChoice::HighContrast;
            self.apply_appearance(cx);
        }
    }
}

/// The shared window (hidden, so the share is paused).
const SHARED: &str = "window:42";

/// Four sources: two screens, a titled window and an untitled one.
pub(super) struct DemoScreens {
    pub(super) access: ScreenAccess,
    /// Act as the Wayland portal: no chooser (tests).
    pub(super) system_picker: bool,
}

impl DemoScreens {
    fn list() -> Vec<ScreenSource> {
        let source = |id: &str, kind, title: &str, size| ScreenSource {
            id: SourceId(id.into()),
            kind,
            title: title.into(),
            size,
        };
        vec![
            source("display:1", SourceKind::Screen, "", (3024, 1964)),
            source("display:2", SourceKind::Screen, "", (2560, 1440)),
            source(SHARED, SourceKind::Window, "Release notes", (1400, 900)),
            source("window:43", SourceKind::Window, "", (900, 700)),
        ]
    }
}

impl ScreenBackend for DemoScreens {
    fn system_picker(&self) -> bool {
        self.system_picker
    }

    fn access(&self) -> ScreenAccess {
        self.access
    }

    fn open_privacy_settings(&self) {}

    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError> {
        Ok(Self::list())
    }

    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
        let (w, h) = source.size;
        let scale = (max.0 as f32 / w as f32).min(max.1 as f32 / h as f32);
        let width = ((w as f32 * scale) as u32).max(2);
        let height = ((h as f32 * scale) as u32).max(2);
        let variant = Self::list()
            .iter()
            .position(|listed| listed.id == source.id)
            .unwrap_or(0);
        Some(Thumbnail {
            width,
            height,
            rgba: picture(width, height, variant),
        })
    }

    fn start(
        &self,
        _source: &SourceId,
        _options: ScreenOptions,
        _sink: FrameSink<CaptureFrame>,
        _preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        Err(CaptureError::Unsupported("No capture in the demo.".into()))
    }
}

/// A made-up desktop: a gradient wallpaper with a few window shapes.
pub(super) fn picture(width: u32, height: u32, variant: usize) -> Vec<u8> {
    let palettes: [([u8; 3], [u8; 3]); 4] = [
        ([0x1d, 0x4e, 0x89], [0x6a, 0x3d, 0x9a]),
        ([0x0f, 0x5c, 0x4f], [0x2b, 0x8a, 0x6b]),
        ([0xf4, 0xf1, 0xea], [0xe6, 0xe0, 0xd2]),
        ([0x2a, 0x2f, 0x36], [0x3b, 0x42, 0x4c]),
    ];
    let (top, bottom) = palettes
        .get(variant % palettes.len())
        .copied()
        .unwrap_or(([0, 0, 0], [0, 0, 0]));
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let t = y as f32 / height.max(1) as f32;
        for x in 0..width {
            let (fx, fy) = (x as f32 / width as f32, t);
            let mut color = [0u8; 3];
            for (channel, (a, b)) in color.iter_mut().zip(top.iter().zip(bottom.iter())) {
                *channel = (*a as f32 + (*b as f32 - *a as f32) * t) as u8;
            }
            // Screens get two windows; window sources get text lines.
            let window_a = (0.08..0.55).contains(&fx) && (0.12..0.7).contains(&fy);
            let window_b = (0.45..0.92).contains(&fx) && (0.3..0.88).contains(&fy);
            let line = (0.1..0.9).contains(&fx) && ((fy * 14.).fract() < 0.28) && fy > 0.15;
            let shade = match variant {
                0 | 1 if window_b => Some([0xf7, 0xf7, 0xf9]),
                0 | 1 if window_a => Some([0x24, 0x28, 0x2e]),
                2 if line => Some([0x9a, 0x95, 0x8a]),
                3 if line => Some([0x5f, 0x68, 0x73]),
                _ => None,
            };
            let [r, g, b] = shade.unwrap_or(color);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    rgba
}

/// How far down `QUILL_DEMO_CALL_CROWD` opens the member list.
const CROWD_SCROLL: f32 = 2000.;

/// One more person in a crowded call: a user can be in the list twice, and
/// the row number keeps the rows apart.
fn listener(user_id: i64, row: usize) -> ParsedGroupCallParticipant {
    ParsedGroupCallParticipant {
        audio_source_id: 5_000 + row as i32,
        is_speaking: false,
        video_enabled: false,
        video_info: None,
        screen_sharing_enabled: false,
        screen_sharing_video_info: None,
        ..participant(user_id, false)
    }
}

fn participant(user_id: i64, me: bool) -> ParsedGroupCallParticipant {
    ParsedGroupCallParticipant {
        participant_id: MessageSender::User { user_id },
        audio_source_id: 0,
        screen_sharing_audio_source_id: 0,
        is_current_user: me,
        is_speaking: !me,
        is_hand_raised: false,
        can_be_muted_for_all_users: false,
        can_be_unmuted_for_all_users: false,
        can_be_muted_for_current_user: false,
        can_be_unmuted_for_current_user: false,
        is_muted_for_all_users: false,
        is_muted_for_current_user: false,
        can_unmute_self: true,
        volume_level: 10000,
        order: String::new(),
        video_enabled: !me,
        screen_sharing_enabled: me,
        video_info: None,
        screen_sharing_video_info: me.then(|| GroupCallVideoInfo {
            endpoint_id: "screen".into(),
            is_paused: true,
            source_groups: Vec::new(),
        }),
    }
}

/// A tile's feed: one made-up desktop, sent at 30 fps when `repeat`.
fn feed(variant: usize, repeat: bool, cx: &mut App) -> Option<DemoFeed> {
    let (width, height) = (640u16, 360u16);
    let rgba = picture(u32::from(width), u32::from(height), variant);
    demo_feed(&rgba, width, height, repeat, cx)
}

impl QuillApp {
    fn demo_call_screen_share(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(feature = "demo-capture")]
        self.demo_high_contrast(cx);
        let denied = std::env::var_os("QUILL_DEMO_SCREEN_DENIED").is_some();
        let audio = std::env::var_os("QUILL_DEMO_SCREEN_AUDIO").is_some();
        let crowd = std::env::var("QUILL_DEMO_CALL_CROWD")
            .ok()
            .and_then(|people| people.parse::<usize>().ok())
            .unwrap_or(0);
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let mut users: Vec<i64> = session.users.keys().copied().collect();
        users.sort_unstable();
        let me = session
            .my_user_id
            .or_else(|| users.first().copied())
            .unwrap_or(1);
        let other = users.iter().copied().find(|id| *id != me).unwrap_or(me + 1);
        let mut participants = vec![participant(me, true), participant(other, false)];
        participants.extend(
            users
                .iter()
                .cycle()
                .take(crowd)
                .enumerate()
                .map(|(row, user)| listener(*user, row)),
        );
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_joined: true,
            screen_sharing: true,
            screen_share_paused: true,
            participants: std::sync::Arc::new(participants),
            ..ActiveGroupCall::fresh(7)
        });
        if crowd > 0 {
            self.group_call
                .members_scroll
                .set_offset(point(px(0.), px(-CROWD_SCROLL)));
        }
        // Your hidden window sent one frame before it paused; the other
        // camera keeps sending.
        let tiles = [
            (me, true, feed(2, false, cx)),
            (other, false, feed(1, true, cx)),
        ];
        for (user_id, screen, feed) in tiles {
            let key = LiveKey::Video {
                call_id: 7,
                user_id,
                screen,
            };
            if let Some(feed) = feed {
                self.demo_ui.group_feeds.insert(key, feed);
            }
        }
        self.screen_share.backend = Some(Arc::new(DemoScreens {
            system_picker: false,
            access: if denied {
                ScreenAccess::Denied
            } else {
                ScreenAccess::Granted
            },
        }));
        self.screen_share.demo_active = Some(SourceId(SHARED.into()));
        self.screen_share.demo_system_audio = audio;
        self.screen_share.demo_active_audio = audio;
        self.open_screen_chooser(ScreenFor::Group, cx);
        cx.notify();
    }
}
