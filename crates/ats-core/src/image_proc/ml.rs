//! MlBgRemover：用 ort + u2netp 真做语义分割得到 alpha mask，复合到原图。
//!
//! **gated by `ml-rembg` feature** —— 默认不编译，避免 onnxruntime 原生 lib
//! 的下载 / 链接复杂性进 CI 与精简构建。Tauri 生产构建建议
//! `cargo build --release --features ml-rembg`。
//!
//! 推理流程（与 rembg/u2net 标准一致）：
//! 1. 解码输入 PNG → RGBA → resize 到 320x320
//! 2. 转 [0,1] float，按 mean=(0.485,0.456,0.406) / std=(1,1,1) 归一化
//! 3. NHWC → NCHW，shape = (1, 3, 320, 320)
//! 4. session.run(input) → mask shape (1, 1, 320, 320)
//! 5. min-max 归一化到 [0, 1] → 升采样 (Lanczos3) 到原图尺寸
//! 6. 把 mask 当 alpha 通道；RGB 通道保留原色

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use image::{ImageFormat, Rgba, RgbaImage, imageops::FilterType};
use ndarray::{Array4, Axis};
use ort::session::Session;
use ort::value::Tensor;
use std::io::Cursor;
use thiserror::Error;

use super::{ImageProcClient, ImageProcError};

const U2NETP_INPUT_SIZE: usize = 320;

#[derive(Debug, Error)]
pub enum MlBgRemoverError {
    #[error("ort init: {0}")]
    OrtInit(String),
    #[error("ort session: {0}")]
    OrtSession(String),
    #[error("image decode: {0}")]
    Decode(String),
    #[error("image encode: {0}")]
    Encode(String),
    #[error("inference: {0}")]
    Inference(String),
}

/// 持有一个 ort Session，外部用 Arc 共享。Session 内部线程安全。
pub struct MlBgRemover {
    session: Arc<std::sync::Mutex<Session>>,
}

impl MlBgRemover {
    /// 从本地 `.onnx` 路径加载模型。建议先用 `image_proc::cache::ensure_model`
    /// 拉到 `<app_data>/models/u2netp.onnx` 再传进来。
    ///
    /// # Errors
    /// - 模型文件不存在 / 不可读
    /// - ort 加载失败（onnxruntime 原生 lib 缺失 / 不兼容）
    pub fn load(model_path: &Path) -> Result<Self, MlBgRemoverError> {
        if !model_path.is_file() {
            return Err(MlBgRemoverError::OrtSession(
                "ONNX model file does not exist".into(),
            ));
        }
        let session = Session::builder()
            .map_err(|e| MlBgRemoverError::OrtSession(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| MlBgRemoverError::OrtSession(e.to_string()))?;
        Ok(Self {
            session: Arc::new(std::sync::Mutex::new(session)),
        })
    }
}

/// 初始化 ONNX Runtime 动态库路径。必须在创建任何 Session 前调用。
///
/// 使用 `ort` 的 `load-dynamic` feature 时，默认会尝试从可执行文件旁加载
/// `onnxruntime.dll`。桌面端 prewarm 会先把官方 DLL 准备到 app data，再通过
/// 这个函数显式绑定路径。
pub fn init_ort_from_dylib(path: &Path) -> Result<(), MlBgRemoverError> {
    if !path.is_file() {
        return Err(MlBgRemoverError::OrtInit(format!(
            "ONNX Runtime library does not exist: {}",
            path.display()
        )));
    }
    run_ort_initializer(path, || {
        ort::init_from(path.display().to_string())
            .commit()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

fn run_ort_initializer(
    path: &Path,
    initialize: impl FnOnce() -> Result<(), String>,
) -> Result<(), MlBgRemoverError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(initialize)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(message)) => Err(ort_init_error(path, &message)),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("ONNX Runtime loader panicked");
            Err(ort_init_error(path, message))
        }
    }
}

