// src/image_utils.rs

use std::fs::File;
use std::io::Write;
use std::path::Path;

use fltk::frame::Frame;
use fltk::image::RgbImage;
use fltk::prelude::WidgetExt;

use fax::encoder::Encoder as FaxEncoder;
use fax::tiff as fax_tiff;
use fax::{Color as FaxColor, VecWriter};
use image::GenericImageView;
use image::error::DecodingError;
use image::imageops::FilterType;
use image::io::Reader as ImageReader;
use image::{DynamicImage, ImageBuffer, ImageError, ImageFormat, ImageResult, Luma, Rgba};
use imageproc::contrast::otsu_level;

use tiff::decoder::{Decoder, DecodingResult};
use tiff::{TiffError, TiffFormatError};

/// Main entry point: load any image, with TIFF/CCITT fallback.
pub fn load_image(path: &str) -> ImageResult<DynamicImage> {
    let p = Path::new(path);

    // 1) Try the normal `image` crate first
    let primary: Result<DynamicImage, ImageError> = (|| {
        let reader = ImageReader::open(p)?;
        reader.decode()
    })();

    match primary {
        Ok(img) => Ok(img),
        Err(primary_err) => {
            // Only try CCITT/TIFF fallback for .tif/.tiff
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if ext == "tif" || ext == "tiff" {
                match load_ccitt_tiff(p) {
                    Ok(img) => Ok(img),
                    Err(e) => {
                        // Wrap the TIFF error into an image::ImageError
                        Err(ImageError::Decoding(DecodingError::new(
                            ImageFormat::Tiff.into(),
                            e,
                        )))
                    }
                }
            } else {
                // Non-TIFF → just return the original error
                Err(primary_err)
            }
        }
    }
}

fn load_ccitt_tiff(path: &Path) -> Result<DynamicImage, TiffError> {
    let file = File::open(path)?;
    let mut decoder = Decoder::new(file)?;

    let (w, h) = decoder.dimensions()?;
    let pixels = (w as usize) * (h as usize);

    match decoder.read_image()? {
        // 8-bit grayscale (w*h bytes) – trivial case
        DecodingResult::U8(buf) => {
            let len = buf.len();

            if len == pixels {
                // Standard 8-bit gray
                let img = ImageBuffer::<Luma<u8>, _>::from_vec(w, h, buf).ok_or_else(|| {
                    TiffError::FormatError(TiffFormatError::InvalidDimensions(w, h))
                })?;
                Ok(DynamicImage::ImageLuma8(img))
            } else {
                // Likely 1-bit-packed bilevel (CCITT/fax style)
                let bytes_per_row = ((w + 7) / 8) as usize;
                let expected = bytes_per_row * (h as usize);

                if len != expected {
                    return Err(TiffError::FormatError(
                        TiffFormatError::CompressedDataCorrupt(format!(
                            "unexpected U8 buffer length {len} for {w}x{h} (expected {expected})"
                        )),
                    ));
                }

                // Expand 1-bit pixels -> 8-bit grayscale (0 or 255)
                let mut out = vec![0u8; pixels];

                for y in 0..h {
                    let row_base = (y as usize) * bytes_per_row;
                    for x in 0..w {
                        let x_us = x as usize;
                        let byte_index = row_base + x_us / 8;
                        let bit_index = 7 - (x_us % 8);
                        let byte = buf[byte_index];
                        let bit = (byte >> bit_index) & 1;

                        // Flip mapping: 0 = black, 1 = white for your scans
                        let val = if bit == 0 { 0u8 } else { 255u8 };
                        out[(y * w + x) as usize] = val;
                    }
                }

                let img = ImageBuffer::<Luma<u8>, _>::from_vec(w, h, out).ok_or_else(|| {
                    TiffError::FormatError(TiffFormatError::InvalidDimensions(w, h))
                })?;
                Ok(DynamicImage::ImageLuma8(img))
            }
        }

        // 16-bit grayscale: downscale to 8-bit
        DecodingResult::U16(buf) => {
            let mut out = Vec::with_capacity(pixels);
            for v in buf {
                out.push((v >> 8) as u8);
            }

            let img = ImageBuffer::<Luma<u8>, _>::from_vec(w, h, out)
                .ok_or_else(|| TiffError::FormatError(TiffFormatError::InvalidDimensions(w, h)))?;
            Ok(DynamicImage::ImageLuma8(img))
        }

        other => Err(TiffError::FormatError(
            TiffFormatError::CompressedDataCorrupt(format!(
                "unsupported TIFF decoding result: {other:?}"
            )),
        )),
    }
}

/// Convert DynamicImage → fltk::image::RgbImage
pub fn dynamic_to_fltk(img: &DynamicImage) -> Option<RgbImage> {
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    let bytes = img.into_raw();
    RgbImage::new(&bytes, w as i32, h as i32, fltk::enums::ColorDepth::Rgba8).ok()
}

/// Resize image to frame size and display it.
pub fn redraw_image(frame: &mut Frame, img: &DynamicImage) {
    let (fw, fh) = (frame.w(), frame.h());
    let resized = img.resize_to_fill(fw as u32, fh as u32, FilterType::Nearest);
    if let Some(rgb) = dynamic_to_fltk(&resized) {
        frame.set_image(Some(rgb));
        frame.redraw();
    }
}

