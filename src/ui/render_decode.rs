//! Pictures that render makes from data on first use (the reply strip's
//! tinted emoji, a spoiler's blurred preview), made on the background
//! executor instead: the element draws nothing until its picture lands, then
//! the views that asked for it are notified, as `image_budget`'s sized loads
//! are.
#![deny(clippy::indexing_slicing)]

use super::lru::Lru;
use gpui_kit::{App, AppContext, EntityId, Window};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::thread::LocalKey;

/// Made pictures by key (`None`: making it failed, so it is not retried),
/// and the views waiting for each one being made.
pub(super) struct OffRender<K, V> {
    made: Lru<K, Option<V>>,
    waiting: HashMap<K, SmallVec<[EntityId; 2]>>,
    /// Hands a picture pushed out of the cache to `image_budget`, so its
    /// texture leaves the GPU atlas.
    retire: fn(V),
}

impl<K: Hash + Eq + Clone, V: Clone> OffRender<K, V> {
    pub(super) fn new(cap: usize, retire: fn(V)) -> Self {
        Self {
            made: Lru::new(cap),
            waiting: HashMap::new(),
            retire,
        }
    }

    fn land(&mut self, key: K, made: Option<V>) -> SmallVec<[EntityId; 2]> {
        let views = self.waiting.remove(&key).unwrap_or_default();
        if let Some(Some(old)) = self.made.insert(key, made) {
            (self.retire)(old);
        }
        views
    }
}

enum Lookup<V> {
    Made(Option<V>),
    Waiting,
    Start,
}

/// The picture under `key` in `cache`, or `None` while `make` runs on the
/// background executor (started on the first ask). The view drawing now is
/// notified when it lands.
pub(super) fn made_or_start<K, V>(
    cache: &'static LocalKey<RefCell<OffRender<K, V>>>,
    key: K,
    make: impl FnOnce() -> Option<V> + Send + 'static,
    window: &Window,
    cx: &mut App,
) -> Option<V>
where
    K: Hash + Eq + Clone + Send + 'static,
    V: Clone + Send + 'static,
{
    let view = window.current_view();
    let lookup = cache.with_borrow_mut(|cache| {
        if let Some(made) = cache.made.get(&key) {
            return Lookup::Made(made);
        }
        match cache.waiting.entry(key.clone()) {
            Entry::Occupied(mut waiting) => {
                if !waiting.get().contains(&view) {
                    waiting.get_mut().push(view);
                }
                Lookup::Waiting
            }
            Entry::Vacant(waiting) => {
                waiting.insert(SmallVec::from_elem(view, 1));
                Lookup::Start
            }
        }
    });
    match lookup {
        Lookup::Made(made) => made,
        Lookup::Waiting => None,
        Lookup::Start => {
            let task = cx.background_spawn(async move { make() });
            cx.spawn(async move |cx| {
                let made = task.await;
                let views = cache.with_borrow_mut(|cache| cache.land(key, made));
                cx.update(|cx| {
                    for view in views {
                        cx.notify(view);
                    }
                });
            })
            .detach();
            None
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{OffRender, made_or_start};
    use gpui_kit::{
        Context, ImageSource, IntoElement, ParentElement, Render, RenderImage, Styled,
        TestAppContext, VisualTestContext, Window, div, img, px,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    thread_local! {
        static CACHE: RefCell<OffRender<u32, Arc<RenderImage>>> =
            RefCell::new(OffRender::new(4, |_| {}));
    }

    /// A view drawing one picture made through the cache: `Some(_)` makes
    /// one, `None` fails to.
    struct Probe {
        shown: bool,
        key: u32,
        makes: bool,
        made: Arc<AtomicU32>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            if !self.shown {
                return div();
            }
            let (key, makes, made) = (self.key, self.makes, self.made.clone());
            let source = ImageSource::Custom(Arc::new(move |window, cx| {
                let made = made.clone();
                let make = move || {
                    made.fetch_add(1, Ordering::Relaxed);
                    let pixel = image::Frame::new(image::RgbaImage::new(1, 1));
                    makes.then(|| Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([pixel]))))
                };
                made_or_start(&CACHE, key, make, window, cx).map(Ok)
            }));
            div().size(px(10.)).child(img(source).size(px(4.)))
        }
    }

    fn drawn(vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui_kit::test]
    fn a_picture_is_made_off_render_and_lands_once(cx: &mut TestAppContext) {
        let made = Arc::new(AtomicU32::new(0));
        for (key, makes) in [(1, true), (2, false)] {
            let probe_made = made.clone();
            let (probe, vcx) = cx.add_window_view(|_, _| Probe {
                shown: false,
                key,
                makes,
                made: probe_made,
            });
            let notified = Rc::new(Cell::new(0));
            let counted = notified.clone();
            let _watch =
                vcx.update(|_, cx| cx.observe(&probe, move |_, _| counted.set(counted.get() + 1)));
            let before = made.load(Ordering::Relaxed);
            probe.update(vcx, |probe, cx| {
                probe.shown = true;
                cx.notify();
            });
            drawn(vcx);
            drawn(vcx);
            assert_eq!(made.load(Ordering::Relaxed), before, "render made nothing");
            vcx.run_until_parked();
            assert_eq!(made.load(Ordering::Relaxed), before + 1, "made once");
            assert!(notified.get() > 0, "the view that asked redraws");
            // Landed (or failed): later frames make nothing again.
            drawn(vcx);
            vcx.run_until_parked();
            drawn(vcx);
            assert_eq!(made.load(Ordering::Relaxed), before + 1);
        }
    }
}
