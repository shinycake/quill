//! Every upsell opens the preview of its own feature, with the buttons of
//! the way it was opened (needs the `demo-capture` feature for gpui-kit's
//! test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::premium_hub::promo::{PreviewOpen, PromoFeature, PromoPage};
use quill::premium_hub::{BusinessFeature, PremiumFeature, parse_premium_features};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.hub.premium = Some(parse_premium_features(&serde_json::json!({
            "features": [
                {"@type": "premiumFeatureIncreasedLimits"},
                {"@type": "premiumFeatureVoiceRecognition"},
                {"@type": "premiumFeatureEmojiStatus"},
                {"@type": "premiumFeatureDisabledAds"},
            ],
            "limits": [],
        })));
    });
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn open(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    run: impl FnOnce(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
) {
    app.update(vcx, |app, cx| run(app, cx));
    frames(vcx);
}

/// The page of the open Premium dialog and the ref it was opened with.
fn premium(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(PromoPage, String)> {
    app.read_with(vcx, |app, _| {
        let hub = &app.session()?.payments.hub;
        hub.premium_open
            .then(|| (hub.promo_page, hub.premium_ref.clone()))
    })
}

fn preview(feature: impl Into<PromoFeature>, opened: PreviewOpen) -> PromoPage {
    PromoPage::Preview {
        feature: feature.into(),
        opened,
    }
}

fn shown(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

#[gpui_kit::test]
fn an_upsell_box_unlocks_through_the_premium_page(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open(&app, vcx, |app, cx| {
        app.open_premium_at(PremiumFeature::EmojiStatus, cx)
    });
    assert_eq!(
        premium(&app, vcx),
        Some((
            preview(PremiumFeature::EmojiStatus, PreviewOpen::Box),
            String::new()
        ))
    );
    assert!(shown(vcx, "premium-preview-unlock"));
    assert!(!shown(vcx, "premium-subscribe"), "a box has one button");
    // Pages through the server's list, without the limits row.
    vcx.update(|window, cx| window.click("premium-preview-next", cx));
    frames(vcx);
    assert_eq!(
        premium(&app, vcx).map(|(page, _)| page),
        Some(preview(PremiumFeature::DisabledAds, PreviewOpen::Box))
    );
    vcx.update(|window, cx| window.click("premium-preview-unlock", cx));
    frames(vcx);
    assert_eq!(
        premium(&app, vcx),
        Some((PromoPage::Overview, "no_ads".into()))
    );
}

#[gpui_kit::test]
fn a_preview_to_buy_shows_the_plans(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open(&app, vcx, |app, cx| {
        app.open_premium_to_buy(BusinessFeature::AccountLinks, cx)
    });
    assert_eq!(
        premium(&app, vcx).map(|(page, _)| page),
        Some(preview(BusinessFeature::AccountLinks, PreviewOpen::ToBuy))
    );
    assert!(!shown(vcx, "premium-preview-unlock"));
    // The Business list that holds the feature pages, in tdesktop's order
    // until the server's arrives.
    vcx.update(|window, cx| window.click("premium-preview-next", cx));
    frames(vcx);
    assert_eq!(
        premium(&app, vcx).map(|(page, _)| page),
        Some(preview(BusinessFeature::StartPage, PreviewOpen::ToBuy))
    );
}

#[gpui_kit::test]
fn the_gifts_preview_closes_with_understood(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open(&app, vcx, |app, cx| {
        app.open_premium_at(PromoFeature::Gifts, cx)
    });
    assert!(shown(vcx, "premium-preview-understood"));
    assert!(!shown(vcx, "premium-preview-next"), "gifts stand alone");
    vcx.update(|window, cx| window.click("premium-preview-understood", cx));
    frames(vcx);
    assert_eq!(premium(&app, vcx), None);
}

#[gpui_kit::test]
fn the_limits_feature_and_refs_open_the_right_page(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open(&app, vcx, |app, cx| {
        app.open_premium_at(PremiumFeature::IncreasedLimits, cx)
    });
    assert_eq!(
        premium(&app, vcx).map(|(page, _)| page),
        Some(PromoPage::Limits)
    );
    open(&app, vcx, |app, cx| {
        app.open_premium_from("lastseen_hidden", cx)
    });
    assert_eq!(
        premium(&app, vcx),
        Some((PromoPage::Overview, "lastseen_hidden".into()))
    );
}

#[gpui_kit::test]
fn a_refused_hide_ads_opens_the_no_ads_preview(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.hub.promo_request = Some(PremiumFeature::DisabledAds.into());
        cx.notify();
    });
    frames(vcx);
    assert_eq!(
        premium(&app, vcx).map(|(page, _)| page),
        Some(preview(PremiumFeature::DisabledAds, PreviewOpen::Box))
    );
    let pending = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.promo_request)
            .is_some()
    });
    assert!(!pending, "the request is taken once");
}
