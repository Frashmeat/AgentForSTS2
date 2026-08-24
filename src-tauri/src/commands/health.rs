use std::sync::Arc;

use ats_adapters::FileTruthSnapshotRepository;
use ats_features::built_in_feature_contracts;
use ats_game_context::TruthSnapshotRepository;
use ats_kernel::{BuildInfo, Sha256Digest};
use serde::Serialize;
use tauri::State;

use crate::build_identity;
use crate::composition::Stage2Composition;
use crate::project_session::ActiveProject;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub status: &'static str,
    pub build: BuildInfo,
    pub game_pack_id: String,
    pub game_pack_sha256: Sha256Digest,
    pub feature_count: usize,
    pub project_open: bool,
    pub truth_ready: bool,
    pub media_generation_registered: bool,
}

#[tauri::command]
pub fn get_health(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
) -> HealthReport {
    health_report(&active, &composition)
}

pub(crate) fn health_report(
    active: &ActiveProject,
    composition: &Stage2Composition,
) -> HealthReport {
    let project_open = active.current().is_ok_and(|value| value.is_some());
    let truth_ready = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf())
        .open_current(composition.pack())
        .is_ok_and(|value| value.is_some());
    HealthReport {
        status: "ok",
        build: build_identity::current(),
        game_pack_id: composition.pack().id().to_string(),
        game_pack_sha256: composition.pack().content_sha256().clone(),
        feature_count: built_in_feature_contracts().len(),
        project_open,
        truth_ready,
        media_generation_registered: true,
    }
}
