//! The contract in `src/lib.rs`, against the text system this OS ships.
//! CI runs this file on macOS, Windows and Linux.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::PlatformTextSystem;

fn assert_holds(system: Arc<dyn PlatformTextSystem>, family: &str) {
    let violations = quill_text_conformance::check(system.as_ref(), family);
    assert!(
        violations.is_empty(),
        "{} contract violations:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn core_text_lays_out_bidi_text_in_visual_order() {
    assert_holds(Arc::new(gpui_apple::AppleTextSystem::new()), "Arial");
}

#[cfg(target_os = "windows")]
#[test]
fn direct_write_lays_out_bidi_text_in_visual_order() {
    assert_holds(gpui_windows::shaping_text_system().unwrap(), "Segoe UI");
}

#[cfg(target_os = "linux")]
#[test]
fn cosmic_text_lays_out_bidi_text_in_visual_order() {
    assert_holds(
        Arc::new(gpui_wgpu::CosmicTextSystem::new("DejaVu Sans")),
        "DejaVu Sans",
    );
}

/// The stand-in text system that GPUI tests (the input engine's, the composer's)
/// run on must order text the way the real ones do, or those tests prove nothing
/// about right-to-left text.
#[test]
fn the_test_text_system_lays_out_bidi_text_in_visual_order() {
    assert_holds(Arc::new(gpui::NoopTextSystem::new()), "Courier");
}
