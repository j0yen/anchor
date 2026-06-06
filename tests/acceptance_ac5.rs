//! AC5 (MUST): anchor plan --format json emits one entry per declared root plus
//! undeclared-live notes; the schema matches the documented ReconcilePlan.

use anchor::{ReconcilePlan, WatchRoot, WatchState, reconcile};
use std::path::PathBuf;

#[test]
fn test_plan_json_output_schema() {
    // Build a plan with 2 declared roots + 1 undeclared live root.
    let declared = vec![
        WatchRoot {
            path: PathBuf::from("/home/jsy/.claude"),
            max_age_secs: None,
        },
        WatchRoot {
            path: PathBuf::from("/home/jsy/brain"),
            max_age_secs: None,
        },
    ];

    let live = vec![
        WatchState {
            path: PathBuf::from("/home/jsy/.claude"),
            clock: None,
            present: true,
        },
        // brain is missing from live → Missing
        WatchState {
            path: PathBuf::from("/tmp/extra"),
            clock: None,
            present: true,
        },
    ];

    let plan = reconcile(&declared, &live, 0);

    // 2 declared + 1 undeclared = 3 entries
    assert_eq!(
        plan.actions.len(),
        3,
        "plan should have 3 entries (2 declared + 1 undeclared)"
    );
    assert_eq!(plan.missing_count, 1, "/home/jsy/brain is missing");

    // Serialize to JSON and verify schema fields.
    let json = serde_json::to_string_pretty(&plan).expect("serialize plan");
    let val: serde_json::Value = serde_json::from_str(&json).expect("re-parse plan JSON");

    assert!(val.get("actions").is_some(), "JSON must have 'actions' field");
    assert!(val.get("summary").is_some(), "JSON must have 'summary' field");
    assert!(
        val.get("missing_count").is_some(),
        "JSON must have 'missing_count' field"
    );

    let actions = val["actions"].as_array().expect("actions should be array");
    assert_eq!(actions.len(), 3);

    // Each entry must have path, status (with 'status' tag), action (with 'action' tag).
    for entry in actions {
        assert!(entry.get("path").is_some(), "entry missing 'path'");
        assert!(entry.get("status").is_some(), "entry missing 'status'");
        assert!(entry.get("action").is_some(), "entry missing 'action'");
    }

    // Deserialize back to ReconcilePlan to verify full round-trip.
    let plan_back: ReconcilePlan = serde_json::from_str(&json).expect("deserialize plan");
    assert_eq!(plan_back.missing_count, plan.missing_count);
    assert_eq!(plan_back.actions.len(), plan.actions.len());
}
