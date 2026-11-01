//! UI integration tests for the Telegram Business dialog against the demo
//! `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
//! Nothing here reaches a live account: demo mode refuses every save.
use super::{MessageKind, Recipients, ScheduleChoice, View};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::business_bots::BotRight;
use quill::business_chat_links::ChatLink;
use quill::business_hours_edit::extract_day;
use quill::premium_hub::BusinessFeature;
use quill::premium_hub::promo::{PreviewOpen, PromoPage};
use quill::quick_replies::{QuickRepliesUpdate, QuickReplyShortcut};
use quill::telegram::envelope::ChatKind;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

/// A Premium demo account with a `hello` shortcut that has messages.
fn seed(app: &Entity<QuillApp>, cx: &mut VisualTestContext, premium: bool) {
    app.update(cx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.payments.premium_option = Some(premium);
        session.apply_quick_replies(QuickRepliesUpdate::Shortcut(QuickReplyShortcut {
            id: 2,
            name: "hello".into(),
            message_count: 1,
        }));
    });
}

fn open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| app.update(cx, |app, cx| app.open_business(window, cx)));
    frames(cx);
}

fn go(app: &Entity<QuillApp>, cx: &mut VisualTestContext, view: View) {
    cx.update(|window, cx| app.update(cx, |app, cx| app.business_go(view, window, cx)));
    frames(cx);
}

fn act(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut QuillApp, &mut gpui_kit::Window, &mut gpui_kit::Context<QuillApp>),
) {
    cx.update(|window, cx| app.update(cx, |app, cx| f(app, window, cx)));
    frames(cx);
}

fn view(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> View {
    app.read_with(cx, |app, _| {
        app.dialogs.business_dialog.as_ref().unwrap().view
    })
}

fn error(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .local_error
            .clone()
    })
}

fn note(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<String> {
    app.read_with(cx, |app, _| {
        app.dialogs.business_dialog.as_ref().unwrap().note.clone()
    })
}

fn back(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    act(app, cx, |app, window, cx| app.business_back(window, cx));
}

#[gpui_kit::test]
fn every_screen_renders(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    assert_eq!(view(&app, vcx), View::Hub);
    for screen in [
        View::Greeting,
        View::Hub,
        View::Away,
        View::Hub,
        View::Hours,
        View::Day(2),
        View::TimeZone,
        View::Hub,
        View::Location,
        View::Hub,
        View::StartPage,
        View::StickerPick,
        View::Hub,
        View::ChatLinks,
        View::Hub,
        View::Chatbots,
        View::Hub,
    ] {
        go(&app, vcx, screen);
        assert_eq!(view(&app, vcx), screen);
    }
    act(&app, vcx, |app, window, cx| {
        app.open_business_link_edit(None, window, cx)
    });
    assert_eq!(view(&app, vcx), View::LinkEdit);
}

fn premium_page(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<PromoPage> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| &s.payments.hub)
            .filter(|hub| hub.premium_open)
            .map(|hub| hub.promo_page)
    })
}

#[gpui_kit::test]
fn without_premium_only_chatbots_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, false);
    open(&app, vcx);
    act(&app, vcx, |app, window, cx| {
        app.business_open_feature(BusinessFeature::GreetingMessage, window, cx)
    });
    // The feature's preview to buy opens above the hub instead.
    assert_eq!(view(&app, vcx), View::Hub);
    assert_eq!(
        premium_page(&app, vcx),
        Some(PromoPage::Preview {
            feature: BusinessFeature::GreetingMessage.into(),
            opened: PreviewOpen::ToBuy,
        })
    );
    act(&app, vcx, |app, window, cx| {
        app.business_open_feature(BusinessFeature::Bots, window, cx)
    });
    assert_eq!(view(&app, vcx), View::Chatbots);
}

#[gpui_kit::test]
fn greeting_messages_open_quick_replies_above_the_hub(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Greeting, true, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.edit_business_messages(MessageKind::Greeting, window, cx)
    });
    let (qr, business) = app.read_with(vcx, |app, _| {
        (
            app.dialogs.quick_replies_dialog.is_some(),
            app.dialogs.business_dialog.is_some(),
        )
    });
    assert!(qr && business);
    // Closing quick replies comes back to the greeting draft.
    act(&app, vcx, |app, _, cx| app.close_quick_replies(cx));
    assert_eq!(view(&app, vcx), View::Greeting);
    let enabled = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .enabled
    });
    assert!(enabled);
}

