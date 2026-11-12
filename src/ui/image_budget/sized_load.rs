//! The display-size decode of a cached picture: the stage that holds its
//! task and the views waiting on it.
#![deny(clippy::indexing_slicing)]

use super::*;

/// A display-size decode on the background executor; views that asked
/// while it ran are notified when it lands (as GPUI's own image loads).
pub(super) struct SizedLoad {
    state: Rc<RefCell<SizedState>>,
}

enum SizedState {
    /// Decoding: the stage holds the task, so dropping the load cancels it.
    Loading {
        views: SmallVec<[EntityId; 2]>,
        task: Task<()>,
    },
    Loaded(LoadResult),
}

impl SizedLoad {
    pub(super) fn new(key: SizedKey, cx: &mut App) -> Self {
        let decode = cx
            .background_executor()
            .spawn(async move { decode_sized(&key) });
        let state = Rc::new_cyclic(|weak: &std::rc::Weak<RefCell<SizedState>>| {
            let weak = weak.clone();
            let task = cx.spawn(async move |cx| {
                let result = decode.await;
                let Some(state) = weak.upgrade() else {
                    return;
                };
                // Nothing holds the state across a turn; if it is borrowed all
                // the same, the picture stays unloaded rather than panic.
                let Ok(mut guard) = state.try_borrow_mut() else {
                    return;
                };
                let previous = std::mem::replace(&mut *guard, SizedState::Loaded(result));
                drop(guard);
                if let SizedState::Loading { views, task } = previous {
                    // This task is the one running: let it finish.
                    task.detach();
                    cx.update(|cx| {
                        for view in views {
                            cx.notify(view);
                        }
                    });
                }
            });
            RefCell::new(SizedState::Loading {
                views: SmallVec::new(),
                task,
            })
        });
        Self { state }
    }

    pub(super) fn get(&self) -> Option<LoadResult> {
        match &*self.state.try_borrow().ok()? {
            SizedState::Loading { .. } => None,
            SizedState::Loaded(result) => Some(result.clone()),
        }
    }

    pub(super) fn use_by(&self, view: EntityId) -> Option<LoadResult> {
        match &mut *self.state.try_borrow_mut().ok()? {
            SizedState::Loading { views, .. } => {
                if !views.contains(&view) {
                    views.push(view);
                }
                None
            }
            SizedState::Loaded(result) => Some(result.clone()),
        }
    }
}
