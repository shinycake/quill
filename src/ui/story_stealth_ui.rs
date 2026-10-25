#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::status_note::StatusNote;
use super::story_viewer::chrome;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::premium_hub::PremiumFeature;
use quill::state::{RequestPurpose, StoryStealthMode};
use quill::story_stealth::{
    StealthButton, StealthPeriods, StealthPress, StealthToast, about_text, active_left, press,
};
use quill::story_tray_walk::story_to_open;
use quill::story_viewer::StoryOpenScope;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const TOAST_DURATION: Duration = Duration::from_secs(4);
const COOLDOWN_LABEL_OPACITY: f32 = 0.5;
const BOX_WIDTH: f32 = 364.;
const LOGO: f32 = 88.;
const LOGO_GLYPH: f32 = 45.;
const TOAST_MAX_WIDTH: f32 = 340.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StealthBox {
    pub(super) open_after: Option<ChatId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShownStealthToast {
    pub(super) toast: StealthToast,
    pub(super) until: Instant,
}

struct StealthFacts {
    mode: StoryStealthMode,
    premium: bool,
    periods: StealthPeriods,
    requested: bool,
}

impl QuillApp {
    fn stealth_facts(&self) -> StealthFacts {
        let session = self.session();
        StealthFacts {
            mode: session.map(|s| s.stories.stealth).unwrap_or_default(),
            premium: session.is_some_and(|s| s.my_is_premium()),
            periods: session
                .map(|s| s.stories.stealth_periods)
                .unwrap_or_default(),
            requested: session.is_some_and(|s| {
                s.requests
                    .has_purpose(RequestPurpose::ActivateStoryStealthMode)
            }),
        }
    }

    pub(in crate::ui) fn setup_story_stealth(
        &mut self,
        open_after: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        let mode = self.stealth_facts().mode;
        if let Some(left) = active_left(mode, now_unix_secs()) {
            if let Some(chat_id) = open_after {
                self.open_peer_stories(chat_id, cx);
            }
            self.show_stealth_toast(StealthToast::already(left), cx);
        } else {
            self.stories.stealth_box = Some(StealthBox { open_after });
            self.ensure_stealth_box_tick(cx);
        }
        cx.notify();
    }

    fn open_peer_stories(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let story = self
            .session()
            .and_then(|s| s.stories.tray.get(&chat_id.0))
            .and_then(story_to_open);
        if let Some(story) = story {
            self.open_story_viewer(chat_id, story, StoryOpenScope::Tray, cx);
        }
    }

    fn show_stealth_toast(&mut self, toast: StealthToast, cx: &mut Context<Self>) {
        if self.stories.viewer.is_open() {
            self.stories.stealth_toast = Some(ShownStealthToast {
                toast,
                until: Instant::now() + TOAST_DURATION,
            });
            self.notify_story(cx);
        } else {
            self.set_status_note(StatusNote::info(toast.plain_text()), cx);
        }
    }

    fn close_stealth_box(&mut self) {
        self.stories.stealth_box = None;
        self.stories.stealth_tick = None;
    }

    fn press_story_stealth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stories.stealth_box.is_none() {
            return;
        }
        let facts = self.stealth_facts();
        match press(facts.mode, now_unix_secs(), facts.premium, facts.requested) {
            StealthPress::EnabledToast => {
                self.close_stealth_box();
                self.show_stealth_toast(StealthToast::activated(facts.periods), cx);
            }
            StealthPress::PremiumPreview => {
                self.open_premium_at(PremiumFeature::UpgradedStories, cx)
            }
            StealthPress::CooldownToast => {
                self.close_stealth_box();
                self.show_stealth_toast(StealthToast::cooldown(), cx);
            }
            StealthPress::Activate => {
                if let Some(live) = self.live.as_mut() {
                    if live.driver.activate_story_stealth_mode().is_err() {
                        self.connection.set_note(StatusNote::error(tr(
                            keys::Q_MEDIA_COULD_NOT_ENABLE_STEALTH_MODE,
                        )));
                    }
                } else if self.demo_session.is_some() {
                    self.connection.set_note(StatusNote::info(tr(
                        keys::Q_MEDIA_DEMO_STEALTH_MODE_NEEDS_LIVE,
                    )));
                }
            }
            StealthPress::Wait => {}
        }
        self.close_kit_dialog_if_done(DialogKind::StoryStealth, window, cx);
        cx.notify();
    }

    pub(in crate::ui) fn drain_story_stealth(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(stealth_box) = self.stories.stealth_box else {
            return false;
        };
        let facts = self.stealth_facts();
        if !facts.mode.is_active(now_unix_secs()) {
            return false;
        }
        self.close_stealth_box();
        if let Some(chat_id) = stealth_box.open_after {
            self.open_peer_stories(chat_id, cx);
        }
        self.show_stealth_toast(StealthToast::activated(facts.periods), cx);
        cx.notify();
        true
    }

    fn ensure_stealth_box_tick(&mut self, cx: &mut Context<Self>) {
        if self.stories.stealth_tick.is_some() {
            return;
        }
        self.stories.stealth_tick = Some(Self::spawn_main(cx, async move |this, cx| {
            loop {
                let into_second = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_millis())
                    .unwrap_or(0);
                let wait = 1000u64.saturating_sub(u64::from(into_second)).max(50);
                cx.background_executor()
                    .timer(Duration::from_millis(wait))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }

    pub(in crate::ui) fn story_stealth_toast(&self) -> Option<AnyElement> {
        let shown = self
            .stories
            .stealth_toast
            .as_ref()
            .filter(|shown| shown.until > Instant::now())?;
        let (text, bold) = super::screen_chooser::split_bold(&shown.toast.text);
        let style = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let highlights: Vec<_> = bold.into_iter().map(|range| (range, style)).collect();
        let label = match shown.toast.title {
            Some(title) => format!("{title}. {text}"),
            None => text.clone(),
        };
        Some(
            div()
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
                        .id("story-stealth-toast")
                        .debug_selector(|| "story-stealth-toast".into())
                        .role(gpui_kit::Role::Status)
                        .aria_label(label)
                        .max_w(px(TOAST_MAX_WIDTH))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .px(px(16.))
                        .py(px(10.))
                        .rounded(px(8.))
                        .bg(hsla(0., 0., 0., 0.7))
                        .text_color(chrome::fg())
                        .text_sm()
                        .children(
                            shown
                                .toast
                                .title
                                .map(|title| div().font_weight(FontWeight::SEMIBOLD).child(title)),
                        )
                        .child(StyledText::new(text).with_highlights(highlights)),
                )
                .into_any_element(),
        )
    }

    pub(in crate::ui) fn build_story_stealth_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::StoryStealth, |this, _, cx| {
                this.close_stealth_box();
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let facts = this.stealth_facts();
            let open_after = this
                .stories
                .stealth_box
                .is_some_and(|stealth_box| stealth_box.open_after.is_some());
            let button = StealthButton::of(facts.mode, now_unix_secs(), facts.premium, open_after);
            let body = stealth_box_body(&facts, cx);
            let footer = stealth_box_button(button, cx);
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .w(px(BOX_WIDTH))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

fn stealth_box_body(facts: &StealthFacts, cx: &mut Context<QuillApp>) -> AnyElement {
    let feature = |id: &'static str, icon: IconName, title: &'static str, about: String| {
        h_flex()
            .id(id)
            .debug_selector(move || id.into())
            .w_full()
            .items_start()
            .gap_4()
            .child(
                Icon::new(icon)
                    .with_size(px(28.))
                    .flex_none()
                    .text_color(cx.theme().primary),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(about),
                    ),
            )
    };
    v_flex()
        .id("story-stealth-box")
        .debug_selector(|| "story-stealth-box".into())
        .w_full()
        .items_center()
        .gap_2()
        .pt_2()
        .pb_1()
        .child(
            div()
                .size(px(LOGO))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().primary)
                .child(
                    Icon::new(IconName::HatGlasses)
                        .with_size(px(LOGO_GLYPH))
                        .text_color(cx.theme().primary_foreground),
                ),
        )
        .child(
            div()
                .pt_2()
                .text_lg()
                .font_semibold()
                .text_color(cx.theme().foreground)
                .child(tr(keys::STEALTH_MODE_TITLE)),
        )
        .child(
            div()
                .id("story-stealth-about")
                .debug_selector(|| "story-stealth-about".into())
                .pb_2()
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(about_text(facts.premium)),
        )
        .child(
            v_flex()
                .w_full()
                .gap_4()
                .child(feature(
                    "story-stealth-past",
                    IconName::RotateCcwClock,
                    tr(keys::STEALTH_MODE_PAST_TITLE),
                    facts.periods.past_about(),
                ))
                .child(feature(
                    "story-stealth-next",
                    IconName::Timer,
                    tr(keys::STEALTH_MODE_NEXT_TITLE),
                    facts.periods.next_about(),
                )),
        )
        .into_any_element()
}

fn stealth_box_button(button: StealthButton, cx: &mut Context<QuillApp>) -> AnyElement {
    let label = button.label();
    let dimmed = matches!(button, StealthButton::Cooldown { .. });
    let locked = matches!(button, StealthButton::Unlock);
    div()
        .w_full()
        .debug_selector(|| "story-stealth-button".into())
        .child(
            Button::new("story-stealth-button")
                .primary()
                .large()
                .w_full()
                .accessibility_label(label.clone())
                .child(
                    div()
                        .font_semibold()
                        .opacity(if dimmed { COOLDOWN_LABEL_OPACITY } else { 1. })
                        .child(label),
                )
                .when(locked, |this| {
                    this.child(Icon::new(IconName::Lock).with_size(px(16.)))
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.press_story_stealth(window, cx);
                })),
        )
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    StoryStealth => DialogSpec::new(
        4030,
        |app| app.stories.stealth_box.is_some(),
        QuillApp::build_story_stealth_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "story_stealth_ui_tests.rs"]
mod tests;
