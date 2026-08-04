use std::fs;
use std::io::Cursor;
use std::path::Path;

use ats_workspace::{PreparedResourceMedia, ResourceMediaProcessor, ResourceTransformOperation};
use thiserror::Error;

const PNG_MEDIA_TYPE: &str = "image/png";
const MAX_MEDIA_BYTES: usize = 64 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16_384;

#[derive(Debug, Error)]
pub enum ResourceMediaError {
    #[error("resource media path is invalid")]
    PathInvalid,
    #[error("resource media type is unsupported")]
    UnsupportedMediaType,
    #[error("resource media bytes are invalid")]
    InvalidMedia,
    #[error("resource media dimensions are unsupported")]
    InvalidDimensions,
    #[error("resource media transform is unsupported")]
    InvalidTransform,
    #[error("resource media I/O failed")]
    Io(#[source] std::io::Error),
    #[error("resource PNG decode failed")]
    Decode(#[source] png::DecodingError),
    #[error("resource PNG encode failed")]
    Encode(#[source] png::EncodingError),
}

pub struct PngResourceMediaProcessor;

impl ResourceMediaProcessor for PngResourceMediaProcessor {
    type Error = ResourceMediaError;

    fn prepare_file(
        &self,
        source_path: &Path,
        declared_media_type: &str,
    ) -> Result<PreparedResourceMedia, Self::Error> {
        let metadata = fs::symlink_metadata(source_path).map_err(ResourceMediaError::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ResourceMediaError::PathInvalid);
        }
        let bytes = fs::read(source_path).map_err(ResourceMediaError::Io)?;
        self.prepare_bytes(bytes, declared_media_type)
    }

    fn prepare_bytes(
        &self,
        bytes: Vec<u8>,
        declared_media_type: &str,
    ) -> Result<PreparedResourceMedia, Self::Error> {
        if declared_media_type != PNG_MEDIA_TYPE {
            return Err(ResourceMediaError::UnsupportedMediaType);
        }
        let decoded = decode_rgba(&bytes)?;
        Ok(PreparedResourceMedia {
            media_type: PNG_MEDIA_TYPE.into(),
            width: decoded.width,
            height: decoded.height,
            has_alpha: decoded.has_alpha,
            bytes,
        })
    }

    fn transform(
        &self,
        source: &PreparedResourceMedia,
        operation: &ResourceTransformOperation,
    ) -> Result<PreparedResourceMedia, Self::Error> {
        let decoded = decode_rgba(&source.bytes)?;
        let (width, height, rgba) = match operation {
            ResourceTransformOperation::Resize { width, height } => {
                validate_dimensions(*width, *height)?;
                (*width, *height, resize_nearest(&decoded, *width, *height)?)
            }
            ResourceTransformOperation::Outline {
                width,
                height,
                radius,
            } => {
                validate_dimensions(*width, *height)?;
                if *radius == 0 || *radius > 64 {
                    return Err(ResourceMediaError::InvalidTransform);
                }
                let resized = resize_nearest(&decoded, *width, *height)?;
                (
                    *width,
                    *height,
                    outline(&resized, *width, *height, *radius)?,
                )
            }
        };
        let bytes = encode_rgba(width, height, &rgba)?;
        Ok(PreparedResourceMedia {
            media_type: PNG_MEDIA_TYPE.into(),
            width,
            height,
            has_alpha: true,
            bytes,
        })
    }
}

struct DecodedRgba {
    width: u32,
    height: u32,
    has_alpha: bool,
    rgba: Vec<u8>,
}

fn decode_rgba(bytes: &[u8]) -> Result<DecodedRgba, ResourceMediaError> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
        return Err(ResourceMediaError::InvalidMedia);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(ResourceMediaError::Decode)?;
    let size = reader
        .output_buffer_size()
        .ok_or(ResourceMediaError::InvalidDimensions)?;
    if size > MAX_MEDIA_BYTES {
        return Err(ResourceMediaError::InvalidDimensions);
    }
    let mut output = vec![0; size];
    let info = reader
        .next_frame(&mut output)
        .map_err(ResourceMediaError::Decode)?;
    validate_dimensions(info.width, info.height)?;
    let source = &output[..info.buffer_size()];
    let (channels, has_alpha) = match info.color_type {
        png::ColorType::Grayscale => (1, false),
        png::ColorType::GrayscaleAlpha => (2, true),
        png::ColorType::Rgb => (3, false),
        png::ColorType::Rgba => (4, true),
        png::ColorType::Indexed => return Err(ResourceMediaError::InvalidMedia),
    };
    let pixels = pixel_count(info.width, info.height)?;
    if source.len() != pixels * channels {
        return Err(ResourceMediaError::InvalidMedia);
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in source.chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
            2 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]),
            3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            4 => rgba.extend_from_slice(pixel),
            _ => return Err(ResourceMediaError::InvalidMedia),
        }
    }
    Ok(DecodedRgba {
        width: info.width,
        height: info.height,
        has_alpha,
        rgba,
    })
}

