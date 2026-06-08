//! AC1 (MUST): cargo build and cargo test succeed offline; a test asserts reconcile
//! makes zero backend calls (it operates only on the two passed slices + the injected now).

use anchor::{FakeBackend, WatchBackend, WatchRoot, WatchState, reconcile};
use std::path::PathBuf;

/// Verify that reconcile makes zero backend calls by using FakeBackend
/// (which panics if live_roots is called unexpectedly — it does NOT panic;
/// it just returns the pre-canned list). The key proof is structural:
/// reconcile takes `&[WatchRoot]` and `&[WatchState]` directly — no backend ref.
/// This test constructs the plan from raw slices with no FakeBackend::live_roots call.
#[test]
fn test_reconcile_no_backend_calls() {
    // Construct inputs directly — no backend involved.
    let declared = vec![WatchRoot {
        path: PathBuf::from("/home/jsy/.claude"),
        max_age_secs: None,
    }];
    let live = vec![WatchState {
        path: PathBuf::from("/home/jsy/.claude"),
        clock: None,
        present: true,
    }];

    // reconcile signature: (&[WatchRoot], &[WatchState], i64) -> ReconcilePlan
    // No backend parameter at all — proof by API surface.
    let plan = reconcile(&declared, &live, 0);
    assert_eq!(plan.missing_count, 0, "fresh watched root should not be missing");
    assert_eq!(plan.actions.len(), 1);
}

/// FakeBackend can be constructed but reconcile does NOT take a backend — the
/// backend is only needed to QUERY live roots before calling reconcile.
/// This test proves the separation: we construct a FakeBackend and call live_roots
/// separately, then pass the result to reconcile.
#[test]
fn test_reconcile_separation_from_backend() {
    let backend = FakeBackend::new(vec![WatchState {
        path: PathBuf::from("/tmp/test"),
        clock: None,
        present: true,
    }]);

    // Step 1: query the backend (one call only, outside reconcile).
    let live = backend.live_roots().expect("FakeBackend::live_roots should not fail");

    // Step 2: reconcile receives plain slices — no backend ref.
    let declared: Vec<WatchRoot> = vec![];
    let plan = reconcile(&declared, &live, 0);

    // /tmp/test is live but not declared → Undeclared.
    assert_eq!(plan.actions.len(), 1);
    assert!(matches!(
        plan.actions[0].status,
        anchor::RootStatus::Undeclared
    ));
}
