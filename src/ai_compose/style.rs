//! Text composition styles (`textCompositionStyle`, `schema/td_api.tl:181`)
//! and the list the Style tabs show.
use crate::text::TextEntity;

/// One style the AI can rewrite a text in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCompositionStyle {
    /// The style's unique name: what `composeTextWithAi.style_name` and the
    /// `t.me/addstyle/<name>` link carry.
    pub name: String,
    /// 0 when the style has no emoji.
    pub custom_emoji_id: i64,
    /// In the app's language.
    pub title: String,
    /// Created by a user (false for the built-in styles).
    pub is_custom: bool,
    /// Created by the current user.
    pub is_creator: bool,
    /// Installs of a created custom style; 0 when unknown.
    pub install_count: i32,
    /// For created custom styles only.
    pub prompt: String,
    /// 0 when none or unknown.
    pub creator_user_id: i64,
    pub english_example: Option<StyleExample>,
}

/// `textCompositionStyleExample` (`schema/td_api.tl:169`): a text before and
/// after the style was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleExample {
    pub source_text: String,
    pub source_entities: Vec<TextEntity>,
    pub result_text: String,
    pub result_entities: Vec<TextEntity>,
}

/// The styles in tab order: the server's order from
/// `updateTextCompositionStyles`, with the custom styles created, edited or
/// added in this session moved to the front, newest first (tdesktop
/// `AiComposeTones::reapplyRecentCustomToneOrder`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleList {
    styles: Vec<TextCompositionStyle>,
    /// Names of recently promoted custom styles, newest first.
    recent: Vec<String>,
}

impl StyleList {
    pub fn styles(&self) -> &[TextCompositionStyle] {
        &self.styles
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    pub fn find(&self, name: &str) -> Option<&TextCompositionStyle> {
        self.styles.iter().find(|style| style.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.styles
            .iter()
            .map(|style| style.name.as_str())
            .collect()
    }

    /// A new list from the server; the recent custom styles keep the front.
    pub fn set_server(&mut self, styles: Vec<TextCompositionStyle>) {
        self.styles = styles;
        self.reapply_recent();
    }

    /// A custom style just created, edited or added goes to the front
    /// (tdesktop `promoteCustomTone`). Built-in styles keep their place.
    pub fn promote_front(&mut self, style: TextCompositionStyle) {
        if !style.is_custom {
            return;
        }
        self.remember(&style.name);
        self.styles.retain(|known| known.name != style.name);
        self.styles.insert(0, style);
    }

    /// [`Self::promote_front`] when only the name is known (an added style):
    /// it moves to the front now if listed, and stays there once the
    /// server's next list includes it.
    pub fn promote_name(&mut self, name: &str) {
        match self.find(name).cloned() {
            Some(style) => self.promote_front(style),
            None => self.remember(name),
        }
    }

    /// A removed or deleted style leaves the list and the recent order.
    pub fn forget(&mut self, name: &str) {
        self.styles.retain(|style| style.name != name);
        self.recent.retain(|recent| recent != name);
    }

    fn remember(&mut self, name: &str) {
        self.recent.retain(|recent| recent != name);
        self.recent.insert(0, name.to_string());
    }

    fn reapply_recent(&mut self) {
        let styles = std::mem::take(&mut self.styles);
        let (mut front, rest): (Vec<_>, Vec<_>) = styles.into_iter().partition(|style| {
            style.is_custom && self.recent.iter().any(|name| *name == style.name)
        });
        front.sort_by_key(|style| self.recent.iter().position(|name| *name == style.name));
        self.recent = front.iter().map(|style| style.name.clone()).collect();
        front.extend(rest);
        self.styles = front;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(name: &str, is_custom: bool) -> TextCompositionStyle {
        TextCompositionStyle {
            name: name.to_string(),
            custom_emoji_id: 0,
            title: name.to_uppercase(),
            is_custom,
            is_creator: false,
            install_count: 0,
            prompt: String::new(),
            creator_user_id: 0,
            english_example: None,
        }
    }

    #[test]
    fn server_order_is_kept_without_promotions() {
        let mut list = StyleList::default();
        list.set_server(vec![style("formal", false), style("pirate", true)]);
        assert_eq!(list.names(), ["formal", "pirate"]);
        assert_eq!(
            list.find("pirate").map(|s| s.title.as_str()),
            Some("PIRATE")
        );
        assert!(list.find("missing").is_none());
    }

    #[test]
    fn promoted_custom_styles_stay_in_front_across_server_lists() {
        let mut list = StyleList::default();
        list.set_server(vec![
            style("formal", false),
            style("pirate", true),
            style("poet", true),
        ]);
        list.promote_front(style("poet", true));
        list.promote_name("pirate");
        assert_eq!(list.names(), ["pirate", "poet", "formal"]);
        list.set_server(vec![
            style("formal", false),
            style("poet", true),
            style("pirate", true),
            style("new", true),
        ]);
        assert_eq!(list.names(), ["pirate", "poet", "formal", "new"]);
    }

    #[test]
    fn built_in_styles_are_never_promoted() {
        let mut list = StyleList::default();
        list.set_server(vec![style("pirate", true), style("formal", false)]);
        list.promote_name("formal");
        assert_eq!(list.names(), ["pirate", "formal"]);
    }

    #[test]
    fn added_style_takes_the_front_once_listed() {
        let mut list = StyleList::default();
        list.set_server(vec![style("formal", false)]);
        list.promote_name("shared");
        assert_eq!(list.names(), ["formal"]);
        list.set_server(vec![style("formal", false), style("shared", true)]);
        assert_eq!(list.names(), ["shared", "formal"]);
    }

    #[test]
    fn forgotten_style_leaves_list_and_order() {
        let mut list = StyleList::default();
        list.set_server(vec![style("formal", false), style("pirate", true)]);
        list.promote_name("pirate");
        list.forget("pirate");
        assert_eq!(list.names(), ["formal"]);
        list.set_server(vec![style("formal", false), style("pirate", true)]);
        assert_eq!(list.names(), ["formal", "pirate"]);
    }
}
