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
        panels_shown: 1.,
        gutter: 0.,
        chat_column: None,
        drawer_to: None,
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

#[test]
fn a_sliding_panel_column_narrows_and_the_chat_takes_the_rest() {
    let input = ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        panels_shown: 0.25,
        ..input(1400.)
    };
    let layout = shell_layout(input);
    assert_eq!(layout.panels, PanelPane::Column(INFO_PANEL_WIDTH));
    assert_eq!(layout.panel_column(), INFO_PANEL_WIDTH * 0.25);
    assert_eq!(layout.chat_insets(0.), (300., INFO_PANEL_WIDTH * 0.25));
    assert_eq!(
        chat_pane_width(input, &layout),
        1400. - 300. - INFO_PANEL_WIDTH * 0.25
    );
}

#[test]
fn a_panel_filling_the_chat_place_does_not_slide() {
    let layout = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        panels_shown: 0.25,
        ..input(900.)
    });
    assert_eq!(layout.panels, PanelPane::Fill);
    assert_eq!(layout.panels_shown, 1.);
    assert_eq!(layout.panel_column(), 0.);
}

// Floating (docs/decisions/floating-layout.md): gutters, the centred
// column's room, the drawer band.

fn floating(width: f32) -> ShellInput {
    let metrics = quill::layout_style::FLOATING;
    ShellInput {
        gutter: metrics.gutter,
        chat_column: metrics.chat_column,
        drawer_to: metrics.drawer_to,
        ..input(width)
    }
}

#[test]
fn the_list_column_owns_its_left_gutter() {
    let layout = shell_layout(floating(1440.));
    assert_eq!(layout.list_dock, ListDock::Beside);
    assert_eq!(layout.list, Some(300.));
    assert_eq!(layout.list_column(0.), 316.);
    assert_eq!(layout.list_column(72.), 388.);
    let input = floating(1440.);
    assert_eq!(chat_pane_width(input, &layout), 1440. - 316.);
}

#[test]
fn the_drag_limit_leaves_the_chat_its_minimum_and_the_gutters() {
    assert_eq!(
        max_list_width(1000., 0., false, 16.),
        1000. - 16. - CHAT_MIN_WIDTH
    );
    assert_eq!(
        max_list_width(1000., 0., true, 16.),
        1000. - 16. - 16. - FORUM_COLUMN_WIDTH - CHAT_MIN_WIDTH
    );
    assert_eq!(max_list_width(1000., 0., false, 0.), 1000. - CHAT_MIN_WIDTH);
}

#[test]
fn a_panel_docks_only_beside_a_full_column() {
    // 316 list column, then panel 300 + 16 and a 696 column with its two
    // gutters: 316 + 316 + 728 = 1360.
    let fits = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..floating(1360.)
    });
    assert_eq!(fits.panels, PanelPane::Column(INFO_PANEL_WIDTH));
    assert_eq!(fits.panel_column(), INFO_PANEL_WIDTH + 16.);
    let short = shell_layout(ShellInput {
        panels: Some(INFO_PANEL_WIDTH),
        ..floating(1359.)
    });
    assert_eq!(short.panels, PanelPane::Over(INFO_PANEL_WIDTH));
    assert!(short.chat);
    // Over leaves the pane its width.
    assert_eq!(short.panel_column(), 0.);
    assert_eq!(chat_pane_width(floating(1359.), &short), 1359. - 316.);
    assert_eq!(short.chat_insets(0.), (316., 0.));
}

#[test]
fn a_classic_panel_never_floats_over_the_chat() {
    for width in (620..2600).step_by(7) {
        let layout = shell_layout(ShellInput {
            panels: Some(INFO_PANEL_WIDTH),
            ..input(width as f32)
        });
        assert!(!matches!(layout.panels, PanelPane::Over(_)), "{width}");
        assert_eq!(layout.list_dock, ListDock::Beside);
    }
}