fn ort_init_error(path: &Path, detail: &str) -> MlBgRemoverError {
    #[cfg(windows)]
    let message = format!(
        "failed to load {}. Install or repair Microsoft Visual C++ 2015-2022 Redistributable (x64), then restart AgentTheSpire. Loader detail: {detail}",
        path.display()
    );
    #[cfg(not(windows))]
    let message = format!("failed to load {}: {detail}", path.display());
    MlBgRemoverError::OrtInit(message)
}

#[async_trait]
impl ImageProcClient for MlBgRemover {
    async fn remove_background(&self, input_png: &[u8]) -> Result<Vec<u8>, ImageProcError> {
        let session = Arc::clone(&self.session);
        let input = input_png.to_vec();
        tokio::task::spawn_blocking(move || run_inference(&session, &input))
            .await
            .map_err(|e| ImageProcError::Runtime(format!("worker join: {e}")))?
            .map_err(map_err)
    }
}

fn map_err(err: MlBgRemoverError) -> ImageProcError {
    match err {
        MlBgRemoverError::Decode(m) => ImageProcError::Decode(m),
        MlBgRemoverError::Encode(m) => ImageProcError::Encode(m),
        MlBgRemoverError::OrtInit(message) => ImageProcError::NotReady(message),
        MlBgRemoverError::OrtSession(message) => ImageProcError::Model(message),
        MlBgRemoverError::Inference(message) => ImageProcError::Runtime(message),
    }
}

