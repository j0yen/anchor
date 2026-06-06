//! AC2 (MUST): RootStatus, WatchRoot, WatchState, ReconcileAction, ReconcilePlan
//! are public and serde-(de)serializable; a round-trip test covers each.

use anchor::{
    ReconcileAction, ReconcileEntry, ReconcilePlan, RootStatus, WatchRoot, WatchState,
};
use std::path::PathBuf;

fn assert_roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) {
    let json = serde_json::to_string(value).expect("serialize failed");
    let back: T = serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(value, &back, "round-trip mismatch for JSON: {json}");
}

#[test]
fn test_serde_roundtrip_all_types() {
    // WatchRoot
    assert_roundtrip(&WatchRoot {
        path: PathBuf::from("/home/jsy/.claude"),
        max_age_secs: Some(86400),
    });
    assert_roundtrip(&WatchRoot {
        path: PathBuf::from("/home/jsy/brain"),
        max_age_secs: None,
    });

    // WatchState
    assert_roundtrip(&WatchState {
        path: PathBuf::from("/home/jsy/wintermute"),
        clock: Some("c:1780493700:1234:0".to_owned()),
        present: true,
    });
    assert_roundtrip(&WatchState {
        path: PathBuf::from("/tmp/missing"),
        clock: None,
        present: false,
    });

    // RootStatus variants
    assert_roundtrip(&RootStatus::Watched);
    assert_roundtrip(&RootStatus::Missing);
    assert_roundtrip(&RootStatus::Stale { age_secs: 99000 });
    assert_roundtrip(&RootStatus::Undeclared);

    // ReconcileAction variants
    assert_roundtrip(&ReconcileAction::Watch {
        path: PathBuf::from("/a"),
    });
    assert_roundtrip(&ReconcileAction::ReseedCursor {
        path: PathBuf::from("/b"),
    });
    assert_roundtrip(&ReconcileAction::NoOp {
        path: PathBuf::from("/c"),
    });
    assert_roundtrip(&ReconcileAction::NoteUndeclared {
        path: PathBuf::from("/d"),
    });

    // ReconcileEntry
    assert_roundtrip(&ReconcileEntry {
        path: PathBuf::from("/home/jsy/.claude"),
        status: RootStatus::Watched,
        action: ReconcileAction::NoOp {
            path: PathBuf::from("/home/jsy/.claude"),
        },
    });

    // ReconcilePlan
    assert_roundtrip(&ReconcilePlan {
        actions: vec![ReconcileEntry {
            path: PathBuf::from("/home/jsy/.claude"),
            status: RootStatus::Watched,
            action: ReconcileAction::NoOp {
                path: PathBuf::from("/home/jsy/.claude"),
            },
        }],
        summary: "1 declared (0 missing, 1 ok/stale), 0 undeclared live".to_owned(),
        missing_count: 0,
    });
}
