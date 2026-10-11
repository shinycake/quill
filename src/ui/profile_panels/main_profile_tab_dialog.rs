//! "Main tab" chooser on your own profile (`setMainProfileTab`).
//! tdesktop picks the tab from the profile's tab strip; Quill lists the
//! choices in a dialog next to the other profile settings rows.

use super::*;
use quill::profile_tab::ProfileTab;

impl QuillApp {
    pub(in crate::ui) fn open_main_tab_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.profile_dialog = Some(ProfileDialog::MainTab);
        cx.notify();
    }

    fn choose_main_profile_tab(&mut self, tab: ProfileTab, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = "Demo mode: this needs a live session.".into();
            cx.notify();
            return true;
        };
        match live.driver.set_main_profile_tab(tab) {
            Ok(_) => {
                self.connection.status_note = format!("Main tab: {}", tab.label());
                true
            }
            Err(_) => {
                self.connection.status_note = "Couldn't reach Telegram; try again.".into();
                cx.notify();
                false
            }
        }
    }

    /// Title, body and footer of the main-tab chooser.
    pub(super) fn main_tab_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let muted = cx.theme().muted_foreground;
        let current = self.session()?.my_main_profile_tab();
        let mut body = div().flex().flex_col().gap_1().child(
            div()
                .text_xs()
                .text_color(muted)
                .pb_1()
                .child("Choose which tab your profile opens on."),
        );
        for (index, tab) in ProfileTab::ALL.into_iter().enumerate() {
            body = body.child(
                action_row(
                    ("main-tab-choice", index as u64),
                    (current == Some(tab)).then_some(IconName::CircleCheck),
                    tab.label(),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.choose_main_profile_tab(tab, cx) {
                        this.close_profile_dialog(cx);
                    }
                    this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                })),
            );
        }
        let footer = Button::new("main-tab-close")
            .label("Close")
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_profile_dialog(cx);
                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
            }));
        Some((
            "Main tab".into(),
            body.into_any_element(),
            div().flex().gap_2().child(footer).into_any_element(),
        ))
    }
}
