use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::rc::Rc;

use fltk::button::RadioButton;
use fltk::dialog::file_chooser;
use fltk::image::RgbImage;
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

use image::ImageFormat;
use image::imageops::overlay;
use image::io::Reader as ImageReader;
use image::{DynamicImage, ImageBuffer, ImageResult, Rgba}; // image 0.24 API
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale, point};

struct ImgState {
    original: Option<DynamicImage>, // pristine image when file was first loaded
    current: Option<DynamicImage>,  // possibly barcoded version
    path: Option<String>,           // current file path on disk
}

fn main() {
    let app = app::App::default();

    let mut win = Window::new(100, 100, 900, 900, "DocuCode");

    let mut vpack = Pack::new(10, 10, 880, 680, "");
    vpack.set_spacing(10);
    vpack.set_type(PackType::Vertical);

    // Top row: buttons
    let mut btn_row = Pack::new(0, 0, 880, 40, "");
    btn_row.set_type(PackType::Horizontal);
    btn_row.set_spacing(10);

    let mut open_btn = Button::new(0, 0, 200, 40, "Open TIFF…");
    let mut barcode_btn = Button::new(0, 0, 200, 40, "Add Barcode");
    let mut undo_btn = Button::new(0, 0, 200, 40, "Undo");

    btn_row.end();

    let labels = ["1", "2", "3", "4", "5", "6", "7", "10"];
    //Radio buttons for selecting doc category

    let mut radio_pack = Pack::new(0, 0, 880, 40, "");
    radio_pack.set_type(PackType::Horizontal);
    radio_pack.set_spacing(10);

    // Make 8 radios
    let mut radios: Vec<RadioButton> = labels
        .iter()
        .map(|label| {
            let mut r = RadioButton::new(0, 0, 100, 40, None);
            r.set_down_frame(fltk::enums::FrameType::DownBox);
            r.set_label(label);
            r
        })
        .collect();

    radio_pack.end();

    // Bottom row: left (list + textbox) + right (image)
    let mut bottom_row = Pack::new(0, 0, 880, 640, "");
    bottom_row.set_type(PackType::Horizontal);
    bottom_row.set_spacing(10);

    // Left column: listbox + textbox
    let mut left_col = Pack::new(0, 0, 220, 720, "");
    left_col.set_type(PackType::Vertical);
    left_col.set_spacing(10);

    let mut file_list = HoldBrowser::new(0, 0, 220, 580, "");
    file_list.set_frame(fltk::enums::FrameType::DownBox);

    let mut text_input = Input::new(0, 0, 220, 30, "");
    text_input.set_value(""); // empty by default
    text_input.set_frame(fltk::enums::FrameType::DownBox); // makes it clearly visible

    left_col.end();

    // make file_list resizable
    left_col.resizable(&file_list);
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
    let img_state = Rc::new(RefCell::new(ImgState {
        original: None,
        current: None,
        path: None,
    }));

    // -------------------------------
    // Open TIFF button (manual chooser)
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = img_state.clone();

        open_btn.set_callback(move |_| {
            if let Some(path) = file_chooser("Select TIFF", "*.tif\t*.tiff", ".", false) {
                match load_image(&path) {
                    Ok(img) => {
                        let mut st = img_state.borrow_mut();
                        st.original = Some(img.to_rgba8().into());
                        st.current = st.original.clone();
                        st.path = Some(path.clone());

                        if let Some(ref cur) = st.current {
                            redraw_image(&mut img_frame, cur);
                        }
                    }
                    Err(e) => dialog::message_default(&format!("Failed to load image:\n{e}")),
                }
            }
        });
    }

    // -------------------------------
    // Barcode button – add + overwrite file
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = img_state.clone();
        let text_input = text_input.clone();

        barcode_btn.set_callback(move |_| {
            // Build barcode contents: *EN-MP<textbox>*
            let user_text = text_input.value();
            let main_barcode_text = format!("*EN-MP{}*", user_text.trim());

            // Left barcode *DT-X___*
            let radio_label = selected_radio(&radios).unwrap_or_else(|| "1".to_string());
            let left_barcode_text = format!("*{}*", radio_label);

            // Load barcode font
            let font = match load_barcode_font("barcode.ttf") {
                Some(f) => f,
                None => {
                    dialog::message_default("Could not load barcode.ttf");
                    return;
                }
            };

            //Mutate image in place
            let mut need_save_path: Option<String> = None;
            {
                let mut st = img_state.borrow_mut();
                if let Some(ref mut img) = st.current {
                    add_font_barcodes_to_image(img, &font, &main_barcode_text, &left_barcode_text);
                    if let Some(ref p) = st.path {
                        need_save_path = Some(p.clone());
                    }
                } else {
                    dialog::message_default("Open or select an image first!");
                    return;
                }
            }

            // Overwrite file on disk with barcoded version
            if let Some(path) = need_save_path {
                let st = img_state.borrow();
                if let Some(ref img) = st.current {
                    if let Err(e) = save_tiff_gray(img, &path) {
                        dialog::message_default(&format!("Failed to save barcoded image:\n{e}"));
                    }
                    // Redisplay
                    redraw_image(&mut img_frame, img);
                }
            }
        });
    }

    // -------------------------------
    // Undo button – restore original + overwrite file
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = img_state.clone();

        undo_btn.set_callback(move |_| {
            // We'll compute this inside the borrow and use it after.
            let path_to_save: Option<String>;
            let current_image: Option<DynamicImage>;

            {
                let mut st = img_state.borrow_mut();

                // Check we actually have something to undo
                if st.original.is_none() || st.path.is_none() {
                    dialog::message_default("Nothing to undo.");
                    return;
                }

                // Clone out of the state while we still hold the mutable borrow
                let orig = st.original.as_ref().unwrap().clone();
                let path = st.path.as_ref().unwrap().clone();

                // Update current in the state
                st.current = Some(orig.clone());

                // Prepare data for use after we drop `st`
                path_to_save = Some(path);
                current_image = Some(orig);
            } // <- `st` borrow ends here

            // Now we can use the cloned values without any borrows in the way
            if let (Some(path), Some(img)) = (path_to_save, current_image) {
                if let Err(e) = save_tiff_gray(&img, &path) {
                    dialog::message_default(&format!("Failed to restore original image:\n{e}"));
                }
                redraw_image(&mut img_frame, &img);
            }
        });
    }

    // -------------------------------
    // Listbox click: load that TIFF from image_dir
    // -------------------------------
    {
        let mut img_frame = img_frame.clone();
        let img_state = img_state.clone();
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
                        let mut st = img_state.borrow_mut();
                        st.original = Some(img.to_rgba8().into());
                        st.current = st.original.clone();
                        st.path = Some(full_path.clone());

                        if let Some(ref cur) = st.current {
                            redraw_image(&mut img_frame, cur);
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
    let resized = img.resize_to_fill(fw as u32, fh as u32, image::imageops::FilterType::Nearest);
    if let Some(rgb) = dynamic_to_fltk(&resized) {
        frame.set_image(Some(rgb));
        frame.redraw();
    }
}

/// Load the barcode font from disk.
fn load_barcode_font(path: &str) -> Option<Font<'static>> {
    let data = fs::read(path).ok()?;
    Font::try_from_vec(data)
}

/// Get selected radio
fn selected_radio(radios: &Vec<fltk::button::RadioButton>) -> Option<String> {
    for r in radios {
        if r.is_toggled() {
            return Some(r.label());
        }
    }
    None
}

fn render_text_barcode_image(
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

fn add_font_barcodes_to_image(
    img: &mut DynamicImage,
    font: &Font<'static>,
    main_text: &str, // e.g. "*EN-MPxxx*"
    left_text: &str, // e.g. "*1*"
) {
    let mut base = img.to_rgba8();
    let (w, _h) = base.dimensions();

    // choose a barcode height in pixels
    let bar_height = 40u32;

    // right barcode
    let main_img = render_text_barcode_image(main_text, font, bar_height);
    let (mw, _mh) = main_img.dimensions();
    let right_x = w.saturating_sub(mw);
    let right_y = 0;
    overlay(&mut base, &main_img, right_x as i64, right_y as i64);

    // left barcode
    let left_img = render_text_barcode_image(left_text, font, bar_height);
    let left_x = 0;
    let left_y = 0;
    overlay(&mut base, &left_img, left_x, left_y);

    *img = DynamicImage::ImageRgba8(base);
}

fn save_tiff_gray(img: &DynamicImage, path: &str) -> ImageResult<()> {
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
