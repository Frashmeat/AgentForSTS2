use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage, imageops::FilterType};

use super::ImageProcError;

const ALPHA_FOREGROUND_MIN: u8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageVariantRole {
    Normal,
    Outline,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageVariantTransform {
    Cover,
    Outline { radius: u32 },
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageVariantSpec {
    pub role: ImageVariantRole,
    pub width: u32,
    pub height: u32,
    pub transform: ImageVariantTransform,
}

#[derive(Debug, Clone)]
pub struct DerivedImageVariant {
    pub role: ImageVariantRole,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn derive_png_variants(
    input_png: &[u8],
    specs: &[ImageVariantSpec],
) -> Result<Vec<DerivedImageVariant>, ImageProcError> {
    let source = image::load_from_memory(input_png)
        .map_err(|err| ImageProcError::Decode(err.to_string()))?
        .to_rgba8();
    specs
        .iter()
        .map(|spec| derive_variant(&source, *spec))
        .collect()
}

fn derive_variant(
    source: &RgbaImage,
    spec: ImageVariantSpec,
) -> Result<DerivedImageVariant, ImageProcError> {
    let image = match spec.transform {
        ImageVariantTransform::Preserve => source.clone(),
        ImageVariantTransform::Cover => cover_resize(source, spec.width, spec.height)?,
        ImageVariantTransform::Outline { radius } => {
            let normal = cover_resize(source, spec.width, spec.height)?;
            outline_from_alpha(&normal, radius)
        }
    };
    let (width, height) = image.dimensions();
    let bytes = encode_png(&image)?;
    Ok(DerivedImageVariant {
        role: spec.role,
        bytes,
        width,
        height,
    })
}

fn cover_resize(
    source: &RgbaImage,
    target_width: u32,
    target_height: u32,
) -> Result<RgbaImage, ImageProcError> {
    if target_width == 0 || target_height == 0 || source.width() == 0 || source.height() == 0 {
        return Err(ImageProcError::Unsupported(
            "image variant dimensions must be non-zero".into(),
        ));
    }
    let width_scale = f64::from(target_width) / f64::from(source.width());
    let height_scale = f64::from(target_height) / f64::from(source.height());
    let scale = width_scale.max(height_scale);
    let resized_width = (f64::from(source.width()) * scale)
        .ceil()
        .max(f64::from(target_width)) as u32;
    let resized_height = (f64::from(source.height()) * scale)
        .ceil()
        .max(f64::from(target_height)) as u32;
    let resized =
        image::imageops::resize(source, resized_width, resized_height, FilterType::Lanczos3);
    let left = (resized_width - target_width) / 2;
    let top = (resized_height - target_height) / 2;
    Ok(image::imageops::crop_imm(&resized, left, top, target_width, target_height).to_image())
}

fn outline_from_alpha(source: &RgbaImage, radius: u32) -> RgbaImage {
    let (width, height) = source.dimensions();
    let mut outline = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 0]));
    let radius = i64::from(radius);
    for y in 0..height {
        for x in 0..width {
            if source.get_pixel(x, y).0[3] >= ALPHA_FOREGROUND_MIN {
                continue;
            }
            let mut alpha = 0_u8;
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    if offset_x * offset_x + offset_y * offset_y > radius * radius {
                        continue;
                    }
                    let sample_x = i64::from(x) + offset_x;
                    let sample_y = i64::from(y) + offset_y;
                    if sample_x < 0
                        || sample_y < 0
                        || sample_x >= i64::from(width)
                        || sample_y >= i64::from(height)
                    {
                        continue;
                    }
                    alpha = alpha.max(source.get_pixel(sample_x as u32, sample_y as u32).0[3]);
                }
            }
            if alpha >= ALPHA_FOREGROUND_MIN {
                outline.put_pixel(x, y, Rgba([255, 255, 255, alpha]));
            }
        }
    }
    outline
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, ImageProcError> {
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|err| ImageProcError::Encode(err.to_string()))?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject_png() -> Vec<u8> {
        let mut image = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for y in 16..48 {
            for x in 16..48 {
                image.put_pixel(x, y, Rgba([220, 40, 20, 255]));
            }
        }
        encode_png(&image).unwrap()
    }

    #[test]
    fn derives_role_specific_dimensions_and_bytes() {
        let specs = [
            ImageVariantSpec {
                role: ImageVariantRole::Normal,
                width: 32,
                height: 32,
                transform: ImageVariantTransform::Cover,
            },
            ImageVariantSpec {
                role: ImageVariantRole::Outline,
                width: 32,
                height: 32,
                transform: ImageVariantTransform::Outline { radius: 2 },
            },
            ImageVariantSpec {
                role: ImageVariantRole::Big,
                width: 64,
                height: 64,
                transform: ImageVariantTransform::Cover,
            },
        ];

        let variants = derive_png_variants(&subject_png(), &specs).unwrap();

        assert_eq!(variants.len(), 3);
        assert_eq!((variants[0].width, variants[0].height), (32, 32));
        assert_eq!((variants[1].width, variants[1].height), (32, 32));
        assert_eq!((variants[2].width, variants[2].height), (64, 64));
        assert_ne!(variants[0].bytes, variants[1].bytes);
        assert_ne!(variants[0].bytes, variants[2].bytes);

        let outline = image::load_from_memory(&variants[1].bytes)
            .unwrap()
            .to_rgba8();
        assert_eq!(
            outline.get_pixel(16, 16).0[3],
            0,
            "outline center must be hollow"
        );
        assert!(outline.pixels().any(|pixel| pixel.0[3] > 0));
    }
}
