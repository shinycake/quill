//! Folder icons drawn from the bundled Lucide set (tdesktop draws its own
//! `FilterIcon` bitmaps; the names are the schema's `chatFolderIcon` names).

use gpui_kit::assets::IconName;

/// The glyph for a `chatFolderIcon` name; unknown names show the generic
/// folder, as tdesktop's `Custom`.
pub(super) fn folder_glyph(name: &str) -> IconName {
    match quill::folder_icons::display_icon_name(name) {
        "Cat" => IconName::Cat,
        "Book" => IconName::Book,
        "Money" => IconName::Banknote,
        "Game" => IconName::Gamepad2,
        "Light" => IconName::Lightbulb,
        "Like" => IconName::ThumbsUp,
        "Note" => IconName::Music,
        "Palette" => IconName::Palette,
        "Travel" => IconName::Luggage,
        "Sport" => IconName::Dumbbell,
        "Favorite" => IconName::Star,
        "Study" => IconName::GraduationCap,
        "Airplane" => IconName::Plane,
        "Private" => IconName::User,
        "Groups" => IconName::Users,
        "All" => IconName::MessageCircle,
        "Unread" => IconName::MessageSquareDot,
        "Bots" => IconName::Bot,
        "Crown" => IconName::Crown,
        "Flower" => IconName::Flower,
        "Home" => IconName::House,
        "Love" => IconName::Heart,
        "Mask" => IconName::Drama,
        "Party" => IconName::PartyPopper,
        "Trade" => IconName::TrendingUp,
        "Work" => IconName::Briefcase,
        "Unmuted" => IconName::Bell,
        "Channels" => IconName::Megaphone,
        "Setup" => IconName::Settings,
        _ => IconName::Folder,
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::assets::IconName;

    #[test]
    fn every_picker_icon_has_a_distinct_glyph() {
        let mut seen = std::collections::HashSet::new();
        for name in quill::folder_icons::ICON_NAMES {
            if name == "Custom" {
                assert!(matches!(super::folder_glyph(name), IconName::Folder));
                continue;
            }
            let glyph = format!("{:?}", super::folder_glyph(name));
            assert!(seen.insert(glyph.clone()), "{name} repeats {glyph}");
            assert_ne!(glyph, "Folder", "{name} fell back to the generic folder");
        }
    }
}
