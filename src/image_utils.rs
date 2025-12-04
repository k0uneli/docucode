use fltk::frame::Frame;
use fltk::image::RgbImage;
use image::{DynamicImage, ImageBuffer, ImageFormat, ImageResult, Rgba};

use fltk::prelude::*;
use image::io::Reader as ImageReader;

/// Load an image file into DynamicImage.
pub fn load_image(path: &str) -> ImageResult<DynamicImage> {
    ImageReader::open(path)?.decode()
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
    let resized = img.resize_to_fill(fw as u32, fh as u32, image::imageops::FilterType::Nearest);
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
