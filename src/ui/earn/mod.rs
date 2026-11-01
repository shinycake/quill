//! Revenue pages and affiliate programs in the info panel (tdesktop
//! `info/channel_statistics/earn/*`, `info/bot/earn/*` and
//! `info/bot/starref/*`), with their boxes. The data and the requests are
//! `quill::earn`; withdrawals only open a Fragment page in the browser.
#![deny(clippy::indexing_slicing)]

use crate::ui::status_note::StatusNote;
mod boxes;
mod chart;
mod demo;
mod entries;
pub(in crate::ui) mod fixture;
mod icons;
mod join;
mod layer;
mod revenue;
mod setup;
mod starref_boxes;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
mod withdraw;

pub(in crate::ui) use icons::*;
pub(crate) use layer::LayerFade;

use super::app::QuillApp;
use super::chat_theme::text_muted;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::earn::{EarnPanel, EarnPeer, EarnState};
use quill::lang::{keys, tr};
use quill::state::{InfoPanelTarget, Session};

/// tdesktop's "Learn more ›" link text.
pub(super) const LEARN_MORE: &str = "Learn more \u{203A}";

/// A section header (tdesktop `AddHeader` / `AddSubsectionTitle`).
pub(super) fn header(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .pt_1()
        .text_sm()
        .font_semibold()
        .text_color(cx.theme().primary)
        .child(text.into())
}

/// Gray text under a block (tdesktop `DividerLabel`).
pub(super) fn about(text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(text_muted()).child(text.into())
}

/// Gray wrapped text with a link inside it ("…agree to the {terms}
/// of…", "… Learn more ›").
pub(super) fn about_link(
    id: &'static str,
    before: &str,
    link: &str,
    after: &str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let mut text = String::from(before);
    if !text.is_empty() {
        text.push(' ');
    }
    let start = text.len();
    text.push_str(link);
    let range = start..text.len();
    if !after.is_empty() {
        text.push(' ');
        text.push_str(after);
    }
    let style = HighlightStyle {
        color: Some(cx.theme().primary),
        ..Default::default()
    };
    div().text_xs().text_color(text_muted()).child(
        InteractiveText::new(
            id,
            StyledText::new(text).with_highlights(vec![(range.clone(), style)]),
        )
        .on_click(vec![range], move |_, window, cx| on_click(window, cx)),
    )
}

/// The page's side padding inside the layer; the bands below reach
/// past it to the layer's edges.
const PAGE_INSET: f32 = 16.;

/// The band between blocks (tdesktop `AddDivider`), edge to edge.
pub(super) fn divider(cx: &App) -> Div {
    div()
        .mx(px(-PAGE_INSET))
        .h(px(8.))
        .flex_none()
        .bg(cx.theme().muted)
        .border_y_1()
        .border_color(cx.theme().border)
}

/// Gray text on the divider band (tdesktop `DividerLabel` and
/// `AddDividerText`), edge to edge; the text keeps WCAG AA on the band.
pub(super) fn divider_label(content: Div, cx: &App) -> Div {
    div()
        .mx(px(-PAGE_INSET))
        .px(px(PAGE_INSET))
        .py_2p5()
        .bg(cx.theme().muted)
        .border_y_1()
        .border_color(cx.theme().border)
        .child(content.text_color(readable_on(text_muted().into(), cx.theme().muted)))
}

/// `fg` held to WCAG AA on `bg`.
pub(super) fn readable_on(fg: Hsla, bg: Hsla) -> Hsla {
    let pack = |color: Hsla| {
        let rgba = color.to_rgb();
        let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
        (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b)
    };
    let out = quill::theme_colorize::readable(pack(fg), pack(bg), quill::theme_colorize::WCAG_AA);
    let mut color: Hsla = rgb(out).into();
    color.a = fg.a;
    color
}

/// A title and its about line with an icon on the left (tdesktop's
/// info rows on the affiliate pages and the learn box).
pub(super) fn info_point(
    icon: gpui_kit::assets::IconName,
    title: &'static str,
    text: &'static str,
    cx: &App,
) -> Div {
    div()
        .flex()
        .gap_3()
        .child(Icon::new(icon).size(px(20.)).text_color(cx.theme().primary))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .child(div().text_sm().font_semibold().child(title))
                .child(about(text)),
        )
}

