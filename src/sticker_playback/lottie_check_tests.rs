use super::{Malformed, check};
use serde_json::{Value, json};

fn with_layers(layers: Value) -> Value {
    json!({"layers": layers})
}

fn with_shapes(shapes: Value) -> Value {
    with_layers(json!([{"ty": 4, "shapes": shapes}]))
}

fn with_assets(layers: Value, assets: Value) -> Value {
    json!({"layers": layers, "assets": assets})
}

fn precomp(id: &str) -> Value {
    json!({"ty": 0, "refId": id})
}

#[test]
fn a_plain_document_passes() {
    let doc = with_shapes(json!([{
        "ty": "gr",
        "it": [
            {"ty": "rc", "s": {"k": [10, 10]}},
            {"ty": "fl", "c": {"k": [1, 0, 0, 1]}},
            {"ty": "tr", "o": {"k": 100}},
        ],
    }]));
    assert_eq!(check(&doc), Ok(()));
}

#[test]
fn a_document_needs_a_layer_list() {
    assert_eq!(check(&json!({})), Err(Malformed::Layer));
    assert_eq!(check(&json!({"layers": 1})), Err(Malformed::Layer));
    assert_eq!(check(&with_layers(json!([1]))), Err(Malformed::Layer));
}

#[test]
fn numbers_past_the_limit_are_refused_only_in_animation_data() {
    let big = with_shapes(json!([{"ty": "rc", "s": {"k": [2_000_000, 1]}}]));
    assert_eq!(check(&big), Err(Malformed::Number));
    let at_the_limit = with_shapes(json!([{"ty": "rc", "s": {"k": [1_048_576, 1]}}]));
    assert_eq!(check(&at_the_limit), Ok(()));
    let elsewhere = json!({"layers": [], "meta": {"t": 1e30}, "op": 1e30});
    assert_eq!(check(&elsewhere), Ok(()));
}

#[test]
fn a_key_with_a_nul_is_refused() {
    let doc = with_layers(json!([{"a\u{0}": 1}]));
    assert_eq!(check(&doc), Err(Malformed::Key));
}

#[test]
fn a_null_in_an_array_is_refused() {
    let doc = with_shapes(json!([{"ty": "rc", "s": {"k": [1, null]}}]));
    assert_eq!(check(&doc), Err(Malformed::Array));
}

#[test]
fn a_property_array_holds_numbers_or_keyframes_never_both() {
    let mixed = with_shapes(json!([{"ty": "rc", "s": {"k": [1, {"t": 0}]}}]));
    assert_eq!(check(&mixed), Err(Malformed::Array));
    let keyframes = with_shapes(json!([{"ty": "rc", "s": {"k": [{"t": 0}, {"t": 1}]}}]));
    assert_eq!(check(&keyframes), Ok(()));
    let nested = with_shapes(json!([{"ty": "rc", "s": {"k": [[1]]}}]));
    assert_eq!(check(&nested), Err(Malformed::Array));
}

#[test]
fn a_layer_kind_is_a_small_whole_number() {
    for kind in [json!(16), json!(1.5), json!("4"), json!(-1), json!(null)] {
        let doc = with_layers(json!([{"ty": kind}]));
        assert_eq!(check(&doc), Err(Malformed::Layer), "{kind}");
    }
    assert_eq!(check(&with_layers(json!([{"nm": "no kind"}]))), Ok(()));
}

#[test]
fn a_precomp_layer_holds_nothing_of_its_own() {
    for own in [json!({"ty": 0, "shapes": []}), json!({"ty": 0, "t": {}})] {
        assert_eq!(check(&with_layers(json!([own]))), Err(Malformed::Layer));
    }
    assert_eq!(check(&with_layers(json!([{"ty": 4, "shapes": []}]))), Ok(()));
}

#[test]
fn layer_fields_have_their_kinds() {
    let shapes = with_layers(json!([{"ty": 4, "shapes": 1}]));
    assert_eq!(check(&shapes), Err(Malformed::Layer));
    let reference = with_layers(json!([{"ty": 0, "refId": 1}]));
    assert_eq!(check(&reference), Err(Malformed::Layer));
}

