//! One off-thread render of a photo edit, owned by the stage that waits for
//! it: the stage holds the job, so leaving the stage (the photo removed,
//! the edit cancelled, a later Done) drops the job and cancels the render.
use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Names one render. A render's answer is applied only to the stage whose
/// job carries the same token, so an earlier render never settles a newer one.
#[derive(Clone)]
pub struct RenderToken(Arc<Cancelled>);

/// Set when the job that owns the render is dropped.
struct Cancelled(AtomicBool);

impl RenderToken {
    pub fn new() -> Self {
        Self(Arc::new(Cancelled(AtomicBool::new(false))))
    }

    /// The job was dropped: work already running off-thread cannot be
    /// stopped, so it checks this and discards what it made.
    pub fn is_cancelled(&self) -> bool {
        self.0.0.load(Ordering::Acquire)
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
/// last copy of the job drops the task, which cancels the render. A copy
/// keeps the render alive, so nothing outside the stage holds one (the edit
/// bar follows [`crate::composer::ComposerEdit::for_display`]); the identity
/// others keep is the token.
/// One pointer wide, so the stage that holds it stays small.
#[derive(Clone)]
pub struct RenderJob(Arc<Held>);

struct Held {
    token: RenderToken,
    /// Held to be dropped, never read.
    _task: Mutex<Box<dyn Any + Send>>,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.token.0.0.store(true, Ordering::Release);
    }
}

impl RenderJob {
    /// The job whose render `task` runs for `token`.
    pub fn new(token: RenderToken, task: impl Any + Send) -> Self {
        Self(Arc::new(Held {
            token,
            _task: Mutex::new(Box::new(task)),
        }))
    }

    pub fn token(&self) -> &RenderToken {
        &self.0.token
    }
}

impl PartialEq for RenderJob {
    fn eq(&self, other: &Self) -> bool {
        self.0.token == other.0.token
    }
}

impl Eq for RenderJob {}

impl fmt::Debug for RenderJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RenderJob({:?})", self.0.token)
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
    fn dropping_the_job_marks_its_token_cancelled() {
        let token = RenderToken::new();
        let job = RenderJob::new(token.clone(), ());
        assert!(!token.is_cancelled());
        drop(job);
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_token_names_one_render() {
        let first = RenderToken::new();
        assert_eq!(first, first.clone());
        assert_ne!(first, RenderToken::new());
    }
}
