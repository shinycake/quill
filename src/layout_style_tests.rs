use super::*;

fn roomy() -> Readings {
    Readings {
        width: 1440.,
        floating_from: FLOATING_FROM,
        contrast: Contrast::Normal,
        power: Power::Normal,
        backdrop: BackdropKind::Picture,
    }
}

fn floating(readings: Readings) -> Chrome {
    resolve(LayoutStyle::Floating, None, readings)
}

#[test]
fn classic_holds_todays_literals() {
    // The contract every render site is held to: Classic draws what Quill
    // drew before the layout existed.
    let c = CLASSIC;
    assert_eq!(c.gutter, 0.);
    assert_eq!(c.bar_gap, 0.);
    assert_eq!(c.radius, 0.);
    assert_eq!(c.divider, 1.);
    assert_eq!(c.chat_column, None);
    assert_eq!(c.bar_height, 0.);
    assert_eq!(c.placement, Placement::Stacked);
    assert_eq!(c.backdrop, Backdrop::History);
    assert_eq!(c.selected_row, SelectedRow::Tint);
    assert_eq!(c.folder_tabs, FolderTabs::Underline);
    assert_eq!(c.search, SearchShape::Field);
    assert_eq!(c.panels, PanelDock::Column);
    assert_eq!(c.title, TitleStrip::Lined);
    assert_eq!(c.rows, RowScale::Quill);
    assert_eq!(c.clip, Clip::Square);
    assert_eq!(c.drawer_to, None);
    let m = c.motion;
    assert_eq!(m.curve, Curve::OutCubic);
    assert_eq!(
        (m.panel_in_ms, m.panel_out_ms, m.sidebar_ms),
        (200, 200, 200)
    );
    assert_eq!(m.bar_reveal, BarReveal::Slide);
    assert_eq!(m.menu, MenuEnter::FadeSlide);
    assert_eq!(m.chat_switch, PageSwitch::Cut);
    assert_eq!(m.folder_page, PageSwitch::Cut);
    assert_eq!(m.composer_ms, 0);
    assert_eq!(m.slides, SlideDraw::Live);
}

#[test]
fn classic_choice_is_classic_at_any_width() {
    for width in [320., 600., 601., 925., 926., 2560.] {
        let readings = Readings { width, ..roomy() };
        assert_eq!(
            resolve(LayoutStyle::Classic, None, readings),
            Chrome::CLASSIC
        );
    }
}

#[test]
fn floating_at_600_and_under_is_classic() {
    for width in [320., 560., 600., 600.5] {
        let readings = Readings { width, ..roomy() };
        assert_eq!(floating(readings), Chrome::CLASSIC, "width {width}");
    }
    let at_601 = floating(Readings {
        width: 601.,
        ..roomy()
    });
    assert_eq!(at_601.metrics, &FLOATING);
}

#[test]
fn floating_roomy_is_shadowed_without_an_edge() {
    let chrome = floating(roomy());
    assert_eq!(chrome.metrics, &FLOATING);
    assert_eq!(chrome.elevation, Elevation::Shadowed);
    assert_eq!(chrome.edge, Edge::None);
}

#[test]
fn high_contrast_is_flat_with_hairlines() {
    let chrome = floating(Readings {
        contrast: Contrast::High,
        ..roomy()
    });
    assert_eq!(
        (chrome.metrics, chrome.elevation, chrome.edge),
        (&FLOATING, Elevation::Flat, Edge::Hairline)
    );
}

#[test]
fn saving_power_is_flat_and_edges_only_a_plain_backdrop() {
    let picture = floating(Readings {
        power: Power::Saver,
        ..roomy()
    });
    assert_eq!(
        (picture.elevation, picture.edge),
        (Elevation::Flat, Edge::None)
    );
    let plain = floating(Readings {
        power: Power::Saver,
        backdrop: BackdropKind::Plain { card_contrast: 3. },
        ..roomy()
    });
    assert_eq!(
        (plain.elevation, plain.edge),
        (Elevation::Flat, Edge::Hairline)
    );
}

#[test]
fn a_plain_backdrop_under_the_floor_gets_a_hairline() {
    let low = floating(Readings {
        backdrop: BackdropKind::Plain {
            card_contrast: CARD_CONTRAST_FLOOR - 0.01,
        },
        ..roomy()
    });
    assert_eq!(
        (low.elevation, low.edge),
        (Elevation::Shadowed, Edge::Hairline)
    );
    let enough = floating(Readings {
        backdrop: BackdropKind::Plain {
            card_contrast: CARD_CONTRAST_FLOOR,
        },
        ..roomy()
    });
    assert_eq!(enough.edge, Edge::None);
}

#[test]
fn the_override_wins_over_the_choice() {
    assert_eq!(
        resolve(
            LayoutStyle::Classic,
            Some(LayoutOverride::Floating),
            roomy()
        )
        .metrics,
        &FLOATING
    );
    assert_eq!(
        resolve(
            LayoutStyle::Floating,
            Some(LayoutOverride::Classic),
            roomy()
        ),
        Chrome::CLASSIC
    );
    assert_eq!(
        LayoutOverride::parse(" Floating "),
        Some(LayoutOverride::Floating)
    );
    assert_eq!(
        LayoutOverride::parse("classic"),
        Some(LayoutOverride::Classic)
    );
    assert_eq!(LayoutOverride::parse("cards"), None);
}

#[test]
fn the_choice_round_trips_and_defaults_to_classic() {
    let json = serde_json::to_string(&LayoutStyle::Floating).unwrap_or_default();
    assert_eq!(json, "\"floating\"");
    let back: Option<LayoutStyle> = serde_json::from_str(&json).ok();
    assert_eq!(back, Some(LayoutStyle::Floating));
    assert_eq!(LayoutStyle::default(), LayoutStyle::Classic);
}

#[test]
fn floating_follows_webk_breakpoints() {
    assert_eq!(FLOATING_FROM, 601.);
    assert_eq!(FLOATING.drawer_to, Some(925.));
    assert_eq!(FLOATING.chat_column, Some(696.));
}

#[test]
fn contrast_ratio_is_wcags() {
    assert!((contrast_ratio(1., 0.) - 21.).abs() < 1e-4);
    assert!((contrast_ratio(0.5, 0.5) - 1.).abs() < 1e-6);
    assert_eq!(contrast_ratio(0.2, 0.8), contrast_ratio(0.8, 0.2));
}

#[test]
fn curves_run_from_rest_to_rest() {
    for curve in [Curve::OutCubic, Curve::Standard] {
        assert!(curve.ease(0.).abs() < 1e-4);
        assert!((curve.ease(1.) - 1.).abs() < 1e-4);
        let mut last = 0.;
        for step in 1..=20 {
            let eased = curve.ease(step as f32 / 20.);
            assert!(eased >= last, "{curve:?} never goes back");
            last = eased;
        }
    }
    // Web K's standard curve: about 0.78 of the way at half time
    // (cubic-bezier(.4, 0, .2, 1) as browsers compute it).
    assert!((Curve::Standard.ease(0.5) - 0.7756).abs() < 0.005);
    assert!((Curve::OutCubic.ease(0.5) - 0.875).abs() < 1e-4);
}
