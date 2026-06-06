//! Property-based invariant tests for anchor.
//!
//! Read-only after scaffold. The edit-agent must NOT modify proptests.

use anchor::{WatchRoot, WatchState, reconcile};
use proptest::prelude::*;
use std::path::PathBuf;

proptest! {
    /// Invariant: missing_count is always <= declared.len()
    #[test]
    fn missing_count_never_exceeds_declared(
        n_declared in 0usize..10,
        n_live in 0usize..10,
        now in 0i64..1_000_000i64,
    ) {
        let declared: Vec<WatchRoot> = (0..n_declared).map(|i| WatchRoot {
            path: PathBuf::from(format!("/declared/{i}")),
            max_age_secs: None,
        }).collect();

        let live: Vec<WatchState> = (0..n_live).map(|i| WatchState {
            path: PathBuf::from(format!("/live/{i}")),
            clock: None,
            present: true,
        }).collect();

        let plan = reconcile(&declared, &live, now);
        prop_assert!(plan.missing_count <= n_declared,
            "missing_count ({}) must not exceed declared count ({})",
            plan.missing_count, n_declared);
    }

    /// Invariant: plan.actions.len() >= declared.len() (undeclared are appended)
    #[test]
    fn actions_at_least_declared(
        n_declared in 0usize..8,
        n_live in 0usize..8,
    ) {
        let declared: Vec<WatchRoot> = (0..n_declared).map(|i| WatchRoot {
            path: PathBuf::from(format!("/declared/{i}")),
            max_age_secs: None,
        }).collect();

        // live roots all use different paths so they are undeclared
        let live: Vec<WatchState> = (100..(100 + n_live)).map(|i| WatchState {
            path: PathBuf::from(format!("/live/{i}")),
            clock: None,
            present: true,
        }).collect();

        let plan = reconcile(&declared, &live, 0);
        prop_assert!(plan.actions.len() >= n_declared,
            "actions ({}) must be >= declared ({})",
            plan.actions.len(), n_declared);
    }
}
