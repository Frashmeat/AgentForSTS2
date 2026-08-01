//! image_proc prewarm 状态：在 app 启动时（异步后台）下载 + 加载 ONNX 模型，
//! 加载好后塞进 `ImageProcState`；asset_generate handler 通过 State 取出。
//!
//! 没启用 `ml-rembg` feature 时整个 prewarm 流程跳过，asset_generate 拿到的是
//! `BgRemoverChain::with_simple_fallback(None)` 等价于裸 SimpleBgRemover。

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use ats_core::failure::ActionableFailure;
#[cfg(feature = "ml-rembg")]
use ats_core::failure::FailureNormalizer;
use ats_core::image_proc::ImageProcClient;
#[cfg(feature = "ml-rembg")]
use ats_core::image_proc::ImageProcError;
use serde::Serialize;
use tokio::sync::Mutex as AsyncMutex;

use crate::AppPaths;
use crate::commands::failure::CommandResult;

/// 进程内共享：primary ML rembg 客户端（feature on + 加载成功时 Some）+
/// 状态 enum（前端可读）。
#[derive(Default)]
struct ImageProcRuntime {
    primary: Option<Arc<dyn ImageProcClient>>,
    status: PrewarmStatus,
}

#[derive(Default)]
pub struct ImageProcState {
    runtime: Mutex<ImageProcRuntime>,
    flight: AsyncMutex<()>,
}

