//! `quill-webview --carrier`: one hidden web view hosting the relay page of
//! a web proxy (TDLib `webProxyCarrier`, `parity:data-proxy-web`).
//!
//! Quill sends [`CarrierCommand::Open`] as the first line on stdin, then
//! `deliver` lines with the bytes TDLib wants the page to get, and finally
//! `close`. This process answers on stdout with what the page posts to its
//! bridge object and with failures (`quill_webview_protocol::carrier`).
//!
//! The window is never shown and the URL is never logged. The bridge
//! object (`carrier_bridge.js`) exists only in the top-level document of
//! the carrier's origin; the page may navigate only within the origin and
//! path prefix, and an injected Content-Security-Policy keeps its loads and
//! requests there too. Deliveries go one at a time: the page acknowledges
//! each before the next, so a hung page fills a bounded queue and fails
//! instead of growing one.

use quill_webview_protocol::carrier::{
    CarrierCommand, CarrierEvent, CarrierFailure, CarrierScope, MAX_DELIVERY_BYTES, MAX_LINE_BYTES,
    MAX_PAGE_BINARY_BYTES, MAX_PAGE_TEXT_BYTES, MAX_PENDING_BYTES, MAX_PENDING_ITEMS, base64_len,
    is_base64,
};
use quill_webview_protocol::lines::{Line, read_bounded_line};
use quill_webview_protocol::{decode, encode};
use std::collections::VecDeque;
use std::io::Write;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, PermissionResponse, WebViewBuilder};

/// How often the loop looks at its timers and asks the page for a sign of
/// life (tdesktop `kProbeInterval`).
const TICK: Duration = Duration::from_secs(3);
/// No word from the page for this long, after it said hello, means its
/// renderer is gone or hung (tdesktop `kHealthTimeout`).
const PAGE_SILENCE: Duration = Duration::from_secs(10);
/// A delivery the page has not acknowledged in this long (tdesktop
/// `kWriteTimeout`).
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
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
    /// A bridge message, already checked for its source and token.
    Page(PageMessage),
    /// The page tried to leave its scope.
    ForbiddenNavigation,
    /// macOS: WebKit's content process died.
    RendererGone,
}

/// What the bridge script posts, after the token.
#[derive(Debug, PartialEq, Eq)]
enum PageMessage {
    /// `h`: the bridge is in place, or answers a probe.
    Hello,
    /// `a<seq>`: the page took a delivery.
    Ack(u64),
    /// `b<base64>`: a binary message.
    Binary(String),
    /// `t<text>`: a string message.
    Text(String),
    /// `f`: the bridge failed (no `onmessage`, the page's handler threw).
    Failed,
    /// Too long, not base64, or unknown: treated as a broken relay.
    Invalid,
}

/// Split one IPC body: `None` unless it starts with the carrier's token.
fn page_message(body: &str, token: &str) -> Option<PageMessage> {
    let rest = body.strip_prefix(token)?;
    let mut chars = rest.chars();
    let kind = chars.next()?;
    let value = chars.as_str();
    Some(match kind {
        'h' if value.is_empty() => PageMessage::Hello,
        'a' => value.parse().map_or(PageMessage::Invalid, PageMessage::Ack),
        'b' if value.len() <= base64_len(MAX_PAGE_BINARY_BYTES)
            && !value.is_empty()
            && is_base64(value) =>
        {
            PageMessage::Binary(value.to_string())
        }
        't' if value.len() <= MAX_PAGE_TEXT_BYTES => PageMessage::Text(value.to_string()),
        'f' => PageMessage::Failed,
        _ => PageMessage::Invalid,
    })
}

