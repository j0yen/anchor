//! AC4 (MUST): reconcile classifies correctly against a FakeBackend snapshot:
//! - declared absent from live → Missing + Watch action
//! - watched with clock older than max_age_secs (injected now) → Stale + ReseedCursor
//! - live not in manifest → Undeclared + NoteUndeclared (never removal)
//! - fresh watched root → Watched + NoOp

use anchor::{ReconcileAction, RootStatus, WatchRoot, WatchState, reconcile};
use std::path::PathBuf;

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn test_reconcile_all_classifications() {
    // Declared roots
    let declared = vec![
        // Will be absent from live → Missing
        WatchRoot {
            path: path("/declared/missing"),
            max_age_secs: None,
        },
        // Will have stale clock → Stale
        WatchRoot {
            path: path("/declared/stale"),
            max_age_secs: Some(3600),
        },
        // Fresh clock → Watched
        WatchRoot {
            path: path("/declared/fresh"),
            max_age_secs: Some(3600),
        },
    ];

    // injected now = 100_000
    let now: i64 = 100_000;

    // Live roots:
    // /declared/missing → absent
    // /declared/stale → clock age = now - 900 = 99100 > 3600 → Stale
    // /declared/fresh → clock age = now - 99500 = 500 < 3600 → Watched
    // /live/undeclared → present but not declared → Undeclared
    let live = vec![
        WatchState {
            path: path("/declared/stale"),
            clock: Some("c:900:1:0".to_owned()), // secs=900, age=99100
            present: true,
        },
        WatchState {
            path: path("/declared/fresh"),
            clock: Some("c:99500:1:0".to_owned()), // secs=99500, age=500
            present: true,
        },
        WatchState {
            path: path("/live/undeclared"),
            clock: None,
            present: true,
        },
    ];

    let plan = reconcile(&declared, &live, now);

    // 3 declared + 1 undeclared = 4 entries
    assert_eq!(plan.actions.len(), 4, "expected 4 entries in plan");

    // Entry 0: /declared/missing → Missing + Watch
    let e0 = &plan.actions[0];
    assert_eq!(e0.path, path("/declared/missing"));
    assert!(
        matches!(e0.status, RootStatus::Missing),
        "expected Missing, got {:?}",
        e0.status
    );
    assert!(
        matches!(&e0.action, ReconcileAction::Watch { path: p } if *p == path("/declared/missing")),
        "expected Watch action, got {:?}",
        e0.action
    );

    // Entry 1: /declared/stale → Stale + ReseedCursor
    let e1 = &plan.actions[1];
    assert_eq!(e1.path, path("/declared/stale"));
    assert!(
        matches!(e1.status, RootStatus::Stale { .. }),
        "expected Stale, got {:?}",
        e1.status
    );
    assert!(
        matches!(&e1.action, ReconcileAction::ReseedCursor { path: p } if *p == path("/declared/stale")),
        "expected ReseedCursor, got {:?}",
        e1.action
    );

    // Entry 2: /declared/fresh → Watched + NoOp
    let e2 = &plan.actions[2];
    assert_eq!(e2.path, path("/declared/fresh"));
    assert!(
        matches!(e2.status, RootStatus::Watched),
        "expected Watched, got {:?}",
        e2.status
    );
    assert!(
        matches!(&e2.action, ReconcileAction::NoOp { .. }),
        "expected NoOp, got {:?}",
        e2.action
    );

    // Entry 3: /live/undeclared → Undeclared + NoteUndeclared (NOT Watch, NOT removal)
    let e3 = &plan.actions[3];
    assert_eq!(e3.path, path("/live/undeclared"));
    assert!(
        matches!(e3.status, RootStatus::Undeclared),
        "expected Undeclared, got {:?}",
        e3.status
    );
    assert!(
        matches!(&e3.action, ReconcileAction::NoteUndeclared { .. }),
        "expected NoteUndeclared, got {:?}",
        e3.action
    );

    // Only 1 missing (the absent declared root)
    assert_eq!(plan.missing_count, 1);
}