impl ImageProcState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 取一份当前 primary client 的引用。`None` 时调用方自动用启发式 fallback。
    #[must_use]
    pub fn primary(&self) -> Option<Arc<dyn ImageProcClient>> {
        self.runtime
            .lock()
            .ok()
            .and_then(|runtime| runtime.primary.clone())
    }

    pub fn set_status(&self, status: PrewarmStatus) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.status = status;
        }
    }

    #[must_use]
    pub fn status_snapshot(&self) -> PrewarmStatus {
        self.runtime
            .lock()
            .map(|runtime| runtime.status.clone())
            .unwrap_or_default()
    }

    fn publish_loaded(&self, loaded: LoadedImageProcessor, attempt: u64) -> PrewarmStatus {
        let status = PrewarmStatus::Ready {
            attempt,
            model: loaded.model,
            model_sha256: loaded.model_sha256,
            runtime_version: loaded.runtime_version,
        };
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.primary = Some(loaded.client);
            runtime.status = status.clone();
        }
        status
    }

    #[cfg(test)]
    fn runtime_snapshot(&self) -> (bool, PrewarmStatus) {
        self.runtime
            .lock()
            .map(|runtime| (runtime.primary.is_some(), runtime.status.clone()))
            .unwrap_or_else(|_| (false, PrewarmStatus::default()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
#[allow(dead_code)] // Ready / Loading 在 ml-rembg feature off 时未构造，但 enum 形状不能变
pub enum PrewarmStatus {
    /// 未启动 prewarm（feature off 或 app 刚启动）
    Idle { attempt: u64 },
    /// 正在下载 / 加载模型
    Loading { attempt: u64, message: String },
    /// ML rembg 已就绪
    Ready {
        attempt: u64,
        model: String,
        model_sha256: String,
        runtime_version: String,
    },
    /// 启动失败（feature off / 网络问题 / onnxruntime 缺失）—— asset_generate 仍会
    /// 走启发式 fallback，UI 提示用户但不致命
    Failed {
        attempt: u64,
        failure: Box<ActionableFailure>,
    },
}

impl PrewarmStatus {
    #[must_use]
    pub fn attempt(&self) -> u64 {
        match self {
            Self::Idle { attempt }
            | Self::Loading { attempt, .. }
            | Self::Ready { attempt, .. }
            | Self::Failed { attempt, .. } => *attempt,
        }
    }

    fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}

impl Default for PrewarmStatus {
    fn default() -> Self {
        Self::Idle { attempt: 0 }
    }
}

struct LoadedImageProcessor {
    client: Arc<dyn ImageProcClient>,
    model: String,
    model_sha256: String,
    runtime_version: String,
}

/// 在 Tauri setup 阶段 spawn 调用：feature on 时下载 u2netp + 加载 MlBgRemover；
/// off 时立刻返回 Idle/Failed（"feature disabled"）。
///
/// 这是一个 best-effort 操作：失败只更新 status 让 UI 看到，不影响 app 启动。
pub async fn prewarm(state: Arc<ImageProcState>, app_data_dir: PathBuf) -> PrewarmStatus {
    run_singleflight(state, move |state, attempt| async move {
        load_primary(state, attempt, app_data_dir).await
    })
    .await
}

async fn run_singleflight<F, Fut>(state: Arc<ImageProcState>, operation: F) -> PrewarmStatus
where
    F: FnOnce(Arc<ImageProcState>, u64) -> Fut,
    Fut: Future<Output = Result<LoadedImageProcessor, ActionableFailure>>,
{
    let observed = state.status_snapshot();
    let observed_attempt = observed.attempt();
    let observed_loading = observed.is_loading();
    let _flight = state.flight.lock().await;
    let current = state.status_snapshot();
    if current.attempt() != observed_attempt
        || (observed_loading && !current.is_loading())
        || matches!(current, PrewarmStatus::Ready { .. })
    {
        return current;
    }

    let attempt = current.attempt().saturating_add(1);
    state.set_status(PrewarmStatus::Loading {
        attempt,
        message: "starting prewarm".into(),
    });
    let terminal = match operation(Arc::clone(&state), attempt).await {
        Ok(loaded) => return state.publish_loaded(loaded, attempt),
        Err(failure) => PrewarmStatus::Failed {
            attempt,
            failure: Box::new(failure),
        },
    };
    state.set_status(terminal.clone());
    terminal
}

#[cfg(not(feature = "ml-rembg"))]
async fn load_primary(
    _state: Arc<ImageProcState>,
    _attempt: u64,
    _app_data_dir: PathBuf,
) -> Result<LoadedImageProcessor, ActionableFailure> {
    Err(ActionableFailure::new(
        "image_proc.feature_disabled",
        ats_core::failure::FailureCategory::Configuration,
        "image_proc.prewarm",
        "This build does not include ML background removal.",
        ats_core::failure::RecoveryAction::None,
        false,
    ))
}

#[cfg(feature = "ml-rembg")]
async fn load_primary(
    state: Arc<ImageProcState>,
    attempt: u64,
    app_data_dir: PathBuf,
) -> Result<LoadedImageProcessor, ActionableFailure> {
    use ats_core::image_proc::cache::{
        ModelSpec, ONNX_RUNTIME_VERSION, OrtDylibSpec, ensure_model, ensure_ort_dylib,
    };
    use ats_core::image_proc::{MlBgRemover, init_ort_from_dylib};

    #[cfg(windows)]
    {
        let spec = OrtDylibSpec::onnxruntime_1_22_windows_x64();
        let runtimes_dir = app_data_dir
            .join("runtimes")
            .join(format!("onnxruntime-{ONNX_RUNTIME_VERSION}"));
        state.set_status(PrewarmStatus::Loading {
            attempt,
            message: "preparing onnxruntime.dll".into(),
        });
        let dll_path = ensure_ort_dylib(&runtimes_dir, &spec)
            .await
            .map_err(|error| prewarm_image_failure(ImageProcError::NotReady(error.to_string())))?;
        init_ort_from_dylib(&dll_path)
            .map_err(|error| prewarm_image_failure(ImageProcError::NotReady(error.to_string())))?;
    }

    let spec = ModelSpec::u2netp();
    let model_sha256 = spec
        .expected_sha256
        .clone()
        .expect("the built-in u2netp model must pin a SHA-256");
    state.set_status(PrewarmStatus::Loading {
        attempt,
        message: format!("downloading {} (~5MB)", spec.file_name),
    });
    let model_path = ensure_model(&app_data_dir.join("models"), &spec)
        .await
        .map_err(|error| prewarm_image_failure(ImageProcError::Model(error.to_string())))?;
    state.set_status(PrewarmStatus::Loading {
        attempt,
        message: "loading onnxruntime session".into(),
    });
    let model_path_for_load = model_path.clone();
    let load_result = tokio::task::spawn_blocking(move || {
        MlBgRemover::load(
            &model_path_for_load,
            model_sha256.clone(),
            ONNX_RUNTIME_VERSION.into(),
        )
        .map(|client| LoadedImageProcessor {
            client: Arc::new(client),
            model: spec.file_name,
            model_sha256,
            runtime_version: ONNX_RUNTIME_VERSION.into(),
        })
    })
    .await;
    match load_result {
        Ok(Ok(loaded)) => Ok(loaded),
        Ok(Err(error)) => Err(prewarm_image_failure(ImageProcError::Model(
            error.to_string(),
        ))),
        Err(_) => Err(ActionableFailure::unclassified("image_proc.prewarm_join")),
    }
}

#[cfg(feature = "ml-rembg")]
fn prewarm_image_failure(error: ImageProcError) -> ActionableFailure {
    FailureNormalizer::image_proc("image_proc.prewarm", &error)
}

#[tauri::command]
#[must_use]
pub fn image_proc_status(state: tauri::State<'_, Arc<ImageProcState>>) -> PrewarmStatus {
    state.status_snapshot()
}

#[tauri::command]
pub async fn retry_image_proc(
    state: tauri::State<'_, Arc<ImageProcState>>,
    paths: tauri::State<'_, AppPaths>,
) -> CommandResult<PrewarmStatus> {
    let state = Arc::clone(state.inner());
    let app_data_root = paths.data.root.clone();
    Ok(prewarm(state, app_data_root).await)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use ats_core::image_proc::SimpleBgRemover;
    use tokio::sync::Notify;

    use super::*;

    fn loaded_processor() -> LoadedImageProcessor {
        LoadedImageProcessor {
            client: Arc::new(SimpleBgRemover::default()),
            model: "fixture.onnx".into(),
            model_sha256: "a".repeat(64),
            runtime_version: "1.22.0".into(),
        }
    }

    #[tokio::test]
    async fn concurrent_attempts_share_one_singleflight_operation() {
        let state = Arc::new(ImageProcState::new());
        let calls = Arc::new(AtomicU32::new(0));
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let entered_wait = entered.notified();
        let first = {
            let state = Arc::clone(&state);
            let calls = Arc::clone(&calls);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            tokio::spawn(run_singleflight(state, move |_, _| async move {
                calls.fetch_add(1, Ordering::SeqCst);
                entered.notify_one();
                release.notified().await;
                Ok(loaded_processor())
            }))
        };
        entered_wait.await;
        let second = {
            let state = Arc::clone(&state);
            let calls = Arc::clone(&calls);
            tokio::spawn(run_singleflight(state, move |_, _| async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(loaded_processor())
            }))
        };
        tokio::task::yield_now().await;
        release.notify_waiters();

        let first_status = first.await.unwrap();
        let second_status = second.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first_status.attempt(), 1);
        assert_eq!(second_status.attempt(), 1);
        assert!(matches!(first_status, PrewarmStatus::Ready { .. }));
        assert!(matches!(second_status, PrewarmStatus::Ready { .. }));
        let (has_primary, status) = state.runtime_snapshot();
        assert!(has_primary);
        assert!(matches!(status, PrewarmStatus::Ready { attempt: 1, .. }));
    }

    #[tokio::test]
    async fn failed_attempt_can_retry_and_increments_attempt_once() {
        let state = Arc::new(ImageProcState::new());
        let failed = run_singleflight(Arc::clone(&state), |_, _| async {
            Err(ActionableFailure::unclassified("image_proc.fixture"))
        })
        .await;
        assert!(matches!(failed, PrewarmStatus::Failed { attempt: 1, .. }));

        let ready =
            run_singleflight(Arc::clone(&state), |_, _| async { Ok(loaded_processor()) }).await;
        assert!(matches!(ready, PrewarmStatus::Ready { attempt: 2, .. }));
        assert!(state.primary().is_some());
    }

    #[cfg(not(feature = "ml-rembg"))]
    #[tokio::test]
    async fn baseline_prewarm_reports_structured_feature_failure() {
        let state = Arc::new(ImageProcState::new());
        let status = prewarm(Arc::clone(&state), PathBuf::from("private-canary")).await;
        let serialized = serde_json::to_value(&status).unwrap();
        assert_eq!(serialized["state"], "failed");
        assert_eq!(serialized["failure"]["code"], "image_proc.feature_disabled");
        assert!(serialized["failure"].get("failure").is_none());
        let PrewarmStatus::Failed { attempt, failure } = status else {
            panic!("baseline prewarm should report a terminal feature failure");
        };
        assert_eq!(attempt, 1);
        assert_eq!(failure.code, "image_proc.feature_disabled");
        assert!(!failure.retryable);
        assert!(
            !serde_json::to_string(&failure)
                .unwrap()
                .contains("private-canary")
        );
    }
}
