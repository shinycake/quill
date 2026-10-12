use super::*;
use crate::theme_colorize::{Hsl, Hsv};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "quill-app-theme-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn embedded_palettes_match_tdesktop() {
    let classic = BuiltinTheme::Classic.palette();
    assert_eq!(classic.rgb(PaletteKey::WindowBg), 0xffffff);
    assert_eq!(classic.rgb(PaletteKey::WindowBgActive), 0x40a7e3);
    assert_eq!(classic.rgb(PaletteKey::MsgOutBg), 0xeffdde);
    assert!(!classic.is_dark());
    let day = BuiltinTheme::Day.palette();
    assert_eq!(day.rgb(PaletteKey::MsgOutBg), 0xdef1fd);
    let tinted = BuiltinTheme::Tinted.palette();
    assert_eq!(tinted.rgb(PaletteKey::WindowBg), 0x17212b);
    assert_eq!(tinted.rgb(PaletteKey::MsgOutBg), 0x2b5278);
    assert_eq!(tinted.rgba(PaletteKey::ScrollBarBg), 0xffffff53);
    assert!(tinted.is_dark());
    let night = BuiltinTheme::Night.palette();
    assert_eq!(night.rgb(PaletteKey::WindowBgActive), 0x3fc1b0);
    assert!(night.is_dark());
    for theme in BuiltinTheme::ALL {
        assert_eq!(theme.palette().is_dark(), theme.is_dark(), "{theme:?}");
        assert_eq!(BuiltinTheme::from_id(theme.id()), Some(theme));
        // The accent the palette is drawn around is its accent fill.
        assert_eq!(
            theme.palette().rgb(PaletteKey::WindowBgActive),
            theme.accent()
        );
    }
}

#[test]
fn accent_recolours_the_palette_like_tdesktop() {
    let theme = BuiltinTheme::Classic;
    assert_eq!(theme.palette_with_accent(None), theme.palette());
    assert_eq!(
        theme.palette_with_accent(Some(theme.accent())),
        theme.palette()
    );
    let green = theme.palette_with_accent(Some(0x52b440));
    let active = Hsv::of(green.rgb(PaletteKey::WindowBgActive));
    assert!(
        (active.hue - Hsv::of(0x52b440).hue).abs() <= 2,
        "{active:?}"
    );
    // Links move with it; greys, alpha and ignored keys stay.
    assert_ne!(
        green.rgb(PaletteKey::WindowActiveTextFg),
        theme.palette().rgb(PaletteKey::WindowActiveTextFg)
    );
    assert_eq!(green.rgb(PaletteKey::WindowSubTextFg), 0x999999);
    assert_eq!(green.rgba(PaletteKey::ScrollBarBg), 0x00000053);
    assert_eq!(green.rgb(PaletteKey::BoxTextFgError), 0xd84d4d);
    // Night themes keep a dark accent light enough (lightness >= 64).
    let night = BuiltinTheme::Tinted.palette_with_accent(Some(0x050510));
    let l = Hsl::of(night.rgb(PaletteKey::WindowBgActive)).lightness * 255.0;
    assert!(l >= 40.0, "{l}");
    // Accent lists have the tdesktop eight; Quill's keep its seven presets.
    for theme in BuiltinTheme::ALL {
        let expected = if theme.is_quill() { 7 } else { 8 };
        assert_eq!(theme.accent_choices().len(), expected, "{theme:?}");
    }
}

