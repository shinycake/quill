use super::SIMILAR_SOURCE_CHANNEL;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::premium_limits::PremiumLimitKind;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::CanPostStoryResult;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-premium-limits")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn set_premium(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, premium: bool) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
            session.payments.hub.limit_box = None;
        }
        cx.notify();
    });
    vcx.run_until_parked();
}

fn show(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, kind: PremiumLimitKind) {
    app.update_in(vcx, |app, _, cx| app.show_premium_limit(kind, 0, cx));
    vcx.run_until_parked();
    vcx.run_until_parked();
}

fn toasts(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, cx| window.notifications(cx).len())
}

fn limit_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<PremiumLimitKind> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.limit_box)
            .map(|(kind, _)| kind)
    })
}

#[gpui_kit::test]
fn the_saved_pins_box_offers_premium_only_to_a_free_account(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    assert_eq!(limit_box(&app, vcx), Some(PremiumLimitKind::SavedPins));
    assert!(vcx.debug_bounds("premium-limit-increase").is_some());
    assert!(vcx.debug_bounds("premium-limit-line").is_some());
    set_premium(&app, vcx, true);
    show(&app, vcx, PremiumLimitKind::SavedPins);
    assert!(vcx.debug_bounds("premium-limit-increase").is_none());
    assert!(vcx.debug_bounds("premium-limit-ok").is_some());
}

#[gpui_kit::test]
fn toast_kinds_push_a_toast_and_open_no_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    set_premium(&app, vcx, false);
    for (n, kind) in [
        PremiumLimitKind::FavoriteStickers,
        PremiumLimitKind::SavedGifs,
        PremiumLimitKind::SimilarChats,
        PremiumLimitKind::OwnedBots,
    ]
    .into_iter()
    .enumerate()
    {
        let before = toasts(vcx);
        show(&app, vcx, kind);
        assert_eq!(toasts(vcx), before + 1, "{kind:?} (#{n})");
        assert_eq!(limit_box(&app, vcx), None, "{kind:?}");
        let queued = app.read_with(vcx, |app, _| {
            app.session().and_then(|s| s.payments.hub.limit_toast)
        });
        assert_eq!(queued, None);
    }
}

#[gpui_kit::test]
fn the_similar_unlock_shows_for_a_free_account_only(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    set_premium(&app, vcx, false);
    app.update_in(vcx, |app, window, cx| {
        app.open_info_panel_target(
            InfoPanelTarget::Supergroup(-SIMILAR_SOURCE_CHANNEL - 1_000_000_000_000),
            window,
            cx,
        );
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("similar-unlock").is_some());
    set_premium(&app, vcx, true);
    assert!(vcx.debug_bounds("similar-unlock").is_none());
}

#[gpui_kit::test]
fn a_story_refused_over_a_limit_opens_its_box_over_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    set_premium(&app, vcx, false);
    for (answer, kind) in [
        (
            CanPostStoryResult::ActiveStoryLimitExceeded,
            Some(PremiumLimitKind::ActiveStories),
        ),
        (
            CanPostStoryResult::WeeklyLimitExceeded { retry_after: 60 },
            Some(PremiumLimitKind::WeeklyStories),
        ),
        (
            CanPostStoryResult::MonthlyLimitExceeded { retry_after: 60 },
            Some(PremiumLimitKind::MonthlyStories),
        ),
        (CanPostStoryResult::BoostNeeded, None),
    ] {
        app.update_in(vcx, |app, window, cx| {
            app.open_story_composer(window, cx);
            if let Some(session) = app.demo_session.as_mut() {
                session.payments.hub.limit_box = None;
                session.stories.post.eligibility = Some(answer.clone());
            }
            app.story_composer_after_check(cx);
        });
        vcx.run_until_parked();
        assert_eq!(limit_box(&app, vcx), kind, "{answer:?}");
        assert_eq!(
            vcx.debug_bounds("premium-limit-increase").is_some(),
            kind.is_some(),
            "{answer:?}"
        );
        let reason = app.read_with(vcx, |app, _| app.stories.composer.local_error.clone());
        assert_eq!(reason, Some(answer.user_message()));
    }
}
