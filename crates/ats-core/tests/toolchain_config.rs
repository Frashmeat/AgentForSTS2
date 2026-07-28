use ats_core::config::Settings;

#[test]
fn godot_path_round_trips_through_settings_json() {
    let mut settings = Settings::default();
    assert!(settings.toolchain.godot_exe_path.is_empty());

    settings.toolchain.godot_exe_path = r"Y:\FixtureGodot\godot.exe".to_string();
    let json = serde_json::to_value(&settings).expect("serialize settings");
    assert_eq!(
        json["toolchain"]["godot_exe_path"],
        r"Y:\FixtureGodot\godot.exe"
    );

    let decoded: Settings = serde_json::from_value(json).expect("deserialize settings");
    assert_eq!(
        decoded.toolchain.godot_exe_path,
        r"Y:\FixtureGodot\godot.exe"
    );
}
