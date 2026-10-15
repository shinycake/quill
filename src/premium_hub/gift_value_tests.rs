use super::*;
use crate::ids::RequestId;
use serde_json::json;

const LAST: &str = include_str!("../../tests/fixtures/gift_value/value_info_last.json");
const AVERAGE: &str = include_str!("../../tests/fixtures/gift_value/value_info_average.json");

fn fixture(text: &str) -> GiftValueInfo {
    let value: Value = serde_json::from_str(text).expect("fixture json");
    parse_gift_value_info(&value).expect("value info")
}

#[test]
fn the_recorded_answer_is_read() {
    let info = fixture(LAST);
    assert_eq!(info.currency, "USD");
    assert_eq!(info.value, 248_750);
    assert!(!info.is_value_average);
    assert_eq!(info.initial_sale_star_count, 500);
    assert_eq!(info.initial_sale_price, 750);
    assert_eq!(info.last_sale_price, 248_750);
    assert!(info.is_last_sale_on_fragment);
    assert_eq!(info.telegram_listed_gift_count, 1_284);
    assert_eq!(info.fragment_listed_gift_count, 37);
    assert_eq!(info.fragment_url, "https://fragment.com/gifts/plushpepe");
    assert!(matches!(
        parse_payload(
            "upgradedGiftValueInfo",
            &serde_json::from_str(LAST).expect("json")
        ),
        Some(GiftValuePayload::Info(_))
    ));
}

#[test]
fn other_objects_and_broken_fields_are_safe() {
    assert_eq!(
        parse_gift_value_info(&json!({"@type": "upgradedGift"})),
        None
    );
    assert_eq!(parse_payload("ok", &json!({"@type": "ok"})), None);
    let info = parse_gift_value_info(&json!({
        "@type": "upgradedGiftValueInfo", "value": -5, "minimum_price": "12",
        "telegram_listed_gift_count": null, "currency": 7,
    }))
    .expect("info");
    assert_eq!(info.value, 0);
    assert_eq!(info.minimum_price, 12);
    assert_eq!(info.telegram_listed_gift_count, 0);
    assert_eq!(info.currency, "");
}

#[test]
fn the_request_carries_the_unique_name() {
    let request: Value = serde_json::from_str(&get_upgraded_gift_value_info(
        RequestId(9),
        "PlushPepe-1204",
    ))
    .expect("json");
    assert_eq!(request["@type"], "getUpgradedGiftValueInfo");
    assert_eq!(request["name"], "PlushPepe-1204");
    assert_eq!(request["@extra"], RequestId(9).as_extra());
}

#[test]
fn prices_are_formatted_like_tdesktop() {
    assert_eq!(format_value_price(248_750, "USD", false), "$2,487.50");
    assert_eq!(format_value_price(750, "USD", true), "~$7.50");
    assert_eq!(format_value_price(0, "USD", false), "$0.00");
    assert_eq!(format_value_price(-3, "USD", false), "$0.00");
    assert_eq!(format_value_price(1_500, "JPY", false), "JP¥1,500");
    assert_eq!(format_value_price(1_234, "KWD", false), "KWD1.234");
}

#[test]
fn dates_read_month_day_year() {
    // Noon UTC: the same calendar day in every common time zone.
    assert_eq!(format_value_date(1_735_819_200), "January 2, 2025");
    assert_eq!(format_value_date(1_758_110_400), "September 17, 2025");
}

#[test]
fn the_change_follows_tdesktops_rounding() {
    assert_eq!(change_percent(248_750, 750).as_deref(), Some("+33,067%"));
    assert_eq!(change_percent(1_500, 1_000).as_deref(), Some("+50%"));
    assert_eq!(change_percent(1_032, 1_000).as_deref(), Some("+3.2%"));
    assert_eq!(change_percent(10_325, 10_000).as_deref(), Some("+3.25%"));
    assert_eq!(change_percent(900, 1_000).as_deref(), Some("-10%"));
    assert_eq!(change_percent(1_000, 1_000).as_deref(), Some("0%"));
    assert_eq!(change_percent(1_000, 0), None);
}

#[test]
fn the_about_line_prefers_a_last_sale_above_the_average() {
    let last = fixture(LAST);
    assert_eq!(about_line(&last), AboutLine::LastSale { on_fragment: true });
    let (text, bold) = about_text(&last, "Plush Pepe", "Plush Pepe #1,204");
    assert_eq!(
        text,
        "This is the price at which Plush Pepe #1,204 was last sold on Fragment."
    );
    assert_eq!(bold, "Plush Pepe #1,204");

    let on_telegram = GiftValueInfo {
        is_last_sale_on_fragment: false,
        ..last.clone()
    };
    let (text, _) = about_text(&on_telegram, "Plush Pepe", "Plush Pepe #1,204");
    assert!(text.ends_with("last sold on Telegram."));

    // tdesktop ignores `is_value_average`: only the two prices decide.
    let below = GiftValueInfo {
        last_sale_price: last.average_sale_price,
        ..last
    };
    assert_eq!(about_line(&below), AboutLine::Average);

    let average = fixture(AVERAGE);
    assert_eq!(about_line(&average), AboutLine::Average);
    let (text, bold) = about_text(&average, "Plush Pepe", "Plush Pepe #1,204");
    assert_eq!(
        text,
        "This is the average sale price of Plush Pepe gifts on Telegram and Fragment over the past month."
    );
    assert_eq!(bold, "Plush Pepe");
}

