use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::rc::Rc;

use fltk::{
    app,
    browser::HoldBrowser,
    button::Button,
    dialog,
    enums::{Align, Color},
    frame::Frame,
    group::{Pack, PackType},
    input::Input,
    prelude::*,
    window::Window,
};
use fltk::dialog::file_chooser;
use fltk::image::RgbImage;

use image::{DynamicImage, GenericImageView, ImageBuffer, ImageResult, Rgba};
use image::io::Reader as ImageReader;

use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};

fn main() {
    let app = app::App::default();

    let mut win = Window::new(100, 100, 900, 700, "TIFF Viewer + Barcode + List + Text");

    let mut vpack = Pack::new(10, 10, 880, 680, "");
    vpack.set_spacing(10);
    vpack.set_type(PackType::Vertical);

    // Top row: buttons
    let mut btn_row = Pack::new(0, 0, 880, 40, "");
    btn_row.set_type(PackType::Horizontal);
    btn_row.set_spacing(10);

    let mut open_btn = Button::new(0, 0, 200, 40, "Open TIFF…");
    let mut barcode_btn = Button::new(0, 0, 200, 40, "Add Barcode");

    btn_row.end();

    // Bottom row: left (list + textbox) + right (image)
    let mut bottom_row = Pack::new(0, 0, 880, 640, "");
    bottom_row.set_type(PackType::Horizontal);
    bottom_row.set_spacing(10);

    // Left column: listbox + textbox
    let mut left_col = Pack::new(0, 0, 220, 640, "");
    left_col.set_type(PackType::Vertical);
    left_col.set_spacing(10);

    let mut file_list = HoldBrowser::new(0, 0, 220, 580, "");
    file_list.set_frame(fltk::enums::FrameType::DownBox);

    let mut text_input = Input::new(0, 0, 220, 30, "");
    text_input.set_value(""); // empty by default

    left_col.end();

    // Right: image frame
    let mut img_frame = Frame::new(0, 0, 650, 640, "");
    img_frame.set_frame(fltk::enums::FrameType::DownBox);
    img_frame.set_align(Align::Inside | Align::Center);
    img_frame.set_color(Color::White);

    bottom_row.end();
    vpack.end();
    win.end();
    win.show();

    // Read config (directory path)
    let image_dir = Rc::new(read_config_dir("config.txt"));
    if image_dir.is_empty() {
        dialog::message_default("config.txt missing or empty; defaulting to current directory.");
    }

    // Fill listbox with TIFF filenames
    if let Err(e) = populate_file_list(&mut file_list, &image_dir) {
        dialog::message_default(&format!("Failed to read image directory:\n{e}"));
    }

    // Shared image state
    let current_image: Rc<RefCell<Option<DynamicImage>>> = Rc::new(RefCell::new(None));

    // -------------------------------
    // Open TIFF button (manual chooser)
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = current_image.clone();

        open_btn.set_callback(move |_| {
            if let Some(path) = file_chooser("Select TIFF", "*.tif\t*.tiff", ".", false) {
                match load_image(&path) {
                    Ok(img) => {
                        *img_state.borrow_mut() = Some(img.to_rgba8().into());
                        if let Some(ref img) = *img_state.borrow() {
                            redraw_image(&mut img_frame, img);
                        }
                    }
                    Err(e) => dialog::message_default(&format!("Failed to load image:\n{e}")),
                }
            }
        });
    }

    // -------------------------------
    // Barcode button – uses barcode.ttf, top-right, small
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = current_image.clone();
        let text_input = text_input.clone();

        barcode_btn.set_callback(move |_| {
            // Build barcode contents: *EN-MP<textbox>*
            let user_text = text_input.value();
            let barcode_text = format!("*EN-MP{}*", user_text.trim());

            // Load barcode font
            let font = match load_barcode_font("barcode.ttf") {
                Some(f) => f,
                None => {
                    dialog::message_default("Could not load barcode.ttf");
                    return;
                }
            };

            // Mutate current image
            let had_image = {
                let mut opt = img_state.borrow_mut();
                if let Some(ref mut img) = *opt {
                    add_barcode_with_font(img, &barcode_text, &font);
                    true
                } else {
                    false
                }
            };

            if !had_image {
                dialog::message_default("Open or select an image first!");
                return;
            }

            // Redisplay
            if let Some(ref img) = *img_state.borrow() {
                redraw_image(&mut img_frame, img);
            }
        });
    }

    // -------------------------------
    // Listbox click: load that TIFF from image_dir
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = current_image.clone();
        let image_dir = image_dir.clone();

        file_list.set_callback(move |b| {
            let idx = b.value(); // selected line
            if idx == 0 {
                return;
            }
            if let Some(filename) = b.text(idx) {
                let full_path = format!("{}/{}", image_dir, filename);
                match load_image(&full_path) {
                    Ok(img) => {
                        *img_state.borrow_mut() = Some(img.to_rgba8().into());
                        if let Some(ref img) = *img_state.borrow() {
                            redraw_image(&mut img_frame, img);
                        }
                    }
                    Err(e) => dialog::message_default(&format!("Failed to load image:\n{e}")),
                }
            }
        });
    }

    app.run().unwrap();
}

