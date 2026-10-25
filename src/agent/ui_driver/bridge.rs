//! The queue between the tool threads and the UI thread. A tool files a
//! [`UiCommand`] with [`send`] and waits; the UI thread drains [`take`] on
//! its agent wake-up (`src/ui/agent_driver.rs`) and answers each one.
//! Nothing here blocks the UI thread or the TDLib receive thread.
//!
//! Also the driving clock: the tree is forced on only while an agent drove
//! within [`DRIVE_WINDOW`] ([`wants_forced`]), and the real-input clock
//! ([`note_real_input`]); the pause itself is in `super::takeover`.

use super::keys::KeyChord;
use super::shot::{Shot, ShotOptions};
use super::tree::{UiNode, UiTree};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where a click, hover or scroll lands.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A node id from `read_ui_tree`.
    Node(String),
    /// A window point in logical pixels.
    Point { x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Button {
    #[default]
    Left,
    Right,
    Middle,
}

/// What the UI thread does for an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    /// Which chat is open and whether the window is active. Answered at
    /// once, without a frame.
    State,
    ReadTree,
    Click {
        target: Target,
        button: Button,
        count: u32,
    },
    Hover {
        target: Target,
    },
    Scroll {
        target: Target,
        dx: f32,
        dy: f32,
    },
    TypeText {
        text: String,
    },
    PressKeys {
        chords: Vec<KeyChord>,
    },
    /// Right-click `target` and report the nodes that appeared.
    OpenMenu {
        target: Target,
    },
    /// A send the composer handed to the agent path ([`UiSend`]) went out:
    /// clear the composer if it still holds `text` for `chat_id`. Answered
    /// at once.
    ComposerSent {
        chat_id: i64,
        text: String,
    },
}

impl UiCommand {
    /// Whether the command sends input to the window.
    pub fn is_input(&self) -> bool {
        !matches!(
            self,
            UiCommand::State | UiCommand::ReadTree | UiCommand::ComposerSent { .. }
        )
    }

