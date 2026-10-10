//! Screenshot demos: stories.

use crate::ui::app::QuillApp;
use crate::ui::contacts::apply_ready_contacts;
use crate::ui::demo::demo_thumb_png_path;
use crate::ui::groups::{apply_ready_channels, apply_ready_channels_admin};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::sponsored::apply_ready_sponsored;
use crate::ui::story_composer::{apply_ready_story_edit, apply_ready_story_post};
use crate::ui::story_viewer::{apply_ready_stories, apply_ready_story_viewers};
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::story_composer::{StoryExpiry, StoryPrivacy};
use std::sync::atomic::Ordering;

register_demos![
    // Broadcast channel demo (injected, no live Telegram): the ungated demo
    // channel (id 13) renders broadcast posts with channel author + view
    // counts, composer hidden for the non-admin viewer, and the join/leave
    // footer.
    DemoSpec::chats(
        "ready-channels",
        "screenshot demo — broadcast channel posts + join footer"
    )
    .setup(QuillApp::demo_ready_channels),
    // Broadcast channel demo (injected, no live Telegram): the demo channel
    // (id 13) with the viewer as an administrator
    // (`rights.can_post_messages: true`), so the composer is visible above
    // the broadcast posts (Phase 2.3).
    DemoSpec::chats(
        "ready-channels-admin",
        "screenshot demo — broadcast channel, admin composer"
    )
    .setup(QuillApp::demo_ready_channels_admin),
    // Channel sponsored / recommended rows + report flow (injected, no live Telegram).
    // Fixture/proof surface only; the channel opens normally in live use.
    DemoSpec::chats(
        "ready-sponsored",
        "screenshot demo — sponsored / recommended channel rows"
    )
    .setup(QuillApp::demo_ready_sponsored),
    // Story viewer demo (injected, no live Telegram): the story tray above
    // the chat list for "Demo chat A"/"Demo chat B" plus the story viewer
    // overlay open on Demo chat A's downloaded photo story (Phase 9.1).
    DemoSpec::chats("ready-stories", "screenshot demo — story viewer")
        .setup(QuillApp::demo_ready_stories),
    // Phase 9.7: story albums / chat page / archive (injected, no live
    // Telegram) — the `ReadyStories` seed plus `storyAlbums`,
    // chat-page `stories` (one pinned) and archive `stories` for chat
    // 11, with the story page overlay open.
    DemoSpec::chats(
        "ready-story-albums",
        "screenshot demo — story albums / chat page / archive"
    )
    .setup(QuillApp::demo_ready_story_albums),
    // Phase 9.8: clickable story areas (injected, no live Telegram) —
    // a photo story carrying one of every `storyAreaType` (location,
    // venue, suggested reaction, message, link, weather, gift) with
    // the viewer open on it.
    DemoSpec::chats(
        "ready-story-areas",
        "screenshot demo — clickable story areas"
    )
    .setup(QuillApp::demo_ready_story_areas),
    // Phase 9.3: story posting composer (injected, no live Telegram) —
    // the `ReadyStories` fixture plus the composer overlay open: a
    // seeded photo path (the demo thumbnail), a caption draft, the
    // privacy selector on Close friends, and a seeded
    // `canPostStoryResultOk` so the status line shows "✓ Eligible to
    // post".
    DemoSpec::chats(
        "ready-story-composer",
        "screenshot demo — story posting composer"
    )
    .setup(QuillApp::demo_ready_story_composer),
    // Phase 9.5: story edit composer (injected, no live Telegram) —
    // the seeded own photo story (id 5, chat 11) carries
    // `can_be_edited`, `is_edited`, a `storyRepostInfo` public origin,
    // link + suggested-reaction areas, and close-friends privacy; the
    // composer opens in edit mode with caption + area inputs prefilled
    // and the media path empty (keep current content).
    DemoSpec::chats("ready-story-edit", "screenshot demo — story edit composer")
        .setup(QuillApp::demo_ready_story_edit),
    // B14: the story viewer's close-friends editor, hide and profile
    // actions (injected, no live Telegram).
    DemoSpec::chats("ready-story-more", "screenshot demo — story close friends")
        .setup(QuillApp::demo_ready_story_more),
    // Story posting / custom-reaction slice demo (injected, no live
    // Telegram): same seed as `ReadyStories`, but Demo chat A's photo
    // story carries a chosen ❤ reaction, interaction counts, and
    // deletable/repliable flags; the viewer opens with the **reaction
    // picker** and **reply row** visible, plus seeded `availableReactions`
    // (emoji + custom-emoji Premium tile with a local sticker thumb —
    // Phase 9.2 / `parity:stories-custom-reactions`). The demo keeps its
    // viewer-only shape — the posting composer is the `ReadyStoryComposer`
    // demo (Phase 9.3).
    DemoSpec::chats(
        "ready-story-post",
        "screenshot demo — story reactions / reply / delete"
    )
    .setup(QuillApp::demo_ready_story_post),
    // B14: a video story playing in the viewer (the generated 12 s demo
    // clip through the native player; no live Telegram).
    DemoSpec::chats(
        "ready-story-video",
        "screenshot demo — story video playback"
    )
    .setup(QuillApp::demo_ready_story_video),
    // Phase 9.5: story viewers list (injected, no live Telegram) — the
    // `ReadyStoryPost` fixture plus a seeded `getStoryInteractions`
    // response (two viewers, one with a ❤ reaction, one forward) with
    // the viewers panel open on the own story.
    DemoSpec::chats(
        "ready-story-viewers",
        "screenshot demo — story viewers list"
    )
    .setup(QuillApp::demo_ready_story_viewers),
];

