//! The main window's bounded image cache over the whole window, not only
//! over `QuillApp`.
//!
//! Dialogs, sheets and notifications render in kit `Root` overlays above
//! `QuillApp`, outside its `CacheScope`, so the pictures they load went to
//! GPUI's own asset cache, which never lets one go: every sticker pack
//! opened kept all its stickers, about 1 MB of RAM and 1 MB of GPU memory
//! each at 512 px. A `Root` plugin wraps the main window's whole surface,
//! overlays included, in the same scope; the bounded cache then evicts
//! them like any other picture once they are off screen.

use super::image_budget::{BoundedImageCache, CacheScope};
use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::*;
use std::cell::RefCell;

thread_local! {
    /// The main window and its cache.
    static COVERED: RefCell<Option<(WindowId, Entity<BoundedImageCache>)>> =
        const { RefCell::new(None) };
}

/// Mount the plugin on windows opened from now on. Call before the main
/// window's `Root` is built.
pub(super) fn register(cx: &mut App) {
    Root::register_plugin::<Cover>(cx, |_, _| Cover);
}

/// The main window draws with `cache`; cover its whole surface with it.
pub(super) fn cover(window: &Window, cache: Entity<BoundedImageCache>) {
    let id = window.window_handle().window_id();
    COVERED.with(|covered| *covered.borrow_mut() = Some((id, cache)));
}

struct Cover;

impl Render for Cover {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl RootPlugin for Cover {
    fn decorate(
        &self,
        surface: AnyElement,
        _root: &Root,
        window: &mut Window,
        _cx: &mut App,
    ) -> impl IntoElement {
        let id = window.window_handle().window_id();
        let cache = COVERED.with(|covered| {
            covered
                .borrow()
                .as_ref()
                .filter(|(covered, _)| *covered == id)
                .map(|(_, cache)| cache.clone())
        });
        match cache {
            Some(cache) => CacheScope::new(cache, surface).into_any_element(),
            None => surface,
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use crate::ui::app::QuillApp;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, Resource, TestAppContext, VisualTestContext, px, size};
    use quill::state::StickerSetViewStage;

    #[gpui_kit::test]
    fn a_dialogs_pictures_load_through_the_bounded_cache(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        // The sticker set box, a kit dialog above `QuillApp`.
        let demo = ScreenshotDemo::named("ready-sticker-set-owned");
        let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
            let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
            *slot_in.borrow_mut() = Some(view.clone());
            Root::new(cx.new(|_| QuillShell::new(view)), window, cx)
        });
        let app = slot.borrow().clone().expect("app");
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        vcx.run_until_parked();
        for _ in 0..3 {
            vcx.update(|window, cx| window.render_frame(cx));
        }

        let (paths, cache) = app.read_with(&vcx, |app, _| {
            let stage = app
                .demo_session
                .as_ref()
                .and_then(|s| s.stickers.sticker_set_view.as_ref())
                .map(|view| &view.stage);
            let Some(StickerSetViewStage::Ready { stickers, .. }) = stage else {
                panic!("the sticker set box is open");
            };
            let paths: Vec<_> = stickers.iter().filter_map(|s| app.panel_still(s)).collect();
            (paths, app.slices.image_cache().expect("cache"))
        });
        assert!(!paths.is_empty(), "the demo's stickers have files");
        cache.read_with(&vcx, |cache, _| {
            for path in paths {
                assert!(cache.holds(&Resource::Path(path.into())));
            }
        });
    }
}
