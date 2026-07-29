use image::RgbaImage;
use serde::Serialize;

use super::ImageProcError;

const TRANSPARENT_ALPHA_MAX: u8 = 8;
const OPAQUE_ALPHA_MIN: u8 = 247;
const NEUTRAL_CHANNEL_TOLERANCE: u8 = 24;
const NEUTRAL_LUMINANCE_MIN: u16 = 140;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageQualitySpec {
    pub min_transparent_fraction: f64,
    pub min_foreground_fraction: f64,
    pub min_largest_component_fraction: f64,
    pub max_edge_foreground_fraction: f64,
    pub max_edge_neutral_foreground_fraction: f64,
}

impl Default for ImageQualitySpec {
    fn default() -> Self {
        Self {
            min_transparent_fraction: 0.05,
            min_foreground_fraction: 0.01,
            min_largest_component_fraction: 0.60,
            max_edge_foreground_fraction: 0.30,
            max_edge_neutral_foreground_fraction: 0.15,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImageQualityIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageQualityReport {
    pub spec: ImageQualitySpec,
    pub width: u32,
    pub height: u32,
    pub total_pixels: u64,
    pub transparent_pixels: u64,
    pub opaque_pixels: u64,
    pub partial_alpha_pixels: u64,
    pub foreground_pixels: u64,
    pub foreground_components: u32,
    pub largest_component_pixels: u64,
    pub edge_pixels: u64,
    pub edge_foreground_pixels: u64,
    pub edge_neutral_foreground_pixels: u64,
    pub transparent_fraction: f64,
    pub foreground_fraction: f64,
    pub largest_component_fraction: f64,
    pub edge_foreground_fraction: f64,
    pub edge_neutral_foreground_fraction: f64,
    pub accepted: bool,
    pub issues: Vec<ImageQualityIssue>,
}

impl ImageQualityReport {
    #[must_use]
    pub fn rejection_summary(&self) -> String {
        self.issues
            .iter()
            .map(|issue| format!("{}: {}", issue.code, issue.message))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub fn analyze_png_quality(
    input_png: &[u8],
    spec: ImageQualitySpec,
) -> Result<ImageQualityReport, ImageProcError> {
    let image = image::load_from_memory(input_png)
        .map_err(|err| ImageProcError::Decode(err.to_string()))?
        .to_rgba8();
    Ok(analyze_rgba(&image, spec))
}

fn analyze_rgba(image: &RgbaImage, spec: ImageQualitySpec) -> ImageQualityReport {
    let (width, height) = image.dimensions();
    let total_pixels = u64::from(width) * u64::from(height);
    let mut transparent_pixels = 0_u64;
    let mut opaque_pixels = 0_u64;
    let mut partial_alpha_pixels = 0_u64;
    let mut foreground_pixels = 0_u64;
    let mut edge_pixels = 0_u64;
    let mut edge_foreground_pixels = 0_u64;
    let mut edge_neutral_foreground_pixels = 0_u64;

    for (x, y, pixel) in image.enumerate_pixels() {
        let [red, green, blue, alpha] = pixel.0;
        if alpha <= TRANSPARENT_ALPHA_MAX {
            transparent_pixels += 1;
        } else {
            foreground_pixels += 1;
            if alpha >= OPAQUE_ALPHA_MIN {
                opaque_pixels += 1;
            } else {
                partial_alpha_pixels += 1;
            }
        }

        let is_edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
        if is_edge {
            edge_pixels += 1;
            if alpha > TRANSPARENT_ALPHA_MAX {
                edge_foreground_pixels += 1;
                let min_channel = red.min(green).min(blue);
                let max_channel = red.max(green).max(blue);
                let luminance = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
                if max_channel.abs_diff(min_channel) <= NEUTRAL_CHANNEL_TOLERANCE
                    && luminance >= NEUTRAL_LUMINANCE_MIN
                {
                    edge_neutral_foreground_pixels += 1;
                }
            }
        }
    }

    let transparent_fraction = fraction(transparent_pixels, total_pixels);
    let foreground_fraction = fraction(foreground_pixels, total_pixels);
    let (foreground_components, largest_component_pixels) = connected_components(image);
    let largest_component_fraction = fraction(largest_component_pixels, foreground_pixels);
    let edge_foreground_fraction = fraction(edge_foreground_pixels, edge_pixels);
    let edge_neutral_foreground_fraction = fraction(edge_neutral_foreground_pixels, edge_pixels);
    let mut issues = Vec::new();
    if transparent_fraction < spec.min_transparent_fraction {
        issues.push(ImageQualityIssue {
            code: "invalid_alpha".into(),
            message: format!(
                "transparent coverage {:.2}% is below required {:.2}%",
                transparent_fraction * 100.0,
                spec.min_transparent_fraction * 100.0
            ),
        });
    }
    if foreground_fraction < spec.min_foreground_fraction {
        issues.push(ImageQualityIssue {
            code: "missing_foreground".into(),
            message: format!(
                "foreground coverage {:.2}% is below required {:.2}%",
                foreground_fraction * 100.0,
                spec.min_foreground_fraction * 100.0
            ),
        });
    }
    if foreground_pixels > 0 && largest_component_fraction < spec.min_largest_component_fraction {
        issues.push(ImageQualityIssue {
            code: "fragmented_foreground".into(),
            message: format!(
                "largest connected foreground contains {:.2}% of foreground pixels, below required {:.2}%",
                largest_component_fraction * 100.0,
                spec.min_largest_component_fraction * 100.0
            ),
        });
    }
    if edge_foreground_fraction > spec.max_edge_foreground_fraction {
        issues.push(ImageQualityIssue {
            code: "foreground_touches_edge".into(),
            message: format!(
                "foreground occupies {:.2}% of border pixels, above allowed {:.2}%",
                edge_foreground_fraction * 100.0,
                spec.max_edge_foreground_fraction * 100.0
            ),
        });
    }
    if edge_neutral_foreground_fraction > spec.max_edge_neutral_foreground_fraction {
        issues.push(ImageQualityIssue {
            code: "likely_background_residue".into(),
            message: format!(
                "light neutral residue occupies {:.2}% of border pixels, above allowed {:.2}%",
                edge_neutral_foreground_fraction * 100.0,
                spec.max_edge_neutral_foreground_fraction * 100.0
            ),
        });
    }

    ImageQualityReport {
        spec,
        width,
        height,
        total_pixels,
        transparent_pixels,
        opaque_pixels,
        partial_alpha_pixels,
        foreground_pixels,
        foreground_components,
        largest_component_pixels,
        edge_pixels,
        edge_foreground_pixels,
        edge_neutral_foreground_pixels,
        transparent_fraction,
        foreground_fraction,
        largest_component_fraction,
        edge_foreground_fraction,
        edge_neutral_foreground_fraction,
        accepted: issues.is_empty(),
        issues,
    }
}

fn connected_components(image: &RgbaImage) -> (u32, u64) {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let mut visited = vec![false; width.saturating_mul(height)];
    let mut components = 0_u32;
    let mut largest = 0_u64;
    let mut stack = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if visited[index] || image.get_pixel(x as u32, y as u32).0[3] <= TRANSPARENT_ALPHA_MAX {
                continue;
            }
            components = components.saturating_add(1);
            visited[index] = true;
            stack.push((x, y));
            let mut size = 0_u64;
            while let Some((current_x, current_y)) = stack.pop() {
                size += 1;
                for (next_x, next_y) in neighbors(current_x, current_y, width, height) {
                    let next_index = next_y * width + next_x;
                    if visited[next_index]
                        || image.get_pixel(next_x as u32, next_y as u32).0[3]
                            <= TRANSPARENT_ALPHA_MAX
                    {
                        continue;
                    }
                    visited[next_index] = true;
                    stack.push((next_x, next_y));
                }
            }
            largest = largest.max(size);
        }
    }
    (components, largest)
}

fn neighbors(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let mut values = [(0, 0); 4];
    let mut count = 0;
    if x > 0 {
        values[count] = (x - 1, y);
        count += 1;
    }
    if x + 1 < width {
        values[count] = (x + 1, y);
        count += 1;
    }
    if y > 0 {
        values[count] = (x, y - 1);
        count += 1;
    }
    if y + 1 < height {
        values[count] = (x, y + 1);
        count += 1;
    }
    values.into_iter().take(count)
}

fn fraction(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, Rgba};

    use super::*;

    fn encode(image: &RgbaImage) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png).unwrap();
        output.into_inner()
    }