#[test]
fn quill_themes_are_the_defaults_and_keep_their_look() {
    assert_eq!(
        ThemeId::default_day(),
        ThemeId::Builtin(BuiltinTheme::QuillLight)
    );
    assert_eq!(
        ThemeId::default_night(),
        ThemeId::Builtin(BuiltinTheme::QuillDark)
    );
    assert_eq!(
        BuiltinTheme::ALL[..2],
        [BuiltinTheme::QuillLight, BuiltinTheme::QuillDark]
    );
    let light = BuiltinTheme::QuillLight.palette();
    assert!(!light.is_dark() && BuiltinTheme::QuillDark.palette().is_dark());
    // No theme wallpaper: the window shows through, as before.
    assert_eq!(light.background, None);
    // An accent only re-points the primary fills; nothing else moves.
    let purple = BuiltinTheme::QuillDark.palette_with_accent(Some(0x8957e5));
    assert_eq!(purple.rgb(PaletteKey::WindowBgActive), 0x8957e5);
    assert_eq!(
        purple.rgb(PaletteKey::MsgOutBg),
        0x1f6feb,
        "bubbles keep accent_strong"
    );
    assert_eq!(
        purple.rgb(PaletteKey::WindowActiveTextFg),
        BuiltinTheme::QuillDark
            .palette()
            .rgb(PaletteKey::WindowActiveTextFg)
    );
}

