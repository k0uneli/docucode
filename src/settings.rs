use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    button::Button,
    dialog,
    group::{Pack, PackType},
    input::Input,
    prelude::*,
    window::Window,
};

use crate::config::{AppConfig, save_config};

pub fn show_settings_dialog(config: Rc<RefCell<AppConfig>>) {
    // Copy current values out & drop the borrow immediately
    let (tiff_dir, csv_path) = {
        let cfg = config.borrow();
        (cfg.tiff_dir.clone(), cfg.csv_path.clone())
    }; // <-- borrow ends here

    // Make a small modal window
    let mut win = Window::new(200, 200, 720, 160, "Settings");

    let mut vpack = Pack::new(250, 10, 500, 140, "");
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
                tiff_input_clone.set_value(&path);
            }
        });
    }

    // Browse CSV
    {
        let mut csv_input_clone = csv_input.clone();
        csv_browse.set_callback(move |_| {
            if let Some(path) = dialog::file_chooser("Choose CSV file", "*.csv", ".", true) {
                csv_input_clone.set_value(&path);
            }
        });
    }

    // OK: single short-lived mutable borrow
    {
        let cfg_rc = config.clone();
        let mut win_clone = win.clone();
        let mut tiff_input_ok = tiff_input.clone();
        let mut csv_input_ok = csv_input.clone();

        ok_btn.set_callback(move |_| {
            {
                let mut cfg = cfg_rc.borrow_mut(); // <-- your line 95
                cfg.tiff_dir = tiff_input_ok.value();
                cfg.csv_path = csv_input_ok.value();

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
