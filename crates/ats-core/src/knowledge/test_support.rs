use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::game_pack::{
    GamePackRegistry, TruthSnapshotStore, TruthSourceKind, VerifiedGameContext,
};

pub(crate) fn fixture_game_context(
    runtime_dir: &Path,
    game_files: &[(&str, &str)],
    library_files: &[(&str, &str)],
) -> VerifiedGameContext {
    let mut pack = GamePackRegistry::built_in()
        .unwrap()
        .require("sts2")
        .unwrap()
        .clone();
    pack.truth_sources[1].kind = TruthSourceKind::LocalFile {
        input_key: "baselib_assembly".into(),
    };

    let raw_dir = runtime_dir.join("fixture-inputs");
    fs::create_dir_all(&raw_dir).unwrap();
    let game_assembly = raw_dir.join("game.dll");
    let baselib_assembly = raw_dir.join("BaseLib.dll");
    fs::write(&game_assembly, b"fixture-game-assembly").unwrap();
    fs::write(&baselib_assembly, b"fixture-baselib-assembly").unwrap();

    let store = TruthSnapshotStore::new(runtime_dir, &pack);
    let mut draft = store.begin(&pack).unwrap();
    draft.stage_source("game", &game_assembly).unwrap();
    draft.stage_source("baselib", &baselib_assembly).unwrap();
    write_index_files(
        &draft.index_output_dir("game").unwrap(),
        game_files,
        "FixtureGame.cs",
        "public class FixtureGame {}",
    );
    write_index_files(
        &draft.index_output_dir("baselib").unwrap(),
        library_files,
        "FixtureBaseLib.cs",
        "public class FixtureBaseLib {}",
    );
    draft
        .finalize(BTreeMap::from([("fixture-indexer".into(), "1.0".into())]))
        .unwrap();

    let registry = GamePackRegistry::from_packs([pack]).unwrap();
    VerifiedGameContext::open_current(runtime_dir, &registry, "sts2").unwrap()
}

fn write_index_files(
    root: &Path,
    files: &[(&str, &str)],
    default_name: &str,
    default_content: &str,
) {
    if files.is_empty() {
        fs::write(root.join(default_name), default_content).unwrap();
        return;
    }
    for (relative_path, content) in files {
        let path = root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}