    /// The tool's name in the activity log (`ui_click`), `None` for the
    /// commands that are not logged.
    pub fn log_name(&self) -> Option<&'static str> {
        Some(match self {
            UiCommand::Click { .. } => "ui_click",
            UiCommand::Hover { .. } => "ui_hover",
            UiCommand::Scroll { .. } => "ui_scroll",
            UiCommand::TypeText { .. } => "ui_type",
            UiCommand::PressKeys { .. } => "ui_keys",
            UiCommand::OpenMenu { .. } => "ui_menu",
            UiCommand::State | UiCommand::ReadTree | UiCommand::ComposerSent { .. } => {
                return None;
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiRequest {
    pub id: u64,
    /// The MCP client's name, for the log.
    pub agent: String,
    pub command: UiCommand,
    /// A screenshot after the command (`super::shot`): of the window for
    /// `ReadTree`, of the window the input left for an input command.
    pub shot: Option<ShotOptions>,
}

/// What `UiCommand::State` answers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiState {
    pub open_chat: Option<i64>,
    pub window_active: bool,
    /// The tree is being built every frame right now.
    pub forced: bool,
}

/// A message the composer was told to send while an agent drove: the UI
/// built the request as it would have sent it and kept the draft; the tool
/// thread sends it through the agent path (grants, pacing, confirmation,
/// activity log) and tells the UI with [`UiCommand::ComposerSent`].
#[derive(Debug, Clone, PartialEq)]
pub struct UiSend {
    pub chat_id: i64,
    pub chat_title: String,
    /// The composer's text, as the draft holds it.
    pub text: String,
    /// The `sendMessage` request, without `@extra`.
    pub request: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiReply {
    State(UiState),
    Tree(UiTree),
    /// `ReadTree` with a screenshot: the tree of the frame the screenshot
    /// shows.
    Shot {
        tree: UiTree,
        shot: Shot,
    },
    /// Input ran; `added` lists the nodes that appeared since, `refusals`
    /// what a handler refused to do for the agent, `ui_send` a composer
    /// send handed to the agent path.
    Done {
        tree: UiTree,
        added: Vec<UiNode>,
        refusals: Vec<String>,
        ui_send: Option<UiSend>,
        /// The screenshot asked for, or why there is none; `tree` is then
        /// the tree of the frame it shows.
        shot: Option<Result<Shot, String>>,
    },
    /// A command without a tree ran (`ComposerSent`).
    Ok,
    /// The UI did not run the command; the text says why and what to do.
    Refused(String),
}

/// A request the UI thread took and must answer.
pub struct Pending {
    pub request: UiRequest,
    reply: SyncSender<UiReply>,
    /// Set when the tool stopped waiting: the UI must not inject any more.
    cancelled: Arc<AtomicBool>,
}

impl Pending {
    pub fn answer(self, reply: UiReply) {
        let _ = self.reply.try_send(reply);
    }

    /// Whether the tool gave up on this command (a retry may be on its way,
    /// so injecting now would act twice).
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    /// Too many commands wait already.
    Busy,
    /// The UI thread did not answer in time.
    Timeout,
}

static QUEUE: Mutex<VecDeque<Pending>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
/// Commands waiting at once, across agents.
const MAX_WAITING: usize = 8;

/// File `command` and wait up to `timeout` for the UI's answer. On timeout
/// the command is withdrawn or, if the UI took it already, cancelled (an
/// answer that raced the withdrawal still counts).
pub fn send(agent: &str, command: UiCommand, timeout: Duration) -> Result<UiReply, BridgeError> {
    post(agent, command)?.wait(timeout)
}

/// [`send`] with a screenshot after the command.
pub fn send_with_shot(
    agent: &str,
    command: UiCommand,
    shot: Option<ShotOptions>,
    timeout: Duration,
) -> Result<UiReply, BridgeError> {
    post_with_shot(agent, command, shot)?.wait(timeout)
}

/// A filed command, waiting for the UI's answer.
pub struct Ticket {
    id: u64,
    rx: Receiver<UiReply>,
    cancelled: Arc<AtomicBool>,
}

impl Ticket {
    /// The answer, if it is in already.
    pub fn poll(&self) -> Option<UiReply> {
        self.rx.try_recv().ok()
    }

    /// Wait up to `timeout`; on timeout the command is withdrawn and
    /// cancelled.
    pub fn wait(self, timeout: Duration) -> Result<UiReply, BridgeError> {
        if let Ok(reply) = self.rx.recv_timeout(timeout) {
            return Ok(reply);
        }
        self.cancelled.store(true, Ordering::SeqCst);
        withdraw(self.id);
        self.rx.try_recv().map_err(|_| BridgeError::Timeout)
    }
}

/// File `command` without waiting (the UI thread is woken).
pub fn post(agent: &str, command: UiCommand) -> Result<Ticket, BridgeError> {
    post_with_shot(agent, command, None)
}

/// [`post`] with a screenshot after the command.
pub fn post_with_shot(
    agent: &str,
    command: UiCommand,
    shot: Option<ShotOptions>,
) -> Result<Ticket, BridgeError> {
    let mut queue = crate::locks::lock_or_recover(&QUEUE);
    if queue.len() >= MAX_WAITING {
        return Err(BridgeError::Busy);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let (reply, rx) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    queue.push_back(Pending {
        request: UiRequest {
            id,
            agent: agent.to_string(),
            command,
            shot,
        },
        reply,
        cancelled: cancelled.clone(),
    });
    drop(queue);
    crate::wake::AGENT.poke();
    Ok(Ticket { id, rx, cancelled })
}

fn withdraw(id: u64) {
    crate::locks::lock_or_recover(&QUEUE).retain(|p| p.request.id != id);
}

/// Every command waiting, oldest first. The UI thread answers each.
pub fn take() -> Vec<Pending> {
    crate::locks::lock_or_recover(&QUEUE).drain(..).collect()
}

/// How many commands wait for the UI.
pub fn waiting() -> usize {
    crate::locks::lock_or_recover(&QUEUE).len()
}

/// Answer every waiting command with `reason` (Agent access went off).
pub fn refuse_all(reason: &str) {
    for pending in take() {
        pending.answer(UiReply::Refused(reason.to_string()));
    }
}

fn epoch() -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    // Starts at 1 so "never" (0) stays distinct.
    epoch().elapsed().as_millis() as u64 + 1
}

/// When an agent last read or drove the window (ms on the bridge clock).
static LAST_DRIVE_MS: AtomicU64 = AtomicU64::new(0);
/// When the user last gave the window real input.
static LAST_REAL_INPUT_MS: AtomicU64 = AtomicU64::new(0);

/// How long after the last command the tree keeps being built.
pub const DRIVE_WINDOW: Duration = Duration::from_secs(30);

/// Record that an agent used the driver now.
pub fn note_drive() {
    LAST_DRIVE_MS.store(now_ms(), Ordering::Relaxed);
}

/// Whether an agent used the driver within [`DRIVE_WINDOW`].
pub fn drove_recently() -> bool {
    let last = LAST_DRIVE_MS.load(Ordering::Relaxed);
    last != 0 && now_ms().saturating_sub(last) < DRIVE_WINDOW.as_millis() as u64
}

/// Whether the window should build its element tree every frame: the
/// switches are on, an agent is connected and one drove recently.
pub fn wants_forced() -> bool {
    crate::agent::access::enabled()
        && crate::agent::access::ui_driver()
        && !crate::agent::activity::connected().is_empty()
        && drove_recently()
}

/// Record input that came from the user, not from the driver.
pub fn note_real_input() {
    LAST_REAL_INPUT_MS.store(now_ms(), Ordering::Relaxed);
}

/// Milliseconds since the user's last real input, `None` if never seen.
pub fn real_input_age_ms() -> Option<u64> {
    let last = LAST_REAL_INPUT_MS.load(Ordering::Relaxed);
    (last != 0).then(|| now_ms().saturating_sub(last))
}

/// Pause or resume the driver. Paused, every driver tool refuses. The
/// pause lives in the takeover record (`super::takeover`).
pub fn set_paused(on: bool) {
    if on {
        super::takeover::pause(super::takeover::PauseReason::Input);
    } else {
        super::takeover::let_continue();
    }
}

pub fn paused() -> bool {
    super::takeover::paused()
}

#[cfg(test)]
mod tests {
    use super::{BridgeError, UiCommand, UiReply, UiState, post, send, take, waiting};
    use std::time::Duration;

    /// The queue is process-wide and turning the switch off drains it, so
    /// these tests take turns with the switch-turning tests.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK)
    }

    #[test]
    fn a_command_goes_in_and_its_reply_comes_out() {
        let _guard = lock();
        let agent = std::thread::spawn(|| send("Test", UiCommand::State, Duration::from_secs(5)));
        let pending = loop {
            let mut taken = take();
            if let Some(pending) = taken.pop() {
                break pending;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(pending.request.command, UiCommand::State);
        assert_eq!(pending.request.agent, "Test");
        assert!(!pending.cancelled());
        pending.answer(UiReply::State(UiState {
            open_chat: Some(7),
            window_active: true,
            forced: false,
        }));
        let reply = agent.join().unwrap_or_else(|_| panic!("agent thread"));
        assert_eq!(
            reply,
            Ok(UiReply::State(UiState {
                open_chat: Some(7),
                window_active: true,
                forced: false,
            }))
        );
        assert_eq!(waiting(), 0);
    }

    #[test]
    fn an_unanswered_command_times_out_and_is_withdrawn() {
        let _guard = lock();
        let result = send("Test", UiCommand::ReadTree, Duration::from_millis(20));
        assert_eq!(result, Err(BridgeError::Timeout));
        assert_eq!(waiting(), 0, "withdrawn");
    }

    #[test]
    fn a_taken_command_is_cancelled_when_its_tool_gives_up() {
        let _guard = lock();
        let ticket = post("Test", UiCommand::ReadTree).unwrap();
        let pending = take().pop().unwrap();
        assert!(!pending.cancelled());
        assert_eq!(
            ticket.wait(Duration::from_millis(5)),
            Err(BridgeError::Timeout)
        );
        assert!(pending.cancelled(), "the UI sees the tool gave up");
    }

    #[test]
    fn the_drive_clock_and_pause_flag() {
        let _guard = lock();
        assert!(
            !super::drove_recently()
                || super::LAST_DRIVE_MS.load(std::sync::atomic::Ordering::Relaxed) != 0
        );
        super::note_drive();
        assert!(super::drove_recently());
        assert!(super::real_input_age_ms().is_none_or(|age| age < u64::MAX));
        super::note_real_input();
        assert!(super::real_input_age_ms().is_some_and(|age| age < 1000));
        super::set_paused(true);
        assert!(super::paused());
        super::set_paused(false);
        assert!(!super::paused());
    }

    #[test]
    fn commands_know_their_log_names() {
        assert_eq!(UiCommand::ReadTree.log_name(), None);
        assert_eq!(
            UiCommand::TypeText { text: "a".into() }.log_name(),
            Some("ui_type")
        );
        assert!(
            !UiCommand::ComposerSent {
                chat_id: 1,
                text: String::new()
            }
            .is_input()
        );
    }
}
