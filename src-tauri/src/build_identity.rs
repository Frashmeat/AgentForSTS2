use ats_kernel::{BuildInfo, BuildVariant};

#[must_use]
pub fn current() -> BuildInfo {
    let variant = match env!("ATS_EMBEDDED_BUILD_VARIANT") {
        "baseline" => BuildVariant::Baseline,
        "ml" => BuildVariant::Ml,
        _ => BuildVariant::Development,
    };
    let mut features = Vec::new();
    if cfg!(feature = "ml-rembg") {
        features.push("ml-rembg".into());
    }
    if cfg!(feature = "e2e") {
        features.push("e2e".into());
    }
    let info = BuildInfo {
        commit: env!("ATS_EMBEDDED_BUILD_COMMIT").into(),
        variant,
        features,
        build_id: env!("ATS_EMBEDDED_BUILD_ID").into(),
    };
    assert!(
        info.is_consistent(),
        "embedded build identity is inconsistent"
    );
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_identity_matches_compiled_features() {
        let info = current();
        assert!(info.is_consistent());
        assert_eq!(
            info.features.contains(&"ml-rembg".to_owned()),
            cfg!(feature = "ml-rembg")
        );
    }
}