pub fn save_tiff_gray(img: &DynamicImage, path: &str) -> ImageResult<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let (width, height) = img.dimensions();

    if width > u16::MAX as u32 {
        return Err(ImageError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "CCITT T.6 encoder supports widths up to u16::MAX",
        )));
    }

    // Convert to 1-bit bilevel (black/white) to align with CCITT Group 4 expectations.
    let gray = img.to_luma8();
    let mut fax_encoder = FaxEncoder::new(VecWriter::new());

    for row in gray.chunks_exact(width as usize) {
        fax_encoder
            .encode_line(
                row.iter().map(|&v| {
                    if v < 128 {
                        FaxColor::Black
                    } else {
                        FaxColor::White
                    }
                }),
                width as u16,
            )
            .expect("fax encoder for group 4 should be infallible");
    }

    // Flush remaining bits and wrap the fax stream into a minimal TIFF header.
    let fax_bytes = fax_encoder
        .finish()
        .expect("VecWriter for fax encoding should be infallible")
        .finish();

    let tiff_bytes = fax_tiff::wrap(&fax_bytes, width, height);

    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(&tiff_bytes)?;
    writer.flush()?;

    Ok(())
}

/// Attempt to locate barcode-like regions and paint them white one-by-one.
///
/// Uses a dark-pixel connected-component search with size and coverage heuristics that
/// roughly match the configured barcode height. Returns the number of cleared regions.
pub fn whiten_barcodes(img: &mut DynamicImage, bar_height: u32) -> usize {
    let mut rgba = img.to_rgba8();
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();

    if w == 0 || h == 0 {
        return 0;
    }

    let threshold = otsu_level(&gray);
    let mut visited = vec![false; (w * h) as usize];
    let mut cleared_regions = 0usize;

    // Per-component row/column tracking to verify that bars span most rows/columns.
    let mut row_marks = vec![false; h as usize];
    let mut col_marks = vec![false; w as usize];

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if visited[idx] {
                continue;
            }

            let pix = gray.get_pixel(x, y).0[0];
            if pix > threshold {
                visited[idx] = true;
                continue;
            }

            // Flood fill to find the connected dark region.
            let mut stack = vec![(x, y)];
            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut dark_pixels: u64 = 0;
            let mut rows_used: Vec<u32> = Vec::new();
            let mut cols_used: Vec<u32> = Vec::new();

            while let Some((cx, cy)) = stack.pop() {
                let cidx = (cy * w + cx) as usize;
                if visited[cidx] {
                    continue;
                }
                visited[cidx] = true;

                let p = gray.get_pixel(cx, cy).0[0];
                if p > threshold {
                    continue;
                }

                dark_pixels += 1;
                if let Some(mark) = row_marks.get_mut(cy as usize) {
                    if !*mark {
                        *mark = true;
                        rows_used.push(cy);
                    }
                }
                if let Some(mark) = col_marks.get_mut(cx as usize) {
                    if !*mark {
                        *mark = true;
                        cols_used.push(cx);
                    }
                }

                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                // 4-neighbour flood fill across the whole page.
                let neighbours = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];

                for (nx, ny) in neighbours {
                    if nx < w && ny < h {
                        let nidx = (ny * w + nx) as usize;
                        if !visited[nidx] {
                            stack.push((nx, ny));
                        }
                    }
                }
            }

            let region_height = max_y - min_y + 1;
            let region_width = max_x - min_x + 1;
            let area = (region_height as u64) * (region_width as u64);
            let dark_ratio = dark_pixels as f32 / area as f32;

            let target_h = bar_height.max(40);
            let min_h = ((target_h as f32) * 0.6).max(20.0) as u32;
            let max_h = ((target_h as f32) * 1.8).max(min_h as f32) as u32;
            let min_w = ((target_h as f32) * 0.3).max(10.0) as u32;
            let max_w = ((target_h as f32) * 6.0).min(w.max(1) as f32) as u32;

            let dark_rows = rows_used.len() as f32;
            let dark_cols = cols_used.len() as f32;
            let row_coverage = dark_rows / (region_height as f32);
            let col_coverage = dark_cols / (region_width as f32);

            let looks_like_barcode = region_height >= min_h
                && region_height <= max_h
                && region_width >= min_w
                && region_width <= max_w
                && dark_ratio >= 0.2
                && dark_ratio <= 0.7
                && row_coverage >= 0.7
                && col_coverage >= 0.2;

            if looks_like_barcode {
                let margin = 2;
                let from_x = min_x.saturating_sub(margin);
                let to_x = (max_x + margin).min(w.saturating_sub(1));
                let from_y = min_y.saturating_sub(margin);
                let to_y = (max_y + margin).min(h.saturating_sub(1));

                for yy in from_y..=to_y {
                    for xx in from_x..=to_x {
                        let idx = ((yy * w + xx) as usize) * 4;
                        let buffer = rgba.as_mut();
                        buffer[idx] = 255;
                        buffer[idx + 1] = 255;
                        buffer[idx + 2] = 255;
                        buffer[idx + 3] = 255;
                    }
                }

                cleared_regions += 1;
            }

            for &ry in &rows_used {
                if let Some(slot) = row_marks.get_mut(ry as usize) {
                    *slot = false;
                }
            }
            for &cx in &cols_used {
                if let Some(slot) = col_marks.get_mut(cx as usize) {
                    *slot = false;
                }
            }
        }
    }

    *img = DynamicImage::ImageRgba8(rgba);
    cleared_regions
}
