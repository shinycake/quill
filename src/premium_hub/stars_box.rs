//! The Stars dialog: closed, the balance and history, or one transaction's
//! details over them. One value, so a selection cannot outlive the box.
#![deny(clippy::indexing_slicing)]

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StarsBox {
    #[default]
    Closed,
    /// Open on the balance and history.
    List,
    /// The details of the transaction with this id.
    Details(String),
}

impl StarsBox {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    /// The transaction whose details show.
    pub fn selected(&self) -> Option<&str> {
        match self {
            Self::Details(id) => Some(id),
            _ => None,
        }
    }

    /// The box opens on its list (an open box keeps what it shows).
    pub fn open(&mut self) {
        if !self.is_open() {
            *self = Self::List;
        }
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Show (`Some`) or leave (`None`) a transaction's details; a closed box
    /// stays closed.
    pub fn select(&mut self, id: Option<String>) {
        if !self.is_open() {
            return;
        }
        *self = match id {
            Some(id) => Self::Details(id),
            None => Self::List,
        };
    }
}
