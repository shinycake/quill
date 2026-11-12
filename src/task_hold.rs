//! The background work a stage waits for (a photo-edit render, the jump a
//! pull started), held by the stage itself. The lib cannot name the UI's task
//! type, so the stage keeps it behind this handle; dropping the last handle
//! cancels the work, so replacing or leaving the stage stops it instead of
//! leaving it to finish and be thrown away.

use std::any::Any;
use std::sync::{Arc, Mutex};

/// Owns whatever runs the work (the UI hands over its task). Clones share
/// it; the work ends when the last clone is dropped.
#[derive(Clone)]
pub struct TaskHold(Arc<Mutex<Box<dyn Any + Send>>>);

impl TaskHold {
    pub fn new<T: Send + 'static>(work: T) -> Self {
        Self(Arc::new(Mutex::new(Box::new(work))))
    }
}

impl PartialEq for TaskHold {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for TaskHold {}

impl std::fmt::Debug for TaskHold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TaskHold")
    }
}

#[cfg(test)]
mod tests {
    use super::TaskHold;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Flag(Arc<AtomicBool>);

    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    /// The work ends with the last handle, not before.
    #[test]
    fn the_work_ends_when_the_last_clone_is_dropped() {
        let ended = Arc::new(AtomicBool::new(false));
        let hold = TaskHold::new(Flag(ended.clone()));
        let copy = hold.clone();
        assert!(hold == copy);
        drop(hold);
        assert!(!ended.load(Ordering::Relaxed));
        drop(copy);
        assert!(ended.load(Ordering::Relaxed));
    }
}