/// `bytes` random bytes as lower-case hex.
fn random_hex(bytes: usize) -> Option<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).ok()?;
    Some(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The page's Content-Security-Policy: everything from the carrier's path
/// prefix, nothing from anywhere else, no frames, no forms.
fn content_policy(scope: &CarrierScope) -> String {
    let page = scope.page_source();
    let socket = scope.socket_source();
    format!(
        "default-src {page}; script-src {page} 'unsafe-inline' 'unsafe-eval' blob:; \
         style-src {page} 'unsafe-inline'; img-src {page} data: blob:; font-src {page} data:; \
         media-src {page} blob:; connect-src {page} {socket}; worker-src {page} blob:; \
         frame-src 'none'; child-src 'none'; object-src 'none'; form-action 'none'; \
         base-uri 'none'"
    )
}

/// A string as a JavaScript literal (JSON, with the two line separators
/// escaped so the script stays one statement on any engine).
fn js_string(text: &str) -> String {
    serde_json::to_string(text)
        .unwrap_or_else(|_| "\"\"".into())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// The bridge script for one carrier.
fn bridge_script(scope: &CarrierScope, token: &str, deliver_name: &str) -> String {
    include_str!("carrier_bridge.js")
        .replace("__ORIGIN__", &js_string(scope.origin()))
        .replace("__TOKEN__", &js_string(token))
        .replace("__POLICY__", &js_string(&content_policy(scope)))
        .replace("__NAME__", &js_string(scope.bridge_object_name()))
        .replace("__DELIVER__", &js_string(deliver_name))
}

/// Stdout, one line per event.
fn emit(event: &CarrierEvent) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(encode(event).as_bytes());
    let _ = out.flush();
}

/// Read the rest of stdin into the loop, one command per line.
fn read_commands(mut input: impl std::io::BufRead, proxy: EventLoopProxy<Wake>) {
    let cap = base64_len(MAX_DELIVERY_BYTES) + 1024;
    loop {
        let wake = match read_bounded_line(&mut input, cap) {
            Ok(Line::Text(line)) => match std::str::from_utf8(&line)
                .ok()
                .and_then(|line| decode::<CarrierCommand>(line).ok())
            {
                Some(command) => Wake::Command(command),
                None => Wake::Hangup,
            },
            Ok(Line::TooLong) | Ok(Line::End) | Err(_) => Wake::Hangup,
        };
        let last = matches!(wake, Wake::Hangup);
        if proxy.send_event(wake).is_err() || last {
            return;
        }
    }
}

/// The relay's state, owned by the event loop.
struct Relay {
    pending: VecDeque<String>,
    pending_bytes: usize,
    /// `(sequence, sent at)` of the delivery the page has yet to take.
    in_flight: Option<(u64, Instant)>,
    sequence: u64,
    /// The bridge said hello at least once.
    bridge_seen: bool,
    /// The page posted a message (so it gets `{"t":"close"}` at the end).
    posted: bool,
    last_heard: Instant,
    closing_at: Option<Instant>,
}

impl Relay {
    fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            pending_bytes: 0,
            in_flight: None,
            sequence: 0,
            bridge_seen: false,
            posted: false,
            last_heard: Instant::now(),
            closing_at: None,
        }
    }

    /// Queue a delivery; `false` when the page is too far behind.
    fn queue(&mut self, data: String) -> bool {
        let bytes = self.pending_bytes.saturating_add(data.len());
        if self.pending.len() >= MAX_PENDING_ITEMS || bytes > base64_len(MAX_PENDING_BYTES) {
            return false;
        }
        self.pending_bytes = bytes;
        self.pending.push_back(data);
        true
    }

    /// The next delivery to hand the page, if none is in flight.
    fn next(&mut self, now: Instant) -> Option<(u64, String)> {
        if self.in_flight.is_some() {
            return None;
        }
        let data = self.pending.pop_front()?;
        self.pending_bytes = self.pending_bytes.saturating_sub(data.len());
        self.sequence = self.sequence.saturating_add(1);
        self.in_flight = Some((self.sequence, now));
        Some((self.sequence, data))
    }

    /// The page took delivery `sequence`; `false` if it was not the one in
    /// flight.
    fn ack(&mut self, sequence: u64) -> bool {
        match self.in_flight {
            Some((expected, _)) if expected == sequence => {
                self.in_flight = None;
                true
            }
            _ => false,
        }
    }

    /// Whether the page stopped answering by `now`.
    fn hung(&self, now: Instant) -> bool {
        let silent =
            self.bridge_seen && now.saturating_duration_since(self.last_heard) > PAGE_SILENCE;
        let stuck = self
            .in_flight
            .is_some_and(|(_, sent)| now.saturating_duration_since(sent) > WRITE_TIMEOUT);
        silent || stuck
    }
}