impl QuillApp {
    fn demo_ready_channels(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — broadcast channel".into();
    }

    fn demo_ready_channels_admin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("admin post — hello from the channel", window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels_admin(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — broadcast channel admin".into();
    }

    fn demo_ready_sponsored(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_sponsored(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — sponsored messages".into();
    }

    fn demo_ready_stories(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_stories(session, &self.demo_sink, &self.demo_seq);
        }
        // Phase 9.1: the tray above the chat list shows the seeded
        // active stories for chats 11/12; the viewer opens on chat 11's
        // downloaded photo story.
        self.open_story_viewer(ChatId(11), 5, cx);
        self.status_note = "screenshot demo — story viewer".into();
    }

    fn demo_ready_story_albums(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase 9.7: seed the albums / chat-page / archive fixtures,
        // then open the story page on chat 11 (demo mode — the live
        // notice renders as the status note).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_story_albums(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_story_page(ChatId(11), window, cx);
        self.status_note = "screenshot demo — story albums / chat page / archive".into();
    }

    fn demo_ready_story_areas(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            story_areas::apply_ready_story_areas(session, &self.demo_sink, &self.demo_seq);
        }
        // Phase 9.8: viewer opens on the seeded photo story carrying
        // one of every `storyAreaType` — the fixture injected the real
        // `story` (with `areas`) through the reducer.
        self.open_story_viewer(ChatId(11), 5, cx);
        self.status_note = "screenshot demo — clickable story areas".into();
    }

