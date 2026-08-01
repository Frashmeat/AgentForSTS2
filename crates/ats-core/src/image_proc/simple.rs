//! 启发式背景去除：把"看起来像白色 / 浅色"的像素 alpha 改为 0。
//!
//! 算法：
//! 1. 解码 PNG → RGBA 缓冲（image crate）
//! 2. 对每个像素：若 R,G,B 都 ≥ threshold 且彼此差 ≤ tolerance，置 alpha = 0
//! 3. 编码为 PNG
//!
//! 默认参数（threshold=235, tolerance=20）适合 AI 文生图常见的纯白 / 浅灰背景。
//! 对带渐变、复杂背景不敏感；那种情况留给后续 ort 实现处理。

use async_trait::async_trait;
use image::{ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

use super::{ImageProcClient, ImageProcError};
use crate::cancellation::CancellationToken;

pub struct SimpleBgRemover {
    /// 亮度阈值：R / G / B 每个通道都 ≥ 此值才视作"背景候选"
    pub threshold: u8,
    /// 通道间最大差：abs(R-G), abs(R-B), abs(G-B) 都 ≤ 此值才认为是"灰度色"（避免误判鲜艳色块）
    pub tolerance: u8,
}

impl Default for SimpleBgRemover {
    fn default() -> Self {
        Self {
            threshold: 235,
            tolerance: 20,
        }
    }
}

#[async_trait]
impl ImageProcClient for SimpleBgRemover {
    async fn remove_background(
        &self,
        input_png: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, ImageProcError> {
        if cancellation.is_cancelled() {
            return Err(ImageProcError::Cancelled);
        }
        let threshold = self.threshold;
        let tolerance = self.tolerance;
        let input = input_png.to_vec();
        let worker_cancellation = cancellation.clone();
        tokio::task::spawn_blocking(move || {
            if worker_cancellation.is_cancelled() {
                return Err(ImageProcError::Cancelled);
            }
            let result = remove_white_background_inner(&input, threshold, tolerance)?;
            if worker_cancellation.is_cancelled() {
                Err(ImageProcError::Cancelled)
            } else {
                Ok(result)
            }
        })
        .await
        .map_err(|e| ImageProcError::Decode(format!("join: {e}")))?
    }
}

/// 同步入口，方便测试和命令行调用。
///
/// # Errors
/// - `ImageProcError::Decode` / `Encode`：解码或编码失败
pub fn remove_white_background(
    input_png: &[u8],
    threshold: u8,
    tolerance: u8,
) -> Result<Vec<u8>, ImageProcError> {
    remove_white_background_inner(input_png, threshold, tolerance)
}

fn remove_white_background_inner(
    input_png: &[u8],
    threshold: u8,
    tolerance: u8,
) -> Result<Vec<u8>, ImageProcError> {
    let img = image::load_from_memory(input_png)
        .map_err(|e| ImageProcError::Decode(e.to_string()))?
        .to_rgba8();

    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels() {
        let [r, g, b, a] = px.0;
        let is_bg = r >= threshold
            && g >= threshold
            && b >= threshold
            && r.abs_diff(g) <= tolerance
            && r.abs_diff(b) <= tolerance
            && g.abs_diff(b) <= tolerance;
        out.put_pixel(x, y, Rgba([r, g, b, if is_bg { 0 } else { a }]));
    }

    let mut buf = Cursor::new(Vec::with_capacity(input_png.len()));
    out.write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| ImageProcError::Encode(e.to_string()))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn make_test_png(w: u32, h: u32, fill: [u8; 4]) -> Vec<u8> {
        let img = ImageBuffer::from_pixel(w, h, Rgba(fill));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn make_two_color_png() -> Vec<u8> {
        let mut img: RgbaImage = ImageBuffer::new(4, 2);
        // 第一行：白色（应被改为透明）
        for x in 0..4 {
            img.put_pixel(x, 0, Rgba([255, 255, 255, 255]));
        }
        // 第二行：鲜艳红色（应保留）
        for x in 0..4 {
            img.put_pixel(x, 1, Rgba([200, 30, 30, 255]));
        }
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn removes_white_pixels_only() {
        let input = make_two_color_png();
        let output = remove_white_background(&input, 235, 20).unwrap();
        let img = image::load_from_memory(&output).unwrap().to_rgba8();

        // 第一行应该 alpha=0（白色被识别为背景）
        for x in 0..4 {
            assert_eq!(
                img.get_pixel(x, 0).0[3],
                0,
                "white row should be transparent"
            );
        }
        // 第二行应该保留 alpha=255（红色非背景）
        for x in 0..4 {
            assert_eq!(img.get_pixel(x, 1).0[3], 255, "red row should be opaque");
        }
    }

    #[test]
    fn preserves_image_dimensions() {
        let input = make_test_png(32, 16, [128, 128, 128, 255]);
        let output = remove_white_background(&input, 235, 20).unwrap();
        let img = image::load_from_memory(&output).unwrap();
        assert_eq!(img.width(), 32);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn rejects_invalid_input() {
        let err = remove_white_background(b"not a png", 235, 20).unwrap_err();
        assert!(matches!(err, ImageProcError::Decode(_)));
    }

    #[test]
    fn vivid_colors_unaffected_even_above_threshold() {
        // R=255,G=200,B=0（亮但通道差大）→ tolerance 检查排除背景
        let input = make_test_png(2, 2, [255, 200, 0, 255]);
        let output = remove_white_background(&input, 235, 20).unwrap();
        let img = image::load_from_memory(&output).unwrap().to_rgba8();
        assert_eq!(
            img.get_pixel(0, 0).0[3],
            255,
            "vivid color should stay opaque"
        );
    }

    #[tokio::test]
    async fn async_client_round_trip() {
        let client = SimpleBgRemover::default();
        let input = make_two_color_png();
        let output = client
            .remove_background(&input, &CancellationToken::new())
            .await
            .unwrap();
        let img = image::load_from_memory(&output).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0[3], 0); // 白色行透明
        assert_eq!(img.get_pixel(0, 1).0[3], 255); // 红色行不透明
    }
}
