//! How many bytes the markup code looks at, counted in tests so they can pin
//! how that work grows with the text. Compiles to nothing elsewhere.

#[cfg(test)]
thread_local! {
    static SCANNED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static BUDGET: std::cell::Cell<u64> = const { std::cell::Cell::new(u64::MAX) };
}

#[inline(always)]
pub(crate) fn add(bytes: usize) {
    #[cfg(test)]
    {
        let scanned = SCANNED.get().saturating_add(bytes as u64);
        SCANNED.set(scanned);
        assert!(scanned <= BUDGET.get(), "scanned past the budget");
    }
    #[cfg(not(test))]
    let _ = bytes;
}

/// The bytes `work` looked at. It panics as soon as they pass `budget`, so a
/// runaway scan fails at once instead of running for minutes.
#[cfg(test)]
pub(crate) fn measure(budget: u64, work: impl FnOnce()) -> u64 {
    SCANNED.set(0);
    BUDGET.set(budget);
    work();
    BUDGET.set(u64::MAX);
    SCANNED.get()
}
