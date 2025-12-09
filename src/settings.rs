use std::cell::RefCell;
use std::env;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use fltk::{
    button::Button,
    dialog,
    frame::Frame,
    group::{Pack, PackType},
    input::Input,
    output::Output,
    prelude::*,
    valuator::{SliderType, ValueSlider},
    window::Window,
};

use crate::config::{AppConfig, save_config};

pub fn show_settings_dialog(config: Rc<RefCell<AppConfig>>) {
    // Copy current values out & drop the borrow immediately
    let (tiff_dir, csv_path, bar_height) = {
        let cfg = config.borrow();
        (cfg.tiff_dir.clone(), cfg.csv_path.clone(), cfg.bar_height)
    }; // <-- borrow ends here

    // Make a small modal window
    let mut win = Window::new(200, 200, 720, 220, "Settings");

    let mut vpack = Pack::new(250, 10, 500, 200, "");
    vpack.set_spacing(10);
    vpack.set_type(PackType::Vertical);

    // --- TIFF folder row ---
    let mut tiff_row = Pack::new(0, 0, 500, 30, "");
    tiff_row.set_type(PackType::Horizontal);
    tiff_row.set_spacing(5);

    let mut tiff_input = Input::new(0, 0, 360, 25, "Scans folder:");
    tiff_input.set_value(&tiff_dir);

    let mut tiff_browse = Button::new(0, 0, 100, 25, "Browse…");
    tiff_row.end();

    // --- CSV file row ---
    let mut csv_row = Pack::new(0, 0, 500, 30, "");
    csv_row.set_type(PackType::Horizontal);
    csv_row.set_spacing(5);

    let mut csv_input = Input::new(0, 0, 360, 25, "Staff Number .csv spreadsheet:");
    csv_input.set_value(&csv_path);

    let mut csv_browse = Button::new(0, 0, 100, 25, "Browse…");
    csv_row.end();

    // --- Barcode height row ---
    let mut height_row = Pack::new(0, 0, 500, 40, "");
    height_row.set_type(PackType::Horizontal);
    height_row.set_spacing(10);

    let mut bar_label = Frame::new(0, 0, 140, 25, "Barcode height:");
    bar_label.set_label_size(14);

    let mut bar_slider = ValueSlider::new(0, 0, 260, 25, "");
    bar_slider.set_type(SliderType::Horizontal);
    bar_slider.set_bounds(80.0, 400.0);
    bar_slider.set_precision(0);
    bar_slider.set_value(bar_height as f64);

    let mut bar_output = Output::new(0, 0, 80, 25, "");
    bar_output.set_value(&format!("{} px", bar_height));

    {
        let mut bar_output_clone = bar_output.clone();
        bar_slider.set_callback(move |s| {
            bar_output_clone.set_value(&format!("{:.0} px", s.value()));
        });
    }

    height_row.end();

    // --- OK / Cancel row ---
    let mut btn_row = Pack::new(0, 0, 500, 30, "");
    btn_row.set_type(PackType::Horizontal);
    btn_row.set_spacing(10);

    let mut ok_btn = Button::new(0, 0, 80, 25, "OK");
    let mut cancel_btn = Button::new(0, 0, 80, 25, "Cancel");
    btn_row.end();

    vpack.end();
    win.end();
    win.make_modal(true);
    win.show();

    // Browse TIFF
    {
        let mut tiff_input_clone = tiff_input.clone();
        tiff_browse.set_callback(move |_| {
            if let Some(path) = dialog::dir_chooser("Choose TIFF folder", ".", true) {
                let absolute = absolutize_path(&path);
                tiff_input_clone.set_value(&absolute);
            }
        });
    }

    // Browse CSV
    {
        let mut csv_input_clone = csv_input.clone();
        csv_browse.set_callback(move |_| {
            if let Some(path) = dialog::file_chooser("Choose CSV file", "*.csv", ".", true) {
                let absolute = absolutize_path(&path);
                csv_input_clone.set_value(&absolute);
            }
        });
    }

    // OK: single short-lived mutable borrow
    {
        let cfg_rc = config.clone();
        let mut win_clone = win.clone();
        let mut tiff_input_ok = tiff_input.clone();
        let mut csv_input_ok = csv_input.clone();
        let mut bar_slider_ok = bar_slider.clone();

        ok_btn.set_callback(move |_| {
            {
                let mut cfg = cfg_rc.borrow_mut(); // <-- your line 95
                cfg.tiff_dir = absolutize_path(&tiff_input_ok.value());
                cfg.csv_path = absolutize_path(&csv_input_ok.value());
                cfg.bar_height = bar_slider_ok.value().round() as u32;

                if let Err(e) = save_config(&cfg) {
                    dialog::alert_default(&format!("Failed to save config:\n{e}"));
                }
            } // <-- mutable borrow ends here

            win_clone.hide();
        });
    }

    // Cancel: just close
    {
        let mut win_clone = win.clone();
        cancel_btn.set_callback(move |_| {
            win_clone.hide();
        });
    }
}

fn absolutize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = absolutize_candidate(trimmed);
    match candidate.canonicalize() {
        Ok(clean) => clean,
        Err(_) => normalize_path(&candidate),
    }
    .to_string_lossy()
    .into_owned()
}

fn absolutize_candidate(path: &str) -> PathBuf {
    let input_path = Path::new(path);
    if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(input_path)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
