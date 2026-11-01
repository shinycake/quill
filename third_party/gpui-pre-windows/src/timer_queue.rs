// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

/// Pending timers ordered by deadline, earliest first; equal deadlines keep
/// the order they were added in.
pub(crate) struct TimerQueue<T> {
    heap: BinaryHeap<Entry<T>>,
    next_seq: u64,
}

struct Entry<T> {
    deadline: Instant,
    seq: u64,
    item: T,
}

impl<T> Entry<T> {
    fn key(&self) -> (Instant, u64) {
        (self.deadline, self.seq)
    }
}

impl<T> PartialEq for Entry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl<T> Eq for Entry<T> {}

impl<T> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Entry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key().cmp(&self.key())
    }
}

/// What the timer thread does next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Nothing is pending: sleep until a timer is added.
    Forever,
    /// Sleep this long, or until a timer is added.
    For(Duration),
}

impl<T> TimerQueue<T> {
    pub(crate) fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }

    /// Adds `item`; true when it is now the earliest, so a sleeping thread
    /// has to wake and shorten its wait.
    pub(crate) fn push(&mut self, deadline: Instant, item: T) -> bool {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        let earliest = self.heap.peek().is_none_or(|first| deadline < first.deadline);
        self.heap.push(Entry {
            deadline,
            seq,
            item,
        });
        earliest
    }

    /// Removes and returns every item due at `now`, earliest first.
    pub(crate) fn pop_due(&mut self, now: Instant) -> Vec<T> {
        let mut due = Vec::new();
        while self.heap.peek().is_some_and(|first| first.deadline <= now) {
            if let Some(entry) = self.heap.pop() {
                due.push(entry.item);
            }
        }
        due
    }

    pub(crate) fn wait(&self, now: Instant) -> Wait {
        match self.heap.peek() {
            None => Wait::Forever,
            Some(first) => Wait::For(first.deadline.saturating_duration_since(now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn an_empty_queue_waits_forever() {
        let queue: TimerQueue<u32> = TimerQueue::new();
        assert_eq!(queue.wait(Instant::now()), Wait::Forever);
    }

    #[test]
    fn the_wait_ends_at_the_earliest_deadline() {
        let start = Instant::now();
        let mut queue = TimerQueue::new();
        assert!(queue.push(start + ms(40), "late"));
        assert!(queue.push(start + ms(16), "frame"));
        assert!(!queue.push(start + ms(30), "middle"));
        assert_eq!(queue.wait(start), Wait::For(ms(16)));
        assert_eq!(queue.wait(start + ms(10)), Wait::For(ms(6)));
        assert_eq!(queue.wait(start + ms(20)), Wait::For(ms(0)));
    }

    #[test]
    fn only_due_timers_fire_in_deadline_order() {
        let start = Instant::now();
        let mut queue = TimerQueue::new();
        queue.push(start + ms(30), 3);
        queue.push(start + ms(10), 1);
        queue.push(start + ms(20), 2);
        assert!(queue.pop_due(start + ms(5)).is_empty());
        assert_eq!(queue.pop_due(start + ms(20)), vec![1, 2]);
        assert_eq!(queue.wait(start + ms(20)), Wait::For(ms(10)));
        assert_eq!(queue.pop_due(start + ms(31)), vec![3]);
        assert_eq!(queue.wait(start + ms(31)), Wait::Forever);
    }

    #[test]
    fn equal_deadlines_fire_in_the_order_added() {
        let at = Instant::now() + ms(16);
        let mut queue = TimerQueue::new();
        for n in 0..5 {
            queue.push(at, n);
        }
        assert_eq!(queue.pop_due(at), vec![0, 1, 2, 3, 4]);
    }
}
