//! Tooltip timing, shared by every tooltip in the app.
//!
//! Telegram Desktop shows every tooltip after the pointer has rested on its
//! trigger for one second (`Ui::Tooltip::Show(1000, ...)` in
//! `lib_ui/ui/widgets/tooltip.cpp` `InstallTooltip`, and the same literal in
//! the history, media, emoji and bot-keyboard widgets) and hides it the
//! moment the pointer leaves (`Tooltip::Hide`). It has no "recently shown"
//! shortcut: a neighbouring trigger waits the full delay again.
//!
//! The kit's tooltips read [`TooltipDefaults`]; Quill's own pointer-anchored
//! link tooltip reads the same global through [`show_delay`], so both agree.

use gpui_base::TooltipDefaults;
use gpui_kit::App;
use std::time::Duration;

/// How long the pointer rests on a trigger before its tooltip shows.
pub(crate) const SHOW_DELAY: Duration = Duration::from_millis(1000);

/// How long a left tooltip lets the next one skip the delay: none, as in
/// Telegram Desktop.
pub(crate) const GRACE_PERIOD: Duration = Duration::ZERO;

/// The timing Quill installs at startup.
pub(crate) fn defaults() -> TooltipDefaults {
    TooltipDefaults::new()
        .with_show_delay(SHOW_DELAY)
        .with_grace_period(GRACE_PERIOD)
}

/// Installs the app-wide tooltip timing. `QuillApp::new_with_demo` calls it,
/// so the app and the screenshot demos both get it before any tooltip shows.
pub(crate) fn install(cx: &mut App) {
    defaults().install(cx);
}

/// The installed show delay (for tooltips the kit does not draw).
pub(crate) fn show_delay(cx: &App) -> Duration {
    TooltipDefaults::global(cx).show_delay()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_telegram_desktop() {
        let defaults = defaults();
        assert_eq!(defaults.show_delay(), Duration::from_millis(1000));
        assert_eq!(defaults.grace_period(), Duration::ZERO);
    }

    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn show_delay_reads_the_installed_global(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(show_delay(cx), Duration::from_millis(500));
            install(cx);
            assert_eq!(show_delay(cx), SHOW_DELAY);
        });
    }
}