fn resize_nearest(
    source: &DecodedRgba,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ResourceMediaError> {
    let target_pixels = pixel_count(width, height)?;
    let mut output = vec![0; target_pixels * 4];
    for y in 0..height {
        let source_y = u64::from(y) * u64::from(source.height) / u64::from(height);
        for x in 0..width {
            let source_x = u64::from(x) * u64::from(source.width) / u64::from(width);
            let source_index = usize::try_from((source_y * u64::from(source.width) + source_x) * 4)
                .map_err(|_| ResourceMediaError::InvalidDimensions)?;
            let target_index =
                usize::try_from((u64::from(y) * u64::from(width) + u64::from(x)) * 4)
                    .map_err(|_| ResourceMediaError::InvalidDimensions)?;
            output[target_index..target_index + 4]
                .copy_from_slice(&source.rgba[source_index..source_index + 4]);
        }
    }
    Ok(output)
}

fn outline(
    source: &[u8],
    width: u32,
    height: u32,
    radius: u32,
) -> Result<Vec<u8>, ResourceMediaError> {
    let mut output = vec![0; pixel_count(width, height)? * 4];
    let radius = i64::from(radius);
    for y in 0..i64::from(height) {
        for x in 0..i64::from(width) {
            let mut alpha = 0u8;
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    if offset_x * offset_x + offset_y * offset_y > radius * radius {
                        continue;
                    }
                    let sample_x = x + offset_x;
                    let sample_y = y + offset_y;
                    if sample_x < 0
                        || sample_y < 0
                        || sample_x >= i64::from(width)
                        || sample_y >= i64::from(height)
                    {
                        continue;
                    }
                    let index = usize::try_from((sample_y * i64::from(width) + sample_x) * 4 + 3)
                        .map_err(|_| ResourceMediaError::InvalidDimensions)?;
                    alpha = alpha.max(source[index]);
                }
            }
            let index = usize::try_from((y * i64::from(width) + x) * 4)
                .map_err(|_| ResourceMediaError::InvalidDimensions)?;
            output[index..index + 4].copy_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Ok(output)
}

fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ResourceMediaError> {
    if rgba.len() != pixel_count(width, height)? * 4 {
        return Err(ResourceMediaError::InvalidMedia);
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(ResourceMediaError::Encode)?;
        writer
            .write_image_data(rgba)
            .map_err(ResourceMediaError::Encode)?;
    }
    Ok(bytes)
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ResourceMediaError> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        Err(ResourceMediaError::InvalidDimensions)
    } else {
        pixel_count(width, height).map(|_| ())
    }
}

fn pixel_count(width: u32, height: u32) -> Result<usize, ResourceMediaError> {
    usize::try_from(u64::from(width) * u64::from(height))
        .ok()
        .filter(|count| count.saturating_mul(4) <= MAX_MEDIA_BYTES)
        .ok_or(ResourceMediaError::InvalidDimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_png(width: u32, height: u32) -> Vec<u8> {
        encode_rgba(
            width,
            height,
            &vec![128; pixel_count(width, height).unwrap() * 4],
        )
        .unwrap()
    }

    fn fixture_rgb_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![128; pixel_count(width, height).unwrap() * 3])
                .unwrap();
        }
        bytes
    }

    #[test]
    fn probes_and_transforms_png_deterministically() {
        let processor = PngResourceMediaProcessor;
        let prepared = processor
            .prepare_bytes(fixture_png(4, 4), PNG_MEDIA_TYPE)
            .unwrap();
        assert_eq!(
            (prepared.width, prepared.height, prepared.has_alpha),
            (4, 4, true)
        );
        let operation = ResourceTransformOperation::Resize {
            width: 2,
            height: 2,
        };
        let first = processor.transform(&prepared, &operation).unwrap();
        let second = processor.transform(&prepared, &operation).unwrap();
        assert_eq!(first, second);
        assert_eq!((first.width, first.height), (2, 2));
    }

    #[test]
    fn rejects_declared_type_mismatch_and_malformed_bytes() {
        let processor = PngResourceMediaProcessor;
        assert!(matches!(
            processor.prepare_bytes(fixture_png(1, 1), "image/jpeg"),
            Err(ResourceMediaError::UnsupportedMediaType)
        ));
        assert!(
            processor
                .prepare_bytes(b"not-png".to_vec(), PNG_MEDIA_TYPE)
                .is_err()
        );
        let rgb = processor
            .prepare_bytes(fixture_rgb_png(2, 3), PNG_MEDIA_TYPE)
            .unwrap();
        assert_eq!((rgb.width, rgb.height, rgb.has_alpha), (2, 3, false));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_symlinked_media_source() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target.png");
        fs::write(&target, fixture_png(1, 1)).unwrap();
        let link = temp.path().join("link.png");
        if symlink_file(&target, &link).is_err() {
            return;
        }
        assert!(matches!(
            PngResourceMediaProcessor.prepare_file(&link, PNG_MEDIA_TYPE),
            Err(ResourceMediaError::PathInvalid)
        ));
    }
}
