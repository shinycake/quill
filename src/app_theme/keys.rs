//! The palette keys Quill paints with, named as in Telegram Desktop's
//! palette (`lib_ui/ui/colors.palette`), so a `.tdesktop-theme` file sets
//! them directly. Every key here is wired to something on screen; keys a
//! theme file sets that Quill does not use are kept in the file but not
//! applied.

/// Where a key shows up, for grouping in the theme editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyGroup {
    Window,
    Controls,
    ChatList,
    Messages,
}

impl KeyGroup {
    pub const ALL: [KeyGroup; 4] = [
        KeyGroup::Window,
        KeyGroup::Controls,
        KeyGroup::ChatList,
        KeyGroup::Messages,
    ];

    pub fn title(self) -> &'static str {
        match self {
            KeyGroup::Window => "Window",
            KeyGroup::Controls => "Buttons, menus and fields",
            KeyGroup::ChatList => "Chat list",
            KeyGroup::Messages => "Messages",
        }
    }
}

macro_rules! palette_keys {
    ($($variant:ident = $name:literal, $group:ident, $label:literal;)*) => {
        /// A palette key Quill reads from a theme.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum PaletteKey {
            $($variant),*
        }

        impl PaletteKey {
            /// Every key, in editor order.
            pub const ALL: &'static [PaletteKey] = &[$(PaletteKey::$variant),*];

            /// The key's name in a `.tdesktop-theme` file.
            pub fn name(self) -> &'static str {
                match self {
                    $(PaletteKey::$variant => $name),*
                }
            }

            /// What the key colours in Quill, for the theme editor.
            pub fn label(self) -> &'static str {
                match self {
                    $(PaletteKey::$variant => $label),*
                }
            }

            pub fn group(self) -> KeyGroup {
                match self {
                    $(PaletteKey::$variant => KeyGroup::$group),*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(PaletteKey::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

palette_keys! {
    WindowBg = "windowBg", Window, "Background";
    WindowFg = "windowFg", Window, "Text";
    WindowBoldFg = "windowBoldFg", Window, "Titles and names";
    WindowSubTextFg = "windowSubTextFg", Window, "Secondary text";
    WindowBgOver = "windowBgOver", Window, "Hovered row";
    WindowBgRipple = "windowBgRipple", Window, "Pressed row";
    WindowBgActive = "windowBgActive", Window, "Accent";
    WindowFgActive = "windowFgActive", Window, "Text on the accent";
    WindowActiveTextFg = "windowActiveTextFg", Window, "Links and active text";
    ActiveButtonBg = "activeButtonBg", Controls, "Primary button";
    ActiveButtonBgOver = "activeButtonBgOver", Controls, "Primary button, hovered";
    ActiveButtonFg = "activeButtonFg", Controls, "Primary button text";
    AttentionButtonFg = "attentionButtonFg", Controls, "Destructive actions";
    BoxTextFgError = "boxTextFgError", Controls, "Errors";
    MenuBg = "menuBg", Controls, "Menus and popovers";
    MenuSeparatorFg = "menuSeparatorFg", Controls, "Separators";
    InputBorderFg = "inputBorderFg", Controls, "Borders";
    PlaceholderFg = "placeholderFg", Controls, "Placeholders";
    FilterInputInactiveBg = "filterInputInactiveBg", Controls, "Panels and code blocks";
    CheckboxFg = "checkboxFg", Controls, "Switches, off";
    ScrollBarBg = "scrollBarBg", Controls, "Scroll bars";
    TitleBgActive = "titleBgActive", Controls, "Title bar";
    DialogsBg = "dialogsBg", ChatList, "Chat list background";
    DialogsUnreadBgMuted = "dialogsUnreadBgMuted", ChatList, "Muted unread badge";
    MsgInBg = "msgInBg", Messages, "Incoming bubble";
    MsgOutBg = "msgOutBg", Messages, "Outgoing bubble";
    MsgOutServiceFg = "msgOutServiceFg", Messages, "Links and names in outgoing bubbles";
}

impl PaletteKey {
    pub const COUNT: usize = Self::ALL.len();

    /// Position in [`PaletteKey::ALL`] (and in a palette's colour array).
    pub fn index(self) -> usize {
        self as usize
    }

    /// tdesktop's `kColorizeIgnoredKeys`: semantic colours an accent never
    /// moves.
    pub fn colorize_ignored(self) -> bool {
        matches!(self, PaletteKey::BoxTextFgError)
    }
}

#[cfg(test)]
mod tests {
    use super::PaletteKey;

    #[test]
    fn names_round_trip_and_indexes_match() {
        for (i, key) in PaletteKey::ALL.iter().enumerate() {
            assert_eq!(key.index(), i);
            assert_eq!(PaletteKey::from_name(key.name()), Some(*key));
            assert!(!key.label().is_empty());
        }
        assert_eq!(PaletteKey::from_name("nope"), None);
    }
}
