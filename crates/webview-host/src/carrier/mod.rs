//! `quill-webview --carrier`: one hidden web view hosting the relay page of
//! a web proxy (TDLib `webProxyCarrier`, `parity:data-proxy-web`).
//!
//! Quill sends [`CarrierCommand::Open`] as the first line on stdin, then
//! `deliver` lines with the bytes TDLib wants the page to get, and finally
//! `close`. This process answers on stdout with what the page posts to its
//! bridge object and with failures (`quill_webview_protocol::carrier`).
//! With [`CarrierCommand::Probe`] instead it only proves a carrier could
//! start here, and exits.
//!
//! The window is never shown and the URL is never logged. The web view is
//! locked down before the page loads ([`lockdown`]); the bridge object
//! ([`script`], `bridge.js`) exists only in the top-level document of the
//! carrier's origin; the page may navigate and load only within the origin
//! and path prefix. Deliveries wait until the page listens and then go one
//! at a time, each acknowledged before the next ([`relay`]), so a hung page
//! fills a bounded queue and fails instead of growing one. The process ends
//! with Quill ([`watchdog`]).

#[cfg(windows)]
mod browser_exit;
mod lockdown;
mod relay;
mod script;
mod watchdog;

use quill_webview_protocol::carrier::{
    CarrierCommand, CarrierEvent, CarrierFailure, CarrierScope, MAX_DELIVERY_BYTES, MAX_LINE_BYTES,
    SecretUrl, base64_len, is_base64,
};
use quill_webview_protocol::encode;
use quill_webview_protocol::lines::{Line, read_bounded_line};
use relay::{PageMessage, Relay, page_message};
use script::{BridgeKeys, bridge_script, hook_call, js_string};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, PageLoadEvent, PermissionResponse, WebViewBuilder};

/// How often the loop looks at its timers and asks the page for a sign of
/// life (tdesktop `kProbeInterval`).
const TICK: Duration = Duration::from_secs(3);
/// How long the page gets to end its relay session after `{"t":"close"}`
/// (the schema's "for example, in 200 milliseconds").
const CLOSE_GRACE: Duration = Duration::from_millis(200);
/// Set (in debug builds only) to let the helper's own tests serve the page
/// from `http://127.0.0.1:<port>`.
const LOOPBACK_ENV: &str = "QUILL_WEBVIEW_CARRIER_LOOPBACK";

/// What wakes the loop.
#[derive(Debug)]
enum Wake {
    Command(CarrierCommand),
    /// Stdin closed or sent something unusable: Quill is gone or broken.
    Hangup,
    /// The lockdown is in place (`true`) or cannot be (`false`).
    LockedDown(bool),
    /// A bridge message, already checked for its source and token.
    Page(PageMessage),
    /// A document started or finished loading.
    LoadStarted,
    LoadFinished,
    /// The HTTP status of the loaded document (0 when unknown).
    HttpStatus(u16),
    /// Linux: the load failed (network, TLS). Elsewhere a failed load
    /// shows as a page that never starts.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    LoadError,
    /// The page tried to leave its scope.
    ForbiddenNavigation,
    /// macOS: WebKit's content process died.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    RendererGone,
    /// Time to look at the timers ([`TICK`], or the close grace ending).
    Tick,
}

/// The page a carrier loads.
struct Page {
    url: SecretUrl,
    scope: CarrierScope,
    keys: BridgeKeys,
}

/// Stdout, one line per event.
fn emit(event: &CarrierEvent) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(encode(event).as_bytes());
    let _ = out.flush();
}

/// Fail with `failure` and leave.
fn fail(failure: CarrierFailure, control_flow: &mut ControlFlow) {
    emit(&CarrierEvent::Failed { failure });
    *control_flow = ControlFlow::Exit;
}

fn load_failed(message: &str) -> CarrierFailure {
    CarrierFailure::LoadFailed {
        message: message.into(),
    }
}