/// Fail with `failure` and leave.
fn fail(failure: CarrierFailure, control_flow: &mut ControlFlow) {
    emit(&CarrierEvent::Failed { failure });
    *control_flow = ControlFlow::Exit;
}

pub fn run() {
    let mut input = std::io::BufReader::new(std::io::stdin());
    let open = match read_bounded_line(&mut input, MAX_LINE_BYTES) {
        Ok(Line::Text(line)) => std::str::from_utf8(&line)
            .ok()
            .and_then(|line| decode::<CarrierCommand>(line).ok()),
        _ => None,
    };
    let Some(CarrierCommand::Open {
        url,
        origin,
        path_prefix,
        bridge_object_name,
    }) = open
    else {
        // Quill went away before saying what to load.
        return;
    };
    let loopback = cfg!(debug_assertions) && std::env::var_os(LOOPBACK_ENV).is_some();
    let scope = CarrierScope::new(&origin, &path_prefix, &bridge_object_name, loopback)
        .filter(|scope| scope.allows_navigation(&url));
    let (Some(scope), Some(token), Some(deliver_id)) = (scope, random_hex(16), random_hex(8))
    else {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed {
                message: "the carrier's address is not one Quill accepts".into(),
            },
        });
        return;
    };
    let deliver_name = format!("__quillCarrier{deliver_id}");

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<Wake>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_activate_ignoring_other_apps(false);
    }
    let window = match WindowBuilder::new()
        .with_title("Quill web proxy")
        .with_visible(false)
        .with_focused(false)
        .with_inner_size(tao::dpi::LogicalSize::new(480.0, 360.0))
        .build(&event_loop)
    {
        Ok(window) => window,
        Err(_) => {
            emit(&CarrierEvent::Failed {
                failure: CarrierFailure::Unsupported,
            });
            return;
        }
    };

    let ipc_proxy = event_loop.create_proxy();
    let nav_proxy = event_loop.create_proxy();
    let ipc_scope = scope.clone();
    let nav_scope = scope.clone();
    let ipc_token = token.clone();
    let builder = WebViewBuilder::new()
        .with_incognito(true)
        .with_initialization_script_for_main_only(
            bridge_script(&scope, &token, &deliver_name),
            true,
        )
        .with_ipc_handler(move |request| {
            let source = request.uri().to_string();
            let message = if ipc_scope.allows_source(&source) {
                page_message(request.body(), &ipc_token)
            } else {
                None
            };
            // Anything without the token or from another origin is not the
            // carrier's bridge and is dropped.
            if let Some(message) = message {
                let _ = ipc_proxy.send_event(Wake::Page(message));
            }
        })
        .with_navigation_handler(move |url| {
            let allowed = nav_scope.allows_navigation(&url);
            if !allowed {
                let _ = nav_proxy.send_event(Wake::ForbiddenNavigation);
            }
            allowed
        })
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .with_permission_handler(|_| PermissionResponse::Deny)
        .with_devtools(false)
        .with_hotkeys_zoom(false)
        .with_back_forward_navigation_gestures(false)
        .with_general_autofill_enabled(false)
        .with_url(&url);
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
        match window.default_vbox() {
            Some(vbox) => builder.build_gtk(vbox),
            None => builder.build(&window),
        }
    };
    #[cfg(not(target_os = "linux"))]
    let built = builder.build(&window);
    let Ok(webview) = built else {
        emit(&CarrierEvent::Failed {
            failure: CarrierFailure::Unsupported,
        });
        return;
    };
    drop(url);

    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || read_commands(input, proxy));
    emit(&CarrierEvent::Ready);

    let mut relay = Relay::new();
    let mut next_tick = Instant::now() + TICK;
    event_loop.run(move |event, _, control_flow| {
        let _window = &window;
        let now = Instant::now();
        if let Some(at) = relay.closing_at {
            if now >= at {
                *control_flow = ControlFlow::Exit;
                return;
            }
            *control_flow = ControlFlow::WaitUntil(at);
            return;
        }
        *control_flow = ControlFlow::WaitUntil(next_tick);
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) if now >= next_tick => {
                next_tick = now + TICK;
                *control_flow = ControlFlow::WaitUntil(next_tick);
                if relay.hung(now) {
                    return fail(CarrierFailure::Terminated, control_flow);
                }
                if relay.bridge_seen {
                    let _ = webview.evaluate_script(&format!(
                        "window[{name}] && window[{name}].probe();",
                        name = js_string(&deliver_name)
                    ));
                }
                emit(&CarrierEvent::Alive);
            }
            Event::UserEvent(Wake::Command(CarrierCommand::Deliver { data })) => {
                if !is_base64(&data)
                    || data.len() > base64_len(MAX_DELIVERY_BYTES)
                    || !relay.queue(data)
                {
                    return fail(CarrierFailure::Terminated, control_flow);
                }
            }
            Event::UserEvent(Wake::Command(CarrierCommand::Close { notify_page })) => {
                if notify_page && relay.posted {
                    let _ = webview.evaluate_script(&format!(
                        "window[{name}] && window[{name}].deliver(0, 't', {close});",
                        name = js_string(&deliver_name),
                        close = js_string(r#"{"t":"close"}"#),
                    ));
                    relay.closing_at = Some(now + CLOSE_GRACE);
                    *control_flow = ControlFlow::WaitUntil(now + CLOSE_GRACE);
                } else {
                    *control_flow = ControlFlow::Exit;
                }
                return;
            }
            // A second `open`, or Quill hung up.
            Event::UserEvent(Wake::Command(CarrierCommand::Open { .. }) | Wake::Hangup) => {
                *control_flow = ControlFlow::Exit;
                return;
            }
            Event::UserEvent(Wake::ForbiddenNavigation) => {
                return fail(
                    CarrierFailure::LoadFailed {
                        message: "the page tried to leave its origin".into(),
                    },
                    control_flow,
                );
            }
            Event::UserEvent(Wake::RendererGone) => {
                return fail(CarrierFailure::Terminated, control_flow);
            }
            Event::UserEvent(Wake::Page(message)) => {
                relay.last_heard = now;
                match message {
                    PageMessage::Hello => relay.bridge_seen = true,
                    PageMessage::Ack(sequence) => {
                        if !relay.ack(sequence) {
                            return fail(CarrierFailure::Terminated, control_flow);
                        }
                    }
                    PageMessage::Binary(data) => {
                        relay.posted = true;
                        emit(&CarrierEvent::Binary { data });
                    }
                    PageMessage::Text(text) => {
                        relay.posted = true;
                        emit(&CarrierEvent::Text { text });
                    }
                    PageMessage::Failed | PageMessage::Invalid => {
                        return fail(CarrierFailure::Terminated, control_flow);
                    }
                }
            }
            _ => {}
        }
        if let Some((sequence, data)) = relay.next(now) {
            let _ = webview.evaluate_script(&format!(
                "window[{name}] && window[{name}].deliver({sequence}, 'b', '{data}');",
                name = js_string(&deliver_name),
            ));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "00112233445566778899aabbccddeeff";

    #[test]
    fn bridge_messages_need_the_token() {
        assert_eq!(
            page_message(&format!("{TOKEN}h"), TOKEN),
            Some(PageMessage::Hello)
        );
        assert_eq!(page_message("h", TOKEN), None);
        assert_eq!(
            page_message("ffffffffffffffffffffffffffffffffh", TOKEN),
            None
        );
        assert_eq!(page_message(TOKEN, TOKEN), None);
        assert_eq!(
            page_message(&format!("{TOKEN}a17"), TOKEN),
            Some(PageMessage::Ack(17))
        );
        assert_eq!(
            page_message(&format!("{TOKEN}ax"), TOKEN),
            Some(PageMessage::Invalid)
        );
        assert_eq!(
            page_message(&format!("{TOKEN}bAAEC"), TOKEN),
            Some(PageMessage::Binary("AAEC".into()))
        );
        assert_eq!(
            page_message(&format!("{TOKEN}b"), TOKEN),
            Some(PageMessage::Invalid)
        );
        assert_eq!(
            page_message(&format!("{TOKEN}b!!"), TOKEN),
            Some(PageMessage::Invalid)
        );
        assert_eq!(
            page_message(&format!(r#"{TOKEN}t{{"t":"x"}}"#), TOKEN),
            Some(PageMessage::Text(r#"{"t":"x"}"#.into()))
        );
        assert_eq!(
            page_message(&format!("{TOKEN}f"), TOKEN),
            Some(PageMessage::Failed)
        );
        assert_eq!(
            page_message(&format!("{TOKEN}z"), TOKEN),
            Some(PageMessage::Invalid)
        );
    }

    #[test]
    fn oversized_page_messages_are_refused() {
        let text = "x".repeat(MAX_PAGE_TEXT_BYTES + 1);
        assert_eq!(
            page_message(&format!("{TOKEN}t{text}"), TOKEN),
            Some(PageMessage::Invalid)
        );
        let data = "A".repeat(base64_len(MAX_PAGE_BINARY_BYTES) + 4);
        assert_eq!(
            page_message(&format!("{TOKEN}b{data}"), TOKEN),
            Some(PageMessage::Invalid)
        );
    }

    #[test]
    fn deliveries_go_one_at_a_time_and_need_their_ack() {
        let mut relay = Relay::new();
        let now = Instant::now();
        assert!(relay.queue("AAAA".into()));
        assert!(relay.queue("BBBB".into()));
        assert_eq!(relay.next(now), Some((1, "AAAA".into())));
        assert_eq!(relay.next(now), None);
        assert!(!relay.ack(2));
        assert!(relay.ack(1));
        assert_eq!(relay.next(now), Some((2, "BBBB".into())));
        assert_eq!(relay.pending_bytes, 0);
    }

    #[test]
    fn a_page_that_falls_behind_or_goes_quiet_is_hung() {
        let mut relay = Relay::new();
        let start = Instant::now();
        for _ in 0..MAX_PENDING_ITEMS {
            assert!(relay.queue("AAAA".into()));
        }
        assert!(!relay.queue("AAAA".into()));
        let mut big = Relay::new();
        assert!(!big.queue("A".repeat(base64_len(MAX_PENDING_BYTES) + 4)));

        let mut relay = Relay::new();
        assert!(relay.queue("AAAA".into()));
        let _ = relay.next(start);
        assert!(!relay.hung(start + Duration::from_secs(1)));
        assert!(relay.hung(start + WRITE_TIMEOUT + Duration::from_secs(1)));

        let mut quiet = Relay::new();
        quiet.last_heard = start;
        // Before the bridge said hello the page is only loading; TDLib's
        // handshake timeout covers that stretch.
        assert!(!quiet.hung(start + PAGE_SILENCE * 3));
        quiet.bridge_seen = true;
        assert!(quiet.hung(start + PAGE_SILENCE + Duration::from_secs(1)));
    }

    #[test]
    fn the_bridge_script_gets_every_value_as_a_literal() {
        let scope = CarrierScope::new(
            "https://proxy.example.com",
            "/relay/",
            "TelegramWebProxy",
            false,
        )
        .unwrap();
        let script = bridge_script(&scope, TOKEN, "__quillCarrier0011");
        for placeholder in [
            "__ORIGIN__",
            "__TOKEN__",
            "__POLICY__",
            "__NAME__",
            "__DELIVER__",
        ] {
            assert!(!script.contains(placeholder), "{placeholder}");
        }
        assert!(script.contains(r#"window.location.origin !== "https://proxy.example.com""#));
        assert!(script.contains(r#"defineProperty(window, "TelegramWebProxy""#));
        assert!(script.contains(&format!(r#"var token = "{TOKEN}";"#)));
    }

    #[test]
    fn the_policy_limits_everything_to_the_prefix() {
        let scope = CarrierScope::new(
            "https://proxy.example.com",
            "/relay/",
            "TelegramWebProxy",
            false,
        )
        .unwrap();
        let policy = content_policy(&scope);
        assert!(policy.starts_with("default-src https://proxy.example.com/relay/;"));
        assert!(policy.contains(
            "connect-src https://proxy.example.com/relay/ wss://proxy.example.com/relay/;"
        ));
        assert!(policy.contains("frame-src 'none'"));
        assert!(!policy.contains("'self'"));
        assert!(!policy.contains('*'));
    }
}
