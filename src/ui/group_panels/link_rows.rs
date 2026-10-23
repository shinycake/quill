//! The Link and Location rows of a group or channel profile (tdesktop
//! `DetailsFiller::setupInfo` in `info_profile_actions.cpp`): the public
//! link as "t.me/name" over "Link", or over "also @b, @c" when the chat
//! has more usernames; a click copies it, the right-click menu copies or
//! shares it and the QR button opens the code. A location-based group
//! shows its address under the link.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::menu::PopupMenuItem;

/// "also @b, @c" under the link: every active username after the first.
fn usernames_subtext(active: &[String]) -> Option<String> {
    let others: Vec<String> = active
        .iter()
        .skip(1)
        .map(|name| format!("@{name}"))
        .collect();
    (!others.is_empty())
        .then(|| format!("{} {}", tr(keys::INFO_USERNAMES_LABEL), others.join(", ")))
}

impl QuillApp {
    pub(super) fn group_link_card(
        &self,
        supergroup_id: i64,
        chat_id: ChatId,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let mut active = session
            .groups
            .supergroup_username_lists
            .get(&supergroup_id)
            .map(|lists| lists.active.clone())
            .unwrap_or_default();
        if active.is_empty()
            && let Some(name) = session
                .supergroup_username(supergroup_id)
                .filter(|name| !name.is_empty())
        {
            active.push(name.to_string());
        }
        let location = session
            .supergroup_full_info(supergroup_id)
            .and_then(|info| info.profile.location.clone());
        if active.is_empty() && location.is_none() {
            return None;
        }
        let mut card = div()
            .flex()
            .flex_col()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border);
        if let Some(name) = active.first().cloned() {
            let collectible = session
                .groups
                .supergroup_username_lists
                .get(&supergroup_id)
                .is_some_and(|lists| lists.collectible.contains(&name));
            let label =
                usernames_subtext(&active).unwrap_or_else(|| tr(keys::INFO_LINK_LABEL).to_string());
            card = card.child(self.group_link_row(chat_id, name, label, collectible, title, cx));
        }
        if let Some(location) = location {
            card = card.child(self.business_location_row(&location, cx));
        }
        Some(card.into_any_element())
    }

    fn group_link_row(
        &self,
        chat_id: ChatId,
        name: String,
        label: String,
        collectible: bool,
        title: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let link = quill::profile_forms::profile_link(&name);
        let shown = link.trim_start_matches("https://").to_string();
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        let owner = cx.entity().downgrade();
        let owner_name = title.to_string();
        let tapped = link.clone();
        let menu_link = link.clone();
        let qr_peer = quill::peer_qr::state::QrPeer::Chat(chat_id.0);
        div()
            .id("group-link")
            .relative()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .pr_10()
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{label}: {shown}"))
            .tab_index(0)
            .hover(move |style| style.bg(hover))
            .child(div().text_sm().text_color(cx.theme().link).child(shown))
            .child(div().text_xs().text_color(muted).child(label))
            .child(
                div()
                    .absolute()
                    .right_2()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .debug_selector(|| "group-link-qr".into())
                    .child(crate::ui::peer_qr::qr_row_button(
                        "group-link-qr",
                        cx.listener(move |this, _, _, cx| this.open_peer_qr_box(qr_peer, cx)),
                    )),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if collectible {
                    this.open_collectible_info(
                        &name,
                        &owner_name,
                        Some(chat_id),
                        tapped.clone(),
                        cx,
                    );
                } else {
                    this.copy_profile_text(&tapped, tr(keys::CHANNEL_PUBLIC_LINK_COPIED), cx);
                }
            }))
            .test_support()
            .context_menu(move |menu, _, _| {
                let copy = CopyAction {
                    text: menu_link.clone(),
                    menu: tr(keys::CONTEXT_COPY_LINK),
                    toast: tr(keys::CHANNEL_PUBLIC_LINK_COPIED),
                };
                let share_owner = owner.clone();
                let share_link = menu_link.clone();
                copy_menu(menu, &owner, std::iter::once(&copy)).item(
                    PopupMenuItem::new(tr(keys::GROUP_INVITE_SHARE)).on_click(move |_, _, cx| {
                        let _ = share_owner.update(cx, |this, cx| {
                            this.dismiss_profile_modal();
                            this.share.link_text = Some(share_link.clone());
                            cx.notify();
                        });
                    }),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::usernames_subtext;

    #[test]
    fn a_second_username_turns_the_label_into_also() {
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(usernames_subtext(&names(&["main"])), None);
        assert_eq!(
            usernames_subtext(&names(&["main", "second", "third"])).as_deref(),
            Some("also @second, @third")
        );
    }
}
