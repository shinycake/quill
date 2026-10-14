//! Settings > Advanced and its Experimental settings page (tdesktop
//! `settings_advanced.cpp`, `settings_experimental.cpp`), plus the "Install
//! beta versions" switch of the update section.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::advanced::experimental::{self, Category, Imported, OptionSpec, Values};
use quill::advanced::update_channel;
use quill::updater::UpdateState;

pub(super) const ADVANCED_PAGE: &str = "Advanced";
pub(super) const EXPERIMENTAL_PAGE: &str = "Experimental settings";

/// The page the Back button of `page` returns to (tdesktop opens Experimental
/// settings from Advanced).
pub(super) fn parent_page(page: &'static str) -> Option<&'static str> {
    (page == EXPERIMENTAL_PAGE).then_some(ADVANCED_PAGE)
}

impl QuillApp {
    /// The Advanced page. tdesktop's holds the update switches, the export
    /// and the Experimental settings entry; Quill's update switches live in
    /// the update section.
    pub(super) fn advanced_settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("settings-experimental")
                    .label(EXPERIMENTAL_PAGE)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings.page = Some(EXPERIMENTAL_PAGE);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The current value of every experimental option.
    pub(super) fn experimental_values(&self) -> Values {
        Values {
            split_profile_media: self.chat_prefs.profile_media_tabs_expanded,
            fast_buttons_mode: experimental::fast_buttons_mode(),
            touchbar_disabled: experimental::touchbar_disabled(),
        }
    }

    /// Change one option; the shared media gallery and the bot buttons follow.
    pub(super) fn set_experimental_option(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        match id {
            experimental::SPLIT_MEDIA_ID => {
                self.set_chat_prefs(cx, |c| c.profile_media_tabs_expanded = on);
                self.apply_profile_media_tabs_option(cx);
            }
            experimental::FAST_BUTTONS_ID => {
                experimental::set_fast_buttons_mode(on);
                // Rows are cached per message revision; the digits appear
                // or go on the next history refresh.
                self.history.rows_key = None;
            }
            // The Touch Bar follows on its next sync (`touch_bar`), so no
            // restart is needed.
            experimental::TOUCHBAR_DISABLED_ID => experimental::set_touchbar_disabled(on),
            _ => {}
        }
        cx.notify();
    }

    fn apply_experimental_values(&mut self, values: Values, cx: &mut Context<Self>) {
        let now = self.experimental_values();
        for id in [
            experimental::SPLIT_MEDIA_ID,
            experimental::FAST_BUTTONS_ID,
            experimental::TOUCHBAR_DISABLED_ID,
        ] {
            let wanted = values.get(id).unwrap_or(false);
            if now.get(id) != Some(wanted) {
                self.set_experimental_option(id, wanted, cx);
            }
        }
    }

    fn experimental_row(&self, spec: OptionSpec, on: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = spec.id;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(spec.name))
                    .when(!spec.description.is_empty(), |column| {
                        column.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(spec.description),
                        )
                    }),
            )
            .child(
                Switch::new(SharedString::from(format!("experimental-{id}")))
                    .checked(on)
                    .accessibility_label(spec.name)
                    .on_click(cx.listener(move |this, &next: &bool, _, cx| {
                        this.set_experimental_option(id, next, cx)
                    })),
            )
            .into_any_element()
    }

    fn experimental_category(
        &self,
        category: Category,
        values: Values,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut column = div().flex().flex_col().gap_3().child(
            div()
                .id(SharedString::from(format!(
                    "experimental-{}",
                    category.title
                )))
                .role(Role::Heading)
                .aria_label(category.title)
                .text_sm()
                .font_semibold()
                .child(category.title),
        );
        for spec in category.options {
            column = column.child(self.experimental_row(
                *spec,
                values.get(spec.id).unwrap_or(false),
                cx,
            ));
        }
        column.into_any_element()
    }

    /// The Experimental settings page: the warning, "Restore default values"
    /// while anything differs, the options by category, then Export and
    /// Import (tdesktop puts those two in the page's top bar menu).
    pub(super) fn experimental_settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let values = self.experimental_values();
        let clipboard = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        let mut page = div().flex().flex_col().gap_4().child(
            div()
                .id("experimental-about")
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(experimental::ABOUT),
        );
        if values.changed() {
            page = page.child(
                Button::new("experimental-restore")
                    .label(experimental::RESTORE_LABEL)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.apply_experimental_values(Values::default(), cx)
                    })),
            );
        }
        page = page.child(self.experimental_category(experimental::PROFILE, values, cx));
        // tdesktop scopes the Touch Bar option to macOS.
        if cfg!(target_os = "macos") {
            page = page.child(self.experimental_category(experimental::INTERFACE, values, cx));
        }
        // Fast buttons mode is listed only while it is on (the
        // `numberbuttons` code turns it on).
        if values.fast_buttons_mode {
            page = page.child(self.experimental_category(experimental::OTHER, values, cx));
        }
        let mut actions = div().flex().gap_2().child(
            Button::new("experimental-export")
                .label("Export")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    let code = experimental::encode_export(this.experimental_values());
                    cx.write_to_clipboard(ClipboardItem::new_string(code));
                    this.connection.status_note = experimental::EXPORTED_TOAST.into();
                    cx.notify();
                })),
        );
        if experimental::is_importable(&clipboard) {
            actions = actions.child(
                Button::new("experimental-import")
                    .label("Import")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.import_experimental(cx))),
            );
        }
        page.child(actions).into_any_element()
    }

    fn import_experimental(&mut self, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        self.connection.status_note = match experimental::decode_import(&text) {
            Imported::NotACode => experimental::INVALID_CODE_TOAST.into(),
            Imported::Unsupported => experimental::UNSUPPORTED_CODE_TOAST.into(),
            Imported::Values(values) => {
                self.apply_experimental_values(values, cx);
                experimental::IMPORTED_TOAST.into()
            }
        };
        cx.notify();
    }

    /// "Install beta versions" in the update section. tdesktop shows it with
    /// the other update options, which are hidden while updates are off or
    /// one is downloading.
    pub(super) fn install_beta_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.appearance.check_updates_on_launch
            || matches!(
                self.settings.update_state,
                UpdateState::Downloading(_) | UpdateState::Installing(_)
            )
        {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(div().text_sm().child(update_channel::LABEL))
                .child(
                    Switch::new("install-beta-versions")
                        .checked(update_channel::install_beta())
                        .accessibility_label(update_channel::LABEL)
                        .on_click(cx.listener(|this, &on: &bool, _, cx| {
                            update_channel::set_install_beta(on);
                            // tdesktop checks again right away.
                            this.check_for_updates(cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-advanced")
        .setup(|app, _, _| app.demo_settings_page(ADVANCED_PAGE)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-experimental")
        .setup(|app, _, _| app.demo_settings_page(EXPERIMENTAL_PAGE)),
];

impl QuillApp {
    /// Settings open on one page (screenshot demos; nothing is saved).
    fn demo_settings_page(&mut self, page: &'static str) {
        self.settings.open = true;
        self.settings.page = Some(page);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "advanced_ui_tests.rs"]
mod tests;
