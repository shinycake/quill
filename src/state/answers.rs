//! Answers of requests a box waits for, each under the request that asked.
//!
//! A box that sent a request keeps the request's id in its own stage and
//! takes the answer with that id. An answer for a box that was closed,
//! reopened or edited since finds no id to match and is dropped, so it can
//! never show or apply under a different box.
use crate::ids::RequestId;

/// The newest answers kept. The open box takes its own on the next poll,
/// so older ones belong to a box that has gone.
const KEPT: usize = 8;

/// Answers (`Ok(T)` or the error line) by the request that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answers<T> {
    items: Vec<(RequestId, Result<T, String>)>,
}

impl<T> Default for Answers<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Answers<T> {
    /// File an answer.
    pub fn push(&mut self, request: RequestId, answer: Result<T, String>) {
        self.items.push((request, answer));
        let extra = self.items.len().saturating_sub(KEPT);
        self.items.drain(..extra);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Forget every answer (a box opened or closed).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The answers to `waiting`, taken. Every other answer is dropped:
    /// nothing open waits for it.
    pub fn take_for(&mut self, waiting: &[RequestId]) -> Vec<(RequestId, Result<T, String>)> {
        std::mem::take(&mut self.items)
            .into_iter()
            .filter(|(request, _)| waiting.contains(request))
            .collect()
    }
}
