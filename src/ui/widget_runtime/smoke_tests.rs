//! Boots real widgets from widget-sandbox's compatibility corpus through
//! the sandbox, the renderer and Quill's painter, on the test thread: no
//! timers, it waits for the runtime's own "rendered" (or "dead") report.
use super::backend::Resources;
use super::ffi::{Native, Shared};
use super::thread::{Canvas, paint_frame};
use serde_json::{Value, json};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

const BACKGROUND: [u8; 3] = [255, 255, 255];

fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("third_party/widgets/sandbox/corpus/compat")
}

fn corpus(name: &str) -> String {
    let path = corpus_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The widget's first whole frame, straight RGBA at 2× of 320 CSS px.
fn first_frame(name: &str) -> (u32, u32, Vec<u8>) {
    boot(name, &corpus(name))
}

/// Boots `html` and paints its first whole frame.
fn boot(name: &str, html: &str) -> (u32, u32, Vec<u8>) {
    let (tx, rx) = channel::<String>();
    let resources = Arc::new(Mutex::new(Resources::default()));
    let shared = Shared {
        events: Box::new(move |json| {
            let _ = tx.send(json);
        }),
        resources: resources.clone(),
    };
    let options = json!({
        "width": 320, "dpr": 2, "maxHeight": 600, "visible": true, "locale": "en",
        "theme": {"colorScheme": "light", "params": {"bg_color": "#ffffff", "text_color": "#000000",
                  "button_color": "#3390ec", "button_text_color": "#ffffff", "hint_color": "#707579"}},
        "context": {"isCopy": false, "canQuery": false, "backgroundQuery": false, "chatType": "private"},
    });
    let mut native = Native::create(html, &options.to_string(), shared).unwrap();
    loop {
        let event: Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        if event["type"] == "change" {
            assert!(event["dead"].is_null(), "{name} stopped: {}", event["dead"]);
            if event["rendered"] == true {
                break;
            }
        }
    }
    let mut canvas = Canvas::new(BACKGROUND);
    let frame = paint_frame(&mut native, &resources, &mut canvas).expect("a frame");
    let pixmap = canvas.pixmap.take().unwrap();
    assert_eq!(pixmap.width() as f32, frame.width * 2.0);
    (pixmap.width(), pixmap.height(), pixmap.data().to_vec())
}

/// Pixels that are not the bubble.
fn drawn(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[..3] != BACKGROUND)
        .count()
}

#[test]
fn tabs_boot_and_draw_text_and_boxes() {
    let (width, height, rgba) = first_frame("tabs.html");
    assert_eq!(width, 640);
    assert!(height >= 100, "height {height}");
    let painted = drawn(&rgba);
    assert!(painted > 2_000, "only {painted} pixels drawn");
}

#[test]
fn a_canvas_chart_draws_its_bars() {
    let (_, _, rgba) = first_frame("canvas-bar-chart.html");
    // The bars are a saturated color; the bubble and text are not.
    let colored = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| {
            let (max, min) = (
                px[..3].iter().max().copied().unwrap_or(0),
                px[..3].iter().min().copied().unwrap_or(0),
            );
            max.saturating_sub(min) > 80
        })
        .count();
    assert!(colored > 1_000, "only {colored} colored pixels");
}

/// Every widget of the compatibility corpus boots, renders and paints
/// without stopping, on every OS (Windows runs this in CI). WebGL and
/// three.js are not built (docs/decisions/message-widgets.md).
#[test]
fn every_corpus_widget_boots_and_paints() {
    let mut names: Vec<String> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".html") && name != "webgl.html" && name != "three-js.html")
        .collect();
    names.sort();
    assert!(names.len() >= 15, "corpus: {names:?}");
    for name in names {
        eprintln!("booting {name}");
        let (width, height, _) = first_frame(&name);
        assert_eq!(width, 640, "{name}");
        assert!(height > 0, "{name}");
    }
}

