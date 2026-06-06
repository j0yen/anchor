//! anchor-boot acceptance tests for ACs 1–4 (offline / CI-safe).
//!
//! AC1: boot/anchor-reconcile.service is a valid systemd unit declaring
//!      After=watchman.service, Wants=watchman.service, and ExecStart invoking
//!      `anchor reconcile --apply`. Verified by parsing the file text (no live
//!      systemd required; systemd-analyze check can be layered on top of this).
//!
//! AC2: boot/install.sh is idempotent against a temp XDG_CONFIG_HOME: running
//!      it twice leaves exactly one unit symlink and exits 0 both times.
//!
//! AC3: boot/install.sh prints the SessionStart hook entry and does NOT write
//!      to ~/.claude/ (touches only $XDG_CONFIG_HOME/systemd/user/).
//!
//! AC4: boot/anchor-session-start.sh exits 0 even when a stubbed `anchor`
//!      exits non-zero (watch failure must not block a session start).
//!
//! AC8 (guard): this file appears in cargo test output — the
//!      self_orphaned_mock_tests guard is satisfied by cargo printing
//!      "Running tests/acceptance_ac_boot.rs" in the test output.

use std::path::PathBuf;

fn boot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("boot")
}

// ─── AC1: service file content validation ────────────────────────────────────

#[test]
fn ac1_service_file_exists_and_has_required_directives() {
    let svc = boot_dir().join("anchor-reconcile.service");
    assert!(
        svc.exists(),
        "boot/anchor-reconcile.service must exist (path: {svc:?})"
    );

    let content = std::fs::read_to_string(&svc)
        .unwrap_or_else(|e| panic!("cannot read service file {svc:?}: {e}"));

    // Must declare After=watchman.service
    assert!(
        content.contains("After=watchman.service"),
        "service must declare After=watchman.service\nContent:\n{content}"
    );

    // Must declare Wants=watchman.service
    assert!(
        content.contains("Wants=watchman.service"),
        "service must declare Wants=watchman.service\nContent:\n{content}"
    );

    // ExecStart must invoke anchor reconcile --apply
    assert!(
        content.contains("anchor reconcile --apply"),
        "service ExecStart must invoke 'anchor reconcile --apply'\nContent:\n{content}"
    );

    // Must be Type=oneshot
    assert!(
        content.contains("Type=oneshot"),
        "service must be Type=oneshot\nContent:\n{content}"
    );

    // Must have [Unit], [Service], [Install] sections
    for section in &["[Unit]", "[Service]", "[Install]"] {
        assert!(
            content.contains(section),
            "service must contain section {section}\nContent:\n{content}"
        );
    }
}

// ─── AC2: install.sh idempotence ─────────────────────────────────────────────

#[test]
fn ac2_install_sh_is_idempotent() {
    let install_sh = boot_dir().join("install.sh");
    assert!(
        install_sh.exists(),
        "boot/install.sh must exist (path: {install_sh:?})"
    );

    // Create isolated temp XDG_CONFIG_HOME to avoid touching the real system.
    let tmp = std::env::temp_dir().join(format!("anchor-boot-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create temp dir");

    let run = |n: u32| -> std::process::ExitStatus {
        let out = std::process::Command::new("bash")
            .arg(&install_sh)
            .env("XDG_CONFIG_HOME", &tmp)
            // Prevent actual systemctl calls inside the test.
            .env("PATH", format!("{}/fake-bin:/usr/bin:/bin", tmp.display()))
            .output()
            .unwrap_or_else(|e| panic!("failed to run install.sh (run {n}): {e}"));
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            panic!(
                "install.sh run {n} failed (exit {:?})\nstdout: {stdout}\nstderr: {stderr}",
                out.status.code()
            );
        }
        out.status
    };

    // First run
    run(1);

    // Verify exactly one unit symlink/file exists
    let systemd_dir = tmp.join("systemd/user");
    let svc_dest = systemd_dir.join("anchor-reconcile.service");
    assert!(
        svc_dest.exists(),
        "after first run, anchor-reconcile.service must exist in $XDG_CONFIG_HOME/systemd/user/"
    );

    // Second run — must also exit 0
    run(2);

    // Still exactly one entry (idempotence: not duplicated)
    let entries: Vec<_> = std::fs::read_dir(&systemd_dir)
        .expect("read systemd dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("anchor-reconcile"))
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        entries.len(),
        1,
        "after two runs, exactly 1 anchor-reconcile* entry must exist in {systemd_dir:?}"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

// ─── AC3: install.sh prints hook entry, does not touch ~/.claude/ ─────────────

#[test]
fn ac3_install_sh_prints_hook_entry_and_does_not_touch_dot_claude() {
    let install_sh = boot_dir().join("install.sh");
    assert!(
        install_sh.exists(),
        "boot/install.sh must exist (path: {install_sh:?})"
    );

    let tmp = std::env::temp_dir().join(format!("anchor-boot-test-ac3-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create temp dir");

    // Use a fake HOME to verify ~/.claude/ is not touched.
    let fake_home = tmp.join("home");
    std::fs::create_dir_all(&fake_home).expect("create fake home");

    let out = std::process::Command::new("bash")
        .arg(&install_sh)
        .env("XDG_CONFIG_HOME", tmp.join("config"))
        .env("HOME", &fake_home)
        .env("PATH", format!("{}/fake-bin:/usr/bin:/bin", tmp.display()))
        .output()
        .unwrap_or_else(|e| panic!("failed to run install.sh: {e}"));

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Must print the SessionStart hook entry
    assert!(
        stdout.contains("SessionStart"),
        "install.sh must print SessionStart hook entry\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("settings.json"),
        "install.sh must mention settings.json in its output\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("anchor-session-start"),
        "install.sh must print the hook script path\nstdout:\n{stdout}"
    );

    // ~/.claude/ (under fake home) must NOT have been created
    let dot_claude = fake_home.join(".claude");
    assert!(
        !dot_claude.exists(),
        "install.sh must not create or write to ~/.claude/ (found: {dot_claude:?})"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ─── AC4: anchor-session-start.sh exits 0 even when anchor fails ─────────────

#[test]
fn ac4_session_start_hook_exits_zero_on_anchor_failure() {
    let hook = boot_dir().join("anchor-session-start.sh");
    assert!(
        hook.exists(),
        "boot/anchor-session-start.sh must exist (path: {hook:?})"
    );

    let tmp = std::env::temp_dir().join(format!("anchor-boot-test-ac4-{}", std::process::id()));
    let fake_bin = tmp.join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("create fake-bin dir");

    // Write a stub `anchor` that always exits non-zero.
    let stub = fake_bin.join("anchor");
    std::fs::write(
        &stub,
        "#!/usr/bin/env bash\necho 'stub: watchman error' >&2\nexit 2\n",
    )
    .expect("write anchor stub");

    // Make stub executable.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
        .expect("chmod anchor stub");

    let out = std::process::Command::new("bash")
        .arg(&hook)
        .env("ANCHOR_BIN", &stub)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .output()
        .unwrap_or_else(|e| panic!("failed to run anchor-session-start.sh: {e}"));

    let exit_code = out.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 0,
        "anchor-session-start.sh must exit 0 even when anchor exits non-zero\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