fn run_inference(
    session: &std::sync::Mutex<Session>,
    input_png: &[u8],
) -> Result<Vec<u8>, MlBgRemoverError> {
    // 1. 解码
    let img =
        image::load_from_memory(input_png).map_err(|e| MlBgRemoverError::Decode(e.to_string()))?;
    let original_w = img.width();
    let original_h = img.height();
    let resized = img
        .resize_exact(
            U2NETP_INPUT_SIZE as u32,
            U2NETP_INPUT_SIZE as u32,
            FilterType::Lanczos3,
        )
        .to_rgb8();

    // 2-3. 归一化 + NHWC→NCHW
    // mean=(0.485,0.456,0.406), std=(1.0,1.0,1.0) —— rembg 项目实测有效
    let mut tensor = Array4::<f32>::zeros((1, 3, U2NETP_INPUT_SIZE, U2NETP_INPUT_SIZE));
    let means = [0.485_f32, 0.456, 0.406];
    for (y, row) in resized.rows().enumerate() {
        for (x, px) in row.enumerate() {
            for c in 0..3 {
                let v = f32::from(px.0[c]) / 255.0 - means[c];
                tensor[(0, c, y, x)] = v;
            }
        }
    }

    // 4. 推理。SessionOutputs 借用 guard，必须在同一作用域内提取完毕。
    let ort_tensor =
        Tensor::from_array(tensor).map_err(|e| MlBgRemoverError::Inference(e.to_string()))?;
    let (shape_owned, raw_owned): (Vec<i64>, Vec<f32>) = {
        let mut guard = session
            .lock()
            .map_err(|e| MlBgRemoverError::Inference(format!("session lock: {e}")))?;
        let outputs = guard
            .run(ort::inputs![ort_tensor])
            .map_err(|e| MlBgRemoverError::Inference(e.to_string()))?;
        let (shape, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| MlBgRemoverError::Inference(format!("extract: {e}")))?;
        // 拷贝出借用，方便后续处理不受 guard 生命周期约束
        let shape_v: Vec<i64> = shape.iter().copied().collect();
        let raw_v: Vec<f32> = raw.to_vec();
        (shape_v, raw_v)
    };

    if shape_owned.len() < 4
        || shape_owned[2] != U2NETP_INPUT_SIZE as i64
        || shape_owned[3] != U2NETP_INPUT_SIZE as i64
    {
        return Err(MlBgRemoverError::Inference(format!(
            "unexpected mask shape: {shape_owned:?}"
        )));
    }
    // raw_owned 是 1×1×H×W flatten，取第 0 通道。
    let mask_2d = ndarray::ArrayView::from_shape(
        (U2NETP_INPUT_SIZE, U2NETP_INPUT_SIZE),
        &raw_owned[..U2NETP_INPUT_SIZE * U2NETP_INPUT_SIZE],
    )
    .map_err(|e| MlBgRemoverError::Inference(format!("reshape mask: {e}")))?;

    // 5. min-max 归一化 → u8
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for v in mask_2d.iter() {
        if *v < min {
            min = *v;
        }
        if *v > max {
            max = *v;
        }
    }
    let range = (max - min).max(1e-6);
    let mut mask_u8 = image::GrayImage::new(U2NETP_INPUT_SIZE as u32, U2NETP_INPUT_SIZE as u32);
    for ((y, x), v) in mask_2d.indexed_iter() {
        let scaled = ((*v - min) / range).clamp(0.0, 1.0) * 255.0;
        mask_u8.put_pixel(x as u32, y as u32, image::Luma([scaled as u8]));
    }
    let _ = (Axis(0),); // 引用 Axis 防止 ndarray 路径 warning

    // 升采样回原尺寸
    let mask_full = image::imageops::resize(&mask_u8, original_w, original_h, FilterType::Lanczos3);

    // 6. 复合：保留原 RGB，alpha = mask
    let original_rgba = img.to_rgba8();
    let mut out = RgbaImage::new(original_w, original_h);
    for (x, y, px) in original_rgba.enumerate_pixels() {
        let alpha = mask_full.get_pixel(x, y).0[0];
        out.put_pixel(x, y, Rgba([px.0[0], px.0[1], px.0[2], alpha]));
    }

    // 7. 编码
    let mut buf = Cursor::new(Vec::with_capacity(input_png.len()));
    out.write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| MlBgRemoverError::Encode(e.to_string()))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_err_preserves_decode_kind() {
        let mapped = map_err(MlBgRemoverError::Decode("x".into()));
        assert!(matches!(mapped, ImageProcError::Decode(_)));
    }

    #[test]
    fn map_err_preserves_encode_kind() {
        let mapped = map_err(MlBgRemoverError::Encode("x".into()));
        assert!(matches!(mapped, ImageProcError::Encode(_)));
    }

    #[test]
    fn map_err_preserves_inference_kind() {
        let mapped = map_err(MlBgRemoverError::Inference("kaboom".into()));
        assert!(matches!(mapped, ImageProcError::Runtime(message) if message == "kaboom"));
    }

    #[test]
    fn map_err_distinguishes_runtime_and_model_readiness() {
        assert!(matches!(
            map_err(MlBgRemoverError::OrtInit("missing runtime".into())),
            ImageProcError::NotReady(_)
        ));
        assert!(matches!(
            map_err(MlBgRemoverError::OrtSession("bad model".into())),
            ImageProcError::Model(_)
        ));
    }

    #[test]
    fn load_returns_err_for_missing_file() {
        let result = MlBgRemover::load(std::path::Path::new("/definitely/not/a/model.onnx"));
        let err = match result {
            Ok(_) => panic!("expected error from missing model path"),
            Err(e) => e,
        };
        assert!(matches!(err, MlBgRemoverError::OrtSession(_)));
    }

    #[test]
    fn init_missing_runtime_returns_error_without_panicking() {
        let err = init_ort_from_dylib(std::path::Path::new("/definitely/not/onnxruntime.dll"))
            .unwrap_err();
        assert!(matches!(err, MlBgRemoverError::OrtInit(_)));
        assert!(err.to_string().contains("does not exist"));
    }

    #[cfg(windows)]
    #[test]
    fn loader_panic_returns_actionable_vc_runtime_error() {
        let err = run_ort_initializer(std::path::Path::new("C:/runtime/onnxruntime.dll"), || {
            panic!("LoadLibraryExW failed")
        })
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Microsoft Visual C++ 2015-2022 Redistributable (x64)"));
        assert!(message.contains("LoadLibraryExW failed"));
    }
}