/* ---------- Helpers ---------- */

/// Read config file: first non-empty line is the directory path.
/// Returns "." if file can't be read.
fn read_config_dir(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(".")
            .to_string(),
        Err(_) => ".".to_string(),
    }
}

/// Populate the file list with all .tif / .tiff in image_dir.
fn populate_file_list(list: &mut HoldBrowser, image_dir: &str) -> io::Result<()> {
    list.clear();

    let dir = Path::new(image_dir);
    if !dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Directory does not exist: {image_dir}"),
        ));
    }

    let mut entries: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            let lower = name.to_lowercase();
            if lower.ends_with(".tif") || lower.ends_with(".tiff") {
                entries.push(name.to_string());
            }
        }
    }

    entries.sort();
    for name in entries {
        list.add(&name);
    }

    Ok(())
}

/// Load an image file into DynamicImage.
fn load_image(path: &str) -> ImageResult<DynamicImage> {
    ImageReader::open(path)?.decode()
}

/// Convert DynamicImage → fltk::image::RgbImage
fn dynamic_to_fltk(img: &DynamicImage) -> Option<RgbImage> {
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    let bytes = img.into_raw();
    RgbImage::new(&bytes, w as i32, h as i32, fltk::enums::ColorDepth::Rgba8).ok()
}

/// Resize image to frame size and display it.
fn redraw_image(frame: &mut Frame, img: &DynamicImage) {
    let (fw, fh) = (frame.w(), frame.h());
    let resized = img.resize_to_fill(
        fw as u32,
        fh as u32,
        image::imageops::FilterType::Nearest,
    );
    if let Some(rgb) = dynamic_to_fltk(&resized) {
        frame.set_image(Some(rgb));
        frame.redraw();
    }
}

/// Load the barcode font from disk.
fn load_barcode_font(path: &str) -> Option<Font<'static>> {
    let data = fs::read(path).ok()?;
    // try_from_vec leaks the data internally so the Font can live 'static
    Font::try_from_vec(data)
}

/// Draw barcode text in the top-right corner, as small as is reasonable.
fn add_barcode_with_font(img: &mut DynamicImage, barcode_text: &str, font: &Font<'static>) {
    let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> = img.to_rgba8();
    let (w, h) = buf.dimensions();

    // Small font: based on image height, but with a low cap
    let target_height = ((h as f32) / 20.0).max(10.0); // ~5% of height, minimum 10px
    let scale = Scale {
        x: target_height,
        y: target_height,
    };

    // Compute text width
    let v_metrics = font.v_metrics(scale);
    let glyphs = font.layout(barcode_text, scale, rusttype::point(0.0, 0.0));
    let text_width: i32 = glyphs
        .clone()
        .filter_map(|g| g.pixel_bounding_box())
        .map(|bb| bb.max.x)
        .max()
        .unwrap_or(0);

    let margin = 5i32;

    let x = (w as i32 - text_width - margin).max(0);
    let y = (margin as f32 + v_metrics.ascent) as i32; // near top

    draw_text_mut(
        &mut buf,
        Rgba([0, 0, 0, 255]),
        x,
        y,
        scale,
        font,
        barcode_text,
    );

    *img = DynamicImage::ImageRgba8(buf);
}
