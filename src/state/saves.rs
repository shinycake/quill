//! A value the server holds plus the saves in flight over it. The row shows
//! the newest save's value; a save that fails drops out of the queue and
//! the row falls back to the next save still out, or to the confirmed
//! value. Nothing is handed from one save to another, so the order in which
//! saves fail or land does not matter.
use crate::ids::RequestId;

/// The confirmed value and the saves not yet answered, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saves<T: Copy> {
    confirmed: T,
    in_flight: Vec<(RequestId, T)>,
}

impl<T: Copy> Saves<T> {
    /// One save goes out under `request` over `confirmed`.
    pub fn first(confirmed: T, request: RequestId, next: T) -> Self {
        Self {
            confirmed,
            in_flight: vec![(request, next)],
        }
    }

    /// What the row shows: the newest save, else the confirmed value.
    pub fn value(&self) -> T {
        self.in_flight
            .last()
            .map_or(self.confirmed, |(_, value)| *value)
    }

    /// Another save goes out under `request`.
    pub fn push(&mut self, request: RequestId, next: T) {
        self.in_flight.push((request, next));
    }

    /// `request` was refused (or never left): only its own value goes.
    /// Returns whether saves are still out.
    pub fn fail(&mut self, request: RequestId) -> bool {
        self.in_flight.retain(|(id, _)| *id != request);
        !self.in_flight.is_empty()
    }

    /// `request` landed: the server holds its value, and every older save
    /// is superseded. Returns whether saves are still out, newer ones.
    pub fn land(&mut self, request: RequestId) -> bool {
        if let Some(at) = self.in_flight.iter().position(|(id, _)| *id == request) {
            if let Some((_, value)) = self.in_flight.get(at) {
                self.confirmed = *value;
            }
            self.in_flight.drain(..=at);
        }
        !self.in_flight.is_empty()
    }

    /// The server's value as of `fetch`, when `fetch` was asked after every
    /// save out: it already includes them, so it replaces them.
    pub fn fetched(&self, fetch: RequestId, server: T) -> Option<T> {
        self.in_flight
            .iter()
            .all(|(id, _)| id.0 < fetch.0)
            .then_some(server)
    }

    pub fn confirmed(&self) -> T {
        self.confirmed
    }
}

/// A setting read from the server and saved optimistically: not read yet,
/// held, or held with saves in flight. A save needs a held value, so a
/// refusal always has something to fall back to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Optimistic<T: Copy> {
    #[default]
    Unknown,
    Ready(T),
    Saving(Saves<T>),
}

impl<T: Copy> Optimistic<T> {
    pub fn value(&self) -> Option<T> {
        match self {
            Self::Unknown => None,
            Self::Ready(value) => Some(*value),
            Self::Saving(saves) => Some(saves.value()),
        }
    }

    /// A read asked as `fetch` answered. It replaces the row unless a save
    /// asked after it is still out: that save's own answer settles the row.
    pub fn read(&mut self, fetch: RequestId, server: T) {
        match self {
            Self::Saving(saves) => {
                if let Some(value) = saves.fetched(fetch, server) {
                    *self = Self::Ready(value);
                }
            }
            _ => *self = Self::Ready(server),
        }
    }

    /// A save goes out under `request`. Without a held value there is
    /// nothing to roll back to, so nothing goes out: returns whether it did.
    pub fn save(&mut self, request: RequestId, next: T) -> bool {
        match self {
            Self::Unknown => return false,
            Self::Ready(confirmed) => *self = Self::Saving(Saves::first(*confirmed, request, next)),
            Self::Saving(saves) => saves.push(request, next),
        }
        true
    }

    pub fn failed(&mut self, request: RequestId) {
        if let Self::Saving(saves) = self
            && !saves.fail(request)
        {
            *self = Self::Ready(saves.confirmed());
        }
    }

    pub fn landed(&mut self, request: RequestId) {
        if let Self::Saving(saves) = self
            && !saves.land(request)
        {
            *self = Self::Ready(saves.confirmed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> RequestId {
        RequestId(n)
    }

    fn two_saves() -> Optimistic<i32> {
        let mut row = Optimistic::Ready(180);
        assert!(row.save(id(1), 30));
        assert!(row.save(id(2), 60));
        row
    }

    #[test]
    fn newest_failing_first_falls_back_to_the_older_save_then_the_server() {
        let mut row = two_saves();
        row.failed(id(2));
        assert_eq!(row.value(), Some(30));
        row.failed(id(1));
        assert_eq!(row.value(), Some(180));
        assert!(matches!(row, Optimistic::Ready(180)));
    }

    #[test]
    fn oldest_failing_first_keeps_the_newer_value_until_it_answers() {
        let mut row = two_saves();
        row.failed(id(1));
        assert_eq!(row.value(), Some(60));
        row.failed(id(2));
        assert_eq!(row.value(), Some(180));
    }

    #[test]
    fn landing_confirms_and_a_late_failure_of_a_superseded_save_changes_nothing() {
        let mut row = two_saves();
        row.landed(id(2));
        assert_eq!(row, Optimistic::Ready(60));
        row.failed(id(1));
        assert_eq!(row.value(), Some(60));
    }

    #[test]
    fn older_landing_then_newer_failing_shows_the_older_value() {
        let mut row = two_saves();
        row.landed(id(1));
        row.failed(id(2));
        assert_eq!(row, Optimistic::Ready(30));
    }

    #[test]
    fn a_read_while_saves_are_out_does_not_move_the_row() {
        let mut row = two_saves();
        row.read(id(0), 180);
        assert_eq!(row.value(), Some(60));
        row.read(id(3), 60);
        assert_eq!(row, Optimistic::Ready(60));
        let mut idle = Optimistic::Unknown;
        idle.read(id(1), 7);
        assert_eq!(idle, Optimistic::Ready(7));
    }

    #[test]
    fn nothing_is_saved_before_the_server_value_is_known() {
        let mut row = Optimistic::Unknown;
        assert!(!row.save(id(1), 5));
        assert_eq!(row.value(), None);
    }
}
