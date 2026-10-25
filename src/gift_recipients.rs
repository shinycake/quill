//! "Gift Premium or Stars": who the gift recipient chooser lists, in
//! tdesktop's order (`ChooseStarGiftRecipient`, boxes/star_gift_box.cpp).
//! The account itself first ("buy yourself a gift"), then contacts with a
//! birthday yesterday, today or tomorrow (today first), then frequent
//! contacts (`CollectGiftFrequentUsers`), then the remaining contacts. A
//! search narrows everyone to one list.
#![deny(clippy::indexing_slicing)]

use crate::chatlist_suggestions::{CloseBirthday, day_after, day_before};

/// The heading above a group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    Birthdays,
    Frequent,
    Contacts,
}

/// The second line under a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    /// The account itself: "buy yourself a gift".
    Yourself,
    BirthdayToday,
    BirthdayYesterday,
    BirthdayTomorrow,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub user_id: i64,
    pub status: RowStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: Option<Heading>,
    pub rows: Vec<Row>,
}

/// What the chooser is built from, and today's date.
pub struct People<'a> {
    pub me: Option<i64>,
    pub birthdays: &'a [CloseBirthday],
    pub frequent: &'a [i64],
    pub contacts: &'a [i64],
    pub month: u8,
    pub day: u8,
}

/// The chooser's sections for `query` (empty: no search). `giftable` says
/// whether a user can get a gift (not a bot); `name` is the shown name.
pub fn sections(
    people: &People<'_>,
    query: &str,
    giftable: impl Fn(i64) -> bool,
    name: impl Fn(i64) -> String,
) -> Vec<Section> {
    let ok = |id: i64| Some(id) != people.me && giftable(id);
    let mut birthdays: Vec<&CloseBirthday> =
        people.birthdays.iter().filter(|b| ok(b.user_id)).collect();
    birthdays.sort_by_key(|b| (!is_today(b, people), b.month, b.day));
    let birthday_ids: Vec<i64> = birthdays.iter().map(|b| b.user_id).collect();
    let mut frequent: Vec<i64> = Vec::new();
    for id in people.frequent.iter().copied() {
        if ok(id) && !birthday_ids.contains(&id) && !frequent.contains(&id) {
            frequent.push(id);
        }
    }
    let mut contacts: Vec<(String, i64)> = people
        .contacts
        .iter()
        .copied()
        .filter(|id| ok(*id) && !birthday_ids.contains(id) && !frequent.contains(id))
        .map(|id| (name(id).to_lowercase(), id))
        .collect();
    contacts.sort();
    contacts.dedup_by_key(|(_, id)| *id);

    let query = query.trim().to_lowercase();
    if !query.is_empty() {
        let rows = birthdays
            .iter()
            .map(|b| b.user_id)
            .chain(frequent.iter().copied())
            .chain(contacts.iter().map(|(_, id)| *id))
            .filter(|id| name(*id).to_lowercase().contains(&query))
            .map(|user_id| Row {
                user_id,
                status: RowStatus::Plain,
            })
            .collect();
        return vec![Section {
            heading: None,
            rows,
        }];
    }

    let mut out = Vec::new();
    if let Some(me) = people.me {
        out.push(Section {
            heading: None,
            rows: vec![Row {
                user_id: me,
                status: RowStatus::Yourself,
            }],
        });
    }
    if !birthdays.is_empty() {
        out.push(Section {
            heading: Some(Heading::Birthdays),
            rows: birthdays
                .iter()
                .map(|b| Row {
                    user_id: b.user_id,
                    status: birthday_status(b, people),
                })
                .collect(),
        });
    }
    let plain = |user_id| Row {
        user_id,
        status: RowStatus::Plain,
    };
    if !frequent.is_empty() {
        out.push(Section {
            heading: Some(Heading::Frequent),
            rows: frequent.into_iter().map(plain).collect(),
        });
    }
    if !contacts.is_empty() {
        out.push(Section {
            heading: Some(Heading::Contacts),
            rows: contacts.into_iter().map(|(_, id)| plain(id)).collect(),
        });
    }
    out
}