    fn demo_ready_story_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase 9.3: the composer opens with a seeded photo path (the
        // demo thumbnail, so the preview renders), a caption draft,
        // and the privacy selector on Close friends. (open resets the
        // server-side round-trip state, so eligibility is seeded after.)
        self.open_story_composer(window, cx);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_stories(session, &self.demo_sink, &self.demo_seq);
            // Phase 9.3: the Saved Messages chat id `postStory` posts
            // to, and a seeded `canPostStoryResultOk` so the status
            // line shows "✓ Eligible to post".
            session.my_user_id = Some(777);
            session.story_post.eligibility =
                Some(quill::telegram::envelope::CanPostStoryResult::Ok { story_count: 0 });
        }
        self.stories.composer_path.update(cx, |input, cx| {
            input.set_value(demo_thumb_png_path(), window, cx);
        });
        self.stories.composer_caption.update(cx, |input, cx| {
            input.set_value("Posting my first story **from Quill**!", window, cx);
        });
        self.stories.composer.privacy = StoryPrivacy::CloseFriends;
        // Phase 9.4: seed the new options so the screenshot shows
        // them — 48h expiry, both toggles on, a link sticker URL and
        // reaction stickers.
        self.stories.composer.expiry = StoryExpiry::TwoDays;
        self.stories.composer.post_to_chat_page = true;
        self.stories.composer.protect_content = true;
        self.stories.composer.link_url = "https://t.me/quill".into();
        self.stories.composer.reaction_emojis = "❤️ 🔥".into();
        self.stories.composer_link.update(cx, |input, cx| {
            input.set_value("https://t.me/quill", window, cx);
        });
        self.stories.composer_reaction.update(cx, |input, cx| {
            input.set_value("❤️ 🔥", window, cx);
        });
        self.status_note = "screenshot demo — story posting composer".into();
    }

    fn demo_ready_story_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase 9.5: seed the editable own-story fixture, then open
        // the composer in edit mode — caption + area inputs prefill
        // from the cached story, the path stays empty.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_story_edit(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_story_edit(11, 5, window, cx);
        self.status_note = "screenshot demo — story edit composer".into();
    }

    fn demo_ready_story_more(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_contacts(session, &self.demo_sink, &self.demo_seq);
            session.open_info_panel = None;
            apply_ready_story_post(session, &self.demo_sink, &self.demo_seq);
            // `getCloseFriends` answered through the real reducer.
            let extra = session.request(quill::state::RequestPurpose::GetCloseFriends, None);
            let json = format!(
                r#"{{"@type":"users","@extra":"{}","total_count":2,"user_ids":[31,33]}}"#,
                extra.0,
            );
            let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_sink.clone();
            if let Some(owned) =
                quill::telegram::client::copy_and_parse(&json, &self.demo_seq, &dyn_sink)
            {
                session.apply(owned);
            }
        }
        // B14: own story 5 with the close-friends editor open.
        self.open_story_viewer(ChatId(11), 5, cx);
        self.open_close_friends_editor(window, cx);
        self.status_note = "screenshot demo — story close friends".into();
    }

    fn demo_ready_story_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_story_post(session, &self.demo_sink, &self.demo_seq);
        }
        // Phase 9.2 / stories-custom-reactions: viewer opens on the
        // seeded own photo story with the reaction picker and the
        // reply row visible, seeded `availableReactions` (emoji +
        // custom-emoji Premium tile with a local sticker thumb), and
        // a chosen ❤ reaction. The composer isn't opened here — it
        // has its own `ReadyStoryComposer` demo (Phase 9.3).
        self.open_story_viewer(ChatId(11), 5, cx);
        self.stories.reaction_picker_open = true;
        self.stories.reply_open = true;
        self.stories.reply_input.update(cx, |input, cx| {
            input.set_value("Great photo!", window, cx);
        });
        self.status_note = "screenshot demo — story reactions / reply / delete".into();
    }

    fn demo_ready_story_video(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_stories(session, &self.demo_sink, &self.demo_seq);
        }
        // B14: the viewer opens on chat 11's video story (story 4) and
        // plays the generated 12 s clip with the native player.
        self.open_story_viewer(ChatId(11), 4, cx);
        self.status_note = "screenshot demo — story video playback".into();
    }

    fn demo_ready_story_viewers(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_story_viewers(session, &self.demo_sink, &self.demo_seq);
        }
        // Phase 9.5: viewer opens on the seeded own photo story with
        // the viewers panel open — the fixture injected a real
        // `storyInteractions` page through the reducer.
        self.open_story_viewer(ChatId(11), 5, cx);
        self.stories.viewers_open = true;
        self.status_note = "screenshot demo — story viewers list".into();
    }
}
