//! The community screens in a window over the `ready-communities` demo:
//! the collapsed row, Pick → Visibility → back, and the requests undo.
use super::CommunityBox;
use super::requests::UNDO_DURATION;
use crate::ui::app::QuillApp;
use crate::ui::chat_row::ChatListItem;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

const RUSTACEANS: i64 = 900;
const GUILD: i64 = 901;
const OWN_CHANNEL: i64 = 66;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-communities");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn listed_chats(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.chat_list
            .items
            .iter()
            .filter_map(|item| match item {
                ChatListItem::Chat { id, .. } => Some(id.0),
                _ => None,
            })
            .collect()
    })
}

#[gpui_kit::test]
fn a_collapsed_community_takes_its_chats_out_of_the_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    let rows: Vec<i64> = app.read_with(&vcx, |app, _| {
        app.chat_list
            .items
            .iter()
            .filter_map(|item| match item {
                ChatListItem::Community { community_id, .. } => Some(*community_id),
                _ => None,
            })
            .collect()
    });
    assert_eq!(rows, vec![RUSTACEANS]);
    let chats = listed_chats(&app, &mut vcx);
    assert!(!chats.contains(&61) && !chats.contains(&62), "{chats:?}");
    assert!(chats.contains(&65) && chats.contains(&67), "{chats:?}");
}

#[gpui_kit::test]
fn ungrouping_brings_the_chats_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        if let Some(community) = session.groups.communities.get_mut(&RUSTACEANS) {
            community.are_chats_collapsed = false;
        }
    });
    frames(&mut vcx);
    let chats = listed_chats(&app, &mut vcx);
    assert!(chats.contains(&61) && chats.contains(&62), "{chats:?}");
}

#[gpui_kit::test]
fn cancel_on_the_visibility_box_goes_back_to_the_picker(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.open_add_to_community(OWN_CHANNEL, cx)
    });
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.choose_community_visibility(
            RUSTACEANS,
            OWN_CHANNEL,
            Some(super::Back::Pick {
                chat_id: OWN_CHANNEL,
            }),
            cx,
        )
    });
    frames(&mut vcx);
    let open = app.read_with(&vcx, |app, _| {
        matches!(
            app.admin.community.flow.open,
            Some(CommunityBox::Visibility { hidden: false, .. })
        )
    });
    assert!(open, "the visibility box opens on Visible");
    app.update_in(&mut vcx, |app, window, cx| {
        app.community_visibility_back(window, cx)
    });
    frames(&mut vcx);
    let back = app.read_with(&vcx, |app, _| {
        matches!(
            app.admin.community.flow.open,
            Some(CommunityBox::Pick {
                chat_id: OWN_CHANNEL
            })
        )
    });
    assert!(back, "Cancel returns to the picker");
}

fn pending_undo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<i64> {
    app.read_with(vcx, |app, _| match &app.admin.community.flow.open {
        Some(CommunityBox::Requests { undo, .. }) => undo.as_ref().map(|undo| undo.chat_id),
        _ => None,
    })
}

#[gpui_kit::test]
fn an_answer_waits_out_its_undo_time(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| app.open_community_requests(GUILD, cx));
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.answer_community_request(70, true, cx)
    });
    frames(&mut vcx);
    assert_eq!(pending_undo(&app, &mut vcx), Some(70));
    vcx.executor().advance_clock(UNDO_DURATION);
    frames(&mut vcx);
    assert_eq!(
        pending_undo(&app, &mut vcx),
        None,
        "sent once the time is up"
    );
}

#[gpui_kit::test]
fn undo_drops_the_answer_and_a_new_answer_replaces_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| app.open_community_requests(GUILD, cx));
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.answer_community_request(70, true, cx)
    });
    app.update(&mut vcx, |app, cx| app.undo_community_request(cx));
    frames(&mut vcx);
    assert_eq!(pending_undo(&app, &mut vcx), None);
    app.update(&mut vcx, |app, cx| {
        app.answer_community_request(70, false, cx)
    });
    app.update(&mut vcx, |app, cx| {
        app.answer_community_request(69, true, cx)
    });
    frames(&mut vcx);
    assert_eq!(pending_undo(&app, &mut vcx), Some(69));
}

