//! OS idle time on Linux, which has no single API: the compositor or the X
//! server answers, and which one depends on the desktop. A sampler thread
//! probes an ordered list of backends once, keeps the first one that
//! answers, and publishes extrapolated readings through [`IdleCache`], so
//! readers (auto-lock, presence, notifications) never block on D-Bus or X.

/// Where idle time comes from, in probe order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdleBackend {
    /// GNOME Shell: `org.gnome.Mutter.IdleMonitor.GetIdletime` (ms, u64).
    Mutter,
    /// KDE Plasma: `org.freedesktop.ScreenSaver.GetSessionIdleTime` (ms, u32).
    ScreenSaver,
    /// X11 MIT-SCREEN-SAVER `QueryInfo` (ms, u32). Real X11 sessions only.
    X11ScreenSaver,
}

/// The session facts backend selection depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct SessionEnv {
    pub x11: bool,
}

impl SessionEnv {
    /// `XDG_SESSION_TYPE=x11`, or no usable session type with
    /// `WAYLAND_DISPLAY` unset and `DISPLAY` set. XWayland also sets
    /// `DISPLAY`, but there the X server only sees X clients' input, so it
    /// must not be used.
    pub(super) fn from_vars(
        session_type: Option<&str>,
        wayland_display: Option<&str>,
        display: Option<&str>,
    ) -> Self {
        let set = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
        let x11 = match session_type.map(str::trim) {
            Some("x11") => true,
            Some("wayland") => false,
            _ => !set(wayland_display) && set(display),
        };
        Self { x11 }
    }

    #[cfg(target_os = "linux")]
    fn detect() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        Self::from_vars(
            var("XDG_SESSION_TYPE").as_deref(),
            var("WAYLAND_DISPLAY").as_deref(),
            var("DISPLAY").as_deref(),
        )
    }
}

/// Backends worth probing for this session, best first.
pub(super) fn candidate_backends(env: SessionEnv) -> &'static [IdleBackend] {
    if env.x11 {
        &[
            IdleBackend::Mutter,
            IdleBackend::ScreenSaver,
            IdleBackend::X11ScreenSaver,
        ]
    } else {
        &[IdleBackend::Mutter, IdleBackend::ScreenSaver]
    }
}

/// The first candidate whose probe answered, with that first reading.
pub(super) fn select_backend(
    candidates: &[IdleBackend],
    mut probe: impl FnMut(IdleBackend) -> Option<u64>,
) -> Option<(IdleBackend, u64)> {
    candidates
        .iter()
        .find_map(|&backend| probe(backend).map(|ms| (backend, ms)))
}

/// Sampled idle state shared between the poller thread and readers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdleCache {
    /// Poller not started or no sample yet.
    Unknown,
    /// `ms` of idle time measured at `taken_ms` on the sampler clock.
    Sample { ms: u64, taken_ms: u64 },
    /// No backend answered the last `failures` tries in a row; the next try
    /// is at `retry_ms`. A desktop shell that restarts misses a sample or
    /// two and is back.
    Failed { failures: u32, retry_ms: u64 },
}

/// The first retry after a failed sample, doubling up to [`IDLE_RETRY_MAX_MS`].
pub(super) const IDLE_RETRY_FIRST_MS: u64 = 2_000;
pub(super) const IDLE_RETRY_MAX_MS: u64 = 120_000;

impl IdleCache {
    /// Idle time at `now_ms` (same clock as `taken_ms`), extrapolated from
    /// the last sample. `None` until a sample exists and while sampling
    /// fails, so readers fall back to input to Quill's own windows.
    pub(super) fn read(self, now_ms: u64) -> Option<u64> {
        match self {
            Self::Sample { ms, taken_ms } => {
                Some(ms.saturating_add(now_ms.saturating_sub(taken_ms)))
            }
            Self::Unknown | Self::Failed { .. } => None,
        }
    }

