//! The cover's story ring opens the stories, the rating pill and the
//! "when?" button open their boxes, and only the Saved Messages chat's
//! info is the Saved Messages cover.

use crate::ui::app::QuillApp;
use crate::ui::demos::profile_cover::{COVER_HIDDEN, COVER_ME, COVER_STORIES};
use crate::ui::keybindings::bind_keys;
use crate::ui::navigation::NavigationAction;
use crate::ui::profile_cover::{ShowOrPremium, ShowOrPremiumBox};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::InfoPanelTarget;

fn new_app(
    cx: &mut TestAppContext,
    demo: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
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
    for _ in 0..3 {
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

#[gpui_kit::test]
fn the_ringed_avatar_opens_the_first_unread_story(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-info-cover-stories");
    assert!(find(&mut vcx, "profile-rating"));
    click(&mut vcx, "profile-cover-avatar");
    let opened = app.read_with(&vcx, |app, _| {
        app.stories
            .viewer
            .waiting()
            .map(|open| (open.chat_id, open.story_id))
            .or_else(|| app.stories.viewer.is_open().then_some((COVER_STORIES, 0)))
    });
    assert_eq!(opened.map(|(chat, _)| chat), Some(COVER_STORIES));
    if let Some((_, story)) = opened.filter(|(_, story)| *story != 0) {
        assert_eq!(story, 2, "the first story after max_read_story_id");
    }
}

#[gpui_kit::test]
fn the_rating_pill_opens_the_rating_box(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-stories");
    click(&mut vcx, "profile-rating");
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::StarsRating));
    click(&mut vcx, "rating-box-understood");
    assert!(app.read_with(&vcx, |app, _| app.profile_cover.rating_box.is_none()));
    assert_ne!(dialog(&shell, &mut vcx), Some(DialogKind::StarsRating));
}

#[gpui_kit::test]
fn hidden_last_seen_offers_to_show_yours(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-last-seen");
    assert!(!find(&mut vcx, "profile-rating"));
    click(&mut vcx, "profile-cover-when");
    assert_eq!(
        app.read_with(&vcx, |app, _| app.profile_cover.show_or_premium),
        Some(ShowOrPremiumBox {
            kind: ShowOrPremium::LastSeen,
            user_id: COVER_HIDDEN,
            stage: crate::ui::profile_cover::ShowStage::Choosing,
        })
    );
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::ShowOrPremium));
    click(&mut vcx, "show-or-premium-show");
    let note = app.read_with(&vcx, |app, _| app.connection.note().text().to_string());
    assert_eq!(note, tr(keys::LASTSEEN_SHOWN_TOAST));
    assert_ne!(dialog(&shell, &mut vcx), Some(DialogKind::ShowOrPremium));
}

#[gpui_kit::test]
fn hidden_last_seen_subscribes_with_the_lastseen_hidden_ref(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-last-seen");
    click(&mut vcx, "profile-cover-when");
    click(&mut vcx, "show-or-premium-premium");
    let premium = app.read_with(&vcx, |app, _| {
        app.session().map(|session| {
            (
                session.payments.hub.premium_open(),
                session.payments.hub.premium_ref().to_string(),
            )
        })
    });
    assert_eq!(premium, Some((true, "lastseen_hidden".to_string())));
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::PremiumFeatures));
}

#[gpui_kit::test]
fn the_saved_messages_chat_info_is_the_saved_cover(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = new_app(cx, "ready-info-cover-saved");
    assert!(find(&mut vcx, "saved-messages-cover"));
    assert!(!find(&mut vcx, "profile-cover-avatar"));
    assert!(!find(&mut vcx, "profile-rating"));
    assert!(!find(&mut vcx, "profile-cover-when"));
}

#[gpui_kit::test]
fn only_the_saved_chat_turns_your_profile_into_saved_messages(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx, "ready-info-cover-self");
    let open = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session().and_then(|s| s.users_state.open_info_panel)
        })
    };
    assert_eq!(open(&app, &mut vcx), Some(InfoPanelTarget::User(COVER_ME)));
    assert!(!find(&mut vcx, "saved-messages-cover"));
    assert!(find(&mut vcx, "profile-rating"));

    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.select_listed_chat(ChatId(COVER_ME), window, cx)
        })
    });
    frames(&mut vcx);
    assert_eq!(open(&app, &mut vcx), Some(InfoPanelTarget::SavedMessages));
    assert!(find(&mut vcx, "saved-messages-cover"));
    assert!(!find(&mut vcx, "profile-rating"));

    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.navigate(NavigationAction::MyProfile, window, cx)
        })
    });
    frames(&mut vcx);
    assert_eq!(open(&app, &mut vcx), Some(InfoPanelTarget::User(COVER_ME)));
    assert!(!find(&mut vcx, "saved-messages-cover"));
    assert!(find(&mut vcx, "profile-rating"));
}

#[gpui_kit::test]
fn your_rating_box_previews_the_pending_rating(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, "ready-info-cover-self");
    click(&mut vcx, "profile-rating");
    assert_eq!(dialog(&shell, &mut vcx), Some(DialogKind::StarsRating));
    click(&mut vcx, "rating-box-preview");
    let mode = |vcx: &VisualTestContext| {
        app.read_with(vcx, |app, _| app.profile_cover.rating_box.map(|b| b.mode))
    };
    assert_eq!(mode(&vcx), Some(super::RatingMode::Future));
    click(&mut vcx, "rating-box-preview");
    assert_eq!(mode(&vcx), Some(super::RatingMode::Current));
}

#[gpui_kit::test]
fn others_have_no_pending_rating(cx: &mut TestAppContext) {
    let (_app, _shell, mut vcx) = new_app(cx, "ready-info-cover-stories");
    click(&mut vcx, "profile-rating");
    assert!(!find(&mut vcx, "rating-box-preview"));
}

/// The box settles on its own request: another save of the Last seen rule
/// (a refusal in the key's state) says nothing about it.
#[gpui_kit::test]
fn the_show_box_settles_on_its_own_request(cx: &mut TestAppContext) {
    use crate::ui::profile_cover::ShowStage;
    use quill::state::RequestPurpose;
    let (app, _shell, mut vcx) = new_app(cx, "ready-info-cover-last-seen");
    click(&mut vcx, "profile-cover-when");
    app.update_in(&mut vcx, |app, _, cx| {
        let request = app
            .demo_session
            .as_mut()
            .map(|session| session.request(RequestPurpose::SetReadDatePrivacy, None))
            .expect("the demo session");
        if let Some(open) = app.profile_cover.show_or_premium.as_mut() {
            open.stage = ShowStage::Saving(request);
        }
        assert!(!app.drain_show_or_premium(cx), "its request is still out");
        assert!(app.profile_cover.show_or_premium.is_some());
        if let Some(session) = app.demo_session.as_mut() {
            session.requests.take(request);
        }
        assert!(app.drain_show_or_premium(cx), "answered");
        assert!(app.profile_cover.show_or_premium.is_none());
    });
}