#[test]
fn forum_columns_take_their_gutter() {
    let beside = shell_layout(ShellInput {
        forum: ForumColumn::Beside,
        ..floating(1440.)
    });
    let input = floating(1440.);
    assert_eq!(
        chat_pane_width(input, &beside),
        1440. - 316. - 16. - FORUM_COLUMN_WIDTH
    );
    let replacing = shell_layout(ShellInput {
        forum: ForumColumn::Replacing,
        ..floating(1440.)
    });
    assert_eq!(replacing.list, None);
    assert_eq!(
        chat_pane_width(input, &replacing),
        1440. - 16. - FORUM_COLUMN_WIDTH
    );
    assert_eq!(replacing.chat_insets(0.).0, 16. + FORUM_COLUMN_WIDTH);
}

#[test]
fn webk_bands_at_600_601_925_926() {
    // 600: the window resolved to Classic's metrics, so the shell sees no
    // drawer band and lays out Classic's one column.
    let at_600 = shell_layout(input(600.));
    assert!(at_600.one_column);
    assert_eq!(at_600.list_dock, ListDock::Beside);
    for width in [601., 925.] {
        let layout = shell_layout(floating(width));
        assert_eq!(layout.list_dock, ListDock::Drawer, "{width}");
        assert!(!layout.one_column && layout.chat);
        assert_eq!(layout.docked, 0.);
        assert_eq!(layout.list_column(72.), 0., "no rail, no column");
        assert!(layout.list_column_hidden());
        assert!(!layout.resize_handle());
        assert!(layout.sidebar_toggle());
        assert_eq!(chat_pane_width(floating(width), &layout), width);
    }
    let at_926 = shell_layout(floating(926.));
    assert_eq!(at_926.list_dock, ListDock::Beside);
    assert_eq!(at_926.docked, 1.);
}

#[test]
fn the_drawer_ignores_docked_and_keeps_the_dragged_width() {
    let hidden = shell_layout(ShellInput {
        docked: 1.,
        list_width: 360.,
        ..floating(800.)
    });
    assert_eq!(hidden.docked, 0.);
    assert_eq!(hidden.list, Some(360.));
    // At 601 the drawer leaves 44 px of chat beside it and the gutters.
    let narrow = shell_layout(ShellInput {
        list_width: 900.,
        ..floating(601.)
    });
    assert_eq!(narrow.list, Some(601. - 32. - DRAWER_CHAT_PEEK));
}

#[test]
fn a_panel_in_the_drawer_band_is_over_the_chat() {
    for width in [601., 700., 925.] {
        let layout = shell_layout(ShellInput {
            panels: Some(SHARED_MEDIA_PANEL_WIDTH),
            ..floating(width)
        });
        assert_eq!(layout.panels, PanelPane::Over(SHARED_MEDIA_PANEL_WIDTH));
        assert!(layout.chat);
    }
}

#[test]
fn forums_in_the_drawer_band_hide_the_topic_column() {
    for forum in [ForumColumn::Beside, ForumColumn::Replacing] {
        let layout = shell_layout(ShellInput {
            forum,
            topic_open: true,
            ..floating(800.)
        });
        assert_eq!(layout.forum, ForumPane::Hidden);
        assert_eq!(layout.list_dock, ListDock::Drawer);
        assert!(
            !ShellInput {
                forum,
                topic_open: true,
                ..floating(800.)
            }
            .back_to_topics()
        );
    }
}

#[test]
fn above_the_band_the_pane_keeps_the_chat_minimum() {
    for width in 926..2600 {
        let width = width as f32;
        let input = ShellInput {
            list_width: 2000.,
            ..floating(width)
        };
        let layout = shell_layout(input);
        assert!(chat_pane_width(input, &layout) >= CHAT_MIN_WIDTH, "{width}");
    }
    for width in 601..=925 {
        let input = floating(width as f32);
        assert_eq!(chat_pane_width(input, &shell_layout(input)), width as f32);
    }
}
