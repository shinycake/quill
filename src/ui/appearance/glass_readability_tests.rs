//! Text on glass stays readable. For every Regular recipe (light and dark,
//! with and without Increase contrast), every grey backdrop from 0 to 255
//! and a few saturated colours, the theme's popover text must reach WCAG
//! 4.5:1 on the glass interior and its secondary text 3:1. The colours are
//! the ones each shipped theme really puts into the kit (this lives with
//! the UI tests because the kit's colours exist only there).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::{AppContext, Entity, Hsla, Rgba, TestAppContext, VisualTestContext, px, size};
use quill::app_theme::{BuiltinTheme, ThemeId};
use quill::glass::{Appearance, REGULAR_RECIPES, Recipe, interior, shade};
use quill::settings::{AutoNight, ThemeChoice};
use quill::theme_colorize::contrast;

const POPOVER_MIN: f64 = 4.5;
const MUTED_MIN: f64 = 3.0;

const SATURATED: [[f32; 3]; 6] = [
    [1., 0., 0.],
    [0., 1., 0.],
    [0., 0., 1.],
    [1., 1., 0.],
    [0., 1., 1.],
    [1., 0., 1.],
];

/// One theme's text colours as the kit holds them.
struct Text {
    theme: String,
    appearance: Appearance,
    popover: Hsla,
    muted: Hsla,
}

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = std::env::temp_dir().join(format!("quill-glass-read-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(600.), px(400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// The text colours of every built-in theme and of high contrast.
fn shipped_text(cx: &mut TestAppContext) -> Vec<Text> {
    let (app, mut vcx) = new_app(cx);
    let mut out = Vec::new();
    app.update_in(&mut vcx, |app, _, cx| {
        let mut capture =
            |name: String, app: &mut QuillApp, cx: &mut gpui_kit::Context<QuillApp>| {
                app.settings.appearance_applied = None;
                app.apply_appearance(cx);
                let theme = cx.theme();
                out.push(Text {
                    theme: name,
                    appearance: if theme.is_dark() {
                        Appearance::Dark
                    } else {
                        Appearance::Light
                    },
                    popover: theme.popover_foreground,
                    muted: theme.muted_foreground,
                });
            };
        app.appearance.auto_night = AutoNight::Off;
        for builtin in BuiltinTheme::ALL {
            if builtin.is_dark() {
                app.appearance.theme = ThemeChoice::Dark;
                app.appearance.night_theme = ThemeId::Builtin(builtin);
            } else {
                app.appearance.theme = ThemeChoice::Light;
                app.appearance.day_theme = ThemeId::Builtin(builtin);
            }
            capture(builtin.name().to_string(), app, cx);
        }
        app.appearance.theme = ThemeChoice::HighContrast;
        capture("High contrast".into(), app, cx);
    });
    out
}

/// `text` over `interior`, as 0xRRGGBB (a translucent colour blends).
fn over(text: Hsla, interior: [f32; 3]) -> u32 {
    let text = Rgba::from(text);
    let channel = |t: f32, under: f32| {
        let v = t * text.a + under * (1. - text.a);
        (v.clamp(0., 1.) * 255.).round() as u32
    };
    let [r, g, b] = interior;
    (channel(text.r, r) << 16) | (channel(text.g, g) << 8) | channel(text.b, b)
}

fn quantized(color: [f32; 3]) -> u32 {
    let q = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (q(color[0]) << 16) | (q(color[1]) << 8) | q(color[2])
}

/// The worst contrasts of `text` over one recipe's interiors.
fn worst(recipe: &Recipe, text: &Text) -> (f64, f64) {
    let mut backdrops: Vec<[f32; 3]> = (0..=255u32)
        .map(|k| {
            let b = k as f32 / 255.;
            [b, b, b]
        })
        .collect();
    backdrops.extend(SATURATED);
    let mut popover = f64::MAX;
    let mut muted = f64::MAX;
    for backdrop in backdrops {
        let glass = if backdrop[0] == backdrop[1] && backdrop[1] == backdrop[2] {
            let v = interior(recipe, backdrop[0]);
            [v, v, v]
        } else {
            shade(recipe, backdrop)
        };
        let under = quantized(glass);
        popover = popover.min(contrast(over(text.popover, glass), under));
        muted = muted.min(contrast(over(text.muted, glass), under));
    }
    (popover, muted)
}

#[gpui_kit::test]
fn text_stays_readable_on_every_recipe_over_any_backdrop(cx: &mut TestAppContext) {
    let themes = shipped_text(cx);
    assert!(themes.len() >= 7, "every built-in theme and high contrast");
    let mut failures = Vec::new();
    for (appearance, increase_contrast, recipe) in REGULAR_RECIPES {
        let (mut popover, mut muted) = (f64::MAX, f64::MAX);
        for text in themes.iter().filter(|t| t.appearance == appearance) {
            let (p, m) = worst(&recipe, text);
            eprintln!(
                "{appearance:?} contrast={increase_contrast} {:<14} popover {p:.2} muted {m:.2} (text {:06x} / {:06x})",
                text.theme,
                over(text.popover, [0.; 3]),
                over(text.muted, [0.; 3])
            );
            if p < POPOVER_MIN || m < MUTED_MIN {
                failures.push(format!(
                    "{appearance:?} contrast={increase_contrast} {}: popover {p:.2}, muted {m:.2}",
                    text.theme
                ));
            }
            popover = popover.min(p);
            muted = muted.min(m);
        }
        eprintln!(
            "== {appearance:?} contrast={increase_contrast}: min popover {popover:.2}, min muted {muted:.2}"
        );
    }
    assert!(
        failures.is_empty(),
        "unreadable glass:\n{}",
        failures.join("\n")
    );
}