/// A widget that formats numbers, dates, plurals, relative times, lists,
/// collation and case through `Intl` and the `toLocale*` methods: on
/// Windows this is the system ICU (`icu_intl.cpp`), elsewhere the basic
/// English formatter.
#[test]
fn an_intl_widget_formats_through_the_host() {
    let html = r#"<!DOCTYPE html><html><body><div id="out"></div><script>
var d = new Date(Date.UTC(2026, 10, 10, 15, 4, 5));
var calls = [
  function () { return (1234567.891).toLocaleString(); },
  function () { return (1234.5).toLocaleString('de-DE'); },
  function () { return new Intl.NumberFormat('en-US', {style: 'currency', currency: 'USD'}).format(42.5); },
  function () { return new Intl.NumberFormat('en-US', {style: 'currency', currency: 'eur', currencyDisplay: 'name'}).format(2); },
  function () { return new Intl.NumberFormat('en-US', {style: 'percent', signDisplay: 'always'}).format(0.256); },
  function () { return new Intl.NumberFormat('en', {maximumSignificantDigits: 3}).format(98765); },
  function () { return new Intl.NumberFormat('en', {minimumFractionDigits: 2}).formatToParts(-1234.5).map(function (p) { return p.type; }).join(','); },
  function () { return JSON.stringify(new Intl.NumberFormat('ja-JP', {style: 'currency', currency: 'JPY'}).resolvedOptions()); },
  function () { return d.toLocaleDateString('en-US', {weekday: 'long', month: 'long', day: 'numeric'}); },
  function () { return d.toLocaleTimeString('en-US', {hour: '2-digit', minute: '2-digit', hour12: false, timeZone: 'UTC'}); },
  function () { return d.toLocaleString(); },
  function () { return d.toLocaleDateString(); },
  function () { return d.toLocaleTimeString(); },
  function () { return new Intl.DateTimeFormat('fr-FR', {dateStyle: 'full', timeStyle: 'short', timeZone: 'Europe/Paris'}).format(d); },
  function () { return new Intl.DateTimeFormat('en-GB', {month: 'short', year: '2-digit', era: 'short', timeZoneName: 'short'}).formatToParts(d).length; },
  function () { return JSON.stringify(new Intl.DateTimeFormat().resolvedOptions()); },
  function () { return new Intl.DateTimeFormat([], {hour: 'numeric', fractionalSecondDigits: 3}).format(d); },
  function () { return new Intl.PluralRules('en-US').select(1); },
  function () { return new Intl.PluralRules('en-US', {type: 'ordinal'}).select(22); },
  function () { return new Intl.PluralRules('en-US', {minimumFractionDigits: 1}).select(1); },
  function () { return new Intl.RelativeTimeFormat('en', {numeric: 'auto'}).format(-1, 'day'); },
  function () { return new Intl.RelativeTimeFormat('es', {style: 'short'}).format(3, 'hours'); },
  function () { return new Intl.ListFormat('en', {type: 'disjunction'}).format(['a', 'b', 'c']); },
  function () { return new Intl.ListFormat('en').format(['a', 'b']); },
  function () { return new Intl.ListFormat('en', {style: 'narrow', type: 'unit'}).format([]); },
  function () { return new Intl.Collator('de', {sensitivity: 'base', numeric: true}).compare('a10', 'A9'); },
  function () { return ['b', 'a', 'C'].sort(new Intl.Collator('en', {sensitivity: 'case'}).compare).join(''); },
  function () { return 'i'.toLocaleUpperCase('tr-TR'); },
  function () { return 'STRASSE'.toLocaleLowerCase(); },
  function () { return (NaN).toLocaleString(); },
  function () { return (-0).toLocaleString(); },
  function () { return (1e21).toLocaleString(); },
  function () { return new Intl.NumberFormat('xx-invalid-tag-zz').format(1); },
];
var lines = calls.map(function (f) { try { return String(f()); } catch (e) { return e.name; } });
document.getElementById('out').textContent = lines.join(' | ');
</script></body></html>"#;
    let (_, height, rgba) = boot("intl", html);
    assert!(height > 0);
    assert!(drawn(&rgba) > 1_000, "the formatted text did not draw");
}