pub fn run() {
    watchdog::follow_parent();
    let mut input = std::io::BufReader::new(std::io::stdin());
    let first = match read_bounded_line(&mut input, MAX_LINE_BYTES) {
        Ok(Line::Text(line)) => std::str::from_utf8(&line)
            .ok()
            .and_then(|line| quill_webview_protocol::decode::<CarrierCommand>(line).ok()),
        _ => None,
    };
    match first {
        Some(CarrierCommand::Open {
            url,
            origin,
            path_prefix,
            bridge_object_name,
            data_dir,
        }) => {
            let loopback = cfg!(debug_assertions) && std::env::var_os(LOOPBACK_ENV).is_some();
            let scope = CarrierScope::new(&origin, &path_prefix, &bridge_object_name, loopback)
                .filter(|scope| scope.allows_navigation(url.expose()));
            let (Some(scope), Some(keys)) = (scope, BridgeKeys::random()) else {
                emit(&CarrierEvent::Failed {
                    failure: load_failed("the carrier's address is not one Quill accepts"),
                });
                return;
            };
            host(Some(Page { url, scope, keys }), data_dir, input);
        }
        Some(CarrierCommand::Probe { data_dir }) => host(None, data_dir, input),
        // Quill went away before saying what to load.
        _ => {}
    }
}

