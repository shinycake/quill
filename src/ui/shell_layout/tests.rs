use super::*;

fn input(width: f32) -> ShellInput {
    ShellInput {
        width,
        rail: 0.,
        list_width: 300.,
        chat_open: true,
        forum: ForumColumn::Hidden,
        topic_open: false,
        panels: None,
    }
}

#[test]
fn wide_window_keeps_list_chat_and_panel_side_by_side() {
    let layout = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..input(1400.)
    });
    assert!(!layout.one_column);
    assert_eq!(layout.list, Some(300.));
    assert!(layout.chat);
    assert_eq!(layout.panels, PanelPane::Column(INFO_PANEL_WIDTH));
    assert!(!layout.chat_back());
}

#[test]
fn panel_shrinks_before_it_replaces_the_chat() {
    // 300 list + 380 chat leaves 290 for the panel.
    let layout = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..input(970.)
    });
    assert_eq!(layout.panels, PanelPane::Column(290.));
    assert!(layout.chat);
    // Less than the panel's minimum: it takes the chat's place.
    let layout = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..input(900.)
    });
    assert_eq!(layout.panels, PanelPane::Fill);
    assert!(!layout.chat);
    assert_eq!(layout.list, Some(300.));
}

#[test]
fn list_never_squeezes_the_chat_below_its_minimum() {
    let layout = shell_layout(ShellInput {
        list_width: 560.,
        ..input(800.)
    });
    assert_eq!(layout.list, Some(800. - CHAT_MIN_WIDTH));
    // Never below the list's own minimum either.
    let layout = shell_layout(ShellInput {
        list_width: 560.,
        ..input(one_column_below(0.))
    });
    assert_eq!(layout.list, Some(MIN_SIDEBAR_WIDTH));
}

#[test]
fn narrow_window_shows_the_chat_alone_with_a_way_back() {
    let layout = shell_layout(input(500.));
    assert!(layout.one_column);
    assert_eq!(layout.list, None);
    assert!(layout.chat);
    assert!(layout.chat_back());
}

#[test]
fn narrow_window_without_a_chat_shows_the_list_filling_it() {
    let layout = shell_layout(ShellInput {
        chat_open: false,
        rail: 52.,
        ..input(500.)
    });
    assert_eq!(layout.list, Some(448.));
    assert!(!layout.chat);
    assert!(!layout.chat_back());
}

#[test]
fn narrow_window_gives_an_open_panel_the_whole_window() {
    let layout = shell_layout(ShellInput {
        panels: Some(DOWNLOADS_PANEL_WIDTH),
        ..input(500.)
    });
    assert_eq!(layout.panels, PanelPane::Fill);
    assert_eq!(layout.list, None);
    assert!(!layout.chat);
}

#[test]
fn folder_rail_counts_toward_the_one_column_threshold() {
    let width = one_column_below(0.) + 10.;
    assert!(!shell_layout(input(width)).one_column);
    assert!(
        shell_layout(ShellInput {
            rail: 68.,
            ..input(width)
        })
        .one_column
    );
}

#[test]
fn narrow_forum_shows_topics_until_one_is_open() {
    let forum = ShellInput {
        forum: ForumColumn::Replacing,
        ..input(500.)
    };
    let layout = shell_layout(forum);
    assert_eq!(layout.forum, ForumPane::Fill);
    assert!(!layout.chat);
    let layout = shell_layout(ShellInput {
        topic_open: true,
        ..forum
    });
    assert_eq!(layout.forum, ForumPane::Hidden);
    assert!(layout.chat_back());
}

#[test]
fn forum_column_replaces_the_list_on_a_two_column_window() {
    let layout = shell_layout(ShellInput {
        forum: ForumColumn::Replacing,
        ..input(900.)
    });
    assert_eq!(layout.list, None);
    assert_eq!(layout.forum, ForumPane::Column { chats_button: true });
    assert!(layout.chat);
}

#[test]
fn forum_column_beside_leaves_the_chat_its_minimum() {
    let layout = shell_layout(ShellInput {
        forum: ForumColumn::Beside,
        list_width: 560.,
        ..input(1100.)
    });
    assert_eq!(
        layout.list,
        Some(1100. - FORUM_COLUMN_WIDTH - CHAT_MIN_WIDTH)
    );
}