#[gpui_kit::test]
fn the_limit_keeps_away_messages_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    app.update(vcx, |app, _| {
        let qr = &mut app.demo_session.as_mut().unwrap().settings.quick_replies;
        qr.shortcut_count_max = Some(1);
    });
    open(&app, vcx);
    go(&app, vcx, View::Away);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Away, true, cx)
    });
    assert!(error(&app, vcx).unwrap().contains("too many quick replies"));
    let enabled = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .away
            .as_ref()
            .unwrap()
            .enabled
    });
    assert!(!enabled);
}

/// Make the first demo chats private chats with users; returns their ids.
fn private_chats(app: &Entity<QuillApp>, cx: &mut VisualTestContext, count: usize) -> Vec<i64> {
    app.update(cx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        let mut ids: Vec<i64> = session.chats.keys().copied().collect();
        ids.sort_unstable();
        ids.truncate(count);
        for id in &ids {
            session.chats.get_mut(id).unwrap().kind = ChatKind::Private {
                user_id: quill::ids::UserId(*id),
            };
        }
        ids
    })
}

#[gpui_kit::test]
fn picking_chats_selects_only_those(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    let chat = private_chats(&app, vcx, 1)[0];
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, window, cx| {
        app.open_business_picker(Recipients::Greeting, true, window, cx)
    });
    assert_eq!(view(&app, vcx), View::Picker);
    let offered = app.read_with(vcx, |app, _| app.business_picker_chats());
    assert!(offered.iter().any(|(id, _)| *id == chat));
    act(&app, vcx, |app, _, cx| {
        if let Some(p) = app.business_mut().and_then(|d| d.picker.as_mut()) {
            assert!(p.chats.toggle_chat(chat));
        }
        cx.notify();
    });
    act(&app, vcx, |app, window, cx| {
        app.finish_business_picker(window, cx)
    });
    assert_eq!(view(&app, vcx), View::Greeting);
    let recipients = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .recipients
            .clone()
    });
    assert!(!recipients.all_but_excluded);
    assert_eq!(recipients.included.chat_ids, vec![chat]);
    // Removing the last chat goes back to "all except".
    act(&app, vcx, |app, _, cx| {
        app.remove_business_entry(Recipients::Greeting, false, Some(chat), None, cx)
    });
    let all = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .greeting
            .as_ref()
            .unwrap()
            .recipients
            .all_but_excluded
    });
    assert!(all);
}

#[gpui_kit::test]
fn a_day_gets_a_set_of_hours(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Hours);
    go(&app, vcx, View::Day(0));
    act(&app, vcx, |app, window, cx| {
        app.add_business_day_row(window, cx)
    });
    act(&app, vcx, |app, window, cx| {
        app.save_business_day(0, window, cx)
    });
    assert_eq!(view(&app, vcx), View::Hours);
    let week = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .hours
            .as_ref()
            .unwrap()
            .week
            .clone()
    });
    assert_eq!(extract_day(&week, 0), vec![(8 * 60, 20 * 60)]);
    assert!(extract_day(&week, 1).is_empty());
}

#[gpui_kit::test]
fn leaving_a_screen_saves_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Greeting);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Greeting, true, cx)
    });
    back(&app, vcx);
    // Demo mode cannot send; the hub says so.
    assert_eq!(view(&app, vcx), View::Hub);
    assert_eq!(note(&app, vcx).as_deref(), Some(QuillApp::DEMO_LINE));
    // An unchanged screen leaves quietly.
    go(&app, vcx, View::Location);
    back(&app, vcx);
    assert_eq!(note(&app, vcx), None);
}

