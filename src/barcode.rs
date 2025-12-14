use image::imageops::overlay;
use image::{DynamicImage, ImageBuffer, Rgba};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale, point};
use std::fs;
use zxingcpp::{self, BarcodeFormat, BarcodeFormats};

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

    let _v_metrics = font.v_metrics(scale);

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
    bar_height: u32,
) {
    let mut base = img.to_rgba8();
    let (w, _h) = base.dimensions();

    // choose a barcode height in pixels
    //let bar_height = 200u32;

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

/// Detect barcodes with zxing-cpp and paint white rectangles over them.
/// Padding is generous to account for positional jitter in some readers.
pub fn clear_barcodes_with_zxingcpp(
    img: &mut DynamicImage,
    bar_height_hint: u32,
) -> Result<usize, String> {
    let luma = img.to_luma8();
    let (width, height) = luma.dimensions();

    let formats = BarcodeFormats::from(BarcodeFormat::Any);
    let reader = zxingcpp::read()
        .formats(formats)
        .try_invert(true)
        .try_rotate(true)
        .try_downscale(true)
        .max_number_of_symbols(32)
        .min_line_count(1);

    let results = reader
        .from(&luma)
        .map_err(|e| format!("zxing-cpp failed to detect barcodes: {e}"))?;

    if results.is_empty() {
        return Ok(0);
    }

    let mut canvas = img.to_rgba8();
    let mut cleared = 0usize;

    for res in results {
        let pos = res.position();
        let xs = [
            pos.top_left.x,
            pos.top_right.x,
            pos.bottom_left.x,
            pos.bottom_right.x,
        ];
        let ys = [
            pos.top_left.y,
            pos.top_right.y,
            pos.bottom_left.y,
            pos.bottom_right.y,
        ];

        let (min_x, max_x) = xs
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let (min_y, max_y) = ys
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));

        if min_x == i32::MAX || min_y == i32::MAX {
            continue;
        }

        let span_x = (max_x - min_x).abs().max(1) as f32;
        let pad_x = (span_x * 0.25).max(20.0);
        let span_y = (max_y - min_y).abs().max(1) as f32;
        let vertical_pad = ((bar_height_hint as f32) * 0.75)
            .max(span_y * 1.75)
            .max(24.0);

        let left = (min_x as f32 - pad_x)
            .floor()
            .clamp(0.0, (width.saturating_sub(1)) as f32) as u32;
        let right = (max_x as f32 + pad_x)
            .ceil()
            .clamp(0.0, (width.saturating_sub(1)) as f32) as u32;
        let center_y = (min_y + max_y) as f32 / 2.0;
        let top = (center_y - vertical_pad)
            .floor()
            .clamp(0.0, (height.saturating_sub(1)) as f32) as u32;
        let bottom = (center_y + vertical_pad)
            .ceil()
            .clamp(0.0, (height.saturating_sub(1)) as f32) as u32;

        if right < left || bottom < top {
            continue;
        }

        for y in top..=bottom {
            for x in left..=right {
                canvas.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        cleared += 1;
    }

    if cleared > 0 {
        *img = DynamicImage::ImageRgba8(canvas);
    }

    Ok(cleared)
}
