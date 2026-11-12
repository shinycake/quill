//! Driver tests: hiding ads before the own full info is cached still
//! fetches the real one, whose answer carries the setting.
use super::paid_messages::{cleanup, extra_of, fixture, ingest, requests};
use crate::state::fetch::Fetch;

#[test]
fn hiding_ads_without_the_own_full_info_still_fetches_it() {
    let mut f = fixture();
    f.1.session.my_user_id = Some(5);
    f.1.session.payments.premium_option = Some(true);
    f.1.session.accept_sponsored_hidden(crate::ids::ChatId(3));
    // Hidden while the real info is on its way, but nothing is made up.
    assert!(f.1.session.sponsored_hidden());
    assert!(!matches!(f.1.session.business_own_info(), Fetch::Ready(())));

    f.1.pump_wanted_full_infos();
    let asks = requests(&f, "getUserFullInfo");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0]["user_id"], 5);
    assert!(f.1.session.sponsored_hidden());
    assert!(!matches!(f.1.session.business_own_info(), Fetch::Ready(())));
    // Pumping again does not ask twice.
    f.1.pump_wanted_full_infos();
    assert_eq!(requests(&f, "getUserFullInfo").len(), 1);

    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"hi"}},"has_sponsored_messages_enabled":false}}"#,
            extra_of(&asks[0])
        ),
    );
    assert!(f.1.session.sponsored_hidden());
    assert!(matches!(f.1.session.business_own_info(), Fetch::Ready(())));
    assert_eq!(
        f.1.session
            .users_state
            .user_full_infos
            .get(&5)
            .map(|info| info.bio.as_str()),
        Some("hi")
    );
    cleanup(f);
}