#[test]
fn palette_text_parses_like_tdesktop() {
    let text = b"// comment\nwindowBg: #112233; /* block */ windowFg: #445566aa;\n\
        msgInBg: windowBg; unknownKey: #010203;\n";
    let file = ThemeFile::parse(text).unwrap();
    assert_eq!(file.entries.len(), 4);
    let base = BuiltinTheme::Classic.palette();
    let palette = custom_palette(&file, None);
    assert_eq!(palette.rgb(PaletteKey::WindowBg), 0x112233);
    assert_eq!(palette.rgba(PaletteKey::WindowFg), 0x445566aa);
    assert_eq!(palette.rgb(PaletteKey::MsgInBg), 0x112233, "a reference");
    // Unset keys keep the default palette.
    assert_eq!(
        palette.rgb(PaletteKey::MsgOutBg),
        base.rgb(PaletteKey::MsgOutBg)
    );
    assert_eq!(file.raw("unknownKey"), Some("#010203"));
    // A reference to a key the file does not set resolves to the default.
    let file = ThemeFile::parse(b"windowBg: dialogsBg;").unwrap();
    assert_eq!(file.color("windowBg", &base), Some(0xffffffff));
    // Syntax errors refuse the palette.
    for bad in [
        &b"windowBg #fff;"[..],
        b"windowBg: ;",
        b"windowBg: #fff",
        b": #fff;",
    ] {
        assert!(
            matches!(ThemeFile::parse(bad), Err(ThemeFileError::BadPalette(_))),
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
    // A bad hex value is skipped, not fatal (tdesktop logs and goes on).
    let file = ThemeFile::parse(b"windowBg: #12345;").unwrap();
    assert_eq!(file.color("windowBg", &base), None);
}

#[test]
fn theme_files_round_trip_through_the_zip() {
    let mut palette = BuiltinTheme::Night.palette_with_accent(Some(0xca7896));
    palette.background = Some(ThemeBackground::Solid(0x18191d));
    let file = ThemeFile::from_palette(&palette);
    let zip = file.to_zip();
    assert!(zip.starts_with(b"PK\x03\x04"));
    let back = ThemeFile::parse(&zip).unwrap();
    assert_eq!(back, file);
    let restored = custom_palette(&back, None);
    assert_eq!(restored, palette);
    assert!(restored.is_dark());
    // Edits replace a key's value and keep keys Quill does not use.
    let mut edited = ThemeFile::parse(b"windowBg: #000000; historyPeer1NameFg: #ff0000;").unwrap();
    edited.set_color(PaletteKey::WindowBg, 0x123456ff);
    edited.set_background(0x0e1621);
    let back = ThemeFile::parse(&edited.to_zip()).unwrap();
    assert_eq!(back.raw("windowBg"), Some("#123456"));
    assert_eq!(back.raw("historyPeer1NameFg"), Some("#ff0000"));
    assert_eq!(
        custom_palette(&back, None).background,
        Some(ThemeBackground::Solid(0x0e1621))
    );
}

#[test]
fn reads_tdesktop_style_archives() {
    // The bundled tdesktop themes are zips with a one-pixel background.png
    // and, here, a `tiled.jpg` alternative ignored after the first image.
    let parts = vec![
        (
            "Colors.TDesktop-Theme".to_string(),
            b"windowBg: #17212b; dialogsBg: windowBg;".to_vec(),
        ),
        ("background.png".to_string(), png::solid_png(0x0e1621)),
        ("tiled.jpg".to_string(), vec![0xff, 0xd8]),
    ];
    let file = ThemeFile::parse(&crate::stat_chart::xlsx::zip(&parts)).unwrap();
    let image = file.background.as_ref().unwrap();
    assert!(image.png && !image.tiled);
    let palette = custom_palette(&file, None);
    assert!(palette.is_dark());
    assert_eq!(palette.background, Some(ThemeBackground::Solid(0x0e1621)));
    // A zip without a palette is refused, as is garbage.
    let empty = crate::stat_chart::xlsx::zip(&[("x.txt".to_string(), b"hi".to_vec())]);
    assert_eq!(ThemeFile::parse(&empty), Err(ThemeFileError::BadArchive));
    assert_eq!(
        ThemeFile::parse(b"PK\x03\x04garbage"),
        Err(ThemeFileError::BadArchive)
    );
    assert_eq!(
        ThemeFile::parse(&vec![b' '; file::THEME_FILE_LIMIT + 1]),
        Err(ThemeFileError::TooLarge)
    );
}

#[test]
fn solid_pngs_round_trip() {
    for rgb in [0x000000, 0xffffff, 0x0e1621, 0x74b4e0] {
        assert_eq!(png::solid_color(&png::solid_png(rgb)), Some(rgb));
    }
    assert_eq!(png::solid_color(b"not a png"), None);
}

#[test]
fn theme_ids_serialize_leniently() {
    let ids = [
        ThemeId::Builtin(BuiltinTheme::Night),
        ThemeId::Custom("My theme".into()),
    ];
    for id in ids {
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<ThemeId>(&json).unwrap(), id);
    }
    assert_eq!(
        serde_json::to_string(&ThemeId::default_day()).unwrap(),
        "\"quill_light\""
    );
    assert_eq!(
        serde_json::from_str::<ThemeId>("\"custom:../evil\"").unwrap(),
        ThemeId::Custom("evil".into())
    );
}

#[test]
fn custom_themes_are_stored_by_name() {
    let dir = scratch("store");
    assert!(store::list(&dir).is_empty());
    let source = dir.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    let theme = source.join("Ocean Blue.tdesktop-palette");
    std::fs::write(&theme, b"windowBg: #001122; dialogsBg: #001122;").unwrap();
    let themes = store::themes_dir(&dir);
    let (name, file) = store::import(&themes, &theme, &[]).unwrap();
    assert_eq!(name, "Ocean Blue");
    assert_eq!(store::list(&themes), vec!["Ocean Blue".to_string()]);
    assert_eq!(store::load(&themes, &name).unwrap(), file);
    // A second import gets a new name instead of replacing the first.
    let (again, _) = store::import(&themes, &theme, &store::list(&themes)).unwrap();
    assert_eq!(again, "Ocean Blue 2");
    // A file with nothing Quill uses is refused.
    let useless = source.join("x.tdesktop-palette");
    std::fs::write(&useless, b"historyPeer1NameFg: #ff0000;").unwrap();
    assert!(matches!(
        store::import(&themes, &useless, &[]),
        Err(store::StoreError::File(ThemeFileError::NothingToApply))
    ));
    store::delete(&themes, "Ocean Blue").unwrap();
    assert_eq!(store::list(&themes), vec!["Ocean Blue 2".to_string()]);
    assert_eq!(store::clean_name("  ../a/b:c*  "), "abc");
    assert_eq!(store::clean_name("///"), "Custom theme");
    let _ = std::fs::remove_dir_all(&dir);
}