fn open_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> &'static str {
    app.read_with(vcx, |app, _| match &app.admin.community.flow.open {
        Some(CommunityBox::Create { .. }) => "create",
        Some(CommunityBox::CreateVisibility { .. }) => "create-visibility",
        Some(CommunityBox::Pick { .. }) => "pick",
        Some(CommunityBox::MuteFor { .. }) => "mute-for",
        Some(_) => "other",
        None => "none",
    })
}

#[gpui_kit::test]
fn create_community_asks_a_name_then_the_visibility_and_comes_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_new_community(OWN_CHANNEL, window, cx)
    });
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.submit_new_community_identity(window, cx)
    });
    frames(&mut vcx);
    assert_eq!(open_box(&app, &mut vcx), "create", "an empty name stays");
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(CommunityBox::Create { draft, .. }) = &app.admin.community.flow.open {
            draft
                .name
                .update(cx, |name, cx| name.set_value("Field Guild", window, cx));
        }
        app.submit_new_community_identity(window, cx)
    });
    frames(&mut vcx);
    assert_eq!(open_box(&app, &mut vcx), "create-visibility");
    app.update_in(&mut vcx, |app, window, cx| {
        app.community_visibility_back(window, cx)
    });
    frames(&mut vcx);
    let name = app.read_with(&vcx, |app, cx| match &app.admin.community.flow.open {
        Some(CommunityBox::Create { draft, .. }) => draft.name.read(cx).value().to_string(),
        _ => String::new(),
    });
    assert_eq!(name, "Field Guild", "Cancel keeps the typed name");
}

#[gpui_kit::test]
fn the_row_menu_mute_submenu_opens_and_leads_to_mute_for(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.admin.community.flow.row_menu = Some(super::RowMenu::at(
            RUSTACEANS,
            gpui_kit::point(px(100.), px(100.)),
        ));
        app.open_chat_submenu(crate::ui::menu_states::ChatMenuSubmenu::Mute, cx);
    });
    frames(&mut vcx);
    let submenu = app.read_with(&vcx, |app, _| {
        app.admin
            .community
            .flow
            .row_menu
            .and_then(|menu| menu.submenu)
    });
    assert_eq!(submenu, Some(crate::ui::menu_states::ChatMenuSubmenu::Mute));
    app.update(&mut vcx, |app, cx| app.close_chat_submenu(cx));
    let submenu = app.read_with(&vcx, |app, _| {
        app.admin
            .community
            .flow
            .row_menu
            .and_then(|menu| menu.submenu)
    });
    assert_eq!(submenu, None, "Left or Escape folds the submenu only");
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_community_mute_for(RUSTACEANS, window, cx)
    });
    frames(&mut vcx);
    assert_eq!(open_box(&app, &mut vcx), "mute-for");
    let shown = app.read_with(&vcx, |app, cx| app.community_mute_for_value(cx));
    assert_eq!(shown, 8 * 60 * 60, "the fields open on tdesktop's 8 hours");
}

#[gpui_kit::test]
fn the_mute_submenu_stays_inside_the_window_near_its_edge(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        app.admin.community.flow.row_menu = Some(super::RowMenu::at(
            RUSTACEANS,
            gpui_kit::point(px(1000.), px(740.)),
        ));
        app.open_chat_submenu(crate::ui::menu_states::ChatMenuSubmenu::Mute, cx);
    });
    frames(&mut vcx);
    let submenu = vcx
        .debug_bounds("community-mute-submenu")
        .expect("the Mute submenu is drawn");
    let window = vcx.update(|window, _| window.viewport_size());
    assert!(
        submenu.origin.x >= px(0.)
            && submenu.origin.y >= px(0.)
            && submenu.right() <= window.width
            && submenu.bottom() <= window.height,
        "{submenu:?} inside {window:?}"
    );
}

#[gpui_kit::test]
fn ungroup_waits_while_show_as_one_chat_is_changing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.request_for_community(
                quill::state::RequestPurpose::Communities(
                    quill::state::CommunitiesPurpose::ToggleCollapsed { collapsed: true },
                ),
                RUSTACEANS,
            );
        }
        app.show_community_box(
            CommunityBox::Ungroup {
                community_id: RUSTACEANS,
            },
            cx,
        );
        app.confirm_community_box(cx);
    });
    frames(&mut vcx);
    let open = app.read_with(&vcx, |app, _| {
        matches!(
            app.admin.community.flow.open,
            Some(CommunityBox::Ungroup { .. })
        )
    });
    assert!(
        open,
        "Enter on Ungroup does nothing while the toggle is in flight"
    );
}
