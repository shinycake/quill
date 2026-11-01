use crate::passcode::*;
use crate::platform::MemorySecretStore;

#[test]
fn idle_cache_extrapolates_and_fails_closed() {
    assert_eq!(IdleCache::Unknown.read(10), None);
    let failed = IdleCache::Unknown.after_sample(0, None);
    assert_eq!(failed.read(10), None);
    let sample = IdleCache::Sample {
        ms: 500,
        taken_ms: 1000,
    };
    assert_eq!(sample.read(1000), Some(500));
    assert_eq!(sample.read(2500), Some(2000));
    assert_eq!(sample.read(900), Some(500));
    let huge = IdleCache::Sample {
        ms: u64::MAX,
        taken_ms: 0,
    };
    assert_eq!(huge.read(10), Some(u64::MAX));
}

#[test]
fn failed_cache_means_no_backend_answered() {
    let candidates = candidate_backends(SessionEnv { x11: true });
    let answer = select_backend(candidates, |_| None);
    assert_eq!(answer, None);
    let cache = IdleCache::Unknown.after_sample(0, answer.map(|(_, ms)| ms));
    assert!(matches!(cache, IdleCache::Failed { failures: 1, .. }));
}

/// The shell restarted: one sample got no answer. Auto-lock reads "away"
/// time again once the shell is back, instead of input to Quill's window
/// for the rest of the session.
#[test]
fn one_failed_idle_sample_is_retried() {
    let failed = IdleCache::Sample { ms: 0, taken_ms: 0 }.after_sample(2_000, None);
    assert_eq!(failed.read(2_000), None);
    assert!(!failed.sample_due(2_001));
    assert!(failed.sample_due(2_000 + IDLE_RETRY_FIRST_MS));
    let back = failed.after_sample(4_000, Some(3_500));
    assert_eq!(back.read(5_000), Some(4_500));
}

#[test]
fn idle_retries_back_off_while_no_backend_answers() {
    let mut cache = IdleCache::Unknown;
    let mut now = 0;
    let mut waits = Vec::new();
    for _ in 0..10 {
        cache = cache.after_sample(now, None);
        let IdleCache::Failed { retry_ms, .. } = cache else {
            panic!("no answer is a failure");
        };
        waits.push(retry_ms - now);
        now = retry_ms;
    }
    assert_eq!(
        waits,
        [
            2_000, 4_000, 8_000, 16_000, 32_000, 64_000, 120_000, 120_000, 120_000, 120_000
        ]
    );
    assert!(waits.iter().all(|wait| *wait <= IDLE_RETRY_MAX_MS));
}

#[test]
fn session_env_detects_real_x11_only() {
    let x11 = |t, w, d| SessionEnv::from_vars(t, w, d).x11;
    assert!(x11(Some("x11"), None, Some(":0")));
    assert!(x11(Some("x11"), Some("wayland-0"), Some(":0")));
    assert!(!x11(Some("wayland"), Some("wayland-0"), Some(":0")));
    assert!(!x11(Some("wayland"), None, Some(":0")));
    assert!(x11(None, None, Some(":0")));
    assert!(x11(Some("tty"), Some(""), Some(":0")));
    assert!(!x11(None, Some("wayland-0"), Some(":0")));
    assert!(!x11(None, None, None));
    assert!(!x11(None, None, Some("")));
}

#[test]
fn candidates_skip_x11_outside_x11_sessions() {
    assert_eq!(
        candidate_backends(SessionEnv { x11: false }),
        &[IdleBackend::Mutter, IdleBackend::ScreenSaver]
    );
    assert_eq!(
        candidate_backends(SessionEnv { x11: true }),
        &[
            IdleBackend::Mutter,
            IdleBackend::ScreenSaver,
            IdleBackend::X11ScreenSaver
        ]
    );
}