/// The name of a peer for rows and boxes.
pub(super) fn peer_name(session: &Session, peer: EarnPeer) -> String {
    session.sender_label(peer.sender())
}

/// The photo path of a peer, when downloaded.
pub(super) fn peer_photo(session: &Session, peer: EarnPeer) -> Option<&str> {
    match peer {
        EarnPeer::User(id) => session
            .user_photo_path(id)
            .or_else(|| session.chat_photo_path(quill::ids::ChatId(id))),
        EarnPeer::Chat(id) => session.chat_photo_path(quill::ids::ChatId(id)),
    }
}

impl QuillApp {
    /// A peer's round photo (or initials).
    pub(super) fn earn_avatar(&self, session: &Session, peer: EarnPeer, size: f32) -> AnyElement {
        let roots = self.media_display_roots();
        let photo = peer_photo(session, peer)
            .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots));
        crate::ui::chat_row::chat_avatar(&peer_name(session, peer), photo.as_deref(), size)
            .into_any_element()
    }

    /// tdesktop `CreatePeerBubble`: a pill with the photo and the name.
    pub(super) fn earn_peer_bubble(&self, session: &Session, peer: EarnPeer, cx: &App) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .pl_0p5()
            .pr_3()
            .py_0p5()
            .rounded_full()
            .bg(cx.theme().secondary)
            .child(self.earn_avatar(session, peer, 24.))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .truncate()
                    .child(peer_name(session, peer)),
            )
    }

    /// Run a driver call on the live path; a failure toasts `failure`
    /// and the demo says it needs a live session.
    pub(super) fn earn_call(
        &mut self,
        failure: &str,
        cx: &mut Context<Self>,
        call: impl FnOnce(
            &mut quill::connect::ConnectDriver<quill::connect::LiveSender>,
        ) -> Result<(), quill::connect::ConnectSendError>,
    ) {
        match self.live.as_mut() {
            Some(live) => {
                if call(&mut live.driver).is_err() {
                    self.connection.set_note(StatusNote::error(failure));
                }
            }
            None => {
                self.connection
                    .set_note(StatusNote::info(tr(keys::Q_MONEY_DEMO_MODE_THIS_NEEDS_A)));
            }
        }
        cx.notify();
    }

    /// The session the earn screens read and write: live or the demo's.
    pub(super) fn earn_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    pub(super) fn earn_mut(&mut self) -> Option<&mut EarnState> {
        self.earn_session_mut().map(|s| &mut s.payments.earn)
    }

    pub(super) fn earn_state(&self) -> Option<&EarnState> {
        self.session().map(|s| &s.payments.earn)
    }

    /// Open an earn page fresh (tdesktop `showSection`) in the layer: a
    /// page opened from another earn page stacks for the back arrow, and
    /// the info panel the layer covers comes back when it closes.
    pub(in crate::ui) fn open_earn(
        &mut self,
        panel: EarnPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.session().and_then(|s| s.users_state.open_info_panel);
        match current {
            Some(InfoPanelTarget::Earn(open)) if open != panel => {
                self.earn.back.push(InfoPanelTarget::Earn(open));
            }
            Some(InfoPanelTarget::Earn(_)) => {}
            other => {
                self.earn.back.clear();
                self.earn.restore = other;
            }
        }
        if self.live.is_some()
            && let Some(earn) = self.earn_mut()
        {
            // A fresh open loads again; going back keeps what was shown.
            match panel {
                EarnPanel::Revenue { .. } => earn.page = None,
                EarnPanel::Setup { .. } => earn.setup = None,
                EarnPanel::Join { .. } => earn.join = None,
            }
        }
        if let EarnPanel::Setup { .. } = panel {
            self.earn.commission = None;
            self.earn.duration = None;
        }
        self.show_earn_layer();
        self.open_info_panel_target(InfoPanelTarget::Earn(panel), window, cx);
    }

    /// The back arrow of an earn page.
    pub(super) fn earn_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.earn.back.pop() {
            Some(target) => self.open_info_panel_target(target, window, cx),
            None => self.close_earn_layer(cx),
        }
    }

    /// Load an earn page on the live path; the info panel's open calls it.
    pub(in crate::ui) fn load_earn_panel(&mut self, panel: EarnPanel, is_dark: bool) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let earn = &live.driver.session.payments.earn;
        let kept = match panel {
            EarnPanel::Revenue { peer, screen } => earn
                .page
                .as_ref()
                .is_some_and(|p| p.peer == peer && p.screen == screen),
            EarnPanel::Setup { bot_user_id } => earn
                .setup
                .as_ref()
                .is_some_and(|s| s.bot_user_id == bot_user_id),
            EarnPanel::Join { peer } => earn.join.as_ref().is_some_and(|j| j.peer == peer),
        };
        if kept {
            return;
        }
        if let EarnPanel::Revenue {
            peer: EarnPeer::Chat(chat_id),
            ..
        } = panel
        {
            // The boost level behind "Switch Off Ads".
            let chat_id = quill::ids::ChatId(chat_id);
            let _ = live.driver.fetch_chat_boost_status(chat_id);
            let _ = live.driver.fetch_chat_boost_features(chat_id);
        }
        if live.driver.open_earn_panel(panel, is_dark).is_err() {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULDNT_LOAD_THE_PAGE)));
        }
    }

    /// Poll-loop half: toasts, the browser and the setup page's close.
    pub(in crate::ui) fn drain_earn(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(earn) = self
            .live
            .as_mut()
            .map(|l| &mut l.driver.session.payments.earn)
        else {
            return false;
        };
        let notes = std::mem::take(&mut earn.notes);
        let url = earn.open_url.take();
        let closed = std::mem::replace(&mut earn.setup_closed, false);
        let progressed = !notes.is_empty() || url.is_some() || closed;
        if let Some(note) = notes.into_iter().last() {
            self.connection.set_note(StatusNote::info(note));
        }
        if let Some(url) = url {
            self.open_link(&url, cx);
        }
        if closed
            && matches!(
                self.session().and_then(|s| s.users_state.open_info_panel),
                Some(InfoPanelTarget::Earn(EarnPanel::Setup { .. }))
            )
        {
            // tdesktop `showBackFromStack` after saving or ending.
            match self.earn.back.pop() {
                Some(target) => {
                    if let Some(live) = self.live.as_mut() {
                        live.driver.set_info_panel(Some(target));
                    }
                }
                None => self.drop_earn_layer(cx),
            }
        }
        if progressed {
            cx.notify();
        }
        progressed
    }

    /// Per-frame half: things that need the window (the amount field).
    pub(in crate::ui) fn earn_frame_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prefill_withdraw_amount(window, cx);
        if matches!(
            self.session().and_then(|s| s.users_state.open_info_panel),
            Some(InfoPanelTarget::Earn(EarnPanel::Setup { .. }))
        ) {
            self.ensure_setup_sliders(window, cx);
        }
        self.join_upkeep();
        self.earn_balances_upkeep();
        let link_open = self.earn_state().is_some_and(|e| e.link_box.is_some());
        match (link_open, self.earn.link_shown) {
            (true, None) => self.earn.link_shown = Some(std::time::Instant::now()),
            (false, Some(_)) => self.earn.link_shown = None,
            _ => {}
        }
    }

    /// The content of an earn page under the layer's top bar: the page,
    /// which scrolls itself so the end of a program list can load the
    /// next page, and the setup page's pinned bottom (tdesktop
    /// `setupBottom` with `setScrollBottomSkip`).
    pub(super) fn earn_panel(&self, panel: EarnPanel, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let body = match panel {
            EarnPanel::Revenue { peer, screen } => self.revenue_page(peer, screen, cx),
            EarnPanel::Setup { bot_user_id } => self.setup_page(bot_user_id, cx),
            EarnPanel::Join { peer } => self.join_page(peer, cx),
        };
        let column = div()
            .id("earn-panel")
            .debug_selector(|| "earn-panel".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.earn.scroll)
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .pb_4()
            .child(body);
        let mut parts = vec![column.into_any_element()];
        if let EarnPanel::Setup { bot_user_id } = panel {
            parts.extend(self.setup_footer(bot_user_id, cx));
        }
        parts
    }
}
