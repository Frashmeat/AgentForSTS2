use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use ats_core::game_pack::{BuildLocalProperty, BuildRecipe, BuildRunner, BuildStep};
use ats_core::project::{LocalBuildInputs, sync_local_props};

fn recipe() -> BuildRecipe {
    BuildRecipe {
        local_properties: vec![
            BuildLocalProperty {
                input_key: "game_assembly".into(),
                property: "GameAssemblyPath".into(),
            },
            BuildLocalProperty {
                input_key: "engine_executable".into(),
                property: "EnginePath".into(),
            },
        ],
        steps: vec![BuildStep {
            id: "publish".into(),
            runner: BuildRunner::DotnetPublish,
        }],
    }
}

fn inputs(game: &str, engine: &str) -> LocalBuildInputs {
    LocalBuildInputs {
        values: BTreeMap::from([
            ("game_assembly".into(), PathBuf::from(game)),
            ("engine_executable".into(), PathBuf::from(engine)),
        ]),
    }
}

#[test]
fn creates_local_props_from_valid_build_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("DemoMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props.example"),
        r#"<Project>
  <PropertyGroup>
    <GameAssemblyPath>C:/default/game.dll</GameAssemblyPath>
    <EnginePath>C:/default/engine.exe</EnginePath>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("template");

    let inputs = inputs(
        r"X:\FixtureGame\data\game.dll",
        r"Y:\FixtureEngine\engine.exe",
    );

    let result =
        sync_local_props(&project_root, Some(&recipe()), &inputs).expect("sync local.props");
    let props = fs::read_to_string(&result.path).expect("read local.props");

    assert!(result.created);
    assert!(props.contains(r"X:\FixtureGame\data\game.dll"));
    assert!(props.contains(r"Y:\FixtureEngine\engine.exe"));
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
    <GameAssemblyPath>C:/old/game.dll</GameAssemblyPath>
    <EnginePath>C:/old/engine.exe</EnginePath>
    <ModsPath>D:/isolated/mods</ModsPath>
    <CustomFlag Condition="'$(CustomFlag)' == ''">keep-me</CustomFlag>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("existing local.props");
    let inputs = inputs(
        r"X:\Fixture Game & Tools\data\game.dll",
        r"Y:\Fixture Engine & Tools\engine.exe",
    );

    let result =
        sync_local_props(&project_root, Some(&recipe()), &inputs).expect("sync local.props");
    let props = fs::read_to_string(&result.path).expect("read local.props");

    assert!(!result.created);
    assert!(props.contains(r"X:\Fixture Game &amp; Tools\data\game.dll"));
    assert!(props.contains(r"Y:\Fixture Engine &amp; Tools\engine.exe"));
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
    let inputs = inputs(
        r"X:\FixtureGame\data\game.dll",
        r"Y:\FixtureEngine\engine.exe",
    );

    sync_local_props(&project_root, Some(&recipe()), &inputs).expect("sync local.props");
    let props = fs::read_to_string(project_root.join("local.props")).expect("read local.props");

    assert!(props.contains(r"<GameAssemblyPath>X:\FixtureGame\data\game.dll</GameAssemblyPath>"));
    assert!(props.contains(r"<EnginePath>Y:\FixtureEngine\engine.exe</EnginePath>"));
    assert!(props.contains("<ModsPath>D:/isolated</ModsPath>"));
}

#[test]
fn replaces_self_closing_managed_properties_without_duplicates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("SelfClosingMod");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props"),
        "<Project><PropertyGroup><GameAssemblyPath/><EnginePath /></PropertyGroup></Project>",
    )
    .expect("existing local.props");
    let inputs = inputs(
        r"X:\FixtureGame\data\game.dll",
        r"Y:\FixtureEngine\engine.exe",
    );

    sync_local_props(&project_root, Some(&recipe()), &inputs).expect("sync local.props");
    let props = fs::read_to_string(project_root.join("local.props")).expect("read local.props");

    assert_eq!(props.matches("<GameAssemblyPath").count(), 1);
    assert_eq!(props.matches("<EnginePath").count(), 1);
    assert!(props.contains(r">X:\FixtureGame\data\game.dll</GameAssemblyPath>"));
    assert!(props.contains(r">Y:\FixtureEngine\engine.exe</EnginePath>"));
}

#[test]
fn rejects_missing_pack_input_before_writing_local_props() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_root = temp.path().join("MissingInput");
    fs::create_dir_all(&project_root).expect("project root");
    fs::write(
        project_root.join("local.props.example"),
        "<Project><PropertyGroup /></Project>",
    )
    .expect("template");
    let inputs = LocalBuildInputs {
        values: BTreeMap::from([("game_assembly".into(), PathBuf::from("C:/fixture/game.dll"))]),
    };

    let error = sync_local_props(&project_root, Some(&recipe()), &inputs).unwrap_err();

    assert!(error.to_string().contains("engine_executable"));
    assert!(!project_root.join("local.props").exists());
}