#[test]
fn backend_selection_keeps_the_first_that_answers() {
    let all = candidate_backends(SessionEnv { x11: true });
    let only = |want: &'static [IdleBackend]| move |b: IdleBackend| want.contains(&b).then_some(42);
    assert_eq!(
        select_backend(all, only(&[IdleBackend::X11ScreenSaver])),
        Some((IdleBackend::X11ScreenSaver, 42))
    );
    assert_eq!(
        select_backend(
            all,
            only(&[IdleBackend::ScreenSaver, IdleBackend::X11ScreenSaver])
        ),
        Some((IdleBackend::ScreenSaver, 42))
    );
    assert_eq!(
        select_backend(all, only(&[IdleBackend::Mutter, IdleBackend::ScreenSaver])),
        Some((IdleBackend::Mutter, 42))
    );
    // Probing stops at the first answer.
    let mut probed = Vec::new();
    let _ = select_backend(all, |b| {
        probed.push(b);
        Some(0)
    });
    assert_eq!(probed, [IdleBackend::Mutter]);
    // A zero reading (just typed) is an answer, not a failure.
    assert_eq!(
        select_backend(all, |_| Some(0)),
        Some((IdleBackend::Mutter, 0))
    );
    // An X11-only candidate list never reaches the X backend on Wayland.
    let wayland = candidate_backends(SessionEnv { x11: false });
    assert_eq!(
        select_backend(wayland, only(&[IdleBackend::X11ScreenSaver])),
        None
    );
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

// Fixed-input regression vectors: the on-disk format (`QPW1` + 24-byte nonce +
// ciphertext||tag, account as AAD, PBKDF2-HMAC-SHA512 KEK) must never change
// across RustCrypto upgrades. Values were computed with pbkdf2 0.12 and
// chacha20poly1305 0.10.
const FIXED_KEK_HEX: &str = "ec5fd8d6b9d65d7bab2c27c5bc09b28f25ed6c006183305304d726fffc259ef1";
const FIXED_SEALED_HEX: &str = "515057314242424242424242424242424242424242424242424242425b3044e3188fd3d6e45dea7d2a498d6a3588c849ab6bd48673d49303db026a9e1612c75dfb212be24f43c5d8c5f7bf22";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fixed_master() -> MasterKey {
    MasterKey(core::array::from_fn(|i| i as u8))
}

#[test]
fn fixed_vector_kek_is_stable() {
    let kek = derive_kek("correct horse battery", b"0123456789abcdef", 1000);
    assert_eq!(hex(kek.as_bytes()), FIXED_KEK_HEX);
}

#[test]
fn fixed_vector_sealed_blob_is_byte_identical() {
    let key = fixed_master();
    let nonce = [0x42u8; NONCE_LEN];
    let cipher = XChaCha20Poly1305::new(<&Key>::from(key.as_bytes()));
    let body = cipher
        .encrypt(
            <&XNonce>::from(&nonce),
            Payload {
                msg: &[0xa5u8; 32],
                aad: b"fixed-account",
            },
        )
        .unwrap();
    let mut sealed = WRAP_MAGIC.to_vec();
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&body);
    assert_eq!(hex(&sealed), FIXED_SEALED_HEX);
}

#[test]
fn fixed_vector_sealed_blob_opens() {
    let sealed: Vec<u8> = FIXED_SEALED_HEX
        .as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let plain = open(&fixed_master(), b"fixed-account", &sealed).unwrap();
    assert_eq!(plain, vec![0xa5u8; 32]);
    assert!(open(&fixed_master(), b"other-account", &sealed).is_err());
}

#[test]
fn a_planned_long_sleep_is_not_a_suspend() {
    let mut tracker = IdleTracker::default();
    assert_eq!(tracker.effective_idle(1_000, 0), 0);
    // The caller slept a minute on purpose, as planned: only the real idle
    // time counts.
    tracker.plan_sleep(60_000);
    assert_eq!(tracker.effective_idle(61_000, 60_000), 60_000);
    // A gap well past the plan is still a suspend; the planned part is
    // already in the OS idle time and is not counted twice.
    tracker.plan_sleep(60_000);
    assert_eq!(
        tracker.effective_idle(1_000_000, 60_100),
        60_100 + (1_000_000 - 61_000 - 60_000)
    );
    // Forgetting the history drops a stale suspend and the stale tick time.
    tracker.reset();
    assert_eq!(tracker.effective_idle(9_000_000, 5), 5);
}

#[test]
fn autolock_sleeps_until_the_deadline_and_parks_when_off() {
    // Off, locked, or no timeout: nothing to time.
    assert_eq!(autolock_sleep_ms(false, false, 300, 0), None);
    assert_eq!(autolock_sleep_ms(true, true, 300, 0), None);
    assert_eq!(autolock_sleep_ms(true, false, 0, 0), None);
    // A five-minute lock after one idle minute: capped (a laptop sleep
    // pauses the timer, so the lock is late by five seconds at most).
    assert_eq!(autolock_sleep_ms(true, false, 300, 60_000), Some(5_000));
    // Close to the deadline: sleep exactly until it.
    assert_eq!(autolock_sleep_ms(true, false, 300, 297_000), Some(3_000));
    // Never a busy loop.
    assert_eq!(autolock_sleep_ms(true, false, 300, 299_900), Some(1_000));
}

#[test]
fn an_idle_hour_with_auto_lock_on_wakes_every_five_seconds_at_most() {
    // The user is away: each wake re-reads the idle time and sleeps until
    // the deadline (the lock itself ends the schedule).
    let timeout_secs = 3600;
    let mut now_ms = 0u64;
    let mut wakes = 0;
    while !autolock_due(now_ms, timeout_secs) {
        let sleep = autolock_sleep_ms(true, false, timeout_secs, now_ms).unwrap();
        now_ms += sleep;
        wakes += 1;
    }
    assert!(wakes <= 721, "{wakes} wakes in an idle hour");
}

#[test]
fn a_laptop_wake_after_a_planned_sleep_locks_on_the_next_check() {
    // Auto-lock after 5 minutes; the check sleeps at most the cap.
    let timeout_secs = 300;
    let planned = autolock_sleep_ms(true, false, timeout_secs, 0).unwrap_or(0);
    assert!(planned <= 5_000, "the sleep is capped at {planned} ms");
    let mut tracker = IdleTracker::default();
    assert_eq!(tracker.effective_idle(1_000, 0), 0);
    tracker.plan_sleep(planned);
    // The lid was shut for an hour: the monotonic idle clock only saw the
    // planned sleep, the wall clock saw the hour.
    let now = 1_000 + 3_600_000;
    let effective = tracker.effective_idle(now, planned);
    assert!(autolock_due(effective, timeout_secs));
    assert_eq!(effective, 3_600_000);
}