#[test]
fn masks_on_a_layer_are_limited() {
    let masks = vec![json!({}); 257];
    let doc = with_layers(json!([{"ty": 4, "masksProperties": masks}]));
    assert_eq!(check(&doc), Err(Malformed::TooComplex));
}

#[test]
fn layers_in_a_document_are_limited() {
    let layers = vec![json!({"ty": 3}); 8193];
    assert_eq!(check(&with_layers(json!(layers))), Err(Malformed::TooComplex));
    let layers = vec![json!({"ty": 3}); 8192];
    assert_eq!(check(&with_layers(json!(layers))), Ok(()));
}

#[test]
fn shapes_are_objects_with_a_text_kind() {
    assert_eq!(check(&with_shapes(json!([1]))), Err(Malformed::Shape));
    assert_eq!(check(&with_shapes(json!([{"ty": 1}]))), Err(Malformed::Shape));
    let group = with_shapes(json!([{"ty": "gr", "it": 1}]));
    assert_eq!(check(&group), Err(Malformed::Shape));
}

#[test]
fn counts_are_limited_directly_and_in_keyframes() {
    for shape in [
        json!({"ty": "sr", "pt": {"k": 1025}}),
        json!({"ty": "rp", "c": {"k": 1025}}),
        json!({"ty": "zz", "r": {"k": 1025}}),
        json!({"ty": "sr", "pt": {"k": [{"s": [1025]}, {"s": [3]}]}}),
    ] {
        let doc = with_shapes(json!([shape]));
        assert_eq!(check(&doc), Err(Malformed::TooComplex), "{doc}");
    }
    let fine = with_shapes(json!([{"ty": "sr", "pt": {"k": 5}}]));
    assert_eq!(check(&fine), Ok(()));
}

#[test]
fn gradient_stops_are_a_small_whole_number() {
    for stops in [json!(1025), json!(1.5), json!("3")] {
        let doc = with_shapes(json!([{"ty": "gf", "g": {"p": stops}}]));
        assert_eq!(check(&doc), Err(Malformed::Shape), "{stops}");
    }
    let doc = with_shapes(json!([{"ty": "gf", "g": {"p": 3}}]));
    assert_eq!(check(&doc), Ok(()));
}

#[test]
fn a_reference_to_nothing_is_fine() {
    assert_eq!(check(&with_layers(json!([precomp("gone")]))), Ok(()));
}

#[test]
fn asset_ids_are_text_or_whole_numbers() {
    for id in [json!(1.5), json!(true)] {
        let doc = with_assets(json!([]), json!([{"id": id, "layers": []}]));
        assert_eq!(check(&doc), Err(Malformed::Asset), "{id}");
    }
    let doc = with_assets(json!([]), json!([{"id": 7, "layers": []}]));
    assert_eq!(check(&doc), Ok(()));
}

#[test]
fn an_asset_without_layers_is_not_a_precomp() {
    let doc = with_assets(json!([precomp("a")]), json!([{"id": "a", "u": "", "p": ""}]));
    assert_eq!(check(&doc), Ok(()));
}

#[test]
fn a_precomp_cannot_reach_itself() {
    let direct = with_assets(
        json!([precomp("a")]),
        json!([{"id": "a", "layers": [precomp("a")]}]),
    );
    assert_eq!(check(&direct), Err(Malformed::Asset));
    let through_another = with_assets(
        json!([]),
        json!([
            {"id": "a", "layers": [precomp("b")]},
            {"id": "b", "layers": [precomp("a")]},
        ]),
    );
    assert_eq!(check(&through_another), Err(Malformed::Asset));
}

#[test]
fn a_reference_goes_where_the_renderer_would_send_it() {
    let doc = with_assets(
        json!([precomp("a\u{0}x")]),
        json!([
            {"id": "a", "layers": [precomp("a")]},
            {"id": "a\u{0}x", "layers": []},
        ]),
    );
    assert_eq!(check(&doc), Err(Malformed::Asset));
}

