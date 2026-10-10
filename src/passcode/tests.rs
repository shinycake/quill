use crate::passcode::*;
use crate::platform::MemorySecretStore;

#[test]
fn idle_cache_extrapolates_and_fails_closed() {
    assert_eq!(IdleCache::Unknown.read(10), None);
    assert_eq!(IdleCache::Failed.read(10), None);
    let sample = IdleCache::Sample {
        ms: 500,
        taken_ms: 1000,
    };
    assert_eq!(sample.read(1000), Some(500));
    assert_eq!(sample.read(2500), Some(2000));
    assert_eq!(sample.read(900), Some(500));
}

#[test]
fn idle_sampling_runs_only_while_reads_are_recent() {
    assert!(!idle_sampling_wanted(None, 10_000));
    assert!(idle_sampling_wanted(Some(1_000), 1_000));
    assert!(idle_sampling_wanted(
        Some(1_000),
        1_000 + IDLE_ARMED_GRACE_MS
    ));
    assert!(!idle_sampling_wanted(
        Some(1_000),
        1_001 + IDLE_ARMED_GRACE_MS
    ));
    // A clock that reads earlier than the stamp still counts as recent.
    assert!(idle_sampling_wanted(Some(2_000), 1_000));
}

const FAST: u32 = 8;

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "quill-passcode-{tag}-{}-{}",
        std::process::id(),
        unix_ms_now()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn acct(name: &str) -> AccountKey {
    AccountKey::new(name).unwrap()
}

#[test]
fn kdf_is_deterministic_per_salt_and_differs_by_input() {
    let a = derive_kek("hunter2", b"salt-salt-salt!!", FAST);
    let b = derive_kek("hunter2", b"salt-salt-salt!!", FAST);
    let c = derive_kek("hunter3", b"salt-salt-salt!!", FAST);
    let d = derive_kek("hunter2", b"other-salt-here!", FAST);
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert_ne!(a.as_bytes(), c.as_bytes());
    assert_ne!(a.as_bytes(), d.as_bytes());
}

#[test]
fn pbkdf2_matches_the_rfc_vector_shape() {
    // PBKDF2-HMAC-SHA512, "password"/"salt", 1 round (published vector).
    let mut out = [0u8; 64];
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(b"password", b"salt", 1, &mut out);
    assert_eq!(&out[..8], &[0x86, 0x7f, 0x70, 0xcf, 0x1a, 0xde, 0x02, 0xcf]);
}

#[test]
fn wrap_unwrap_round_trip_and_tamper_detection() {
    let master = MasterKey::generate();
    let key = DatabaseKey::generate();
    let sealed = wrap_db_key(&master, &acct("primary"), &key).unwrap();
    let back = unwrap_db_key(&master, &acct("primary"), &sealed).unwrap();
    assert_eq!(back.as_bytes(), key.as_bytes());
    // Bound to the account id.
    assert_eq!(
        unwrap_db_key(&master, &acct("other"), &sealed).unwrap_err(),
        PasscodeError::Wrong
    );
    // Wrong master key.
    let other = MasterKey::generate();
    assert!(unwrap_db_key(&other, &acct("primary"), &sealed).is_err());
    // Flipped bit.
    let mut bad = sealed.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(unwrap_db_key(&master, &acct("primary"), &bad).is_err());
    // Wrapping is salted: same input, different bytes.
    assert_ne!(
        sealed,
        wrap_db_key(&master, &acct("primary"), &key).unwrap()
    );
}

#[test]
fn retry_schedule_matches_tdesktop() {
    assert_eq!(retry_wait_ms(0, 0), None);
    assert_eq!(retry_wait_ms(2, 0), None);
    assert_eq!(retry_wait_ms(3, 0), Some(5_000));
    assert_eq!(retry_wait_ms(3, 4_000), Some(1_000));
    assert_eq!(retry_wait_ms(3, 5_000), None);
    assert_eq!(retry_wait_ms(4, 9_999), Some(1));
    assert_eq!(retry_wait_ms(5, 0), Some(15_000));
    assert_eq!(retry_wait_ms(6, 0), Some(20_000));
    assert_eq!(retry_wait_ms(7, 0), Some(25_000));
    assert_eq!(retry_wait_ms(8, 0), Some(30_000));
    assert_eq!(retry_wait_ms(40, 29_999), Some(1));
}

