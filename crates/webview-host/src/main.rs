//! `quill-webview`: the window a Telegram mini app runs in.
//!
//! Quill (GPUI) has no web view and, on Linux, no GTK main loop, so the
//! system web view lives here, in a helper process Quill starts per mini
//! app. The two talk over this process's stdin (commands) and stdout
//! (events), one JSON line each (`quill-webview-protocol`).
//!
//! The window holds one web view whose top document is a trusted shell
//! page served from the `quill` custom scheme ([`shell`] module): header,
//! bottom buttons, popups and menu. The bot page itself sits in a
//! sandboxed `<iframe>` inside the shell. The shell relays the Telegram
//! bridge's `postMessage` traffic to this process (`window.ipc`), and this
//! process forwards it to Quill unparsed, capped in size. Nothing here
//! decides anything about bot content; Quill does.
//!
//! What this process refuses on its own:
//! - navigations to anything but `http(s)`, `about:blank` and the shell
//!   (so no `file:`); refused URLs are reported as `open_external`;
//! - `window.open` (reported as `open_external`, never opened here);
//! - downloads (reported, never saved);
//! - every permission request (camera, microphone, location, ...).
//!
//! Exit: when Quill closes its end of the pipe, when Quill sends `close`,
//! or when the web view cannot be created (an `error` event explains).

use quill_webview_protocol::{
    HelperEvent, HostCommand, MAX_IPC_MESSAGE_BYTES, MAX_WEB_APP_EVENT_BYTES, ShellEvent, decode,
    encode,
};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::header::CONTENT_TYPE;
use wry::http::{Request, Response, StatusCode};
use wry::{NewWindowResponse, PermissionResponse, WebContext, WebViewBuilder};

mod capture;
mod shell;

/// Telegram Desktop's panel: `botWebViewPanelSize` 384x694 plus its 60 px
/// header (`payments.style`).
const WINDOW_WIDTH: f64 = 384.0;
const WINDOW_HEIGHT: f64 = 694.0 + 56.0;
const MIN_WIDTH: f64 = 320.0;
const MIN_HEIGHT: f64 = 420.0;

/// Command-line options. All optional; Quill passes them.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    /// Where the web view keeps cookies and storage for this bot.
    data_dir: Option<PathBuf>,
    /// Initial window title.
    title: String,
    /// Write a PNG of the window once the frame has loaded (macOS; demos).
    capture: Option<PathBuf>,
    /// 32 hex characters naming the WebKit data store (macOS, where a
    /// directory cannot be given); storage stays per bot and account.
    data_id: Option<[u8; 16]>,
}

/// 32 hex characters as 16 bytes.
fn parse_data_id(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut id = [0u8; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(id)
}

fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Options {
    let mut options = Options {
        title: "Mini App".to_string(),
        ..Options::default()
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => options.data_dir = args.next().map(PathBuf::from),
            "--capture" => options.capture = args.next().map(PathBuf::from),
            "--data-id" => options.data_id = args.next().as_deref().and_then(parse_data_id),
            "--title" => {
                if let Some(title) = args.next() {
                    options.title = title;
                }
            }
            _ => {}
        }
    }
    options
}

/// `QUILL_WEBVIEW_DEBUG=1` prints what the web view does to stderr.
fn debug(message: impl FnOnce() -> String) {
    if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
        eprintln!("quill-webview: {}", message());
    }
}

/// Stdout, shared by the event loop and the web view callbacks.
#[derive(Clone)]
struct Output(Arc<Mutex<std::io::Stdout>>);

impl Output {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(std::io::stdout())))
    }

    fn send(&self, event: HelperEvent) {
        if let Ok(mut out) = self.0.lock() {
            let _ = out.write_all(encode(&event).as_bytes());
            let _ = out.flush();
        }
    }
}