    /// Whether the sampler should try a sample at `now_ms`.
    pub(super) fn sample_due(self, now_ms: u64) -> bool {
        match self {
            Self::Failed { retry_ms, .. } => now_ms >= retry_ms,
            Self::Unknown | Self::Sample { .. } => true,
        }
    }

    /// The cache after a sample taken at `now_ms` answered `ms`, or nothing.
    pub(super) fn after_sample(self, now_ms: u64, ms: Option<u64>) -> Self {
        if let Some(ms) = ms {
            return Self::Sample {
                ms,
                taken_ms: now_ms,
            };
        }
        let failures = match self {
            Self::Failed { failures, .. } => failures.saturating_add(1),
            Self::Unknown | Self::Sample { .. } => 1,
        };
        let wait = IDLE_RETRY_FIRST_MS
            .saturating_mul(1 << failures.saturating_sub(1).min(16))
            .min(IDLE_RETRY_MAX_MS);
        Self::Failed {
            failures,
            retry_ms: now_ms.saturating_add(wait),
        }
    }
}

/// How long the sampler keeps running after the last read. The auto-lock
/// tick reads every second while armed, so a gap this long means auto-lock
/// is off, locked, or the app is gone.
pub(super) const IDLE_ARMED_GRACE_MS: u64 = 5_000;

/// Whether the sampler should take another sample: auto-lock asked for the
/// idle time recently enough.
pub(super) fn idle_sampling_wanted(last_read_ms: Option<u64>, now_ms: u64) -> bool {
    last_read_ms.is_some_and(|t| now_ms.saturating_sub(t) <= IDLE_ARMED_GRACE_MS)
}

#[cfg(target_os = "linux")]
pub(super) use sampler::{linux_idle_known, linux_idle_ms};

#[cfg(target_os = "linux")]
mod sampler {
    use super::*;
    use crate::locks::LockExt;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    static IDLE: std::sync::Mutex<IdleCache> = std::sync::Mutex::new(IdleCache::Unknown);

    /// Time of the latest read (`u64::MAX` = never).
    static IDLE_LAST_READ: AtomicU64 = AtomicU64::new(u64::MAX);

    /// True while a sampler thread is alive.
    static IDLE_RUNNING: AtomicBool = AtomicBool::new(false);

    fn cache() -> IdleCache {
        *IDLE.lock_or_recover()
    }