    #[test]
    fn accepts_centered_transparent_subject() {
        let mut image = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for y in 16..48 {
            for x in 16..48 {
                image.put_pixel(x, y, Rgba([200, 40, 30, 255]));
            }
        }

        let report = analyze_png_quality(&encode(&image), ImageQualitySpec::default()).unwrap();

        assert!(report.accepted, "issues={:?}", report.issues);
        assert_eq!(report.edge_foreground_pixels, 0);
        assert!(report.transparent_fraction > 0.70);
    }

    #[test]
    fn rejects_fully_opaque_image() {
        let image = RgbaImage::from_pixel(32, 32, Rgba([30, 80, 120, 255]));

        let report = analyze_png_quality(&encode(&image), ImageQualitySpec::default()).unwrap();

        assert!(!report.accepted);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "invalid_alpha")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "foreground_touches_edge")
        );
    }

    #[test]
    fn rejects_checkerboard_residue_on_border() {
        let mut image = RgbaImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let value = if (x / 8 + y / 8) % 2 == 0 { 0 } else { 190 };
                let alpha = if value == 0 { 0 } else { 255 };
                image.put_pixel(x, y, Rgba([value, value, value, alpha]));
            }
        }
        for y in 20..44 {
            for x in 20..44 {
                image.put_pixel(x, y, Rgba([180, 20, 20, 255]));
            }
        }

        let report = analyze_png_quality(&encode(&image), ImageQualitySpec::default()).unwrap();

        assert!(!report.accepted);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "likely_background_residue")
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "fragmented_foreground")
        );
        assert!(report.foreground_components > 2);
    }
}