/// What the shell posts through `window.ipc.postMessage`.
#[derive(serde::Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum IpcMessage {
    /// A bridge event relayed from the frame; `data` is the raw JSON.
    #[serde(rename = "webapp")]
    WebApp {
        event: String,
        #[serde(default)]
        data: String,
    },
    Shell {
        event: ShellEvent,
    },
    /// The shell page's script is in place; queued commands may go.
    ShellReady,
}

/// What wakes the event loop.
#[derive(Debug)]
enum Message {
    Command(Box<HostCommand>),
    ShellReady,
    /// The shell reported its frame loaded (`--capture` waits for it).
    FrameLoaded,
}

/// Turn one IPC message into the event Quill gets, or nothing when the
/// message is malformed or oversized.
fn ipc_event(body: &str) -> Option<HelperEvent> {
    if body.len() > MAX_IPC_MESSAGE_BYTES {
        return None;
    }
    match serde_json::from_str::<IpcMessage>(body).ok()? {
        IpcMessage::ShellReady => None,
        IpcMessage::WebApp { event, data } => {
            if data.len() > MAX_WEB_APP_EVENT_BYTES || event.len() > 128 || event.is_empty() {
                return None;
            }
            Some(HelperEvent::WebApp { event, data })
        }
        IpcMessage::Shell { event } => Some(HelperEvent::Shell { event }),
    }
}

/// Whether a navigation (in any frame) may happen here. Only the web and
/// the shell: no `file:`, no other schemes. Refused URLs go to Quill as
/// `open_external`, which routes `tg:` and `mailto:` links itself.
fn navigation_allowed(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower == "about:blank"
        || lower == "about:srcdoc"
        || lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("blob:http")
        || lower.starts_with("data:")
        || lower.starts_with(shell::ORIGIN)
}

/// The shell command as a line of JavaScript for the shell page.
fn shell_command_script(command: &HostCommand) -> String {
    let json = serde_json::to_string(command).unwrap_or_else(|_| "{}".into());
    // JSON allows the two Unicode line separators raw; keep the script one
    // line whatever the JavaScript engine's vintage.
    let json = json
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    format!("window.__quillShell && window.__quillShell.command({json});")
}

fn serve_shell(request: Request<Vec<u8>>) -> Response<std::borrow::Cow<'static, [u8]>> {
    let path = request.uri().path().trim_start_matches('/');
    debug(|| format!("serve {}", request.uri()));
    match shell::asset(path) {
        Some((mime, body)) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, mime)
            .header("Cache-Control", "no-store")
            .body(std::borrow::Cow::Borrowed(body.as_bytes()))
            .unwrap_or_else(|_| Response::new(std::borrow::Cow::Borrowed(&[]))),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(std::borrow::Cow::Borrowed(&b""[..]))
            .unwrap_or_else(|_| Response::new(std::borrow::Cow::Borrowed(&[]))),
    }
}

/// Read commands from stdin until it closes, then ask the loop to exit.
fn read_commands(proxy: EventLoopProxy<Message>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match decode::<HostCommand>(&line) {
            Ok(command) => {
                if proxy
                    .send_event(Message::Command(Box::new(command)))
                    .is_err()
                {
                    return;
                }
            }
            Err(_) => continue,
        }
    }
    let _ = proxy.send_event(Message::Command(Box::new(HostCommand::Close)));
}

