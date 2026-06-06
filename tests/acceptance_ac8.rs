//! AC8 (MUST): README documents the manifest format, the type surface, and the
//! WatchBackend trait so anchor-probe / anchor-reconcile / anchor-boot have a
//! contract to extend.

use std::path::PathBuf;

#[test]
fn test_readme_documents_contract() {
    let readme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let readme = std::fs::read_to_string(&readme_path)
        .unwrap_or_else(|e| panic!("README.md must exist at {readme_path:?}: {e}"));

    // Manifest format section
    assert!(
        readme.contains("roots.toml") || readme.contains("manifest"),
        "README must document the manifest format (roots.toml / manifest section)"
    );

    // Core types
    for type_name in &["WatchRoot", "WatchState", "RootStatus", "ReconcileAction", "ReconcilePlan"] {
        assert!(
            readme.contains(type_name),
            "README must mention the core type `{type_name}`"
        );
    }

    // WatchBackend trait
    assert!(
        readme.contains("WatchBackend"),
        "README must document the WatchBackend trait"
    );

    // Extension contract for sibling PRDs
    let has_extension_contract = readme.contains("anchor-probe")
        || readme.contains("anchor-reconcile")
        || readme.contains("anchor-boot")
        || readme.contains("extend");
    assert!(
        has_extension_contract,
        "README must mention the extension contract (anchor-probe/reconcile/boot or 'extend')"
    );
}
