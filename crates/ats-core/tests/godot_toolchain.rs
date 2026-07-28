use std::path::PathBuf;
use std::time::Duration;

use ats_core::toolchain::{GodotValidationError, validate_godot_executable};

#[test]
fn rejects_missing_godot_executable() {
    let missing = PathBuf::from(r"Z:\definitely-missing\godot.exe");
    let error = validate_godot_executable(&missing, Duration::from_secs(1))
        .expect_err("missing executable must fail");

    assert!(matches!(error, GodotValidationError::NotAFile(path) if path == missing));
}

#[test]
fn rejects_existing_non_godot_executable() {
    let current_test_binary = std::env::current_exe().expect("current test binary");
    let error = validate_godot_executable(&current_test_binary, Duration::from_secs(5))
        .expect_err("non-Godot executable must fail");

    assert!(matches!(
        error,
        GodotValidationError::VersionCommandFailed { .. }
            | GodotValidationError::UnsupportedVersion { .. }
    ));
}

#[test]
fn accepts_configured_godot_4_5_1() {
    let Some(path) = std::env::var_os("ATS_E2E_GODOT_PATH").map(PathBuf::from) else {
        return;
    };

    let installation = validate_godot_executable(&path, Duration::from_secs(5))
        .expect("configured Godot 4.5.1 should validate");

    assert_eq!(installation.executable, path);
    assert!(installation.version.starts_with("4.5.1"));
}
