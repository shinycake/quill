//! Locks that recover from poisoning (`docs/contributing/panic-safety.md`,
//! rule 10).
//!
//! A `std::sync::Mutex` is poisoned when a thread panics while holding it,
//! and every later `lock().expect(..)` then panics too, usually on the UI
//! thread. Quill's locks guard plain data (queues, slots, caches, hooks)
//! that stays usable after a writer died halfway, so the guard is taken
//! back with `PoisonError::into_inner`. The panic itself is already in the
//! crash log (`crate::crash_log`).

use std::sync::{
    Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError,
};

/// Lock `mutex`, taking the guard back if a panicking holder poisoned it.
pub fn lock_or_recover<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `try_lock` that takes the guard back from a poisoned mutex. `None` only
/// when another thread holds the lock right now.
pub fn try_lock_or_recover<T: ?Sized>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Read-lock `lock`, taking the guard back if it was poisoned.
pub fn read_or_recover<T: ?Sized>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

/// Write-lock `lock`, taking the guard back if it was poisoned.
pub fn write_or_recover<T: ?Sized>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

/// Method form of [`lock_or_recover`], for the end of a field chain
/// (`shared.hook.lock_or_recover()`).
pub trait LockExt<T: ?Sized> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> LockExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        lock_or_recover(self)
    }
}

/// Method form of [`read_or_recover`] and [`write_or_recover`].
pub trait RwLockExt<T: ?Sized> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        read_or_recover(self)
    }

    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        write_or_recover(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{LockExt, RwLockExt, lock_or_recover, try_lock_or_recover};
    use std::sync::{Arc, Mutex, RwLock};

    #[test]
    fn a_poisoned_mutex_still_locks() {
        let shared = Arc::new(Mutex::new(vec![1]));
        let writer = shared.clone();
        let joined = std::thread::spawn(move || {
            let mut guard = writer.lock().unwrap();
            guard.push(2);
            panic!("holder dies");
        })
        .join();
        assert!(joined.is_err());
        assert!(shared.is_poisoned());
        assert_eq!(*lock_or_recover(&shared), vec![1, 2]);
        shared.lock_or_recover().push(3);
        assert_eq!(*shared.lock_or_recover(), vec![1, 2, 3]);
        assert_eq!(
            try_lock_or_recover(&shared).map(|guard| guard.len()),
            Some(3)
        );
        let held = shared.lock_or_recover();
        assert!(try_lock_or_recover(&shared).is_none(), "held elsewhere");
        drop(held);
    }

    #[test]
    fn a_poisoned_rwlock_still_reads_and_writes() {
        let shared = Arc::new(RwLock::new(1));
        let writer = shared.clone();
        let joined = std::thread::spawn(move || {
            let _guard = writer.write().unwrap();
            panic!("holder dies");
        })
        .join();
        assert!(joined.is_err());
        *shared.write_or_recover() += 1;
        assert_eq!(*shared.read_or_recover(), 2);
    }
}
