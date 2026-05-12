//! image_proc prewarm 状态：在 app 启动时（异步后台）下载 + 加载 ONNX 模型，
//! 加载好后塞进 `ImageProcState`；asset_generate handler 通过 State 取出。
//!
//! 没启用 `ml-rembg` feature 时整个 prewarm 流程跳过，asset_generate 拿到的是
//! `BgRemoverChain::with_simple_fallback(None)` 等价于裸 SimpleBgRemover。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use ats_core::image_proc::ImageProcClient;
use serde::Serialize;

/// 进程内共享：primary ML rembg 客户端（feature on + 加载成功时 Some）+
/// 状态 enum（前端可读）。
#[derive(Default)]
pub struct ImageProcState {
    primary: Mutex<Option<Arc<dyn ImageProcClient>>>,
    status: Mutex<PrewarmStatus>,
}

impl ImageProcState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 取一份当前 primary client 的引用。`None` 时调用方自动用启发式 fallback。
    #[must_use]
    pub fn primary(&self) -> Option<Arc<dyn ImageProcClient>> {
        self.primary.lock().ok().and_then(|g| g.clone())
    }

    #[cfg_attr(not(feature = "ml-rembg"), allow(dead_code))]
    pub fn set_primary(&self, client: Arc<dyn ImageProcClient>) {
        if let Ok(mut g) = self.primary.lock() {
            *g = Some(client);
        }
    }

    pub fn set_status(&self, status: PrewarmStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = status;
        }
    }

    #[must_use]
    pub fn status_snapshot(&self) -> PrewarmStatus {
        self.status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase", tag = "state")]
#[allow(dead_code)] // Ready / Loading 在 ml-rembg feature off 时未构造，但 enum 形状不能变
pub enum PrewarmStatus {
    /// 未启动 prewarm（feature off 或 app 刚启动）
    #[default]
    Idle,
    /// 正在下载 / 加载模型
    Loading {
        message: String,
    },
    /// ML rembg 已就绪
    Ready {
        model: String,
    },
    /// 启动失败（feature off / 网络问题 / onnxruntime 缺失）—— asset_generate 仍会
    /// 走启发式 fallback，UI 提示用户但不致命
    Failed {
        message: String,
    },
}

/// 在 Tauri setup 阶段 spawn 调用：feature on 时下载 u2netp + 加载 MlBgRemover；
/// off 时立刻返回 Idle/Failed（"feature disabled"）。
///
/// 这是一个 best-effort 操作：失败只更新 status 让 UI 看到，不影响 app 启动。
pub async fn prewarm(state: Arc<ImageProcState>, app_data_dir: PathBuf) {
    state.set_status(PrewarmStatus::Loading {
        message: "starting prewarm".into(),
    });

    #[cfg(not(feature = "ml-rembg"))]
    {
        let _ = app_data_dir; // 没用上，避免 unused warning
        state.set_status(PrewarmStatus::Failed {
            message: "ml-rembg feature disabled at build time".into(),
        });
    }

    #[cfg(feature = "ml-rembg")]
    {
        use ats_core::image_proc::MlBgRemover;
        use ats_core::image_proc::cache::{ModelSpec, ensure_model};

        let spec = ModelSpec::u2netp();
        let models_dir = app_data_dir.join("models");
        state.set_status(PrewarmStatus::Loading {
            message: format!("downloading {} (~5MB)", spec.file_name),
        });
        let model_path = match ensure_model(&models_dir, &spec).await {
            Ok(p) => p,
            Err(err) => {
                state.set_status(PrewarmStatus::Failed {
                    message: format!("model download: {err}"),
                });
                return;
            }
        };
        state.set_status(PrewarmStatus::Loading {
            message: "loading onnxruntime session".into(),
        });
        let model_path_for_load = model_path.clone();
        let load_result = tokio::task::spawn_blocking(move || {
            MlBgRemover::load(&model_path_for_load)
        })
        .await;
        match load_result {
            Ok(Ok(remover)) => {
                state.set_primary(Arc::new(remover));
                state.set_status(PrewarmStatus::Ready {
                    model: spec.file_name,
                });
            }
            Ok(Err(err)) => {
                state.set_status(PrewarmStatus::Failed {
                    message: format!("ml session: {err}"),
                });
            }
            Err(join) => {
                state.set_status(PrewarmStatus::Failed {
                    message: format!("prewarm join: {join}"),
                });
            }
        }
    }
}

#[tauri::command]
#[must_use]
pub fn image_proc_status(
    state: tauri::State<'_, Arc<ImageProcState>>,
) -> PrewarmStatus {
    state.status_snapshot()
}
