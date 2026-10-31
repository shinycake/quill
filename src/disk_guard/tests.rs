use super::*;

const FLOORS: DiskFloors = DiskFloors {
    warn: 1000,
    warn_clear: 1200,
    close: 100,
    open: 200,
};

fn walk(start: DiskState, reads: &[Option<u64>]) -> Vec<DiskState> {
    let mut state = start;
    reads
        .iter()
        .map(|free| {
            state = state.next(*free, &FLOORS);
            state
        })
        .collect()
}

#[test]
fn a_disk_filling_up_warns_then_closes() {
    assert_eq!(
        walk(
            DiskState::Healthy,
            &[Some(5000), Some(1000), Some(999), Some(100), Some(99)]
        ),
        [
            DiskState::Healthy,
            DiskState::Healthy,
            DiskState::Low,
            DiskState::Low,
            DiskState::Full
        ]
    );
}

#[test]
fn a_fast_fill_goes_straight_to_full() {
    assert_eq!(
        DiskState::Healthy.next(Some(0), &FLOORS),
        DiskState::Full,
        "no read landed in the warning range"
    );
}

#[test]
fn full_holds_until_the_open_floor_then_recovers() {
    assert_eq!(
        walk(
            DiskState::Full,
            &[Some(100), Some(199), Some(200), Some(1200)]
        ),
        [
            DiskState::Full,
            DiskState::Full,
            DiskState::Low,
            DiskState::Healthy
        ]
    );
}

#[test]
fn a_large_recovery_leaves_full_for_healthy_in_one_read() {
    assert_eq!(
        DiskState::Full.next(Some(9000), &FLOORS),
        DiskState::Healthy
    );
}

#[test]
fn the_bar_does_not_flicker_around_the_warning_floor() {
    assert_eq!(
        walk(
            DiskState::Low,
            &[Some(1000), Some(1100), Some(1199), Some(1200), Some(1100)]
        ),
        [
            DiskState::Low,
            DiskState::Low,
            DiskState::Low,
            DiskState::Healthy,
            DiskState::Healthy
        ]
    );
}

#[test]
fn a_read_that_says_nothing_never_changes_the_state() {
    for state in [DiskState::Healthy, DiskState::Low, DiskState::Full] {
        assert_eq!(state.next(None, &FLOORS), state);
    }
}

#[test]
fn opening_is_refused_only_on_a_read_below_the_open_floor() {
    assert!(refuses_open(Some(199), &FLOORS));
    assert!(!refuses_open(Some(200), &FLOORS));
    assert!(!refuses_open(None, &FLOORS), "the disk may be fine");
}

#[test]
fn only_a_read_below_the_close_floor_names_a_full_disk() {
    assert!(was_full(Some(99), &FLOORS));
    assert!(!was_full(Some(100), &FLOORS));
    assert!(!was_full(None, &FLOORS));
}

#[test]
fn reads_speed_up_as_the_disk_nears_the_close_floor() {
    let floors = super::FLOORS;
    assert_eq!(next_read_in(Some(u64::MAX), &floors), LONGEST_READ);
    assert_eq!(next_read_in(None, &floors), LONGEST_READ);
    assert_eq!(
        next_read_in(Some(floors.close + 10 * FASTEST_WRITER), &floors),
        Duration::from_secs(10)
    );
    assert_eq!(next_read_in(Some(floors.close), &floors), SHORTEST_READ);
    assert_eq!(next_read_in(Some(0), &floors), SHORTEST_READ);
}

#[test]
fn the_floors_keep_their_order() {
    let floors = super::FLOORS;
    assert!(floors.close < floors.open);
    assert!(floors.open < floors.warn);
    assert!(floors.warn < floors.warn_clear);
}