fn main() {
    let options = parse_args(std::env::args().skip(1));
    let out = Output::new();

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<Message>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        // No second Dock icon: the window belongs to Quill in spirit.
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_activate_ignoring_other_apps(true);
    }

    let window = match WindowBuilder::new()
        .with_title(&options.title)
        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
        .build(&event_loop)
    {
        Ok(window) => window,
        Err(error) => {
            out.send(HelperEvent::Error {
                message: format!("window: {error}"),
            });
            std::process::exit(1);
        }
    };

    let mut context = WebContext::new(options.data_dir.clone());
    let ready_proxy = event_loop.create_proxy();
    let builder = {
        let nav_out = out.clone();
        let window_out = out.clone();
        let download_out = out.clone();
        let ipc_out = out.clone();
        WebViewBuilder::new_with_web_context(&mut context)
            .with_custom_protocol(shell::SCHEME.to_string(), |_, request| serve_shell(request))
            .with_url(shell::INDEX_URL)
            .with_ipc_handler(move |request| {
                debug(|| format!("ipc {}", request.body()));
                if request.body().contains("shell_ready")
                    && matches!(
                        serde_json::from_str::<IpcMessage>(request.body()),
                        Ok(IpcMessage::ShellReady)
                    )
                {
                    let _ = ready_proxy.send_event(Message::ShellReady);
                } else if let Some(event) = ipc_event(request.body()) {
                    if matches!(
                        event,
                        HelperEvent::Shell {
                            event: ShellEvent::FrameLoaded
                        }
                    ) {
                        let _ = ready_proxy.send_event(Message::FrameLoaded);
                    }
                    ipc_out.send(event);
                }
            })
            .with_navigation_handler(move |url| {
                debug(|| format!("navigate {url}"));
                if navigation_allowed(&url) {
                    true
                } else {
                    nav_out.send(HelperEvent::OpenExternal { url });
                    false
                }
            })
            .with_new_window_req_handler(move |url, _| {
                window_out.send(HelperEvent::OpenExternal { url });
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |url, _| {
                download_out.send(HelperEvent::OpenExternal { url });
                false
            })
            .with_permission_handler(|_| PermissionResponse::Deny)
            .with_devtools(false)
            .with_hotkeys_zoom(false)
            .with_back_forward_navigation_gestures(false)
            .with_general_autofill_enabled(false)
    };
    #[cfg(target_os = "windows")]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        // `https://quill.localhost`: a secure context, so the frame is one too.
        builder.with_https_scheme(true)
    };
    #[cfg(target_os = "macos")]
    let builder = match options.data_id {
        // WKWebView keeps cookies and storage per data-store identifier
        // (macOS 14+); the directory alone would land in the default store.
        Some(id) => {
            use wry::WebViewBuilderExtDarwin;
            builder.with_data_store_identifier(id)
        }
        None => builder,
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
    let webview = match built {
        Ok(webview) => webview,
        Err(error) => {
            out.send(HelperEvent::Error {
                message: format!("web view: {error}"),
            });
            std::process::exit(1);
        }
    };

    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || read_commands(proxy));
    out.send(HelperEvent::Ready);

    // Commands that arrive before the shell script runs wait here.
    let mut shell_ready = false;
    let mut queued: Vec<HostCommand> = Vec::new();
    // `--capture`: the snapshot waits a moment after the frame loads so
    // the page has painted.
    let capture_path = options.capture.clone();
    let mut capture_at: Option<Instant> = None;
    event_loop.run(move |event, _, control_flow| {
        // Keep the context alive as long as the web view.
        let _context = &context;
        *control_flow = match capture_at {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        };
        match event {
            Event::UserEvent(Message::FrameLoaded) => {
                if capture_path.is_some() && capture_at.is_none() {
                    capture_at = Some(Instant::now() + Duration::from_millis(1500));
                    *control_flow = ControlFlow::WaitUntil(capture_at.unwrap_or_else(Instant::now));
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let (Some(path), Some(at)) = (capture_path.clone(), capture_at)
                    && Instant::now() >= at
                {
                    capture_at = None;
                    *control_flow = ControlFlow::Wait;
                    let capture_out = out.clone();
                    capture::capture(&webview, path.clone(), move |ok| {
                        debug(|| format!("capture {} -> {ok}", path.display()));
                        if !ok {
                            capture_out.send(HelperEvent::Error {
                                message: "capture failed".into(),
                            });
                        }
                    });
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => out.send(HelperEvent::Shell {
                event: ShellEvent::Close { force: false },
            }),
            Event::UserEvent(Message::Command(command))
                if matches!(*command, HostCommand::Close) =>
            {
                *control_flow = ControlFlow::Exit
            }
            Event::UserEvent(Message::ShellReady) => {
                shell_ready = true;
                for command in queued.drain(..) {
                    let _ = webview.evaluate_script(&shell_command_script(&command));
                }
            }
            Event::UserEvent(Message::Command(command)) => {
                let command = *command;
                if let HostCommand::SetTitle { title } | HostCommand::Load { title, .. } = &command
                {
                    window.set_title(title);
                }
                if shell_ready {
                    let _ = webview.evaluate_script(&shell_command_script(&command));
                } else if queued.len() < 256 {
                    queued.push(command);
                }
            }
            Event::LoopDestroyed => out.send(HelperEvent::Closed),
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_pick_data_dir_and_title() {
        let options = parse_args(
            ["--title", "Weather", "--data-dir", "/tmp/x", "--bogus"]
                .into_iter()
                .map(String::from),
        );
        assert_eq!(options.title, "Weather");
        assert_eq!(options.data_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(parse_args(Vec::<String>::new()).title, "Mini App");
    }

    #[test]
    fn data_ids_are_32_hex_characters() {
        assert_eq!(
            parse_data_id("00112233445566778899aabbccddeeff"),
            Some([
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ])
        );
        assert_eq!(parse_data_id("abc"), None);
        assert_eq!(parse_data_id("zz112233445566778899aabbccddeeff"), None);
    }

    #[test]
    fn navigation_policy_allows_the_web_and_the_shell_only() {
        assert!(navigation_allowed("https://example.com/app"));
        assert!(navigation_allowed("HTTP://example.com/"));
        assert!(navigation_allowed("about:blank"));
        assert!(navigation_allowed(&format!("{}index.html", shell::ORIGIN)));
        assert!(!navigation_allowed("file:///etc/passwd"));
        assert!(!navigation_allowed("tg://resolve?domain=x"));
        assert!(!navigation_allowed("javascript:alert(1)"));
        assert!(!navigation_allowed("mailto:a@b.c"));
        assert!(!navigation_allowed("ftp://host/"));
    }

    #[test]
    fn ipc_messages_become_events_within_limits() {
        let event = ipc_event(r#"{"t":"webapp","event":"web_app_close","data":"{}"}"#);
        assert_eq!(
            event,
            Some(HelperEvent::WebApp {
                event: "web_app_close".into(),
                data: "{}".into(),
            })
        );
        let event = ipc_event(r#"{"t":"shell","event":{"kind":"menu","id":"reload"}}"#);
        assert_eq!(
            event,
            Some(HelperEvent::Shell {
                event: ShellEvent::Menu {
                    id: "reload".into()
                },
            })
        );
        assert_eq!(ipc_event("garbage"), None);
        assert_eq!(ipc_event(r#"{"t":"webapp","event":"","data":""}"#), None);
        let huge = format!(
            r#"{{"t":"webapp","event":"web_app_data_send","data":"{}"}}"#,
            "x".repeat(MAX_WEB_APP_EVENT_BYTES + 1)
        );
        assert_eq!(ipc_event(&huge), None);
    }

    #[test]
    fn shell_commands_are_passed_as_json_literals() {
        let script = shell_command_script(&HostCommand::SetTitle {
            title: "a\u{2028}b</script>".into(),
        });
        assert!(script.starts_with("window.__quillShell && window.__quillShell.command({"));
        assert!(script.contains(r#""kind":"set_title""#));
        // serde escapes the line separator, so the literal stays one line.
        assert!(!script.contains('\u{2028}'));
    }

    #[test]
    fn shell_assets_are_served_by_path() {
        let request = Request::builder()
            .uri(format!("{}index.html", shell::ORIGIN))
            .body(Vec::new())
            .unwrap();
        let response = serve_shell(request);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        let request = Request::builder()
            .uri(format!("{}nope.txt", shell::ORIGIN))
            .body(Vec::new())
            .unwrap();
        assert_eq!(serve_shell(request).status(), StatusCode::NOT_FOUND);
    }
}