#[test]
fn a_number_id_matches_its_decimal_text() {
    let doc = with_assets(
        json!([precomp("7")]),
        json!([{"id": 7, "layers": [precomp("7")]}]),
    );
    assert_eq!(check(&doc), Err(Malformed::Asset));
}

#[test]
fn precomps_nest_only_so_deep() {
    let mut assets: Vec<Value> = (0..40)
        .map(|n| json!({"id": n.to_string(), "layers": [precomp(&(n + 1).to_string())]}))
        .collect();
    assets.push(json!({"id": "40", "layers": []}));
    let doc = with_assets(json!([precomp("0")]), json!(assets));
    assert_eq!(check(&doc), Err(Malformed::TooComplex));
}

#[test]
fn nesting_is_limited_in_whatever_order_the_layers_come() {
    let ids = 0..40;
    let assets: Vec<Value> = ids
        .clone()
        .map(|n| json!({"id": n.to_string(), "layers": [precomp(&(n + 1).to_string())]}))
        .collect();
    let deepest_first: Vec<Value> = ids.rev().map(|n| precomp(&n.to_string())).collect();
    let doc = with_assets(json!(deepest_first), json!(assets));
    assert_eq!(check(&doc), Err(Malformed::TooComplex));
}

#[test]
fn groups_nest_only_so_deep() {
    let mut shape = json!({"ty": "rc"});
    for _ in 0..70 {
        shape = json!({"ty": "gr", "it": [shape]});
    }
    assert_eq!(check(&with_shapes(json!([shape]))), Err(Malformed::TooComplex));
}

#[test]
fn shared_precomps_count_every_time_they_are_shown() {
    let levels = 12;
    let mut assets: Vec<Value> = (0..levels)
        .map(|n| {
            let next = (n + 1).to_string();
            let layers = vec![precomp(&next); 4];
            json!({"id": n.to_string(), "layers": layers})
        })
        .collect();
    assets.push(json!({"id": levels.to_string(), "layers": []}));
    let doc = with_assets(json!([precomp("0")]), json!(assets));
    assert_eq!(check(&doc), Err(Malformed::TooComplex));
}

#[test]
fn repeaters_in_a_list_multiply_what_they_copy() {
    let repeater = json!({"ty": "rp", "c": {"k": 1000}});
    let rect = json!({"ty": "rc"});
    let doc = with_shapes(json!([rect, repeater, repeater]));
    assert_eq!(check(&doc), Err(Malformed::TooComplex));
    let doc = with_shapes(json!([rect, repeater]));
    assert_eq!(check(&doc), Ok(()));
}

#[test]
fn every_paint_in_a_list_counts_against_its_paths() {
    let mut shapes = vec![json!({"ty": "st"}); 1100];
    shapes.extend(vec![json!({"ty": "rc"}); 1100]);
    assert_eq!(check(&with_shapes(json!(shapes))), Err(Malformed::TooComplex));
    let mut shapes = vec![json!({"ty": "st"}); 5];
    shapes.extend(vec![json!({"ty": "rc"}); 1100]);
    assert_eq!(check(&with_shapes(json!(shapes))), Ok(()));
}

#[test]
fn modifiers_in_a_list_multiply_the_paths_before_them() {
    let vertices = vec![json!([0, 0]); 64];
    let path = json!({"ty": "sh", "ks": {"k": {"v": vertices}}});
    let modifiers = |count| vec![json!({"ty": "rd", "r": {"k": 9}}); count];
    let mut shapes = vec![path.clone()];
    shapes.extend(modifiers(3));
    assert_eq!(check(&with_shapes(json!(shapes))), Ok(()));
    let mut shapes = vec![path];
    shapes.extend(modifiers(20));
    assert_eq!(check(&with_shapes(json!(shapes))), Err(Malformed::TooComplex));
}

#[test]
fn an_offset_multiplies_more_than_a_rounded_corner() {
    let stacked = |kind: &str| {
        let mut shapes = vec![json!({"ty": "rc"})];
        shapes.extend(vec![json!({"ty": kind}); 6]);
        with_shapes(json!(shapes))
    };
    assert_eq!(check(&stacked("rd")), Ok(()));
    assert_eq!(check(&stacked("op")), Err(Malformed::TooComplex));
}

