//! Pictures in the main window's dialogs go through its bounded image
//! cache.
//!
//! `QuillApp` draws inside `image_budget::CacheScope`, but kit dialogs draw
//! in the window's root layer above it and paint in a deferred pass after
//! everything else, outside any scope. Their pictures went to GPUI's own
//! asset cache, which never lets one go: every sticker pack opened in the
//! sticker set box kept all its stickers, about 1 MB of RAM and 1 MB of GPU
//! memory each at 512 px. A dialog's surface now draws inside a scope over
//! the main window's cache, so its pictures are evicted like any other once
//! they are off screen.
//!
//! A dialog shown in a chat window draws through that window's own cache
//! (`ChatWindowView::images`), which counts that window's frames.

use super::app::QuillApp;
use super::image_budget::CacheScope;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;

/// `dialog`, drawing its surface through the cache of the window it opens
/// in: the main window's or a chat window's.
pub(super) fn cover(dialog: Dialog, app: &QuillApp, window: &Window, cx: &App) -> Dialog {
    let id = window.window_handle().window_id();
    let cache = if app.windows.main.is_some_and(|main| main.window_id() == id) {
        app.slices.image_cache()
    } else {
        app.windows.image_cache_for(id, cx)
    };
    match cache {
        Some(cache) => {
            dialog.wrap_surface(move |surface| CacheScope::new(cache, surface).into_any_element())
        }
        None => dialog,
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
    fn a_dialogs_pictures_are_painted_from_the_bounded_cache(cx: &mut TestAppContext) {
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
        // Decodes land between frames; the frame after paints them.
        for _ in 0..4 {
            vcx.run_until_parked();
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
        let images: Vec<_> = cache.read_with(&vcx, |cache, _| {
            paths
                .into_iter()
                .map(|path| cache.loaded(&Resource::Path(path.into())))
                .collect()
        });
        vcx.update(|window, _| {
            for image in images {
                let image = image.expect("loaded through the bounded cache");
                // GPUI's own copy would be the one in the atlas otherwise.
                assert!(window.has_image_atlas_entry(&image), "painted from it");
            }
        });
    }
}