#[test]
fn enable_unlock_and_rate_limit_persist() {
    let root = temp_root("limit");
    let inner = MemorySecretStore::new();
    let a = acct("primary");
    inner.put(&a, &DatabaseKey::generate()).unwrap();
    let state = UnlockState::new();
    enable(
        &root,
        &inner,
        std::slice::from_ref(&a),
        "correct horse",
        FAST,
        &state,
    )
    .unwrap();
    state.clear();
    let t0 = 1_000_000;
    for _ in 0..3 {
        assert_eq!(
            unlock(&root, &state, "nope", t0).unwrap_err(),
            PasscodeError::Wrong
        );
    }
    // The right passcode is refused while flooded...
    assert_eq!(
        unlock(&root, &state, "correct horse", t0 + 1_000).unwrap_err(),
        PasscodeError::Flood { retry_in_ms: 4_000 }
    );
    assert!(!state.is_unlocked());
    // ...the counter survived (it is on disk, not in memory)...
    assert_eq!(lock_wait_ms(&root, t0 + 1_000), Some(4_000));
    // ...and after the wait it works and resets the counter.
    unlock(&root, &state, "correct horse", t0 + 5_000).unwrap();
    assert!(state.is_unlocked());
    assert_eq!(load_config(&root).unwrap().bad_tries, 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn enable_moves_the_key_out_of_the_os_store_and_disable_restores_it() {
    let root = temp_root("migrate");
    let inner = MemorySecretStore::new();
    let a = acct("primary");
    let b = acct("account-1");
    let key_a = DatabaseKey::generate();
    let key_b = DatabaseKey::generate();
    inner.put(&a, &key_a).unwrap();
    inner.put(&b, &key_b).unwrap();
    let state = Arc::new(UnlockState::new());
    enable(&root, &inner, &[a.clone(), b.clone()], "pass", FAST, &state).unwrap();
    // The OS store no longer has the keys.
    assert!(inner.get(&a).unwrap().is_none());
    assert!(inner.get(&b).unwrap().is_none());
    // The passcode store serves them while unlocked...
    let store = PasscodeStore::new(inner.clone(), &root, state.clone());
    assert_eq!(store.get(&a).unwrap().unwrap().as_bytes(), key_a.as_bytes());
    // ...and refuses with Locked once the master key is forgotten.
    state.clear();
    assert_eq!(store.get(&a).unwrap_err(), SecretStoreError::Locked);
    assert_eq!(store.put(&a, &key_a).unwrap_err(), SecretStoreError::Locked);
    // Wrong passcode cannot disable; right one restores both keys.
    assert_eq!(
        disable(&root, &inner, &[a.clone(), b.clone()], "bad", &state, 1).unwrap_err(),
        PasscodeError::Wrong
    );
    disable(&root, &inner, &[a.clone(), b.clone()], "pass", &state, 2).unwrap();
    assert_eq!(inner.get(&a).unwrap().unwrap().as_bytes(), key_a.as_bytes());
    assert_eq!(inner.get(&b).unwrap().unwrap().as_bytes(), key_b.as_bytes());
    assert!(!is_enabled(&root));
    assert!(!wrapped_key_path(&root, &a).exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn change_rewraps_only_the_master_key() {
    let root = temp_root("change");
    let inner = MemorySecretStore::new();
    let a = acct("primary");
    let key = DatabaseKey::generate();
    inner.put(&a, &key).unwrap();
    let state = Arc::new(UnlockState::new());
    enable(&root, &inner, std::slice::from_ref(&a), "old", FAST, &state).unwrap();
    let before = std::fs::read(wrapped_key_path(&root, &a)).unwrap();
    assert_eq!(
        change(&root, "old", "old", FAST, &state, 1).unwrap_err(),
        PasscodeError::Same
    );
    assert_eq!(
        change(&root, "wrong", "new", FAST, &state, 1).unwrap_err(),
        PasscodeError::Wrong
    );
    change(&root, "old", "new", FAST, &state, 2).unwrap();
    assert_eq!(before, std::fs::read(wrapped_key_path(&root, &a)).unwrap());
    state.clear();
    assert_eq!(
        unlock(&root, &state, "old", 10).unwrap_err(),
        PasscodeError::Wrong
    );
    unlock(&root, &state, "new", 20).unwrap();
    let store = PasscodeStore::new(inner, &root, state);
    assert_eq!(store.get(&a).unwrap().unwrap().as_bytes(), key.as_bytes());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn failed_enable_rolls_back_and_loses_nothing() {
    let root = temp_root("rollback");
    let inner = MemorySecretStore::new();
    let a = acct("primary");
    let key = DatabaseKey::generate();
    inner.put(&a, &key).unwrap();
    inner.lock();
    let state = UnlockState::new();
    let err = enable(
        &root,
        &inner,
        std::slice::from_ref(&a),
        "pass",
        FAST,
        &state,
    )
    .unwrap_err();
    assert_eq!(err, PasscodeError::Store(SecretStoreError::Locked));
    assert!(!is_enabled(&root));
    assert!(!state.is_unlocked());
    inner.unlock();
    assert_eq!(inner.get(&a).unwrap().unwrap().as_bytes(), key.as_bytes());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn passcode_is_never_written_to_disk() {
    let root = temp_root("plain");
    let inner = MemorySecretStore::new();
    let a = acct("primary");
    inner.put(&a, &DatabaseKey::generate()).unwrap();
    let state = UnlockState::new();
    enable(
        &root,
        &inner,
        std::slice::from_ref(&a),
        "s3cret-passcode",
        FAST,
        &state,
    )
    .unwrap();
    let config = std::fs::read_to_string(config_path(&root)).unwrap();
    assert!(!config.contains("s3cret-passcode"));
    let wrapped = std::fs::read(wrapped_key_path(&root, &a)).unwrap();
    assert!(!wrapped.windows(8).any(|w| w == b"s3cret-p"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_passcode_validation() {
    assert_eq!(validate_new("", "", None), Err(PasscodeError::Empty));
    assert_eq!(validate_new("a", "b", None), Err(PasscodeError::Mismatch));
    assert_eq!(validate_new("a", "a", Some("a")), Err(PasscodeError::Same));
    assert_eq!(validate_new("a", "a", Some("b")), Ok(()));
}

#[test]
fn autolock_timer_logic() {
    assert!(!autolock_due(59_999, 60));
    assert!(autolock_due(60_000, 60));
    assert_eq!(autolock_remaining_ms(10_000, 60), 50_000);
    assert_eq!(autolock_remaining_ms(90_000, 60), 0);
    assert_eq!(parse_hhmm("10:00"), Some(36_000));
    assert_eq!(parse_hhmm("0:05"), Some(300));
    assert_eq!(parse_hhmm("0:00"), None);
    assert_eq!(parse_hhmm("1:75"), None);
    assert_eq!(parse_hhmm("abc"), None);
    assert_eq!(autolock_label(60), "1 min");
    assert_eq!(autolock_label(18_000), "5 h");
    assert_eq!(autolock_label(5_400), "1h 30m");
}

#[test]
fn a_suspend_counts_as_idle_and_input_clears_it() {
    let mut tracker = IdleTracker::default();
    assert_eq!(tracker.effective_idle(1_000, 0), 0);
    assert_eq!(tracker.effective_idle(1_040, 40), 40);
    // The machine slept for ten minutes: the monotonic idle clock paused.
    assert_eq!(tracker.effective_idle(601_040, 80), 80 + 600_000);
    assert!(autolock_due(tracker.effective_idle(601_080, 120), 300));
    // Input resets idle (the in-window clock restarts from zero).
    assert_eq!(tracker.effective_idle(601_120, 0), 0);
}
