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
        docked: 1.,
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

#[test]
fn replacing_forum_never_leaves_the_chat_below_its_minimum() {
    for width in 620..660 {
        let layout = shell_layout(ShellInput {
            forum: ForumColumn::Replacing,
            topic_open: true,
            ..input(width as f32)
        });
        assert!(layout.one_column, "{width}");
        assert!(layout.chat && layout.chat_back(), "{width}");
    }
    let width = FORUM_COLUMN_WIDTH + CHAT_MIN_WIDTH;
    let layout = shell_layout(ShellInput {
        forum: ForumColumn::Replacing,
        topic_open: true,
        ..input(width)
    });
    assert!(!layout.one_column);
    assert_eq!(layout.forum, ForumPane::Column { chats_button: true });
    assert!(layout.chat);
}

#[test]
fn one_column_boundary_is_exact() {
    for rail in [0., 68.] {
        let below = one_column_below(rail);
        let at = shell_layout(ShellInput {
            rail,
            ..input(below)
        });
        assert!(!at.one_column, "rail {rail}");
        assert_eq!(at.list, Some(MIN_SIDEBAR_WIDTH));
        let under = shell_layout(ShellInput {
            rail,
            ..input(below - 0.5)
        });
        assert!(under.one_column, "rail {rail}");
    }
}

#[test]
fn panel_boundary_is_exact() {
    for rail in [0., 68.] {
        let list = 300.;
        let width = rail + list + CHAT_MIN_WIDTH + PANEL_MIN_WIDTH;
        let panel = ShellInput {
            rail,
            panels: Some(INFO_PANEL_WIDTH),
            ..input(width)
        };
        let at = shell_layout(panel);
        assert_eq!(at.panels, PanelPane::Column(PANEL_MIN_WIDTH), "rail {rail}");
        assert!(at.chat);
        let under = shell_layout(ShellInput {
            width: width - 0.5,
            ..panel
        });
        assert_eq!(under.panels, PanelPane::Fill, "rail {rail}");
        assert!(!under.chat);
    }
}

#[test]
fn back_steps_to_topics_only_in_a_one_column_forum() {
    let forum = ShellInput {
        forum: ForumColumn::Replacing,
        topic_open: true,
        ..input(500.)
    };
    assert!(forum.back_to_topics());
    assert!(
        !ShellInput {
            topic_open: false,
            ..forum
        }
        .back_to_topics()
    );
    assert!(
        !ShellInput {
            forum: ForumColumn::Hidden,
            ..forum
        }
        .back_to_topics()
    );
    assert!(
        !ShellInput {
            width: 900.,
            ..forum
        }
        .back_to_topics()
    );
}

#[test]
fn the_chat_pane_is_what_the_other_columns_leave() {
    let pane = |input: ShellInput| chat_pane_width(input, &shell_layout(input));
    // List beside the chat.
    assert_eq!(pane(input(1000.)), 700.);
    // Side panel beside it too: 300 list + 290 panel.
    assert_eq!(
        pane(ShellInput {
            panels: Some(INFO_PANEL_WIDTH),
            ..input(970.)
        }),
        CHAT_MIN_WIDTH
    );
    // The folder rail comes off as well.
    assert_eq!(
        pane(ShellInput {
            rail: 70.,
            ..input(1000.)
        }),
        630.
    );
    // One column: the whole window.
    assert_eq!(pane(input(400.)), 400.);
}

#[test]
fn a_toast_centres_over_the_conversation_pane() {
    let wide = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        rail: 60.,
        ..input(1400.)
    });
    assert_eq!(wide.chat_insets(60.), (360., INFO_PANEL_WIDTH));
    let narrow = shell_layout(input(500.));
    assert!(narrow.one_column);
    assert_eq!(narrow.chat_insets(0.), (0., 0.));
    let list_only = shell_layout(ShellInput {
        chat_open: false,
        ..input(500.)
    });
    assert_eq!(list_only.chat_insets(0.), (0., 0.));
}

#[test]
fn a_toast_centres_in_forum_and_fill_layouts() {
    let beside = shell_layout(ShellInput {
        forum: ForumColumn::Beside,
        rail: 60.,
        ..input(1400.)
    });
    assert!(matches!(beside.forum, ForumPane::Column { .. }));
    let (left, right) = beside.chat_insets(60.);
    assert_eq!(left, beside.list.unwrap_or(0.) + 60. + FORUM_COLUMN_WIDTH);
    assert_eq!(right, 0.);
    let replacing = shell_layout(ShellInput {
        forum: ForumColumn::Replacing,
        ..input(1400.)
    });
    assert_eq!(replacing.list, None);
    assert_eq!(replacing.chat_insets(0.), (FORUM_COLUMN_WIDTH, 0.));
    let fill = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..input(900.)
    });
    assert_eq!(fill.panels, PanelPane::Fill);
    assert_eq!(fill.chat_insets(0.), (0., 0.));
}

#[test]
fn a_hidden_list_column_gives_the_chat_the_whole_window() {
    let input = ShellInput {
        rail: 72.,
        docked: 0.,
        ..input(1200.)
    };
    let layout = shell_layout(input);
    assert!(!layout.one_column);
    // The list keeps its width to slide back in, but takes no room.
    assert_eq!(layout.list, Some(300.));
    assert_eq!(layout.list_column(72.), 0.);
    assert!(layout.list_column_hidden());
    assert!(!layout.resize_handle());
    assert!(layout.sidebar_toggle());
    assert_eq!(layout.chat_insets(72.), (0., 0.));
    assert_eq!(chat_pane_width(input, &layout), 1200.);
}

#[test]
fn a_sliding_list_column_narrows_without_squeezing_the_list() {
    let input = ShellInput {
        rail: 72.,
        docked: 0.5,
        ..input(1200.)
    };
    let layout = shell_layout(input);
    assert_eq!(layout.list, Some(300.));
    assert_eq!(layout.list_column(72.), 186.);
    assert!(layout.resize_handle());
    assert_eq!(layout.chat_insets(72.), (186., 0.));
    assert_eq!(chat_pane_width(input, &layout), 1200. - 186.);
}

#[test]
fn a_hidden_column_leaves_room_for_a_side_panel() {
    // Shown, 300 list + 380 chat leaves only 220: the panel fills.
    let shown = ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..input(900.)
    };
    assert_eq!(shell_layout(shown).panels, PanelPane::Fill);
    let layout = shell_layout(ShellInput {
        docked: 0.,
        ..shown
    });
    assert_eq!(layout.panels, PanelPane::Column(INFO_PANEL_WIDTH));
    assert!(layout.chat);
}

#[test]
fn one_column_ignores_a_hidden_list_column() {
    let narrow = one_column_below(0.) - 1.;
    let layout = shell_layout(ShellInput {
        chat_open: false,
        docked: 0.,
        ..input(narrow)
    });
    assert!(layout.one_column);
    assert_eq!(layout.list, Some(narrow));
    assert_eq!(layout.list_column(0.), narrow);
    assert!(!layout.sidebar_toggle());
    assert!(!layout.list_column_hidden());
}
