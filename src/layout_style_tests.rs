use super::*;

fn roomy() -> Readings {
    Readings {
        contrast: Contrast::Normal,
        power: Power::Normal,
        backdrop: BackdropKind::Picture,
    }
}

fn floating(readings: Readings) -> Chrome {
    resolve(Look::Cards, || readings)
}

#[test]
fn classic_choice_is_lines_at_any_width() {
    for width in [320., 600., 601., 925., 926., 2560.] {
        assert_eq!(look_for(LayoutStyle::Classic, None, width), Look::Lines);
    }
}

#[test]
fn floating_at_600_and_under_is_lines() {
    for width in [320., 560., 600., 600.5] {
        assert_eq!(
            look_for(LayoutStyle::Floating, None, width),
            Look::Lines,
            "width {width}"
        );
    }
    assert_eq!(look_for(LayoutStyle::Floating, None, 601.), Look::Cards);
}

#[test]
fn lines_are_classic_whatever_the_readings() {
    let asked = std::cell::Cell::new(false);
    let read = || {
        asked.set(true);
        roomy()
    };
    assert_eq!(resolve(Look::Lines, read), Chrome::CLASSIC);
    assert!(!asked.get(), "lines read nothing");
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
        look_for(LayoutStyle::Classic, Some(LayoutStyle::Floating), 1440.),
        Look::Cards
    );
    assert_eq!(
        look_for(LayoutStyle::Floating, Some(LayoutStyle::Classic), 1440.),
        Look::Lines
    );
    assert_eq!(
        LayoutStyle::parse(" Floating "),
        Some(LayoutStyle::Floating)
    );
    assert_eq!(LayoutStyle::parse("classic"), Some(LayoutStyle::Classic));
    assert_eq!(LayoutStyle::parse("cards"), None);
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
