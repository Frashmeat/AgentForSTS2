use std::fs;
use std::path::PathBuf;

use ats_core::project::{LocalBuildPaths, sync_local_props};

#[test]
fn creates_local_props_from_valid_build_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("DemoMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props.example"),
        r#"<Project>
  <PropertyGroup>
    <SteamLibraryPath>C:/default/steamapps</SteamLibraryPath>
    <GodotPath>C:/default/godot.exe</GodotPath>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("template");

    let paths = LocalBuildPaths {
        sts2_dll_path: PathBuf::from(
            r"X:\FixtureSteam\steamapps\common\Slay the Spire 2\data_sts2_windows_x86_64\sts2.dll",
        ),
        godot_exe_path: PathBuf::from(r"Y:\FixtureGodot\godot.exe"),
    };

    let result = sync_local_props(&project_root, &paths).expect("sync local.props");
    let props = fs::read_to_string(&result.path).expect("read local.props");

    assert!(result.created);
    assert!(props.contains(r"X:\FixtureSteam\steamapps"));
    assert!(props.contains(r"Y:\FixtureGodot\godot.exe"));
}

#[test]
fn updates_managed_properties_without_losing_custom_xml() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("ExistingMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props"),
        r#"<Project>
  <PropertyGroup>
    <SteamLibraryPath>C:/old/steamapps</SteamLibraryPath>
    <GodotPath>C:/old/godot.exe</GodotPath>
    <ModsPath>D:/isolated/mods</ModsPath>
    <CustomFlag Condition="'$(CustomFlag)' == ''">keep-me</CustomFlag>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("existing local.props");
    let paths = LocalBuildPaths {
        sts2_dll_path: PathBuf::from(
            r"X:\Fixture Steam & Tools\steamapps\common\Slay the Spire 2\data\sts2.dll",
        ),
        godot_exe_path: PathBuf::from(r"Y:\Fixture Godot & MegaDot\godot.exe"),
    };

    let result = sync_local_props(&project_root, &paths).expect("sync local.props");
    let props = fs::read_to_string(&result.path).expect("read local.props");

    assert!(!result.created);
    assert!(props.contains(r"X:\Fixture Steam &amp; Tools\steamapps"));
    assert!(props.contains(r"Y:\Fixture Godot &amp; MegaDot\godot.exe"));
    assert!(props.contains("<ModsPath>D:/isolated/mods</ModsPath>"));
    assert!(props.contains("<CustomFlag Condition=\"'$(CustomFlag)' == ''\">keep-me</CustomFlag>"));
}

#[test]
fn inserts_missing_managed_properties_into_existing_project() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("LegacyMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props"),
        "<Project><PropertyGroup><ModsPath>D:/isolated</ModsPath></PropertyGroup></Project>",
    )
    .expect("legacy local.props");
    let paths = LocalBuildPaths {
        sts2_dll_path: PathBuf::from(r"X:\FixtureSteam\steamapps\common\STS2\data\sts2.dll"),
        godot_exe_path: PathBuf::from(r"Y:\FixtureGodot\godot.exe"),
    };

    sync_local_props(&project_root, &paths).expect("sync local.props");
    let props = fs::read_to_string(project_root.join("local.props")).expect("read local.props");

    assert!(props.contains(r"<SteamLibraryPath>X:\FixtureSteam\steamapps</SteamLibraryPath>"));
    assert!(props.contains(r"<GodotPath>Y:\FixtureGodot\godot.exe</GodotPath>"));
    assert!(props.contains("<ModsPath>D:/isolated</ModsPath>"));
}

#[test]
fn replaces_self_closing_managed_properties_without_duplicates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("SelfClosingMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props"),
        "<Project><PropertyGroup><SteamLibraryPath/><GodotPath /></PropertyGroup></Project>",
    )
    .expect("existing local.props");
    let paths = LocalBuildPaths {
        sts2_dll_path: PathBuf::from(r"X:\FixtureSteam\steamapps\common\STS2\data\sts2.dll"),
        godot_exe_path: PathBuf::from(r"Y:\FixtureGodot\godot.exe"),
    };

    sync_local_props(&project_root, &paths).expect("sync local.props");
    let props = fs::read_to_string(project_root.join("local.props")).expect("read local.props");

    assert_eq!(props.matches("<SteamLibraryPath").count(), 1);
    assert_eq!(props.matches("<GodotPath").count(), 1);
    assert!(props.contains(r">X:\FixtureSteam\steamapps</SteamLibraryPath>"));
    assert!(props.contains(r">Y:\FixtureGodot\godot.exe</GodotPath>"));
}
