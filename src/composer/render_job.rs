//! One off-thread render of a photo edit, owned by the stage that waits for
//! it: the stage holds the job, so leaving the stage (the photo removed,
//! the edit cancelled, a later Done) drops the job and cancels the render.
use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex};

/// Names one render. A render's answer is applied only to the stage whose
/// job carries the same token, so an earlier render never settles a newer one.
#[derive(Clone)]
pub struct RenderToken(Arc<()>);

impl RenderToken {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl Default for RenderToken {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for RenderToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RenderToken {}

impl fmt::Debug for RenderToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RenderToken({:p})", Arc::as_ptr(&self.0))
    }
}

/// A render in flight: its token, and the task that runs it. Dropping the
/// last copy of the job drops the task, which cancels the render. Only the
/// stage clones it (a tray copy); the identity others keep is the token.
#[derive(Clone)]
pub struct RenderJob {
    token: RenderToken,
    /// Held to be dropped, never read.
    _task: Arc<Mutex<Box<dyn Any + Send>>>,
}

impl RenderJob {
    /// The job whose render `task` runs for `token`.
    pub fn new(token: RenderToken, task: impl Any + Send) -> Self {
        Self {
            token,
            _task: Arc::new(Mutex::new(Box::new(task))),
        }
    }

    pub fn token(&self) -> &RenderToken {
        &self.token
    }
}

impl PartialEq for RenderJob {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token
    }
}

impl Eq for RenderJob {}

impl fmt::Debug for RenderJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RenderJob({:?})", self.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Flag(Arc<AtomicBool>);

    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn dropping_the_job_drops_its_task() {
        let dropped = Arc::new(AtomicBool::new(false));
        let job = RenderJob::new(RenderToken::new(), Flag(dropped.clone()));
        assert!(!dropped.load(Ordering::SeqCst));
        drop(job);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_token_names_one_render() {
        let first = RenderToken::new();
        assert_eq!(first, first.clone());
        assert_ne!(first, RenderToken::new());
    }
}