#[test]
fn a_zigzag_multiplies_by_its_ridges() {
    let ridges = |count| json!({"ty": "zz", "r": {"k": count}});
    let rect = json!({"ty": "rc"});
    let fine = with_shapes(json!([rect, ridges(100)]));
    assert_eq!(check(&fine), Ok(()));
    let stacked = with_shapes(json!([rect, ridges(1000), ridges(1000)]));
    assert_eq!(check(&stacked), Err(Malformed::TooComplex));
}

#[test]
fn a_long_path_counts_by_its_vertices() {
    let path = |count| vec![json!([0, 0]); count];
    let copied = |path: Vec<Value>| {
        with_shapes(json!([
            {"ty": "sh", "ks": {"k": {"v": path}}},
            {"ty": "rp", "c": {"k": 1024}},
        ]))
    };
    assert_eq!(check(&copied(path(100))), Ok(()));
    assert_eq!(check(&copied(path(1000))), Err(Malformed::TooComplex));
    let keyframed = with_shapes(json!([
        {"ty": "sh", "ks": {"k": [{"s": [{"v": path(1000)}]}]}},
        {"ty": "rp", "c": {"k": 1024}},
    ]));
    assert_eq!(check(&keyframed), Err(Malformed::TooComplex));
}

#[test]
fn easing_counts_toward_a_repeat() {
    let eased = |y| {
        with_shapes(json!([{"ty": "sr", "pt": {"k": [
            {"s": [100], "e": [100], "i": {"y": [y]}, "o": {"y": [0]}},
            {"s": [100]},
        ]}}]))
    };
    assert_eq!(check(&eased(1)), Ok(()));
    assert_eq!(check(&eased(100)), Err(Malformed::TooComplex));
}

#[test]
fn layers_that_draw_offscreen_cost_more() {
    let plain = vec![json!({"ty": 3}); 4000];
    assert_eq!(check(&with_layers(json!(plain))), Ok(()));
    let matte = vec![json!({"ty": 3, "tt": 1}); 4000];
    assert_eq!(check(&with_layers(json!(matte))), Err(Malformed::TooComplex));
    let effect = vec![json!({"ty": 3, "ef": [{"ty": 29}]}); 4000];
    assert_eq!(check(&with_layers(json!(effect))), Err(Malformed::TooComplex));
    let masked = vec![json!({"ty": 3, "masksProperties": [{}]}); 4000];
    assert_eq!(check(&with_layers(json!(masked))), Err(Malformed::TooComplex));
    let slider = vec![json!({"ty": 3, "ef": [{"ty": 5}]}); 4000];
    assert_eq!(check(&with_layers(json!(slider))), Ok(()));
}

#[test]
fn a_dashed_stroke_costs_more_than_a_plain_one() {
    let shapes = |dashed: bool| {
        let stroke = if dashed {
            json!({"ty": "st", "d": []})
        } else {
            json!({"ty": "st"})
        };
        let mut shapes = vec![json!({"ty": "rc"}); 4000];
        shapes.push(stroke);
        with_shapes(json!(shapes))
    };
    assert_eq!(check(&shapes(false)), Ok(()));
    assert_eq!(check(&shapes(true)), Err(Malformed::TooComplex));
}

#[test]
fn text_counts_each_character_as_its_costliest_glyph() {
    let doc = |length| {
        let text = "a".repeat(length);
        let vertices = vec![json!([0, 0]); 100];
        json!({
            "layers": [{"ty": 5, "t": {"d": {"k": [{"s": {"t": text}, "t": 0}]}}}],
            "chars": [{"data": {"shapes": [
                {"ty": "sh", "ks": {"k": {"v": vertices}}},
            ]}}],
        })
    };
    assert_eq!(check(&doc(100)), Ok(()));
    assert_eq!(check(&doc(100_000)), Err(Malformed::TooComplex));
}
