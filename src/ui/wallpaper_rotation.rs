//! The freeform wallpaper turning on send (tdesktop
//! `ChatTheme::rotateComplexGradientBackground`, called by the history
//! widgets when an outgoing message appears). Each outgoing message moves
//! the gradient half a phase; the new picture renders on the wallpaper
//! thread and crossfades in over 200 ms. "Wallpaper rotation" in Battery
//! and animations turns it off; reduced motion swaps without the fade.

use super::app::QuillApp;
use super::wallpaper::{ChatLook, rotate};
use super::wallpaper_cache::fade_layer;
use gpui_kit::*;

impl QuillApp {
    /// The fading-out picture while the open chat's wallpaper turns; keeps
    /// the frame clock ticking until the fade ends.
    pub(super) fn wallpaper_fade(
        &self,
        look: &ChatLook,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let layer = fade_layer(look.wallpaper.as_ref()?)?;
        self.request_animation_tick(60, cx);
        Some(layer)
    }

    /// An outgoing message appeared at the bottom of the open chat.
    pub(super) fn rotate_wallpaper_on_send(&mut self, cx: &mut Context<Self>) {
        if quill::power_saving::on(quill::power_saving::Flag::ChatBackground) {
            return;
        }
        let look = self.chat_look(self.viewed_chat().map(|c| c.0), cx);
        if look.wallpaper.as_ref().is_some_and(rotate) {
            cx.notify();
        }
    }
}
