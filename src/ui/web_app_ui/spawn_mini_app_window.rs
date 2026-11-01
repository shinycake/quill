//! Methods moved out of `web_app_ui.rs` to keep files under 1000 lines.

use super::helper_pipe::{HelperWriter, Line, read_bounded_line};
use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    pub(super) fn spawn_mini_app_window(&mut self, launch: WebAppLaunch, cx: &mut Context<Self>) {
        let Some(path) = helper_path() else {
            self.set_status_note(
                StatusNote::silent(tr(keys::Q_MEDIA_THE_MINI_APP_WINDOW_QUILL_2)),
                cx,
            );
            return;
        };
        let account = self
            .live
            .as_ref()
            .map(|live| live.driver.session.account.to_string())
            .unwrap_or_else(|| "default".into());
        let mut command = Command::new(path);
        command
            .arg("--title")
            .arg(&launch.bot_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The helper is quiet unless `QUILL_WEBVIEW_DEBUG` is set, in
            // which case its trace goes to Quill's stderr.
            .stderr(if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        if let Some(dir) = data_dir(&account, launch.bot_user_id) {
            let _ = std::fs::create_dir_all(&dir);
            command.arg("--data-dir").arg(dir);
        }
        command
            .arg("--data-id")
            .arg(data_store_id(&account, launch.bot_user_id));
        // No console window flashes up next to the mini app on Windows.
        quill::media_tools::NoConsole::no_console(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                self.set_status_note(
                    StatusNote::error(tr(keys::Q_MEDIA_THE_MINI_APP_WINDOW_COULDNT)),
                    cx,
                );
                return;
            }
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return;
        };
        let (sender, events) = mpsc::channel();
        quill::thread_guard::spawn_guarded("quill-webview-reader", move || {
            let mut reader = std::io::BufReader::new(stdout);
            // A message at the IPC limit can grow when JSON-escaped.
            let cap = quill_webview_protocol::MAX_IPC_MESSAGE_BYTES * 4;
            let mut logged_too_long = false;
            loop {
                let line = match read_bounded_line(&mut reader, cap) {
                    Ok(Line::Text(line)) => line,
                    Ok(Line::TooLong) => {
                        if !std::mem::replace(&mut logged_too_long, true) {
                            eprintln!(
                                "mini app helper sent a line over {cap} bytes; dropping such lines"
                            );
                        }
                        continue;
                    }
                    Ok(Line::End) | Err(_) => break,
                };
                let Ok(line) = String::from_utf8(line) else {
                    continue;
                };
                if let Ok(event) = decode::<HelperEvent>(&line)
                    && sender.send(event).is_err()
                {
                    break;
                }
            }
            let _ = sender.send(HelperEvent::Closed);
        })
        .ok();
        let in_attachment_menu = self.session().is_some_and(|session| {
            session
                .bots
                .web_apps
                .attachment_menu_bots
                .iter()
                .any(|bot| bot.bot_user_id == launch.bot_user_id && bot.is_added)
        });
        let privacy_policy_url = self
            .session()
            .and_then(|session| session.bots.bot_info.get(&launch.bot_user_id))
            .and_then(|info| info.as_ref())
            .map(|info| info.privacy_policy_url.clone())
            .filter(|url| quill::text::openable_http_url(url));
        self.mini_apps.window = Some(MiniAppWindow {
            child: Some(child),
            writer: HelperWriter::spawn(stdin),
            wedged: false,
            events,
            launch,
            theme: theme_params(cx),
            ready: false,
            closing_confirmation: false,
            settings_button: false,
            popups: HashMap::new(),
            popup_seq: 0,
            waiting_write_access: false,
            data_sent: false,
            privacy_policy_url,
            in_attachment_menu,
            emoji: Default::default(),
            phone: Default::default(),
            invoice: None,
        });
        self.ensure_mini_app_poll(cx);
    }

    /// Close the window (and tell TDLib) without waiting for the helper.
    pub(in crate::ui) fn close_mini_app_window(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.mini_apps.window.take() else {
            return;
        };
        let launch_id = window.launch.launch_id;
        drop(window);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.close_web_app(launch_id);
        }
        cx.notify();
    }

    /// Poll the helper and the session's mini-app answers while something
    /// is going on; stops by itself when nothing is left.
    pub(super) fn ensure_mini_app_poll(&mut self, cx: &mut Context<Self>) {
        if self.mini_apps.poll.is_some() {
            return;
        }
        self.mini_apps.generation += 1;
        let generation = self.mini_apps.generation;
        let task = Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        this.mini_apps.generation == generation && this.poll_mini_apps(cx)
                    })
                    .unwrap_or(false);
                if !keep {
                    let _ = this.update(cx, |this, _| {
                        if this.mini_apps.generation == generation {
                            this.mini_apps.poll = None;
                        }
                    });
                    break;
                }
            }
        });
        self.mini_apps.poll = Some(task);
    }

    /// One poll step; `true` while there is still something to wait for.
    pub(super) fn poll_mini_apps(&mut self, cx: &mut Context<Self>) -> bool {
        self.poll_mini_app_session(cx);
        let events: Vec<HelperEvent> = match self.mini_apps.window.as_ref() {
            Some(window) => window.events.try_iter().collect(),
            None => Vec::new(),
        };
        for event in events {
            self.handle_helper_event(event, cx);
        }
        self.mini_apps.window.is_some()
            || self.mini_apps.awaiting_attach_bot.is_some()
            || self.mini_apps.awaiting_search.is_some()
            || self.mini_apps.awaiting_toggle.is_some()
            || self
                .session()
                .is_some_and(|session| session.bots.web_apps.pending.is_some())
    }

    /// The session's one-shot answers.
    pub(super) fn poll_mini_app_session(&mut self, cx: &mut Context<Self>) {
        self.poll_emoji_status(cx);
        self.poll_phone(cx);
        self.poll_invoice(cx);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let apps = &mut live.driver.session.bots.web_apps;
        let open_result = apps.open_result.take();
        let write_access = apps.write_access.take();
        let custom_replies = std::mem::take(&mut apps.custom_replies);
        let attach_bot = apps.attachment_menu_bot.take();
        let toggled = apps.attachment_menu_toggled.take();
        let found = apps.found.take();
        let data_sent = apps.data_sent.take();

        if let Some(result) = open_result {
            match result {
                WebAppOpenResult::Opened {
                    bot_user_id,
                    chat_id,
                    source,
                    launch_id,
                    url,
                } => {
                    let bot_name = self
                        .session()
                        .and_then(|session| session.users.get(&bot_user_id))
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| tr(keys::Q_MEDIA_MINI_APP).into());
                    self.connection.clear_note();
                    self.spawn_mini_app_window(
                        WebAppLaunch {
                            bot_user_id,
                            bot_name,
                            chat_id,
                            source,
                            launch_id,
                            url,
                        },
                        cx,
                    );
                }
                WebAppOpenResult::Failed { message, .. } => {
                    self.set_status_note(StatusNote::error(&message), cx)
                }
            }
        }
        if let Some((bot_id, result)) = write_access {
            self.finish_write_access(bot_id, result, cx);
        }
        for reply in custom_replies {
            if let Some(window) = self.mini_apps.window.as_mut() {
                let reply = match reply.result {
                    Ok(json) => BridgeReply::custom_method_result(
                        &reply.req_id,
                        serde_json::from_str(&json).unwrap_or(serde_json::Value::Null),
                    ),
                    Err(error) => BridgeReply::custom_method_error(&reply.req_id, &error),
                };
                window.emit(reply);
            }
        }
        if let Some(result) = attach_bot
            && let Some(then) = self.mini_apps.awaiting_attach_bot.take()
        {
            match result {
                Ok(bot) if bot.is_added => self.start_web_app(then, cx),
                Ok(bot) => {
                    self.mini_apps.confirm = Some(WebAppConfirm::AddToMenu {
                        bot,
                        allow_write: true,
                        then,
                    });
                    cx.notify();
                }
                Err(message) => self.set_status_note(StatusNote::error(&message), cx),
            }
        }
        if let Some(result) = toggled {
            match result {
                Ok((_, added)) => {
                    self.set_status_note(
                        if added {
                            StatusNote::silent(tr(keys::BOT_ADD_TO_MENU_DONE))
                        } else {
                            StatusNote::silent(tr(keys::BOT_REMOVE_FROM_MENU_DONE))
                        },
                        cx,
                    );
                    if let Some(then) = self.mini_apps.awaiting_toggle.take() {
                        self.request_web_app(then, false, cx);
                    }
                }
                Err(message) => {
                    self.mini_apps.awaiting_toggle = None;
                    self.set_status_note(StatusNote::error(&message), cx);
                }
            }
        }
        if let Some(result) = found
            && let Some(then) = self.mini_apps.awaiting_search.take()
        {
            match result {
                Ok((_, app)) => {
                    let bot_id = then.bot_id();
                    let verified = self
                        .session()
                        .and_then(|session| session.users.get(&bot_id))
                        .is_some_and(|user| user.verification.is_verified);
                    if app.skip_confirmation
                        || (!app.request_write_access
                            && !trust::needs_confirmation(bot_id, verified, false))
                    {
                        self.request_web_app(then, false, cx);
                    } else {
                        self.mini_apps.confirm = Some(WebAppConfirm::OpenTerms {
                            then,
                            request_write: app.request_write_access,
                            allow_write: true,
                        });
                        cx.notify();
                    }
                }
                Err(message) => self.set_status_note(StatusNote::error(&message), cx),
            }
        }
        if let Some(result) = data_sent {
            match result {
                Ok(()) => {
                    self.set_status_note(StatusNote::silent(tr(keys::Q_MEDIA_SENT_TO_THE_BOT)), cx)
                }
                Err(message) => self.set_status_note(StatusNote::error(&message), cx),
            }
        }
    }

    pub(super) fn finish_write_access(
        &mut self,
        bot_id: i64,
        result: WriteAccessResult,
        cx: &mut Context<Self>,
    ) {
        let Some(window) = self.mini_apps.window.as_mut() else {
            return;
        };
        if window.launch.bot_user_id != bot_id || !window.waiting_write_access {
            return;
        }
        match result {
            WriteAccessResult::Allowed | WriteAccessResult::Granted => {
                window.waiting_write_access = false;
                window.emit(BridgeReply::write_access_requested(true));
            }
            WriteAccessResult::Denied => {
                window.waiting_write_access = false;
                window.emit(BridgeReply::write_access_requested(false));
            }
            WriteAccessResult::NeedsConsent => {
                let name = window.launch.bot_name.clone();
                window.show_popup(
                    PopupPurpose::WriteAccess,
                    popup(
                        tr(keys::Q_MEDIA_ALLOW_MESSAGING),
                        tr_with(
                            keys::Q_MEDIA_ALLOW_TO_SEND_YOU_MESSAGES,
                            &[("name", &name.to_string())],
                        ),
                        vec![
                            ("cancel", tr(keys::CANCEL), "cancel"),
                            ("allow", tr(keys::Q_MEDIA_ALLOW), "default"),
                        ],
                    ),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn handle_helper_event(&mut self, event: HelperEvent, cx: &mut Context<Self>) {
        match event {
            HelperEvent::Ready => {
                let Some(window) = self.mini_apps.window.as_mut() else {
                    return;
                };
                window.ready = true;
                let menu = window.menu();
                let command = HostCommand::Load {
                    url: window.launch.url.clone(),
                    title: window.launch.bot_name.clone(),
                    theme: window.theme.bridge_colors(),
                    menu,
                };
                window.send(&command);
            }
            HelperEvent::WebApp { event, data } => {
                // Unknown or malformed events are dropped, as tdesktop does.
                if let Ok(parsed) = parse_event(&event, &data) {
                    self.handle_web_app_event(parsed, cx);
                }
            }
            HelperEvent::Shell { event } => self.handle_shell_event(event, cx),
            HelperEvent::OpenExternal { url } => self.open_from_mini_app(&url, cx),
            HelperEvent::Closed => self.close_mini_app_window(cx),
            HelperEvent::Error { message } => {
                let note = if message.contains("web view") {
                    if cfg!(target_os = "windows") {
                        StatusNote::info(tr(keys::Q_MEDIA_MINI_APPS_NEED_THE_MICROSOFT))
                    } else if cfg!(target_os = "linux") {
                        StatusNote::info(tr(keys::Q_MEDIA_MINI_APPS_NEED_WEBKITGTK_LIBWEBKIT2GTK))
                    } else {
                        StatusNote::error(tr(keys::Q_MEDIA_THE_MINI_APP_WINDOW_COULDNT_2))
                    }
                } else {
                    StatusNote::error(tr_with(
                        keys::Q_MEDIA_THE_MINI_APP_WINDOW_FAILED,
                        &[("message", &message.to_string())],
                    ))
                };
                self.close_mini_app_window(cx);
                self.set_status_note(note, cx);
            }
        }
    }

    /// A URL the page tried to leave the frame for.
    pub(super) fn open_from_mini_app(&mut self, url: &str, cx: &mut Context<Self>) {
        if is_telegram_link(url) {
            self.queue_deep_link(url.to_string());
            cx.notify();
            return;
        }
        if !quill::text::openable_http_url(url) {
            return;
        }
        let link = LinkTarget::Url {
            url: url.to_string(),
            label: None,
        };
        match open_decision(&link) {
            OpenDecision::Open => self.open_link(url, cx),
            OpenDecision::Confirm {
                url,
                shown,
                suspicious,
            } => {
                self.message_ui.open_link_confirm =
                    Some(super::super::entity_links::OpenLinkConfirm {
                        url,
                        shown,
                        suspicious,
                        from_poll_option: false,
                    });
                cx.activate(true);
                cx.notify();
            }
        }
    }

    pub(super) fn with_window(&mut self, f: impl FnOnce(&mut MiniAppWindow)) {
        if let Some(window) = self.mini_apps.window.as_mut() {
            f(window);
        }
    }

    pub(super) fn handle_web_app_event(&mut self, event: WebAppEvent, cx: &mut Context<Self>) {
        let Some((bot_id, bot_name, source, data_sent, waiting_write_access)) =
            self.mini_apps.window.as_ref().map(|window| {
                (
                    window.launch.bot_user_id,
                    window.launch.bot_name.clone(),
                    window.launch.source.clone(),
                    window.data_sent,
                    window.waiting_write_access,
                )
            })
        else {
            return;
        };
        match event {
            WebAppEvent::Ready | WebAppEvent::RequestTheme | WebAppEvent::RequestViewport => {}
            WebAppEvent::Haptic => {}
            WebAppEvent::Close => self.close_mini_app_window(cx),
            WebAppEvent::DataSend { data } => {
                if !source.allows_data_send() || data_sent {
                    return;
                }
                self.with_window(|window| window.data_sent = true);
                let button_text = match &source {
                    LaunchSource::KeyboardButton { button_text } => button_text.clone(),
                    _ => String::new(),
                };
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.send_web_app_data(bot_id, &button_text, &data));
                if !matches!(sent, Some(Ok(_))) {
                    self.set_status_note(
                        StatusNote::error(tr(keys::Q_MEDIA_COULDNT_SEND_THE_APPS_DATA)),
                        cx,
                    );
                }
                // tdesktop closes the panel once the data is on its way.
                self.close_mini_app_window(cx);
            }
            WebAppEvent::SwitchInlineQuery { query, chat_types } => {
                if chat_types.is_empty() {
                    let username = self
                        .session()
                        .and_then(|session| session.users.get(&bot_id))
                        .map(|user| user.username.clone())
                        .unwrap_or_default();
                    if !username.is_empty() {
                        self.mini_apps.render_action = Some(RenderAction::SwitchInline {
                            query: format!("@{username} {query}"),
                        });
                        self.close_mini_app_window(cx);
                        cx.activate(true);
                    }
                }
            }
            WebAppEvent::SetupMainButton(setup) => self.with_window(|window| {
                window.send(&HostCommand::MainButton {
                    button: bottom_button(setup),
                });
            }),
            WebAppEvent::SetupSecondaryButton(setup) => self.with_window(|window| {
                window.send(&HostCommand::SecondaryButton {
                    button: bottom_button(setup),
                });
            }),
            WebAppEvent::SetupBackButton { visible } => self.with_window(|window| {
                window.send(&HostCommand::BackButton { visible });
            }),
            WebAppEvent::SetupSettingsButton { visible } => self.with_window(|window| {
                window.settings_button = visible;
                let menu = window.menu();
                window.send(&HostCommand::SetMenu { menu });
            }),
            WebAppEvent::OpenLink { url, .. } => self.open_from_mini_app(&url, cx),
            WebAppEvent::OpenTgLink { url } => {
                self.queue_deep_link(url);
                cx.activate(true);
                cx.notify();
            }
            WebAppEvent::OpenInvoice { slug } => self.request_mini_app_invoice(slug, cx),
            WebAppEvent::OpenPopup(spec) => self.with_window(|window| {
                window.show_popup(
                    PopupPurpose::App,
                    Popup {
                        id: String::new(),
                        title: spec.title,
                        message: spec.message,
                        buttons: spec
                            .buttons
                            .into_iter()
                            .map(|button| PopupButton {
                                id: button.id,
                                text: button.text,
                                kind: button.kind,
                            })
                            .collect(),
                    },
                );
            }),
            WebAppEvent::RequestWriteAccess => {
                if waiting_write_access {
                    self.with_window(|window| {
                        window.emit(BridgeReply::write_access_requested(false));
                    });
                    return;
                }
                self.with_window(|window| window.waiting_write_access = true);
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.can_bot_send_messages(bot_id));
                if !matches!(sent, Some(Ok(_))) {
                    self.with_window(|window| {
                        window.waiting_write_access = false;
                        window.emit(BridgeReply::write_access_requested(false));
                    });
                }
            }
            WebAppEvent::RequestPhone => self.request_mini_app_phone(),
            WebAppEvent::RequestEmojiStatusAccess => self.request_emoji_status_access(cx),
            WebAppEvent::SetEmojiStatus {
                custom_emoji_id,
                duration_secs,
            } => self.request_set_emoji_status(custom_emoji_id, duration_secs, cx),
            WebAppEvent::ReadTextFromClipboard { req_id } => self.with_window(|window| {
                window.show_popup(
                    PopupPurpose::Clipboard { req_id },
                    popup(
                        tr(keys::Q_MEDIA_PASTE_FROM_THE_CLIPBOARD),
                        tr_with(
                            keys::Q_MEDIA_ASKS_TO_READ_WHAT_YOU,
                            &[("bot_name", &bot_name.to_string())],
                        ),
                        vec![
                            ("cancel", tr(keys::CANCEL), "cancel"),
                            ("allow", tr(keys::Q_MEDIA_PASTE), "default"),
                        ],
                    ),
                );
            }),
            WebAppEvent::SetupClosingBehavior { need_confirmation } => {
                self.with_window(|window| window.closing_confirmation = need_confirmation);
            }
            WebAppEvent::SetHeaderColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::HeaderColor { color });
            }),
            WebAppEvent::SetBackgroundColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::BackgroundColor { color });
            }),
            WebAppEvent::SetBottomBarColor(spec) => self.with_window(|window| {
                let color = window.resolve_color(&spec);
                window.send(&HostCommand::BottomBarColor { color });
            }),
            WebAppEvent::InvokeCustomMethod {
                req_id,
                method,
                params,
            } => {
                let sent = self.live.as_mut().map(|live| {
                    live.driver.send_web_app_custom_request(
                        bot_id,
                        &req_id,
                        &method,
                        &params.to_string(),
                    )
                });
                if !matches!(sent, Some(Ok(_))) {
                    self.with_window(|window| {
                        window.emit(BridgeReply::custom_method_error(&req_id, "UNKNOWN_ERROR"));
                    });
                }
            }
            WebAppEvent::Unsupported { reply, .. } => {
                if let Some(reply) = reply {
                    self.with_window(|window| window.emit(reply));
                }
            }
        }
    }

    pub(super) fn handle_shell_event(&mut self, event: ShellEvent, cx: &mut Context<Self>) {
        let Some((bot_id, closing_confirmation, privacy_policy_url)) =
            self.mini_apps.window.as_ref().map(|window| {
                (
                    window.launch.bot_user_id,
                    window.closing_confirmation,
                    window.privacy_policy_url.clone(),
                )
            })
        else {
            return;
        };
        match event {
            ShellEvent::Close { force } => {
                if closing_confirmation && !force {
                    self.with_window(|window| {
                        window.show_popup(
                            PopupPurpose::CloseConfirm,
                            popup(
                                tr(keys::Q_MEDIA_CLOSE_THE_MINI_APP),
                                tr(keys::Q_MEDIA_CHANGES_YOU_MADE_MAY_NOT).into(),
                                vec![
                                    ("cancel", tr(keys::CANCEL), "cancel"),
                                    ("close", tr(keys::Q_MEDIA_CLOSE_ANYWAY), "destructive"),
                                ],
                            ),
                        );
                    });
                } else {
                    self.close_mini_app_window(cx);
                }
            }
            ShellEvent::Back | ShellEvent::Reload | ShellEvent::FrameLoaded => {}
            ShellEvent::Menu { id } => match id.as_str() {
                "open_bot" => {
                    self.mini_apps.render_action = Some(RenderAction::OpenBotChat { bot_id });
                    cx.activate(true);
                    cx.notify();
                }
                "terms" => self.open_link(MINI_APP_TERMS_URL, cx),
                "privacy" => {
                    if let Some(url) = privacy_policy_url {
                        self.open_link(&url, cx);
                    }
                }
                "remove_from_menu" => {
                    let name = self
                        .mini_apps
                        .window
                        .as_ref()
                        .map(|window| window.launch.bot_name.clone())
                        .unwrap_or_default();
                    self.mini_apps.confirm = Some(WebAppConfirm::RemoveFromMenu { bot_id, name });
                    cx.notify();
                }
                _ => {}
            },
            ShellEvent::PopupClosed { id, button } => {
                let Some(purpose) = self
                    .mini_apps
                    .window
                    .as_mut()
                    .and_then(|window| window.popups.remove(&id))
                else {
                    return;
                };
                match purpose {
                    PopupPurpose::App => self.with_window(|window| {
                        window.emit(BridgeReply::popup_closed(&button));
                    }),
                    PopupPurpose::WriteAccess => {
                        let sent = (button == "allow")
                            .then(|| {
                                self.live
                                    .as_mut()
                                    .map(|live| live.driver.allow_bot_to_send_messages(bot_id))
                            })
                            .flatten();
                        if !matches!(sent, Some(Ok(_))) {
                            self.with_window(|window| {
                                window.waiting_write_access = false;
                                window.emit(BridgeReply::write_access_requested(false));
                            });
                        }
                    }
                    PopupPurpose::Clipboard { req_id } => {
                        let text = (button == "allow")
                            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
                            .flatten();
                        self.with_window(|window| {
                            window.emit(BridgeReply::clipboard_text_received(
                                &req_id,
                                text.as_deref(),
                            ));
                        });
                    }
                    PopupPurpose::CloseConfirm => {
                        if button == "close" {
                            self.close_mini_app_window(cx);
                        }
                    }
                    PopupPurpose::EmojiStatusAccess => {
                        self.answer_emoji_status_access(&button, cx);
                    }
                    PopupPurpose::EmojiStatus => self.answer_set_emoji_status(&button, cx),
                    PopupPurpose::Phone => self.answer_phone_prompt(&button, cx),
                }
            }
        }
    }

    /// Render-time half: what needs the window.
    pub(in crate::ui) fn run_pending_mini_app_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.mini_apps.render_action.take() else {
            return;
        };
        match action {
            RenderAction::SwitchInline { query } => {
                self.insert_switch_inline_query(&query, window, cx);
            }
            RenderAction::OpenBotChat { bot_id } => self.open_user_chat(bot_id, window, cx),
            RenderAction::OpenInvoice { slug } => self.open_mini_app_invoice(slug, window, cx),
        }
    }

    /// Whether the open window shows `bot_id`'s app (the Apps tab row).
    pub(in crate::ui) fn mini_app_open_for(&self, bot_id: i64) -> bool {
        self.mini_apps
            .window
            .as_ref()
            .is_some_and(|window| window.launch.bot_user_id == bot_id)
    }
}
