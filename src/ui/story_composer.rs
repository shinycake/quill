//! story composer overlay.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{StoryPostOutcome, StoryPostState};
use quill::story_composer::{StoryComposer, StoryExpiry, StoryMediaKind, StoryPrivacy};
use quill::telegram::requests::input_story_content;

impl QuillApp {
    /// Phase 9.3: open the story composer (tray "+" tile). Resets the
    /// server-side round-trip state so a reopened composer doesn't show
    /// the previous post's outcome. Phase 9.5: also fetches
    /// `getChatsToPostStories` for the "post as" picker.
    pub(super) fn open_story_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stories.composer = StoryComposer::open();
        self.stories
            .composer_path
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_caption
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_user_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        // Phase 9.4: reset the area inputs too.
        self.stories
            .composer_link
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_reaction
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(live) = self.live.as_mut() {
            live.driver.session.story_post = StoryPostState::default();
            // Phase 9.5: eligible "post as" chats (channels/supergroups).
            let _ = live.driver.get_chats_to_post_stories();
        }
        if let Some(session) = self.demo_session.as_mut() {
            session.story_post = StoryPostState::default();
        }
        cx.notify();
    }

    /// Phase 9.5: open the composer in edit mode for a posted story.
    /// Caption + area inputs are prefilled from the cached story; the
    /// media path stays empty (keep current content unless replaced).
    pub(super) fn open_story_edit(
        &mut self,
        poster_chat_id: i64,
        story_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stories.composer = StoryComposer::open_edit(poster_chat_id, story_id);
        let (caption, link_url, reactions) = self
            .session()
            .and_then(|s| s.stories.get(&(poster_chat_id, story_id)))
            .map(|story| {
                (
                    story.caption.clone(),
                    story.area_link_url.clone().unwrap_or_default(),
                    story.area_reaction_emojis.join(" "),
                )
            })
            .unwrap_or_default();
        self.stories
            .composer_path
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_caption
            .update(cx, |input, cx| input.set_value(&caption, window, cx));
        self.stories
            .composer_link
            .update(cx, |input, cx| input.set_value(&link_url, window, cx));
        self.stories
            .composer_reaction
            .update(cx, |input, cx| input.set_value(&reactions, window, cx));
        cx.notify();
    }

    /// Phase 9.5: open the composer in repost mode for a source story.
    /// The new post carries `postStory.from_story_full_id`.
    pub(super) fn open_story_repost(
        &mut self,
        poster_chat_id: i64,
        story_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stories.composer = StoryComposer::open_repost(poster_chat_id, story_id);
        self.stories
            .composer_path
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_caption
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_user_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_link
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stories
            .composer_reaction
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(live) = self.live.as_mut() {
            live.driver.session.story_post = StoryPostState::default();
            let _ = live.driver.get_chats_to_post_stories();
        }
        if let Some(session) = self.demo_session.as_mut() {
            session.story_post = StoryPostState::default();
        }
        cx.notify();
    }

    /// Phase 9.3: close the story composer (Escape / backdrop / Close).
    pub(super) fn close_story_composer(&mut self, cx: &mut Context<Self>) {
        self.stories.composer.close();
        cx.notify();
    }

    /// Phase 9.3: Post pressed — validate the path, then run the
    /// `canPostStory` eligibility check. The render tick converts the
    /// answer into a `postStory` or an ineligible reason
    /// (`story_composer_after_check`). Phase 9.5: in edit mode the
    /// media path is optional (empty = keep content) and the save goes
    /// straight to `editStory` — no eligibility check.
    pub(super) fn story_composer_post(&mut self, cx: &mut Context<Self>) {
        // Phase 9.3: `postStory` already sent, answer not yet landed —
        // the button is disabled while `busy`; this guards a
        // stale-snapshot race from re-running canPostStory→postStory.
        if self.stories.composer.post_sent || self.stories.composer.save_sent {
            return;
        }
        let path = self
            .stories
            .composer_path
            .read(cx)
            .value()
            .trim()
            .to_string();
        let kind = StoryMediaKind::detect(&path);
        // Phase 9.4: sync the area inputs into composer state before
        // validating — the link URL check runs up front so a typo
        // surfaces locally instead of failing the post.
        self.stories.composer.link_url = self
            .stories
            .composer_link
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.stories.composer.reaction_emojis = self
            .stories
            .composer_reaction
            .read(cx)
            .value()
            .trim()
            .to_string();
        let is_edit = self.stories.composer.is_edit();
        let error = if !is_edit && path.is_empty() {
            Some("Enter a photo or video file path")
        } else if !path.is_empty() && kind == StoryMediaKind::Unknown {
            Some("Not a photo or video file — check the extension")
        } else if !path.is_empty() && !std::path::Path::new(&path).is_file() {
            Some("File not found — check the path")
        } else if !is_edit && self.stories.composer.needs_users() {
            Some("Pick at least one user for \"Selected users\"")
        } else {
            self.stories.composer.link_url_error()
        };
        if let Some(error) = error {
            self.stories.composer.local_error = Some(error.into());
            cx.notify();
            return;
        }
        if is_edit {
            self.story_composer_save_edit(cx, &path, kind);
            return;
        }
        // Phase 9.5: the check runs on the "post as" chat — own stories
        // when `as_chat_id` is `None`. Computed before the mutable
        // `live` borrow below.
        let target = self.story_composer_target_chat();
        if let Some(live) = self.live.as_mut() {
            self.stories.composer.local_error = None;
            self.stories.composer.check_sent = true;
            live.driver.session.story_post.check_error = None;
            live.driver.session.story_post.eligibility = None;
            live.driver.session.story_post.outcome = StoryPostOutcome::None;
            if live.driver.check_can_post_story(target).is_err() {
                self.stories.composer.check_sent = false;
                self.stories.composer.local_error =
                    Some("Could not check posting eligibility".into());
            }
        } else if self.demo_session.is_some() {
            self.stories.composer.local_error = Some("demo — posting runs with live TDLib".into());
        }
        cx.notify();
    }

    /// Phase 9.5: the chat the current composer post targets — the
    /// "post as" channel/supergroup, or the user's own story chat.
    pub(super) fn story_composer_target_chat(&self) -> ChatId {
        let me = self.session().and_then(|s| s.my_user_id).unwrap_or(0);
        self.stories
            .composer
            .as_chat_id
            .map(ChatId)
            .unwrap_or(ChatId(me))
    }

    /// Phase 9.5: Save pressed in edit mode — send `editStory` with
    /// `null` for every unchanged field (media empty = keep; areas
    /// only change together with new media, per `td_api.tl:13732`).
    pub(super) fn story_composer_save_edit(
        &mut self,
        cx: &mut Context<Self>,
        path: &str,
        kind: StoryMediaKind,
    ) {
        let Some((poster_chat_id, story_id)) = self.stories.composer.edit_target else {
            return;
        };
        let caption = self.stories.composer_caption.read(cx).value().to_string();
        let content = (!path.is_empty()).then(|| input_story_content(kind, path));
        // Areas can't be edited unless the content changes — keep them
        // untouched when the media is kept.
        let areas = content.as_ref().map(|_| self.stories.composer.areas_json());
        let Some(live) = self.live.as_mut() else {
            self.stories.composer.local_error = Some("demo — editing runs with live TDLib".into());
            cx.notify();
            return;
        };
        self.stories.composer.local_error = None;
        match live.driver.edit_story(
            ChatId(poster_chat_id),
            story_id,
            content,
            areas,
            Some(&caption),
        ) {
            Ok(_) => self.stories.composer.save_sent = true,
            Err(_) => {
                self.stories.composer.local_error = Some("Could not send the edit request".into());
            }
        }
        cx.notify();
    }

    /// Phase 9.3: the `canPostStory` answer landed (render tick) — post
    /// when eligible, otherwise surface the reason in the composer.
    /// Phase 9.5: posts to the "post as" target chat and carries the
    /// repost source (`from_story_full_id`).
    pub(super) fn story_composer_after_check(&mut self, cx: &mut Context<Self>) {
        let (eligibility, check_error) = match self.session() {
            Some(session) => (
                session.story_post.eligibility.clone(),
                session.story_post.check_error.clone(),
            ),
            None => (None, None),
        };
        if let Some(error) = check_error {
            self.stories.composer.local_error = Some(error);
        } else if let Some(result) = eligibility {
            if !result.can_post() {
                self.stories.composer.local_error = Some(result.user_message());
            } else {
                // Phase 9.5: the target chat matches the eligibility
                // check (the picker is disabled while `check_sent`).
                // Hoisted before the mutable `live` borrow below.
                let target = self.story_composer_target_chat();
                let from_story = self.stories.composer.repost_source;
                if let Some(live) = self.live.as_mut() {
                    let path = self
                        .stories
                        .composer_path
                        .read(cx)
                        .value()
                        .trim()
                        .to_string();
                    let caption = self.stories.composer_caption.read(cx).value().to_string();
                    let kind = StoryMediaKind::detect(&path);
                    let privacy = self.stories.composer.privacy;
                    let user_ids = self.stories.composer.selected_user_ids.clone();
                    // Phase 9.4: areas + options are read from composer state
                    // (synced in `story_composer_post`), so what lands in the
                    // `postStory` JSON is exactly what the UI showed.
                    let areas = self.stories.composer.areas_json();
                    let active_period = self.stories.composer.expiry.seconds();
                    let is_posted_to_chat_page = self.stories.composer.post_to_chat_page;
                    let protect_content = self.stories.composer.protect_content;
                    match live.driver.post_story(
                        target,
                        kind,
                        &path,
                        &caption,
                        privacy,
                        &user_ids,
                        areas,
                        active_period,
                        from_story,
                        is_posted_to_chat_page,
                        protect_content,
                    ) {
                        Ok(_) => {
                            self.stories.composer.local_error = None;
                            // Phase 9.3: request sent, answer not yet landed —
                            // the Post button stays disabled (busy) until the
                            // outcome moves, so a second press can't post a
                            // duplicate story.
                            self.stories.composer.post_sent = true;
                            // Fresh eligibility for the next post.
                            live.driver.session.story_post.eligibility = None;
                        }
                        Err(_) => {
                            self.stories.composer.local_error =
                                Some("Could not send the post request".into());
                        }
                    }
                } else {
                    self.stories.composer.local_error =
                        Some("demo — posting runs with live TDLib".into());
                }
            }
        }
        cx.notify();
    }

    /// Phase 9.3: one status line for the composer — local validation
    /// errors first, then the server-side pending / succeeded / failed
    /// outcome, then the eligibility check state.
    pub(super) fn story_composer_status(&self) -> Option<String> {
        let composer = &self.stories.composer;
        if let Some(error) = &composer.local_error {
            return Some(format!("✗ {error}"));
        }
        // Phase 9.5: edit-mode save state — the tick closes the
        // composer on success or moves the failure into local_error.
        if composer.is_edit() {
            return composer.save_sent.then(|| "Saving…".into());
        }
        let post = self.session().map(|session| session.story_post.clone())?;
        match &post.outcome {
            StoryPostOutcome::Posting { .. } => Some("Posting…".into()),
            StoryPostOutcome::Succeeded => {
                Some("✓ Posted — it will appear in your story tray".into())
            }
            StoryPostOutcome::Failed(message) => Some(format!("✗ {message}")),
            StoryPostOutcome::None => {
                if composer.post_sent {
                    Some("Sending…".into())
                } else if composer.check_sent {
                    Some("Checking eligibility…".into())
                } else if let Some(error) = &post.check_error {
                    Some(format!("✗ {error}"))
                } else {
                    post.eligibility.as_ref().map(|result| {
                        if result.can_post() {
                            "✓ Eligible to post".into()
                        } else {
                            format!("✗ {}", result.user_message())
                        }
                    })
                }
            }
        }
    }

    /// Phase 9.3: toggle a contact in the composer's "Selected users" set.
    pub(super) fn toggle_story_composer_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        self.stories.composer.toggle_user(user_id);
        cx.notify();
    }

    /// Phase 9.3: the story composer overlay — path entry (no native file
    /// picker yet), photo preview, caption, the 4-way privacy selector
    /// with a contact picker for "Selected users", and the Post button.
    /// `canPostStory` is checked on every Post press; the status line
    /// shows the honest pending / succeeded / failed states.
    pub(super) fn story_composer_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .stories
            .composer_path
            .read(cx)
            .value()
            .trim()
            .to_string();
        let kind = StoryMediaKind::detect(&path);
        let file_exists = !path.is_empty() && std::path::Path::new(&path).is_file();
        // Phase 9.5: edit / repost modes reshape the composer — edit
        // hides privacy + expiry + toggles (`editStory` has no such
        // fields); the areas are only editable when the media is being
        // replaced (schema: areas can't change unless content does).
        let is_edit = self.stories.composer.is_edit();
        let is_repost = self.stories.composer.repost_source.is_some();
        let show_privacy = !is_edit && self.stories.composer.as_chat_id.is_none();
        let areas_editable = !is_edit || !path.is_empty();
        let title = if is_edit {
            "Edit story"
        } else if is_repost {
            "Repost story"
        } else {
            "New story"
        };

        let preview: AnyElement = match (kind, file_exists) {
            (StoryMediaKind::Photo, true) => img(std::path::Path::new(&path))
                .id("story-composer-preview")
                .w(px(180.))
                .h(px(240.))
                .aspect_ratio(px(180.) / px(240.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .bg(bg_deep())
                .with_fallback(|| {
                    div()
                        .w(px(180.))
                        .h(px(240.))
                        .rounded_md()
                        .bg(bg_deep())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(text_bright())
                        .child("could not render")
                        .into_any_element()
                })
                .into_any_element(),
            (StoryMediaKind::Video, true) => div()
                .id("story-composer-preview")
                .w(px(180.))
                .h(px(240.))
                .rounded_md()
                .bg(bg_deep())
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(text_bright())
                .child("Video — uploads the full file")
                .into_any_element(),
            _ => div()
                .id("story-composer-preview")
                .w(px(180.))
                .h(px(240.))
                .rounded_md()
                .bg(bg_deep())
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(text_muted())
                .child("Photo/video preview")
                .into_any_element(),
        };

        // Phase 6: kit RadioGroup (was: buttons with a ☑/☐ prefix).
        // Controlled: the chosen index writes the value.
        let composer_privacy_selected = StoryPrivacy::ALL
            .iter()
            .position(|option| self.stories.composer.privacy == *option);
        let privacy = div().flex().flex_col().gap_1().child(
            RadioGroup::vertical("story-composer-privacy")
                .selected_index(composer_privacy_selected)
                .children(StoryPrivacy::ALL.iter().map(|option| {
                    Radio::new(format!("story-composer-privacy-{}", option.label()))
                        .label(option.label())
                }))
                .on_click(cx.listener(move |this, &ix, _, cx| {
                    this.stories.composer.privacy = StoryPrivacy::ALL[ix];
                    this.stories.composer.local_error = None;
                    cx.notify();
                })),
        );

        let users_picker: Option<AnyElement> =
            (self.stories.composer.privacy == StoryPrivacy::SelectedUsers).then(|| {
                let query = self.stories.composer_user_search.read(cx).value();
                let rows = self.g1_contact_rows(&query, cx);
                let selected = self.stories.composer.selected_user_ids.clone();
                let mut list = div()
                    .id("story-composer-users")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(160.))
                    .overflow_y_scroll();
                if rows.is_empty() {
                    list = list.child(
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child("No contacts found"),
                    );
                }
                for row in rows.iter().take(50) {
                    list = list.child(self.g1_contact_checkbox(
                        "story-composer".to_string(),
                        row,
                        selected.contains(&row.user_id),
                        row.user_id,
                        cx,
                    ));
                }
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        Textarea::new(&self.stories.composer_user_search)
                            .aria_label("Search story recipients")
                            .h(px(32.)),
                    )
                    .child(list)
                    .into_any_element()
            });

        // Phase 6: kit RadioGroup (was: buttons with a ☑/☐ prefix).
        // Controlled: the chosen index writes the value.
        let composer_expiry_selected = StoryExpiry::ALL
            .iter()
            .position(|option| self.stories.composer.expiry == *option);
        let expiry = div().flex().flex_col().gap_1().child(
            RadioGroup::horizontal("story-composer-expiry")
                .selected_index(composer_expiry_selected)
                .children(StoryExpiry::ALL.iter().map(|option| {
                    Radio::new(format!("story-composer-expiry-{}", option.label()))
                        .label(option.label())
                }))
                .on_click(cx.listener(move |this, &ix, _, cx| {
                    this.stories.composer.expiry = StoryExpiry::ALL[ix];
                    this.stories.composer.local_error = None;
                    cx.notify();
                })),
        );

        let toggles = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                // Phase 6: kit Checkboxes (were: ghost buttons with ☑/☐
                // labels). Controlled: write the requested value, keep
                // clearing local_error on change.
                Checkbox::new("story-composer-post-to-chat-page")
                    .label("Post to chat page (keep accessible after expiry)")
                    .checked(self.stories.composer.post_to_chat_page)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.stories.composer.post_to_chat_page = on;
                        this.stories.composer.local_error = None;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("story-composer-protect-content")
                    .label("Protect content (no forwarding)")
                    .checked(self.stories.composer.protect_content)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.stories.composer.protect_content = on;
                        this.stories.composer.local_error = None;
                        cx.notify();
                    })),
            );

        let status = self.story_composer_status();
        let busy = self.stories.composer.check_sent
            || self.stories.composer.post_sent
            || self.stories.composer.save_sent
            || self.session().is_some_and(|session| {
                matches!(session.story_post.outcome, StoryPostOutcome::Posting { .. })
            });

        // Phase 9.5: "Post as" picker — the user's own stories plus the
        // channels/supergroups from `getChatsToPostStories`. Disabled
        // while a check/post is in flight so the eligibility answer
        // always matches the target chat.
        let post_as: Option<AnyElement> = (!is_edit).then(|| {
            let as_chats: Vec<(i64, String)> = self
                .session()
                .map(|session| {
                    session
                        .story_post_as_chats
                        .iter()
                        .map(|id| {
                            let title = session
                                .chats
                                .get(id)
                                .map(|chat| chat.title.clone())
                                .unwrap_or_else(|| format!("Chat {id}"));
                            (*id, title)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Phase 6: kit RadioGroup (was: buttons with a ☑/☐ prefix).
            // Controlled: the chosen index writes the value. Index 0 is
            // "Myself", the rest are the eligible chats.
            let as_selected: Option<usize> = match self.stories.composer.as_chat_id {
                None => Some(0),
                Some(chat_id) => as_chats
                    .iter()
                    .position(|(id, _)| *id == chat_id)
                    .map(|ix| ix + 1),
            };
            let picker = RadioGroup::vertical("story-composer-as")
                .selected_index(as_selected)
                .disabled(busy)
                .children(
                    std::iter::once(Radio::new("story-composer-as-self").label("Myself")).chain(
                        as_chats.iter().map(|(chat_id, title)| {
                            Radio::new(format!("story-composer-as-{chat_id}")).label(title.clone())
                        }),
                    ),
                )
                .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                    this.stories.composer.as_chat_id = if ix == 0 {
                        None
                    } else {
                        Some(as_chats[ix - 1].0)
                    };
                    this.stories.composer.local_error = None;
                    cx.notify();
                }));
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(text_muted()).child("Post as"))
                .child(picker)
                .into_any_element()
        });

        div()
            .id("story-composer-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("story-composer-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(scrim())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_composer(cx);
                    })),
            )
            .child(
                div()
                    .id("story-composer-panel")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .max_w(px(480.))
                    .max_h_full()
                    // Phase 9.4: the options section outgrew the window —
                    // scroll instead of clipping the Post button.
                    .overflow_y_scroll()
                    .rounded_lg()
                    .bg(bg_canvas())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_semibold().text_color(text_bright()).child(title))
                            .child(
                                div()
                                    .id("story-composer-close")
                                    .role(gpui_kit::Role::Button)
                                    .aria_label("Close story composer")
                                    .tab_index(0)
                                    .cursor_pointer()
                                    .pressable(cx.theme())
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .text_color(text_bright())
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_story_composer(cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_2()
                            .child(preview)
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(text_muted())
                                            // Phase 9.5: in edit mode the
                                            // path is optional — empty
                                            // keeps the current media.
                                            .child(if is_edit {
                                                "Replace media (optional — empty keeps the current)"
                                                    .to_string()
                                            } else {
                                                format!("{} file", kind.label())
                                            }),
                                    )
                                    .child(
                                        Textarea::new(&self.stories.composer_path)
                                            .aria_label("Story media file path")
                                            .h(px(40.)),
                                    )
                                    .child(
                                        div().text_xs().text_color(text_muted()).child("Caption"),
                                    )
                                    .child(
                                        Textarea::new(&self.stories.composer_caption)
                                            .aria_label("Story caption")
                                            .h(px(64.)),
                                    ),
                            ),
                    )
                    // Phase 9.5: the "post as" picker (new posts and
                    // reposts only); privacy is hidden in edit mode and
                    // for channel/supergroup posts (server-ignored there
                    // — `postStory`, td_api.tl:13715).
                    .when_some(post_as, |this, picker| this.child(picker))
                    .when(show_privacy, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Who can see it"),
                        )
                        .child(privacy)
                    })
                    .when_some(users_picker.filter(|_| show_privacy), |this, picker| {
                        this.child(picker)
                    })
                    // Phase 9.4: expiry, areas (link + suggested-reaction
                    // stickers), and the post-to-chat-page /
                    // protect-content toggles. Phase 9.5: expiry and the
                    // toggles have no `editStory` fields — hidden in edit
                    // mode; areas only apply when the media is replaced.
                    .when(!is_edit, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Expires after"),
                        )
                        .child(expiry)
                    })
                    .when(areas_editable, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Story stickers"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Link sticker URL"),
                        )
                        .child(
                            Textarea::new(&self.stories.composer_link)
                                .aria_label("Story link")
                                .h(px(32.)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .child("Reaction stickers (emoji, space-separated)"),
                        )
                        .child(
                            Textarea::new(&self.stories.composer_reaction)
                                .aria_label("Story reaction emoji")
                                .h(px(32.)),
                        )
                    })
                    .when(!is_edit, |this| {
                        this.child(div().text_xs().text_color(text_muted()).child("Options"))
                            .child(toggles)
                    })
                    .when_some(status, |this, status| {
                        this.child(div().text_sm().text_color(text_bright()).child(status))
                    })
                    .child(
                        Button::new("story-composer-post")
                            // Phase 9.5: the button follows the composer mode.
                            .label(if busy {
                                "Working…"
                            } else if is_edit {
                                "Save story"
                            } else if is_repost {
                                "Repost story"
                            } else {
                                "Post story"
                            })
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.story_composer_post(cx);
                            })),
                    ),
            )
    }
}
