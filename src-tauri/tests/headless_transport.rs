use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn production_binary_serves_versioned_jsonl_without_starting_the_ui() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentthespire-desktop"))
        .arg("--headless-jsonl")
        .env("SPIREFORGE_APP_DATA_ROOT", temp.path().join("app-data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"schemaVersion":1,"requestId":"health-1","command":{{"name":"health"}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"schemaVersion":1,"requestId":"invalid-1","command":{{"name":"health","unknown":true}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"schemaVersion":1,"requestId":"shutdown-1","command":{{"name":"shutdown"}}}}"#
    )
    .unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let responses = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["requestId"], "health-1");
    assert_eq!(responses[0]["ok"], true);
    assert_eq!(responses[0]["result"]["featureCount"], 12);
    assert_eq!(responses[1]["requestId"], "invalid-1");
    assert_eq!(responses[1]["ok"], false);
    assert_eq!(responses[1]["failure"]["code"], "run.input_invalid");
    assert_eq!(responses[2]["requestId"], "shutdown-1");
    assert_eq!(responses[2]["result"]["shutdown"], true);
    assert!(
        responses
            .iter()
            .all(|response| response["build"].is_object())
    );
    assert!(!stdout.contains("api_key"));
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_process_arguments_fail_instead_of_starting_the_ui() {
    let status = Command::new(env!("CARGO_BIN_EXE_agentthespire-desktop"))
        .arg("--unknown-headless-command")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}

#[test]
fn headless_project_lifecycle_persists_a_run_and_releases_the_lock() {
    let temp = tempfile::tempdir().unwrap();
    let app_data = temp.path().join("app-data");
    let projects = temp.path().join("projects");
    let game_assembly = temp.path().join("sts2.dll");
    let godot = temp.path().join("godot.exe");
    fs::create_dir_all(&projects).unwrap();
    fs::write(&game_assembly, b"fixture").unwrap();
    fs::write(&godot, b"fixture").unwrap();
    fs::create_dir_all(&app_data).unwrap();
    let config = app_data.join("config.json");
    fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({
            "knowledge": {"sts2_dll_path": game_assembly},
            "toolchain": {"godot_exe_path": godot}
        }))
        .unwrap(),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_agentthespire-desktop"))
        .arg("--headless-jsonl")
        .env("SPIREFORGE_APP_DATA_ROOT", &app_data)
        .env("SPIREFORGE_CONFIG_PATH", &config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let create = serde_json::json!({
        "schemaVersion": 1,
        "requestId": "create-1",
        "command": {
            "name": "create_project",
            "input": {"parentDir": projects, "projectName": "HeadlessProject"}
        }
    });
    for request in [
        create,
        serde_json::json!({
            "schemaVersion": 1,
            "requestId": "runs-1",
            "command": {"name": "list_runs"}
        }),
        serde_json::json!({
            "schemaVersion": 1,
            "requestId": "close-1",
            "command": {"name": "close_project"}
        }),
        serde_json::json!({
            "schemaVersion": 1,
            "requestId": "shutdown-1",
            "command": {"name": "shutdown"}
        }),
    ] {
        writeln!(stdin, "{}", serde_json::to_string(&request).unwrap()).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 4);
    assert!(responses.iter().all(|response| response["ok"] == true));
    assert_eq!(responses[0]["result"]["name"], "HeadlessProject");
    assert_eq!(responses[1]["result"].as_array().unwrap().len(), 1);
    assert_eq!(responses[1]["result"][0]["featureId"], "project.create");
    assert_eq!(responses[1]["result"][0]["status"], "succeeded");
    let project_root = temp.path().join("projects/HeadlessProject");
    let reopened = ats_workspace::ProjectFolder::open(&project_root).unwrap();
    drop(reopened);
}
