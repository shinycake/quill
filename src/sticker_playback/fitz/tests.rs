use super::{FitzType, apply_fitzpatrick};
use serde_json::Value;

// Raw JSON, so the key order is the file's (ThorVG wants "ty" first in a
// shape, and Telegram's parser only recolours what follows "fitz").
const FITZ: &str = r#""fitz":[{"o":16711680,"f12":65280,"f3":255,"f4":8421504,"f5":1,"f6":2},{"o":8421504,"f3":16777215}]"#;

fn document(before: &str, after: &str) -> Value {
    serde_json::from_str(&format!(
        r#"{{"v":"5.5.2","fr":60,"ip":0,"op":60,"w":512,"h":512,{before}"layers":[{after}]}}"#
    ))
    .unwrap()
}

fn tone(json: &str, fitz: FitzType) -> (bool, Value) {
    let mut value = document(&format!("{FITZ},"), json);
    let changed = apply_fitzpatrick(&mut value, fitz);
    (changed, value)
}

fn rgb(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .map(|c| (c.as_f64().unwrap() * 255.0).round() as u8)
        .collect()
}

const RED_FILL: &str =
    r#"{"ty":4,"shapes":[{"ty":"fl","c":{"a":0,"k":[1,0,0,0.5]},"o":{"a":0,"k":100}}]}"#;

#[test]
fn a_static_fill_takes_the_tones_color_and_keeps_its_alpha() {
    let (changed, value) = tone(RED_FILL, FitzType::Type3);
    assert!(changed);
    let k = &value["layers"][0]["shapes"][0]["c"]["k"];
    assert_eq!(rgb(k), [0, 0, 255]);
    assert_eq!(k[3], 0.5);
    for (fitz, expected) in [
        (FitzType::Type12, [0, 255, 0]),
        (FitzType::Type4, [128, 128, 128]),
        (FitzType::Type5, [0, 0, 1]),
        (FitzType::Type6, [0, 0, 2]),
    ] {
        let (_, value) = tone(RED_FILL, fitz);
        assert_eq!(rgb(&value["layers"][0]["shapes"][0]["c"]["k"]), expected);
    }
}

#[test]
fn no_tone_changes_nothing() {
    let (changed, value) = tone(RED_FILL, FitzType::None);
    assert!(!changed);
    assert_eq!(value, document(&format!("{FITZ},"), RED_FILL));
}

#[test]
fn a_tone_missing_from_an_entry_is_black() {
    // The second entry has no "f12": Telegram's parser reads it as 0.
    let gray = r#"{"ty":4,"shapes":[{"ty":"st","c":{"a":0,"k":[0.5,0.5,0.5,1]}}]}"#;
    let (changed, value) = tone(gray, FitzType::Type12);
    assert!(changed);
    assert_eq!(rgb(&value["layers"][0]["shapes"][0]["c"]["k"]), [0, 0, 0]);
    // 0.5 rounds to 0x80 and matches the entry's 0x808080.
    let (_, value) = tone(gray, FitzType::Type3);
    assert_eq!(
        rgb(&value["layers"][0]["shapes"][0]["c"]["k"]),
        [255, 255, 255]
    );
}

#[test]
fn keyframed_strokes_in_nested_groups_change_at_both_ends() {
    let json = r#"{"ty":4,"shapes":[{"ty":"gr","it":[{"ty":"gr","it":[{"ty":"st","c":{"a":1,"k":[{"t":0,"s":[1,0,0,1],"e":[0.2,0.2,0.2,1]},{"t":30,"s":[0.2,0.2,0.2,1],"e":[1,0,0,1]},{"t":60}]}}]}]}]}"#;
    let (changed, value) = tone(json, FitzType::Type3);
    assert!(changed);
    let k = &value["layers"][0]["shapes"][0]["it"][0]["it"][0]["c"]["k"];
    assert_eq!(rgb(&k[0]["s"]), [0, 0, 255]);
    assert_eq!(rgb(&k[0]["e"]), [51, 51, 51]);
    assert_eq!(rgb(&k[1]["s"]), [51, 51, 51]);
    assert_eq!(rgb(&k[1]["e"]), [0, 0, 255]);
}

#[test]
fn gradients_keep_their_stops() {
    let json = r#"{"ty":4,"shapes":[{"ty":"gf","g":{"p":2,"k":{"a":0,"k":[0,1,0,0,1,1,0,0]}},"c":{"a":0,"k":[1,0,0,1]}}]}"#;
    let (changed, value) = tone(json, FitzType::Type3);
    assert!(!changed);
    assert_eq!(value, document(&format!("{FITZ},"), json));
}

#[test]
fn solid_layers_and_precomps_are_recoloured() {
    let mut value: Value = serde_json::from_str(&format!(
        r##"{{"v":"5.5.2",{FITZ},"assets":[{{"id":"comp_0","layers":[{{"ty":1,"sc":"#ff0000"}},{{"ty":1,"sc":"#FF0000"}},{{"ty":1,"sc":"red"}}]}}],"layers":[]}}"##
    ))
    .unwrap();
    assert!(apply_fitzpatrick(&mut value, FitzType::Type12));
    let layers = &value["assets"][0]["layers"];
    assert_eq!(layers[0]["sc"], "#00ff00");
    assert_eq!(layers[1]["sc"], "#00ff00");
    assert_eq!(layers[2]["sc"], "red");
}

#[test]
fn only_what_follows_the_table_is_recoloured() {
    let mut value = document("", RED_FILL);
    let object = value.as_object_mut().unwrap();
    let fitz: Value = serde_json::from_str(&format!("{{{FITZ}}}")).unwrap();
    object.insert("fitz".into(), fitz["fitz"].clone());
    assert!(!apply_fitzpatrick(&mut value, FitzType::Type3));
}

#[test]
fn recolouring_keeps_the_key_order() {
    let (_, value) = tone(RED_FILL, FitzType::Type3);
    let text = value.to_string();
    assert!(
        text.contains(r#"{"ty":"fl","c":{"a":0,"k":[0.0,0.0,1.0,0.5]},"o""#),
        "{text}"
    );
    assert!(text.starts_with(r#"{"v":"5.5.2","fr":60"#), "{text}");
}

#[test]
fn tdlib_types_map_like_telegram_x() {
    let mapped: Vec<FitzType> = (0..=7).map(FitzType::from_tdlib).collect();
    assert_eq!(
        mapped,
        [
            FitzType::None,
            FitzType::Type12,
            FitzType::Type12,
            FitzType::Type3,
            FitzType::Type4,
            FitzType::Type5,
            FitzType::Type6,
            FitzType::None,
        ]
    );
}