#[test]
fn the_table_has_every_known_row() {
    let rows = table_rows(&fixture(LAST), "Plush Pepe");
    let kinds: Vec<ValueRowKind> = rows.iter().map(|row| row.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ValueRowKind::InitialSale,
            ValueRowKind::InitialPrice,
            ValueRowKind::LastSale,
            ValueRowKind::LastPrice,
            ValueRowKind::MinimumPrice,
            ValueRowKind::AveragePrice,
        ]
    );
    let row = |kind| rows.iter().find(|row| row.kind == kind).expect("row");
    assert_eq!(row(ValueRowKind::InitialSale).label, "Initial Sale");
    assert_eq!(row(ValueRowKind::InitialSale).value, "January 2, 2025");
    let initial = row(ValueRowKind::InitialPrice);
    assert_eq!(initial.value, "500 (~$7.50)");
    assert!(initial.stars_icon);
    assert_eq!(
        row(ValueRowKind::LastPrice).change.as_deref(),
        Some("+33,067%")
    );
    assert_eq!(row(ValueRowKind::MinimumPrice).value, "$1,823.00");
    assert_eq!(
        row(ValueRowKind::MinimumPrice).tooltip.as_deref(),
        Some("$1,823.00 is the floor price for Plush Pepe gifts listed on Telegram and Fragment.")
    );
    assert_eq!(row(ValueRowKind::AveragePrice).label, "Average Price");
    assert_eq!(
        row(ValueRowKind::AveragePrice).tooltip.as_deref(),
        Some(
            "$2,149.00 is the average sale price of Plush Pepe gifts on Telegram and Fragment over the past month."
        )
    );
}

#[test]
fn unknown_rows_are_left_out_but_the_initial_price_stays() {
    let rows = table_rows(&GiftValueInfo::default(), "Plush Pepe");
    let kinds: Vec<ValueRowKind> = rows.iter().map(|row| row.kind).collect();
    assert_eq!(kinds, vec![ValueRowKind::InitialPrice]);
    let average = table_rows(&fixture(AVERAGE), "Plush Pepe");
    assert!(
        average
            .iter()
            .all(|row| row.kind != ValueRowKind::LastSale && row.kind != ValueRowKind::LastPrice)
    );
}

#[test]
fn availability_lists_telegram_then_fragment() {
    let links = availability(&fixture(LAST));
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].platform, Platform::Telegram);
    assert_eq!(links[0].text, "1,284 {emoji} for sale on Telegram {arrow}");
    assert_eq!(links[1].platform, Platform::Fragment);
    assert_eq!(links[1].text, "37 {emoji} for sale on Fragment {arrow}");

    let one = availability(&fixture(AVERAGE));
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].text, "1 {emoji} for sale on Telegram {arrow}");
}

#[test]
fn only_an_https_fragment_link_is_offered() {
    let mut info = fixture(LAST);
    assert_eq!(
        fragment_link(&info),
        Some("https://fragment.com/gifts/plushpepe")
    );
    for bad in [
        "http://fragment.com/x",
        "javascript:alert(1)",
        "",
        "https://",
    ] {
        info.fragment_url = bad.into();
        assert_eq!(fragment_link(&info), None, "{bad}");
        assert!(
            availability(&info)
                .iter()
                .all(|link| link.platform != Platform::Fragment)
        );
    }
}

#[test]
fn stale_answers_and_failures_are_dropped() {
    let mut lookup = GiftValueLookup {
        name: "PlushPepe-1".into(),
        request: 7,
        loading: true,
        ..GiftValueLookup::default()
    };
    assert!(!lookup.land(6, fixture(LAST)));
    assert!(!lookup.fail(6, "old".into()));
    assert!(lookup.loading && lookup.info.is_none() && lookup.error.is_none());
    assert!(lookup.fail(7, "FLOOD_WAIT".into()));
    assert_eq!(lookup.error.as_deref(), Some("FLOOD_WAIT"));
    assert!(!lookup.open);
    // Not loading any more: a late answer for the same id is dropped too.
    assert!(!lookup.land(7, fixture(LAST)));

    lookup.request = 8;
    lookup.loading = true;
    assert!(lookup.land(8, fixture(LAST)));
    assert!(lookup.open && !lookup.loading && lookup.error.is_none());
}

#[test]
fn the_row_needs_an_estimate() {
    let gift = json!({"@type": "upgradedGift", "id": "1", "regular_gift_id": "7001",
        "title": "Plush Pepe", "name": "PlushPepe-1204", "number": 1204,
        "value_currency": "USD", "value_amount": 248_750, "value_usd_amount": 248_750});
    let collectible = super::super::upgraded::parse_collectible(&gift).expect("collectible");
    assert_eq!(row_value(&collectible).as_deref(), Some("~$2,487.50"));
    let lookup = GiftValueLookup::new(&collectible);
    assert_eq!(lookup.name, "PlushPepe-1204");
    assert_eq!(lookup.full_name, "Plush Pepe #1,204");
    assert_eq!(lookup.regular_gift_id, 7001);

    let mut none = collectible.clone();
    none.value_currency.clear();
    assert_eq!(row_value(&none), None);
    none.value_currency = "USD".into();
    none.value_amount = 0;
    assert_eq!(row_value(&none), None);
}