fn is_today(b: &CloseBirthday, people: &People<'_>) -> bool {
    (b.month, b.day) == (people.month, people.day)
}

fn birthday_status(b: &CloseBirthday, people: &People<'_>) -> RowStatus {
    let date = (b.month, b.day);
    if is_today(b, people) {
        RowStatus::BirthdayToday
    } else if date == day_before(people.month, people.day) {
        RowStatus::BirthdayYesterday
    } else if date == day_after(people.month, people.day) {
        RowStatus::BirthdayTomorrow
    } else {
        RowStatus::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: i64 = 1;
    const BOT: i64 = 99;

    fn birthday(user_id: i64, month: u8, day: u8) -> CloseBirthday {
        CloseBirthday {
            user_id,
            day,
            month,
            year: None,
        }
    }

    fn name(id: i64) -> String {
        match id {
            2 => "Zoe".into(),
            3 => "Ada".into(),
            4 => "Max".into(),
            5 => "Bea".into(),
            6 => "Cy".into(),
            _ => format!("User {id}"),
        }
    }

    fn build(people: &People<'_>, query: &str) -> Vec<Section> {
        sections(people, query, |id| id != BOT, name)
    }

    fn ids(section: &Section) -> Vec<i64> {
        section.rows.iter().map(|row| row.user_id).collect()
    }

    #[test]
    fn yourself_birthdays_frequent_then_contacts() {
        let birthdays = [birthday(4, 3, 15), birthday(5, 3, 14), birthday(6, 3, 13)];
        let people = People {
            me: Some(ME),
            birthdays: &birthdays,
            frequent: &[ME, 5, 2, BOT],
            contacts: &[2, 3, 4, 5, BOT],
            month: 3,
            day: 14,
        };
        let out = build(&people, "");
        let headings: Vec<_> = out.iter().map(|s| s.heading).collect();
        assert_eq!(
            headings,
            [
                None,
                Some(Heading::Birthdays),
                Some(Heading::Frequent),
                Some(Heading::Contacts)
            ]
        );
        let yourself = out.first().expect("yourself");
        assert_eq!(
            yourself.rows,
            [Row {
                user_id: ME,
                status: RowStatus::Yourself
            }]
        );
        let birthdays = out.get(1).expect("birthdays");
        assert_eq!(ids(birthdays), [5, 6, 4], "today first, then by date");
        let statuses: Vec<_> = birthdays.rows.iter().map(|row| row.status).collect();
        assert_eq!(
            statuses,
            [
                RowStatus::BirthdayToday,
                RowStatus::BirthdayYesterday,
                RowStatus::BirthdayTomorrow
            ]
        );
        assert_eq!(
            ids(out.get(2).expect("frequent")),
            [2],
            "no self, bots or birthdays"
        );
        assert_eq!(ids(out.get(3).expect("contacts")), [3], "the rest, by name");
    }

    #[test]
    fn empty_groups_have_no_heading() {
        let people = People {
            me: Some(ME),
            birthdays: &[],
            frequent: &[],
            contacts: &[2, 3],
            month: 1,
            day: 1,
        };
        let out = build(&people, "");
        assert_eq!(out.len(), 2);
        let contacts = out.get(1).expect("contacts");
        assert_eq!(contacts.heading, Some(Heading::Contacts));
        assert_eq!(ids(contacts), [3, 2]);
    }

    #[test]
    fn a_search_lists_matching_people_once_without_yourself() {
        let birthdays = [birthday(5, 1, 1)];
        let people = People {
            me: Some(ME),
            birthdays: &birthdays,
            frequent: &[4],
            contacts: &[2, 3, 4, 5],
            month: 1,
            day: 1,
        };
        let out = build(&people, " a ");
        assert_eq!(out.len(), 1);
        let found = out.first().expect("results");
        assert_eq!(found.heading, None);
        assert_eq!(ids(found), [5, 4, 3]);
        assert!(found.rows.iter().all(|row| row.status == RowStatus::Plain));
    }
}
