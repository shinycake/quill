use super::*;

const FLOORS: DiskFloors = DiskFloors {
    warn: 1000,
    warn_clear: 1200,
    close: 100,
    open: 200,
};

const NONE: DiskClients = DiskClients {
    running: false,
    retry_starts: false,
};
const RUNNING: DiskClients = DiskClients {
    running: true,
    ..NONE
};
const RETRY: DiskClients = DiskClients {
    retry_starts: true,
    ..NONE
};
const RUNNING_AND_RETRY: DiskClients = DiskClients {
    running: true,
    retry_starts: true,
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
fn going_full_closes_clients_and_leaving_full_reconnects() {
    use DiskState::{Full, Healthy, Low};
    for before in [Healthy, Low] {
        assert_eq!(before.step(Full, NONE), DiskStep::Close);
        assert_eq!(before.step(Full, RUNNING_AND_RETRY), DiskStep::Close);
    }
    assert_eq!(Full.step(Full, RETRY), DiskStep::Hold, "all closed");
    assert_eq!(Full.step(Full, RUNNING), DiskStep::Close, "one opened");
    assert_eq!(Full.step(Low, NONE), DiskStep::Reconnect);
    assert_eq!(Full.step(Healthy, RETRY), DiskStep::Reconnect);
}

#[test]
fn a_refused_start_goes_again_only_outside_full() {
    use DiskState::{Healthy, Low};
    assert_eq!(Low.step(Low, RUNNING_AND_RETRY), DiskStep::RetryStarts);
    assert_eq!(Low.step(Healthy, RUNNING_AND_RETRY), DiskStep::RetryStarts);
    assert_eq!(Low.step(Low, RUNNING), DiskStep::Hold);
    assert_eq!(Healthy.step(Healthy, RUNNING), DiskStep::Hold);
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
    assert_eq!(next_read_in(Some(u64::MAX), &floors, 0), LONGEST_READ);
    assert_eq!(next_read_in(None, &floors, 0), LONGEST_READ);
    assert_eq!(
        next_read_in(Some(floors.close + 10 * FASTEST_WRITER_PER_SEC), &floors, 0),
        Duration::from_secs(10)
    );
    assert_eq!(next_read_in(Some(floors.close), &floors, 0), SHORTEST_READ);
    assert_eq!(next_read_in(Some(0), &floors, 0), SHORTEST_READ);
}

#[test]
fn a_disk_falling_faster_than_assumed_is_read_sooner() {
    let floors = super::FLOORS;
    let free = Some(floors.close + 10 * FASTEST_WRITER_PER_SEC);
    let fast = 4 * FASTEST_WRITER_PER_SEC;
    assert_eq!(
        next_read_in(free, &floors, fast),
        Duration::from_millis(2500),
        "a copy four times the assumed writer"
    );
    assert_eq!(
        next_read_in(Some(floors.close), &floors, fast),
        super::QUICKEST_READ
    );
    assert_eq!(
        next_read_in(free, &floors, FASTEST_WRITER_PER_SEC / 2),
        Duration::from_secs(10),
        "a slow fall never slows the reads"
    );
}

#[test]
fn the_fall_is_measured_between_two_readings() {
    let secs = Duration::from_secs;
    assert_eq!(fall_per_sec(Some(1000), Some(400), secs(2)), 300);
    assert_eq!(fall_per_sec(Some(400), Some(1000), secs(2)), 0, "it grew");
    assert_eq!(fall_per_sec(None, Some(1), secs(2)), 0);
    assert_eq!(fall_per_sec(Some(5), Some(1), Duration::ZERO), 4000);
}

#[test]
fn the_floors_keep_their_order() {
    let gib = 1 << 30;
    let big = super::DataUse {
        accounts: 6,
        largest_database: 3 * gib / 2,
    };
    for floors in [super::FLOORS, DiskFloors::for_data(big)] {
        assert!(floors.close < floors.open, "{floors:?}");
        assert!(floors.open < floors.warn, "{floors:?}");
        assert!(floors.warn < floors.warn_clear, "{floors:?}");
    }
}

#[test]
fn a_disk_that_would_refuse_the_next_open_shows_the_bar_first() {
    let gib: u64 = 1 << 30;
    let floors = DiskFloors::for_data(super::DataUse {
        accounts: 3,
        largest_database: 3 * gib / 2,
    });
    let free = Some(floors.open - 1);
    assert!(refuses_open(free, &floors));
    assert_eq!(DiskState::Healthy.next(free, &floors), DiskState::Low);
    assert_eq!(DiskFloors::for_data(super::DataUse::ONE).warn, gib);
}

#[cfg(unix)]
#[test]
fn a_symlinked_files_folder_is_read_where_it_really_lives() {
    use std::path::{Path, PathBuf};
    let root = crate::scratch::unique_dir("quill-disk-folders");
    let elsewhere = crate::scratch::unique_dir("quill-disk-elsewhere");
    std::fs::create_dir_all(root.join("accounts/primary/tdlib")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("accounts/primary/files")).unwrap();
    let folders = super::folders::data_folders(&root);
    let elsewhere = std::fs::canonicalize(&elsewhere).unwrap();
    assert!(folders.contains(&elsewhere), "{folders:?}");
    assert!(
        !folders.iter().any(|folder| folder.ends_with("files")),
        "the link itself is not read: {folders:?}"
    );
    let read = |folder: &Path| Volume::Free(if folder == elsewhere { 10 } else { 5000 });
    assert_eq!(super::folders::tightest(&folders, read).free, Some(10));
    let none: &[PathBuf] = &[];
    assert_eq!(super::folders::tightest(none, read).free, None);
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[test]
fn a_missing_data_root_still_gets_a_reading() {
    let root = std::env::temp_dir().join("quill-no-such-root/at/all");
    assert!(data_free_bytes(&root).is_some());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::super::linux::{Statvfs, room_under, source_in, zero_needs_a_write};
    use std::path::{Path, PathBuf};

    fn quota(soft: u64, hard: u64, used: u64) -> libc::dqblk {
        // SAFETY: an all-zero dqblk is valid.
        let mut quota: libc::dqblk = unsafe { std::mem::zeroed() };
        quota.dqb_bsoftlimit = soft;
        quota.dqb_bhardlimit = hard;
        quota.dqb_curspace = used;
        quota.dqb_valid = libc::QIF_BLIMITS | libc::QIF_SPACE;
        quota
    }

    #[test]
    fn a_user_quota_counts_as_the_free_space() {
        assert_eq!(room_under(&quota(0, 1024, 1024 * 1000)), Some(24 * 1024));
        assert_eq!(
            room_under(&quota(512, 1024, 0)),
            Some(512 * 1024),
            "soft first"
        );
        assert_eq!(room_under(&quota(10, 0, 1 << 30)), Some(0), "over it");
        assert_eq!(room_under(&quota(0, 0, 5)), None, "no limit");
        let mut unset = quota(0, 1024, 0);
        unset.dqb_valid = 0;
        assert_eq!(room_under(&unset), None);
    }

    #[test]
    fn a_zero_on_btrfs_or_as_root_is_tried_with_a_write() {
        let zero = |btrfs, free_to_root| Statvfs {
            available: 0,
            free_to_root,
            btrfs,
        };
        assert!(zero_needs_a_write(&zero(true, 0), false), "btrfs metadata");
        assert!(zero_needs_a_write(&zero(false, 4096), true), "root reserve");
        assert!(!zero_needs_a_write(&zero(false, 4096), false), "not root");
        assert!(!zero_needs_a_write(&zero(false, 0), true), "really full");
        let some = Statvfs {
            available: 1,
            free_to_root: 1,
            btrfs: true,
        };
        assert!(!zero_needs_a_write(&some, true));
    }

    #[test]
    fn the_quota_device_is_the_longest_mount_holding_the_path() {
        let mountinfo = "\
22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw
40 22 8:17 / /home rw,relatime shared:2 - ext4 /dev/sdb1 rw,usrquota
41 22 8:33 / /mnt/my\\040disk rw - xfs /dev/sdc1 rw";
        let source = |path: &str| source_in(mountinfo, Path::new(path));
        assert_eq!(source("/home/me/.local"), Some(PathBuf::from("/dev/sdb1")));
        assert_eq!(source("/var/lib"), Some(PathBuf::from("/dev/sda1")));
        assert_eq!(source("/mnt/my disk/q"), Some(PathBuf::from("/dev/sdc1")));
        assert_eq!(source_in("garbage", Path::new("/")), None);
    }

    #[test]
    fn a_real_folder_reads_on_linux() {
        assert!(
            super::super::linux::read_available(&std::env::temp_dir())
                .free()
                .is_some()
        );
    }

    #[test]
    fn a_write_that_fits_is_no_full_disk_and_leaves_nothing() {
        let folder = crate::scratch::unique_dir("quill-disk-write");
        assert!(!super::super::linux::small_write_fails(&folder));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&folder);
    }
}

#[test]
fn the_close_floor_grows_by_each_account_that_closes() {
    use super::{DataUse, DiskFloors, MIB};
    let one = DiskFloors::for_data(DataUse::ONE);
    let six = DiskFloors::for_data(DataUse {
        accounts: 6,
        ..DataUse::ONE
    });
    assert_eq!(six.close - one.close, 5 * 14 * MIB);
    assert_eq!(six.open - six.close, one.open - one.close);
    assert_eq!(
        DiskFloors::for_data(DataUse {
            accounts: 0,
            ..DataUse::ONE
        }),
        one
    );
    assert_eq!(super::FLOORS, one);
}

#[test]
fn only_accounts_with_a_database_count() {
    let root = crate::scratch::unique_dir("quill-disk-accounts");
    let files: [(&str, &[(&str, usize)]); 3] = [
        ("a", &[("db.sqlite", 300), ("db.sqlite-wal", 50)]),
        ("b", &[("td.binlog", 1000)]),
        ("c", &[]),
    ];
    for (name, files) in files {
        let tdlib = root.join("accounts").join(name).join("tdlib");
        std::fs::create_dir_all(&tdlib).unwrap();
        for (file, size) in files {
            std::fs::write(tdlib.join(file), vec![0; *size]).unwrap();
        }
    }
    let data = super::folders::data_use(&root);
    assert_eq!(data.accounts, 2);
    assert_eq!(data.largest_database, 350, "the binlog is not migrated");
    assert_eq!(data_floors(&root), super::DiskFloors::for_data(data));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_open_floor_leaves_room_to_migrate_the_largest_database() {
    use super::{DataUse, DiskFloors, MIB};
    let small = DiskFloors::for_data(DataUse::ONE);
    let big = DiskFloors::for_data(DataUse {
        largest_database: 2048 * MIB,
        ..DataUse::ONE
    });
    assert_eq!(big.close, small.close);
    assert_eq!(big.open - small.open, 2048 * MIB);
}

#[test]
fn one_low_reading_waits_for_a_second_before_full() {
    use DiskState::{Full, Healthy, Low};
    assert_eq!(Healthy.confirmed(Some(0), &FLOORS, false), (Healthy, true));
    assert_eq!(Healthy.confirmed(Some(0), &FLOORS, true), (Full, false));
    assert_eq!(Low.confirmed(Some(99), &FLOORS, false), (Low, true));
    assert_eq!(Low.confirmed(Some(99), &FLOORS, true), (Full, false));
    assert_eq!(
        Low.confirmed(Some(500), &FLOORS, true),
        (Low, false),
        "the second read disagreed"
    );
    assert_eq!(
        Low.confirmed(None, &FLOORS, true),
        (Low, true),
        "no reading"
    );
    assert_eq!(Full.confirmed(Some(0), &FLOORS, false), (Full, false));
    assert_eq!(Full.confirmed(Some(250), &FLOORS, false), (Low, false));
}

#[test]
fn the_close_floor_covers_the_wait_for_the_second_reading() {
    let floors = super::FLOORS;
    let room =
        super::FASTEST_WRITER_PER_SEC * (SHORTEST_READ.as_secs() + super::CONFIRM_READ.as_secs());
    assert!(floors.close >= room + super::APFS_RESERVE + super::CLOSE_WRITES_PER_ACCOUNT);
}

#[test]
fn one_unwritable_folder_makes_the_reading_unwritable() {
    use std::path::{Path, PathBuf};
    let folders = [
        PathBuf::from("/a"),
        PathBuf::from("/b"),
        PathBuf::from("/c"),
    ];
    let read = |folder: &Path| match folder.to_str() {
        Some("/a") => Volume::Free(500),
        Some("/b") => Volume::NoSizes,
        _ => Volume::Unwritable,
    };
    assert_eq!(
        super::folders::tightest(&folders, read),
        DataReading {
            free: Some(500),
            unwritable: true
        }
    );
    let fine = |_: &Path| Volume::NoSizes;
    assert!(!super::folders::tightest(&folders, fine).unwritable);
}

#[cfg(unix)]
#[test]
fn a_root_whose_volume_went_away_is_unwritable_and_a_deleted_one_is_not() {
    let root = crate::scratch::unique_dir("quill-disk-gone");
    let device = data_root_device(&root);
    assert!(device.is_some());
    assert!(!read_data(&root, device).unwritable, "still there");
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        !read_data(&root, device).unwritable,
        "deleted on the same volume: the parent's reading stands"
    );
    let elsewhere = device.map(|device| device.wrapping_add(1));
    assert!(read_data(&root, elsewhere).unwritable, "the volume left");
    assert!(!read_data(&root, None).unwritable, "never seen");
}
