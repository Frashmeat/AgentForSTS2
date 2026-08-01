use std::env;

const RELEASE_COMMIT_LEN: usize = 40;

fn main() {
    for key in ["ATS_BUILD_COMMIT", "ATS_BUILD_VARIANT", "ATS_BUILD_ID"] {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let ml_enabled = env::var_os("CARGO_FEATURE_ML_REMBG").is_some();
    let e2e_enabled = env::var_os("CARGO_FEATURE_E2E").is_some();
    let variant = env::var("ATS_BUILD_VARIANT").unwrap_or_else(|_| "development".into());
    let commit = env::var("ATS_BUILD_COMMIT").unwrap_or_else(|_| "unknown".into());
    let build_id = env::var("ATS_BUILD_ID").unwrap_or_else(|_| "dev".into());

    validate_identity(&variant, &commit, &build_id, ml_enabled, e2e_enabled);
    println!("cargo:rustc-env=ATS_EMBEDDED_BUILD_VARIANT={variant}");
    println!("cargo:rustc-env=ATS_EMBEDDED_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=ATS_EMBEDDED_BUILD_ID={build_id}");
}

fn validate_identity(
    variant: &str,
    commit: &str,
    build_id: &str,
    ml_enabled: bool,
    e2e_enabled: bool,
) {
    match variant {
        "development" => {}
        "baseline" if ml_enabled => {
            panic!("ATS_BUILD_VARIANT=baseline cannot enable the ml-rembg feature")
        }
        "ml" if !ml_enabled => {
            panic!("ATS_BUILD_VARIANT=ml requires the ml-rembg feature")
        }
        "baseline" | "ml" => {
            assert!(
                !e2e_enabled,
                "candidate builds cannot enable the e2e feature"
            );
            assert!(
                commit.len() == RELEASE_COMMIT_LEN
                    && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "candidate builds require ATS_BUILD_COMMIT to be a full 40-character Git SHA"
            );
            assert!(
                build_id != "dev",
                "candidate builds require a non-development ATS_BUILD_ID"
            );
        }
        _ => panic!("ATS_BUILD_VARIANT must be development, baseline, or ml"),
    }
    validate_bounded_value("ATS_BUILD_COMMIT", commit, 64);
    validate_bounded_value("ATS_BUILD_ID", build_id, 96);
}

fn validate_bounded_value(name: &str, value: &str, max_len: usize) {
    assert!(
        !value.is_empty()
            && value.len() <= max_len
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
        "{name} must contain only bounded ASCII identifier characters"
    );
}