/// Run one carrier (`page`), or the probe (`None`).
fn host(
    page: Option<Page>,
    data_dir: Option<String>,
    input: impl std::io::BufRead + Send + 'static,
) {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<Wake>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_activate_ignoring_other_apps(false);
    }
    // Quill's commands, and its hangup, are heard from here on, even while
    // the web view is still being built.
    let commands = event_loop.create_proxy();
    let reader = std::thread::Builder::new()
        .name("quill-carrier-commands".into())
        .spawn(move || {
            watchdog::read_commands(input, move |command| {
                commands
                    .send_event(command.map_or(Wake::Hangup, Wake::Command))
                    .is_ok()
            })
        });
    if reader.is_err() {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::Terminated,
        });
        return;
    }
    let awake = lockdown::stay_awake();
    let data_dir = data_dir.map(PathBuf::from);

    let Ok(window) = WindowBuilder::new()
        .with_title("Quill web proxy")
        .with_visible(false)
        .with_focused(false)
        .with_inner_size(tao::dpi::LogicalSize::new(480.0, 360.0))
        .build(&event_loop)
    else {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::Unsupported,
        });
        return;
    };

    // Nothing the view does counts until Quill's page is asked for (wry
    // may load a blank page of its own first).
    let armed = Arc::new(AtomicBool::new(false));
    #[cfg(windows)]
    let mut context = wry::WebContext::new(data_dir.as_ref().map(|dir| dir.join("webview2")));
    #[cfg(windows)]
    let builder = WebViewBuilder::new_with_web_context(&mut context);
    #[cfg(not(windows))]
    let builder = WebViewBuilder::new();
    let load_proxy = event_loop.create_proxy();
    let load_armed = armed.clone();
    let load_scope = page.as_ref().map(|page| page.scope.clone());
    let builder = builder
        .with_incognito(true)
        .with_focused(false)
        .with_on_page_load_handler(move |event, url| {
            if load_armed.load(Ordering::Acquire) {
                // The top-level document only: a frame of the page's own
                // (a blank one, say) passes the navigation check below,
                // but the page itself must stay in scope.
                let in_scope = load_scope
                    .as_ref()
                    .is_some_and(|scope| scope.allows_navigation(&url));
                let _ = load_proxy.send_event(match event {
                    _ if !in_scope => Wake::ForbiddenNavigation,
                    PageLoadEvent::Started => Wake::LoadStarted,
                    PageLoadEvent::Finished => Wake::LoadFinished,
                });
            }
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_permission_handler(|_| PermissionResponse::Deny)
        .with_devtools(false)
        .with_hotkeys_zoom(false)
        .with_back_forward_navigation_gestures(false)
        .with_general_autofill_enabled(false);
    let builder = match &page {
        Some(page) => {
            let ipc_proxy = event_loop.create_proxy();
            let nav_proxy = event_loop.create_proxy();
            let ipc_scope = page.scope.clone();
            let nav_scope = page.scope.clone();
            let ipc_token = page.keys.token.clone();
            let nav_armed = armed.clone();
            builder
                .with_initialization_script_for_main_only(
                    bridge_script(&page.scope, &page.keys),
                    false,
                )
                .with_ipc_handler(move |request| {
                    let source = request.uri().to_string();
                    let message = if ipc_scope.allows_source(&source) {
                        page_message(request.body(), &ipc_token)
                    } else {
                        None
                    };
                    // Anything without the token or from another origin is
                    // not the carrier's bridge and is dropped.
                    if let Some(message) = message {
                        let _ = ipc_proxy.send_event(match message {
                            PageMessage::LeftScope => Wake::ForbiddenNavigation,
                            message => Wake::Page(message),
                        });
                    }
                })
                .with_navigation_handler(move |url| {
                    // Some engines ask for frames too (WebView2's frames
                    // go through the lockdown's own handler).
                    let allowed = lockdown::may_load(&nav_scope, &url);
                    if !allowed && nav_armed.load(Ordering::Acquire) {
                        let _ = nav_proxy.send_event(Wake::ForbiddenNavigation);
                    }
                    allowed
                })
        }
        None => builder.with_navigation_handler(|_| false),
    };
    let builder = lockdown::configure(builder);
    #[cfg(target_os = "macos")]
    let builder = {
        use wry::WebViewBuilderExtDarwin;
        let gone = event_loop.create_proxy();
        builder.with_on_web_content_process_terminate_handler(move || {
            let _ = gone.send_event(Wake::RendererGone);
        })
    };
    #[cfg(target_os = "linux")]
    let built = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        window.default_vbox().map(|vbox| builder.build_gtk(vbox))
    };
    #[cfg(not(target_os = "linux"))]
    let built = Some(builder.build(&window));
    let Some(Ok(webview)) = built else {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::Unsupported,
        });
        return;
    };
    // The probe locks down a page nobody loads, to prove it can be done.
    let probe_scope = CarrierScope::new("https://quill.invalid", "/", "TelegramWebProxy", false);
    let Some(scope) = page.as_ref().map(|page| page.scope.clone()).or(probe_scope) else {
        return;
    };
    lockdown::lock_down(
        &webview,
        &scope,
        data_dir.as_deref(),
        event_loop.create_proxy(),
    );
    #[cfg(windows)]
    let mut browser = browser_exit::BrowserProcess::of(&webview);
    let mut view = Some(webview);
    #[cfg(windows)]
    let mut context = Some(context);

    // The timers run on threads of their own (`watchdog::tick_every`).
    let ticks = event_loop.create_proxy();
    if watchdog::tick_every(TICK, move || ticks.send_event(Wake::Tick).is_ok()).is_err() {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::Terminated,
        });
        return;
    }
    // Set by a `close` that says no other carrier shares the data folder.
    #[cfg(windows)]
    let mut wait_for_browser = false;
    let close_timer = event_loop.create_proxy();
    let status_proxy = event_loop.create_proxy();
    let mut relay = Relay::new(Instant::now());
    let mut next_tick = Instant::now() + TICK;
    event_loop.run(move |event, _, control_flow| {
        let _keep = (&window, &awake);
        if let Event::LoopDestroyed = event {
            // The web view and its environment go first, then (on Windows,
            // for the last carrier) the helper waits for the browser
            // process to let go of the data folder.
            drop(view.take());
            #[cfg(windows)]
            {
                drop(context.take());
                if let Some(browser) = browser.take()
                    && wait_for_browser
                {
                    browser.wait();
                }
            }
            return;
        }
        let Some(webview) = view.as_ref() else {
            return;
        };
        let now = Instant::now();
        *control_flow = ControlFlow::Wait;
        if let Some(at) = relay.closing_at {
            // Only the end of the close grace matters now.
            if now >= at {
                *control_flow = ControlFlow::Exit;
            }
            return;
        }
        let loading = armed.load(Ordering::Acquire);
        match event {
            // Ticks may bunch up behind a busy loop; about one per `TICK`
            // counts (the thread's clock and this one differ a little).
            Event::UserEvent(Wake::Tick) if now + TICK / 2 >= next_tick => {
                next_tick = now + TICK;
                if loading {
                    if let Some(failure) = relay.failure(now) {
                        return fail(failure, control_flow);
                    }
                    if relay.started()
                        && let Some(page) = &page
                    {
                        let _ = webview.evaluate_script(&hook_call(&page.keys, "probe", ""));
                    }
                }
                emit(&CarrierEvent::Alive);
            }
            Event::UserEvent(Wake::LockedDown(locked)) => {
                if !locked {
                    return fail(CarrierFailure::Unsupported, control_flow);
                }
                emit(&CarrierEvent::Ready);
                let Some(page) = &page else {
                    *control_flow = ControlFlow::Exit;
                    return;
                };
                armed.store(true, Ordering::Release);
                relay.load_started(now);
                if webview.load_url(page.url.expose()).is_err() {
                    return fail(
                        load_failed("the relay page could not be opened"),
                        control_flow,
                    );
                }
            }
            Event::UserEvent(Wake::Command(CarrierCommand::Deliver { data })) => {
                if !is_base64(&data)
                    || data.len() > base64_len(MAX_DELIVERY_BYTES)
                    || !relay.queue(data)
                {
                    return fail(CarrierFailure::Terminated, control_flow);
                }
            }
            Event::UserEvent(Wake::Command(CarrierCommand::Close { notify_page, last })) => {
                #[cfg(windows)]
                {
                    wait_for_browser = last;
                }
                #[cfg(not(windows))]
                let _ = last;
                match &page {
                    Some(page) if notify_page && relay.posted => {
                        let close = js_string(r#"{"t":"close"}"#);
                        let _ = webview.evaluate_script(&hook_call(
                            &page.keys,
                            "deliver",
                            &format!("0, 't', {close}"),
                        ));
                        relay.closing_at = Some(now + CLOSE_GRACE);
                        let timer = close_timer.clone();
                        if !watchdog::wake_after(CLOSE_GRACE, move || {
                            let _ = timer.send_event(Wake::Tick);
                        }) {
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    _ => *control_flow = ControlFlow::Exit,
                }
                return;
            }
            // A second `open` or `probe`, or Quill hung up.
            Event::UserEvent(
                Wake::Command(CarrierCommand::Open { .. } | CarrierCommand::Probe { .. })
                | Wake::Hangup,
            ) => {
                *control_flow = ControlFlow::Exit;
                return;
            }
            Event::UserEvent(Wake::LoadStarted) if loading => relay.load_started(now),
            Event::UserEvent(Wake::LoadFinished) if loading => {
                relay.load_finished(now);
                // An HTTP error page is on the carrier's origin too, so the
                // bridge alone would make it look alive.
                let proxy = status_proxy.clone();
                let _ =
                    webview.evaluate_script_with_callback(script::STATUS_SCRIPT, move |result| {
                        let status = result.trim().parse().unwrap_or(0);
                        let _ = proxy.send_event(Wake::HttpStatus(status));
                    });
            }
            Event::UserEvent(Wake::HttpStatus(status)) if loading && status >= 400 => {
                return fail(
                    load_failed(&format!("the relay page answered HTTP {status}")),
                    control_flow,
                );
            }
            Event::UserEvent(Wake::LoadError) if loading => {
                return fail(load_failed("the relay page could not load"), control_flow);
            }
            Event::UserEvent(Wake::ForbiddenNavigation) => {
                return fail(
                    load_failed("the page tried to leave its origin"),
                    control_flow,
                );
            }
            Event::UserEvent(Wake::RendererGone) => {
                return fail(CarrierFailure::Terminated, control_flow);
            }
            Event::UserEvent(Wake::Page(message)) if loading => {
                if relay.heard(&message, now).is_err() {
                    return fail(CarrierFailure::Terminated, control_flow);
                }
                match message {
                    PageMessage::Binary(data) => emit(&CarrierEvent::Binary { data }),
                    PageMessage::Text(text) => emit(&CarrierEvent::Text { text }),
                    _ => {}
                }
            }
            _ => {}
        }
        if let (Some(page), Some((sequence, data))) = (&page, relay.next(now)) {
            let _ = webview.evaluate_script(&hook_call(
                &page.keys,
                "deliver",
                &format!("{sequence}, 'b', '{data}'"),
            ));
        }
    });
}