#[gpui_kit::test]
fn an_away_message_needs_messages_and_a_valid_range(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    open(&app, vcx);
    go(&app, vcx, View::Away);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Away, true, cx)
    });
    back(&app, vcx);
    assert_eq!(view(&app, vcx), View::Hub);
    assert_eq!(
        note(&app, vcx).as_deref(),
        Some("The away message stays off until it has messages.")
    );
    go(&app, vcx, View::Away);
    act(&app, vcx, |app, _, cx| {
        app.set_business_enabled(MessageKind::Away, true, cx)
    });
    // A custom range that ends before it starts keeps the screen open.
    act(&app, vcx, |app, window, cx| {
        let away = app.business_mut().unwrap().away.as_mut().unwrap();
        away.schedule = ScheduleChoice::Custom;
        let (start, end) = (away.start.clone(), away.end.clone());
        let value =
            |unix, cx: &gpui_kit::App| crate::ui::scheduled::picker_value(unix, cx).unwrap();
        let (late, early) = (value(1_900_000_000, cx), value(1_800_000_000, cx));
        start.update(cx, |s, cx| s.set_date_time(late, window, cx));
        end.update(cx, |s, cx| s.set_date_time(early, window, cx));
    });
    back(&app, vcx);
    assert_eq!(view(&app, vcx), View::Away);
    assert_eq!(
        error(&app, vcx).as_deref(),
        Some("The end time must be after the start time.")
    );
}

#[gpui_kit::test]
fn the_start_page_takes_a_sticker_from_the_panel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    let sticker = quill::telegram::envelope::StickerItem {
        custom_emoji_id: None,
        id: 7,
        set_id: 3,
        emoji: "👋".into(),
        width: 512,
        height: 512,
        format: quill::telegram::envelope::StickerFormat::Webp,
        file_id: quill::ids::FileId(901),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    };
    app.update(vcx, |app, _| {
        let panel = &mut app.demo_session.as_mut().unwrap().stickers.stickers;
        panel.stickers.push(sticker.clone());
    });
    open(&app, vcx);
    // Outside the picker a click is not taken.
    let taken = app.update(vcx, |app, cx| {
        app.route_business_start_page_sticker(sticker.file_id, cx)
    });
    assert!(!taken);
    go(&app, vcx, View::StartPage);
    go(&app, vcx, View::StickerPick);
    let taken = app.update(vcx, |app, cx| {
        app.route_business_start_page_sticker(sticker.file_id, cx)
    });
    assert!(taken);
    assert_eq!(view(&app, vcx), View::StartPage);
    let chosen = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .start_page
            .as_ref()
            .unwrap()
            .sticker
            .clone()
    });
    assert_eq!(chosen, Some(sticker));
    // Typing stops at the title limit.
    act(&app, vcx, |app, window, cx| {
        let title = app
            .business_mut()
            .unwrap()
            .start_page
            .as_ref()
            .unwrap()
            .title
            .clone();
        title.update(cx, |input, cx| input.focus(window, cx));
    });
    vcx.simulate_input(&"x".repeat(50));
    frames(vcx);
    let len = app.read_with(vcx, |app, cx| {
        let page = app
            .dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .start_page
            .as_ref()
            .unwrap();
        page.title.read(cx).value().chars().count()
    });
    assert_eq!(len, quill::business_start_page::TITLE_MAX);
}

