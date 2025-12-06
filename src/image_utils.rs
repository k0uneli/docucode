// src/image_utils.rs

use std::fs::File;
use std::path::Path;

use fltk::frame::Frame;
use fltk::image::RgbImage;
use fltk::prelude::WidgetExt;

use image::io::Reader as ImageReader;
use image::{
    DynamicImage, ImageBuffer, ImageError, ImageFormat, ImageResult, Luma, Rgba,
};
use image::error::DecodingError;
use image::imageops::FilterType;

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
                let img = ImageBuffer::<Luma<u8>, _>::from_vec(w, h, buf)
                    .ok_or_else(|| {
                        TiffError::FormatError(
                            TiffFormatError::InvalidDimensions(w, h)
                        )
                    })?;
                Ok(DynamicImage::ImageLuma8(img))
            } else {
                // Likely 1-bit-packed bilevel (CCITT/fax style)
                let bytes_per_row = ((w + 7) / 8) as usize;
                let expected = bytes_per_row * (h as usize);

                if len != expected {
                    return Err(TiffError::FormatError(
                        TiffFormatError::CompressedDataCorrupt(
                            format!(
                                "unexpected U8 buffer length {len} for {w}x{h} (expected {expected})"
                            ),
                        ),
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

                let img = ImageBuffer::<Luma<u8>, _>::from_vec(w, h, out)
                    .ok_or_else(|| {
                        TiffError::FormatError(
                            TiffFormatError::InvalidDimensions(w, h)
                        )
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
                .ok_or_else(|| {
                    TiffError::FormatError(
                        TiffFormatError::InvalidDimensions(w, h)
                    )
                })?;
            Ok(DynamicImage::ImageLuma8(img))
        }

        other => Err(TiffError::FormatError(
            TiffFormatError::CompressedDataCorrupt(
                format!("unsupported TIFF decoding result: {other:?}"),
            ),
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

    // Convert to 8-bit grayscale to reduce size
    let gray = img.to_luma8();
    let dyn_gray = DynamicImage::ImageLuma8(gray);

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // Let `image` encode a grayscale TIFF with its default comould i use the fax compressionpression
    dyn_gray.write_to(&mut writer, ImageFormat::Tiff)
}
