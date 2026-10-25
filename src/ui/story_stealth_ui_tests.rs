use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::premium_hub::PremiumFeature;
use quill::premium_hub::promo::{PreviewOpen, PromoPage};
use quill::state::StoryStealthMode;
use quill::stories_menu::StoryMenuAction;

const LEO: i64 = 105;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-story-viewer");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx.debug_bounds(selector).expect(selector);
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

fn now() -> i32 {
    i32::try_from(quill::local_time::now_unix()).unwrap_or(0)
}

fn seed(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    premium: bool,
    mode: StoryStealthMode,
) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
            session.stories.stealth = mode;
        }
        cx.notify();
    });
    frames(vcx);
}

fn open_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, open_after: Option<ChatId>) {
    app.update(vcx, |app, cx| app.setup_story_stealth(open_after, cx));
    frames(vcx);
}

fn box_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.stories.stealth_box.is_some())
}

fn toast(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(Option<String>, String)> {
    app.read_with(vcx, |app, _| {
        app.stories.stealth_toast.as_ref().map(|shown| {
            (
                shown.toast.title.map(str::to_string),
                shown.toast.text.clone(),
            )
        })
    })
}

fn button_label(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("session");
        quill::story_stealth::StealthButton::of(
            session.stories.stealth,
            quill::local_time::now_unix(),
            session.my_is_premium(),
            app.stories
                .stealth_box
                .is_some_and(|stealth_box| stealth_box.open_after.is_some()),
        )
        .label()
    })
}

#[gpui_kit::test]
fn hide_my_view_is_in_the_menu_of_a_users_story(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true, StoryStealthMode::default());
    let entry = app.read_with(vcx, |app, _| {
        app.stories
            .viewer
            .current()
            .and_then(|item| app.story_stealth_entry(item))
            .map(|entry| entry.locked)
    });
    assert_eq!(entry, Some(false));
    seed(&app, vcx, false, StoryStealthMode::default());
    let locked = app.read_with(vcx, |app, _| {
        app.stories
            .viewer
            .current()
            .and_then(|item| app.story_stealth_entry(item))
            .map(|entry| entry.locked)
    });
    assert_eq!(locked, Some(true), "a free account sees the lock");
}

#[gpui_kit::test]
fn a_premium_account_enables_and_the_box_closes_on_the_update(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true, StoryStealthMode::default());
    open_box(&app, vcx, None);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::StoryStealth));
    assert!(find(vcx, "story-stealth-box"));
    assert!(find(vcx, "story-stealth-past"));
    assert!(find(vcx, "story-stealth-next"));
    assert_eq!(button_label(&app, vcx), "Enable Stealth Mode");
    assert!(
        app.read_with(vcx, |app, _| app.story_playback_paused()),
        "the story waits under the box"
    );
    click(vcx, "story-stealth-button");
    assert!(box_open(&app, vcx), "the box waits for TDLib's answer");

    let on = StoryStealthMode {
        active_until_date: now() + 1500,
        cooldown_until_date: now() + 3600,
    };
    seed(&app, vcx, true, on);
    app.update(vcx, |app, cx| {
        assert!(app.drain_story_stealth(cx));
    });
    frames(vcx);
    assert!(!box_open(&app, vcx));
    assert_eq!(dialog(&shell, vcx), None);
    let (title, text) = toast(&app, vcx).expect("the Stealth Mode On toast");
    assert_eq!(title.as_deref(), Some("Stealth Mode On"));
    assert!(text.contains("**25 minutes**"), "{text}");
    assert!(find(vcx, "story-stealth-toast"));
}

#[gpui_kit::test]
fn a_free_account_unlocks_through_the_premium_preview(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, false, StoryStealthMode::default());
    open_box(&app, vcx, None);
    assert_eq!(button_label(&app, vcx), "Unlock Stealth Mode");
    click(vcx, "story-stealth-button");
    assert!(box_open(&app, vcx), "Unlock keeps the box");
    let page = app.read_with(vcx, |app, _| {
        app.session().map(|session| session.payments.hub.promo_page)
    });
    assert_eq!(
        page,
        Some(PromoPage::Preview {
            feature: PremiumFeature::UpgradedStories.into(),
            opened: PreviewOpen::Box,
        })
    );
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::PremiumFeatures));
    app.update(vcx, |app, cx| app.close_premium(cx));
    frames(vcx);
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::StoryStealth));
}

#[gpui_kit::test]
fn the_cooldown_counts_down_and_a_press_only_toasts(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let cooling = StoryStealthMode {
        active_until_date: now() - 60,
        cooldown_until_date: now() + 4 * 60 + 30,
    };
    seed(&app, vcx, true, cooling);
    open_box(&app, vcx, None);
    let label = button_label(&app, vcx);
    assert!(label.starts_with("Available in 4:"), "{label}");
    click(vcx, "story-stealth-button");
    assert!(!box_open(&app, vcx));
    assert_eq!(dialog(&shell, vcx), None);
    let (title, text) = toast(&app, vcx).expect("the cooldown toast");
    assert_eq!(title, None);
    assert!(text.contains("**Stealth Mode**"), "{text}");
}

#[gpui_kit::test]
fn while_stealth_is_on_an_entry_point_only_toasts(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let on = StoryStealthMode {
        active_until_date: now() + 125,
        cooldown_until_date: now() + 3600,
    };
    seed(&app, vcx, true, on);
    open_box(&app, vcx, None);
    assert!(!box_open(&app, vcx));
    assert_eq!(dialog(&shell, vcx), None);
    let (title, _) = toast(&app, vcx).expect("the already-on toast");
    assert_eq!(title.as_deref(), Some("You are in Stealth Mode"));
}

#[gpui_kit::test]
fn view_anonymously_on_a_tile_opens_the_stories_once_on(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.close_story_viewer(cx));
    seed(&app, vcx, true, StoryStealthMode::default());
    let entry = app.read_with(vcx, |app, _| {
        app.story_menu_entries(LEO)
            .into_iter()
            .map(|item| (item.action, item.label))
            .find(|(action, _)| matches!(action, StoryMenuAction::ViewAnonymously { .. }))
    });
    assert_eq!(
        entry,
        Some((
            StoryMenuAction::ViewAnonymously { locked: false },
            "View Anonymously"
        ))
    );
    open_box(&app, vcx, Some(ChatId(LEO)));
    assert_eq!(dialog(&shell, vcx), Some(DialogKind::StoryStealth));
    assert_eq!(button_label(&app, vcx), "Enable and open the story");
    let on = StoryStealthMode {
        active_until_date: now() + 1500,
        cooldown_until_date: now() + 3600,
    };
    seed(&app, vcx, true, on);
    app.update(vcx, |app, cx| {
        app.drain_story_stealth(cx);
    });
    frames(vcx);
    let opened = app.read_with(vcx, |app, _| {
        app.stories.viewer.current().map(|item| item.chat_id)
    });
    assert_eq!(opened, Some(ChatId(LEO)));
    assert!(
        toast(&app, vcx).is_some(),
        "the toast shows over the stories"
    );
}