#[gpui_kit::test]
fn an_empty_chat_shows_the_start_page(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    let user = private_chats(&app, vcx, 1)[0];
    let none = app.update(vcx, |app, cx| {
        app.business_start_page_intro(user, cx).is_some()
    });
    assert!(!none);
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        let extras = quill::telegram::envelope::UserProfileExtras {
            business: Some(quill::business_info::BusinessInfo {
                start_page: Some(quill::business_start_page::StartPage {
                    title: "Welcome".into(),
                    message: "Ask about prices".into(),
                    sticker: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        session.users_state.user_full_infos.insert(
            user,
            quill::state::UserFullInfoData {
                bio: String::new(),
                photo_file_id: None,
                photo_id: None,
                blocked: false,
                extras,
                ..Default::default()
            },
        );
    });
    let shown = app.update(vcx, |app, cx| {
        app.business_start_page_intro(user, cx).is_some()
    });
    assert!(shown);
}

#[gpui_kit::test]
fn a_link_opens_in_its_editor(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    let link = ChatLink {
        link: "https://t.me/m/Ab".into(),
        text: "Hi there".into(),
        markup: "**Hi** there".into(),
        title: "Promo".into(),
        view_count: 3,
    };
    app.update(vcx, |app, _| {
        app.demo_session
            .as_mut()
            .unwrap()
            .settings
            .business_extra
            .chat_links = Some(vec![link.clone()]);
    });
    open(&app, vcx);
    go(&app, vcx, View::ChatLinks);
    act(&app, vcx, |app, window, cx| {
        app.open_business_link_edit(Some(link.clone()), window, cx)
    });
    assert_eq!(view(&app, vcx), View::LinkEdit);
    let (text, name) = app.read_with(vcx, |app, cx| {
        let edit = app
            .dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .link_edit
            .as_ref()
            .unwrap();
        (
            edit.text.read(cx).value().to_string(),
            edit.name.read(cx).value().to_string(),
        )
    });
    assert_eq!(text, "**Hi** there");
    assert_eq!(name, "Promo");
    act(&app, vcx, |app, window, cx| {
        app.save_business_link(window, cx)
    });
    assert_eq!(error(&app, vcx).as_deref(), Some(QuillApp::DEMO_LINE));
}

#[gpui_kit::test]
fn a_found_bot_is_added_and_asked_about_on_leave(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    seed(&app, vcx, true);
    let bot = private_chats(&app, vcx, 1)[0];
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.users.insert(
            bot,
            quill::telegram::envelope::ParsedUser {
                id: bot,
                first_name: "Shop".into(),
                username: "shop_bot".into(),
                is_bot: true,
                can_connect_to_business: true,
                ..Default::default()
            },
        );
        session.settings.business_extra.lookup = quill::state::BotLookup {
            generation: 1,
            query: "shop_bot".into(),
            exact: Some(Some(bot)),
            mine: Some(Vec::new()),
            global: Some(Vec::new()),
        };
    });
    open(&app, vcx);
    go(&app, vcx, View::Chatbots);
    act(&app, vcx, |app, window, cx| {
        let input = app
            .business_mut()
            .unwrap()
            .chatbot
            .as_ref()
            .unwrap()
            .username
            .clone();
        input.update(cx, |input, cx| input.set_value("@shop_bot", window, cx));
    });
    // Leaving with a found but not added bot asks first.
    back(&app, vcx);
    assert_eq!(view(&app, vcx), View::Chatbots);
    let asked = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .chatbot
            .as_ref()
            .unwrap()
            .confirm_leave
    });
    assert!(asked);
    act(&app, vcx, |app, _, cx| app.add_business_bot(bot, cx));
    assert_eq!(
        note(&app, vcx).as_deref(),
        Some("Shop now manages your account.")
    );
    act(&app, vcx, |app, _, cx| {
        app.set_business_bot_right(BotRight::TransferStars, true, cx)
    });
    let warning = app.read_with(vcx, |app, _| {
        app.dialogs
            .business_dialog
            .as_ref()
            .unwrap()
            .chatbot
            .as_ref()
            .unwrap()
            .warning
            .clone()
    });
    assert_eq!(
        warning.as_deref(),
        Some("The bot Shop will be able to transfer your stars.")
    );
    // A bot's exclusions in "Only Selected Chats" are chats only.
    act(&app, vcx, |app, _, cx| {
        let bot = app.business_mut().unwrap().chatbot.as_mut().unwrap();
        bot.recipients.included.chat_ids = vec![1];
        bot.recipients.all_but_excluded = false;
        cx.notify();
    });
    act(&app, vcx, |app, window, cx| {
        app.open_business_picker(Recipients::Bot, false, window, cx)
    });
    assert!(!app.read_with(vcx, |app, _| app.business_picker_types()));
    back(&app, vcx);
    assert_eq!(view(&app, vcx), View::Chatbots);
    back(&app, vcx);
    assert_eq!(view(&app, vcx), View::Hub);
    assert_eq!(note(&app, vcx).as_deref(), Some(QuillApp::DEMO_LINE));
}
