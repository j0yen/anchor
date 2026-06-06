//! AC3 (MUST): RootsConfig::load parses config/roots.example.toml; a test with
//! a fixture manifest yields the expected Vec<WatchRoot>, including a per-root
//! max_age_secs override.

use anchor::{RootsConfig, WatchRoot};
use std::path::PathBuf;

#[test]
fn test_roots_config_load_fixture() {
    // Load the shipped example manifest.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config/roots.example.toml");

    let cfg = RootsConfig::load(&manifest).expect("should parse roots.example.toml");
    assert!(
        cfg.roots.len() >= 3,
        "example manifest should have at least 3 roots, got {}",
        cfg.roots.len()
    );

    // Verify .claude root has max_age_secs set to 86400.
    let claude_root = cfg
        .roots
        .iter()
        .find(|r| r.path == PathBuf::from("/home/jsy/.claude"))
        .expect(".claude root should be in example manifest");
    assert_eq!(
        claude_root.max_age_secs,
        Some(86400),
        ".claude root should have max_age_secs = 86400"
    );

    // Verify a root WITHOUT max_age_secs has None.
    let bin_root = cfg
        .roots
        .iter()
        .find(|r| r.path == PathBuf::from("/home/jsy/.local/bin"));
    if let Some(root) = bin_root {
        assert_eq!(
            root.max_age_secs, None,
            ".local/bin root should have no max_age_secs"
        );
    }
}

#[test]
fn test_roots_config_load_inline_fixture() {
    // Write a temporary TOML and verify parsing with a per-root override.
    let toml = r#"
[[root]]
path = "/tmp/watched"
max_age_secs = 3600

[[root]]
path = "/tmp/also-watched"
"#;

    let dir = tempdir_simple();
    let path = dir.join("roots.toml");
    std::fs::write(&path, toml).expect("write temp manifest");

    let cfg = RootsConfig::load(&path).expect("parse inline fixture");
    assert_eq!(cfg.roots.len(), 2);
    assert_eq!(
        cfg.roots[0],
        WatchRoot {
            path: PathBuf::from("/tmp/watched"),
            max_age_secs: Some(3600),
        }
    );
    assert_eq!(
        cfg.roots[1],
        WatchRoot {
            path: PathBuf::from("/tmp/also-watched"),
            max_age_secs: None,
        }
    );
}

/// Minimal substitute for tempdir — just uses /tmp directly with a unique name.
fn tempdir_simple() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = PathBuf::from(format!("/tmp/anchor-test-{ts}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