    fn now_ms() -> u64 {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        u64::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn last_read() -> Option<u64> {
        match IDLE_LAST_READ.load(Ordering::Relaxed) {
            u64::MAX => None,
            t => Some(t),
        }
    }

    /// Reads the cache a background thread refreshes every two seconds.
    /// A read starts the thread; it pauses once reads stop (auto-lock off or
    /// locked). While no backend answers it tries again less and less
    /// often. Never blocks on D-Bus or X.
    pub(in crate::passcode) fn linux_idle_ms() -> Option<u64> {
        IDLE_LAST_READ.store(now_ms(), Ordering::Relaxed);
        start();
        cache().read(now_ms())
    }

    /// True once a sample succeeded; starts nothing (safe from render).
    pub(in crate::passcode) fn linux_idle_known() -> bool {
        cache().read(0).is_some()
    }

    /// Connections the sampler thread owns, opened on first use.
    struct Sources {
        #[cfg(feature = "ui")]
        bus: Option<zbus::blocking::Connection>,
        x11: Option<(x11rb::rust_connection::RustConnection, u32)>,
    }

    impl Sources {
        fn open(env: SessionEnv) -> Self {
            Self {
                // A shell that takes the call and never answers would
                // leave the last sample to extrapolate into a lock.
                #[cfg(feature = "ui")]
                bus: zbus::blocking::connection::Builder::session()
                    .and_then(|bus| bus.method_timeout(Duration::from_secs(2)).build())
                    .ok(),
                x11: if env.x11 { connect_x11() } else { None },
            }
        }

        fn sample(&self, backend: IdleBackend) -> Option<u64> {
            match backend {
                #[cfg(feature = "ui")]
                IdleBackend::Mutter => {
                    let reply = self
                        .bus
                        .as_ref()?
                        .call_method(
                            Some("org.gnome.Mutter.IdleMonitor"),
                            "/org/gnome/Mutter/IdleMonitor/Core",
                            Some("org.gnome.Mutter.IdleMonitor"),
                            "GetIdletime",
                            &(),
                        )
                        .ok()?;
                    reply.body().deserialize::<u64>().ok()
                }
                #[cfg(feature = "ui")]
                IdleBackend::ScreenSaver => {
                    let bus = self.bus.as_ref()?;
                    // KDE exports the object at both paths.
                    ["/org/freedesktop/ScreenSaver", "/ScreenSaver"]
                        .into_iter()
                        .find_map(|path| {
                            let reply = bus
                                .call_method(
                                    Some("org.freedesktop.ScreenSaver"),
                                    path,
                                    Some("org.freedesktop.ScreenSaver"),
                                    "GetSessionIdleTime",
                                    &(),
                                )
                                .ok()?;
                            reply.body().deserialize::<u32>().ok().map(u64::from)
                        })
                }
                #[cfg(not(feature = "ui"))]
                IdleBackend::Mutter | IdleBackend::ScreenSaver => None,
                IdleBackend::X11ScreenSaver => {
                    use x11rb::protocol::screensaver::ConnectionExt as _;
                    let (conn, root) = self.x11.as_ref()?;
                    let info = conn.screensaver_query_info(*root).ok()?.reply().ok()?;
                    Some(u64::from(info.ms_since_user_input))
                }
            }
        }
    }

    fn connect_x11() -> Option<(x11rb::rust_connection::RustConnection, u32)> {
        use x11rb::connection::Connection;
        let (conn, screen) = x11rb::rust_connection::RustConnection::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        Some((conn, root))
    }

    /// One reading: the backend chosen earlier if it still answers, else a
    /// fresh probe of the candidates.
    fn take_sample(
        sources: &Sources,
        chosen: Option<IdleBackend>,
        candidates: &[IdleBackend],
    ) -> Option<(IdleBackend, u64)> {
        if let Some(backend) = chosen
            && let Some(ms) = sources.sample(backend)
        {
            return Some((backend, ms));
        }
        select_backend(candidates, |backend| sources.sample(backend))
    }

    /// Starts the sampler thread unless one is running. The thread exits
    /// when reads stop, and the next read restarts it.
    fn start() {
        if IDLE_RUNNING.swap(true, Ordering::AcqRel) {
            return;
        }
        now_ms(); // pin the clock origin before the first sample
        // A sample from before a pause would extrapolate to a bogus huge idle
        // time (and lock spuriously); drop it until the fresh one lands. A
        // failure keeps its backoff.
        {
            let mut idle = IDLE.lock_or_recover();
            if matches!(*idle, IdleCache::Sample { .. }) {
                *idle = IdleCache::Unknown;
            }
        }
        let spawned = crate::thread_guard::spawn_guarded("quill-idle", || {
            let env = SessionEnv::detect();
            let candidates = candidate_backends(env);
            let mut sources = Sources::open(env);
            let mut chosen = None;
            loop {
                if !idle_sampling_wanted(last_read(), now_ms()) {
                    IDLE_RUNNING.store(false, Ordering::Release);
                    // A read may have raced the exit; pick it back up.
                    if idle_sampling_wanted(last_read(), now_ms())
                        && !IDLE_RUNNING.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                }
                let before = cache();
                if before.sample_due(now_ms()) {
                    if matches!(before, IdleCache::Failed { .. }) {
                        // The bus or the X server may be the one that went away.
                        sources = Sources::open(env);
                    }
                    let sample = take_sample(&sources, chosen, candidates);
                    if let Some((backend, _)) = sample {
                        chosen = Some(backend);
                    }
                    let mut idle = IDLE.lock_or_recover();
                    *idle = idle.after_sample(now_ms(), sample.map(|(_, ms)| ms));
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        if spawned.is_err() {
            IDLE_RUNNING.store(false, Ordering::Release);
        }
    }
}
