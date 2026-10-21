//! The Omarchy screenshot demo and its Tokyo Night fixture.

use super::QuillApp;

/// Tokyo Night's `colors.toml`, for the screenshot demo and tests.
pub(crate) const DEMO_COLORS: &str = r##"mode = "dark"
accent = "#7aa2f7"
selection = "#292e42"
muted = "#414868"
background = "#1a1b26"
dark_background = "#13141c"
darker_background = "#0e0e14"
lighter_background = "#24283b"
foreground = "#a9b1d6"
dark_foreground = "#565f89"
bright_foreground = "#c0caf5"
red = "#f7768e"
"##;

impl QuillApp {
    /// Writes [`DEMO_COLORS`] as Omarchy's current theme under `root`.
    pub(crate) fn demo_omarchy(&mut self, root: &std::path::Path) {
        let theme = root.join("theme");
        let _ = std::fs::create_dir_all(&theme);
        let _ = std::fs::write(theme.join("colors.toml"), DEMO_COLORS);
        let _ = std::fs::write(root.join("theme.name"), "Tokyo Night");
        self.use_omarchy_dir(root.to_path_buf());
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-omarchy").setup(
        |app, _, cx| {
            app.demo_omarchy(&quill::scratch::process_dir("quill-demo-omarchy"));
            app.appearance.follow_omarchy = true;
            app.settings.appearance_applied = None;
            app.apply_appearance(cx);
            app.open_settings_on(quill::settings_catalog::Page::Appearance);
        }
    ),
];
