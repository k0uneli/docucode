use image::imageops::overlay;
use image::{DynamicImage, ImageBuffer, Rgba};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale, point};
use std::fs;

/// Load the barcode font from disk.
pub fn load_barcode_font(path: &str) -> Option<Font<'static>> {
    let data = fs::read(path).ok()?;
    Font::try_from_vec(data)
}
pub fn render_text_barcode_image(
    text: &str,
    font: &Font<'static>,
    target_height: u32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    // font size
    let scale = Scale {
        x: target_height as f32,
        y: target_height as f32,
    };

    let v_metrics = font.v_metrics(scale);

    // baseline so the text fits within the image
    let baseline = target_height as f32;

    // lay out glyphs
    let glyphs: Vec<_> = font.layout(text, scale, point(0.0, baseline)).collect();

    // compute tight bounding box
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for g in &glyphs {
        if let Some(bb) = g.pixel_bounding_box() {
            min_x = min_x.min(bb.min.x);
            min_y = min_y.min(bb.min.y);
            max_x = max_x.max(bb.max.x);
            max_y = max_y.max(bb.max.y);
        }
    }

    if min_x == i32::MAX {
        // empty string fallback: 1x1 white
        return ImageBuffer::from_pixel(1, 1, Rgba([255, 255, 255, 255]));
    }

    let width = (max_x - min_x) as u32;
    let height = (max_y - min_y) as u32;

    // white background
    let mut img = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));

    // draw text shifted so it fits in the image
    let offset_x = -min_x;
    let offset_y = -min_y;

    draw_text_mut(
        &mut img,
        Rgba([0, 0, 0, 255]),
        offset_x,
        offset_y,
        scale,
        font,
        text,
    );

    img
}

pub fn add_font_barcodes_to_image(
    img: &mut DynamicImage,
    font: &Font<'static>,
    main_text: &str, // e.g. "*EN-MPxxx*"
    left_text: &str, // e.g. "*1*"
) {
    let mut base = img.to_rgba8();
    let (w, _h) = base.dimensions();

    // choose a barcode height in pixels
    let bar_height = 75u32;

    // right barcode
    let main_img = render_text_barcode_image(main_text, font, bar_height);
    let (mw, _mh) = main_img.dimensions();
    let right_x = w.saturating_sub(mw);
    let right_y = 0;
    if main_text != "*EN-MP*" {
        overlay(&mut base, &main_img, right_x as i64, right_y as i64);
    }

    // left barcode
    let left_img = render_text_barcode_image(left_text, font, bar_height);
    let left_x = 0;
    let left_y = 0;
    if left_text != "*DT-None*" {
        overlay(&mut base, &left_img, left_x, left_y);
    }
    *img = DynamicImage::ImageRgba8(base);
}
