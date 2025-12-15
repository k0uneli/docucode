#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use fltk::{
    app,
    browser::HoldBrowser,
    button::{Button, RadioButton},
    dialog,
    enums::{Align, CallbackTrigger, Color, Event, EventState, Key, Shortcut},
    frame::Frame,
    group::{Pack, PackType},
    input::Input,
    menu::{Choice, MenuFlag, SysMenuBar},
    prelude::*,
    window::Window,
};

use image::DynamicImage; // image 0.24 API

mod image_utils;
use image_utils::{load_image, redraw_image, save_tiff_gray};

mod barcode;
use barcode::{add_font_barcodes_to_image, clear_barcodes_with_zxingcpp, load_barcode_font};

mod files;
use files::{populate_file_list, reload_tiff_list};

mod config;
use config::load_config;

mod settings;
use settings::show_settings_dialog;

mod employees;
use employees::load_employees_from_csv;

struct ImgState {
    original: Option<DynamicImage>, // pristine image when file was first loaded
    current: Option<DynamicImage>,  // possibly barcoded version
    path: Option<String>,           // current file path on disk
}

fn main() {
    let app = app::App::default();

    let config = Rc::new(RefCell::new(load_config()));

    let mut win = Window::new(100, 100, 1280, 1000, "DocuCode");

    let mut menubar = SysMenuBar::new(0, 0, 1000, 25, "");

    // Shared image state
    let img_state = Rc::new(RefCell::new(ImgState {
        original: None,
        current: None,
        path: None,
    }));

    // Right: image frame
    let mut img_frame = Frame::new(250, 135, 1000, 1414, "");
    img_frame.set_frame(fltk::enums::FrameType::DownBox);
    img_frame.set_align(Align::Inside | Align::Center);
    img_frame.set_color(Color::White);

    let mut f_for_menu = img_frame.clone();
    let img_state_for_menu = Rc::clone(&img_state);

    menubar.add(
        "&File/Open...\t", // shown as File → Open...
        fltk::enums::Shortcut::Ctrl | 'o',
        MenuFlag::Normal,
        move |_| {
            // TODO: call your existing "open" logic here
            println!("File > Open clicked");
            open_image_dialog(&img_state_for_menu, &mut f_for_menu);
        },
    );

    menubar.add(
        "&File/Quit\t",
        fltk::enums::Shortcut::Ctrl | 'q',
        MenuFlag::Normal,
        |_| {
            // simple quit
            app::quit();
        },
    );

    let mut f_for_undo = img_frame.clone();
    let img_state_for_undo = Rc::clone(&img_state);

    // Settings window
    {
        let cfg_rc = config.clone();
        menubar.add(
            "&Edit/Settings...\t",
            Shortcut::Ctrl | 's',
            MenuFlag::Normal,
            move |_| {
                show_settings_dialog(cfg_rc.clone());
            },
        );
    }

    menubar.add(
        "&Edit/Undo\t",
        fltk::enums::Shortcut::Ctrl | 'z',
        MenuFlag::Normal,
        move |_| {
            // you can call your undo logic here
            // We'll compute this inside the borrow and use it after.
            let path_to_save: Option<String>;
            let current_image: Option<DynamicImage>;

            {
                let mut st = img_state_for_undo.borrow_mut();

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
                redraw_image(&mut f_for_undo, &img);
            }
            println!("Edit > Undo clicked");
        },
    );

    menubar.add(
        "&Help/Tips\t",
        fltk::enums::Shortcut::Ctrl | 'h',
        MenuFlag::Normal,
        |_| {
            // simple quit
            print!("help");
            dialog::message_default(
                "Tips & Shortcuts:\n\n\
Ctrl+O  Open TIFF\n\
Ctrl+R  Reload list\n\
Ctrl+S  Settings\n\
Ctrl+Z  Undo (restore original)\n\
Ctrl+D  Clear barcodes\n\
Space   Add barcode\n\
Down    Select next file\n\
Alt+E/P/D/H/W/T/N  Set doc type dropdown\n\
Alt+1..7,0         Set category radio (0 = 100)\n\
\n\
Ctrl+Q  Quit\n\
Ctrl+H  This help\n\
Ctrl+A  About",
            );
        },
    );

    menubar.add(
        "&Help/About\t",
        fltk::enums::Shortcut::Ctrl | 'a',
        MenuFlag::Normal,
        |_| {
            // simple quit
            dialog::message_default(
                "DocuCode V2:\n\n\
             • Written by Lucas Valente 2023, 2025\n\
             • For Melbourne Pathology use\n\
             • For issues, please email me at lucasjamesvalente(at)gmail.com",
            );
            print!("help");
        },
    );

    let mut vpack = Pack::new(10, 35, 880, 680, "");
    vpack.set_spacing(10);
    vpack.set_type(PackType::Vertical);

    // Top row: buttons
    let mut btn_row = Pack::new(0, 25, 880, 40, "");
    btn_row.set_type(PackType::Horizontal);
    btn_row.set_spacing(10);

    let mut barcode_btn = Button::new(0, 25, 200, 40, "Add Barcode");
    let mut clear_btn = Button::new(0, 25, 200, 40, "Clear Barcodes");
    let mut reload_btn = Button::new(0, 25, 200, 40, "Refresh");

    let mut type_choice = Choice::new(0, 25, 80, 40, None);
    type_choice.add_choice("E|P|D|H|W|T|None");

    type_choice.set_value(0);
    btn_row.end();

    let labels = ["1", "2", "3", "4", "5", "6", "7", "100"];
    //Radio buttons for selecting doc category

    let mut radio_pack = Pack::new(0, 25, 880, 40, "");
    radio_pack.set_type(PackType::Horizontal);
    radio_pack.set_spacing(10);

    // Make 8 radios
    let mut radios: Vec<RadioButton> = labels
        .iter()
        .map(|label| {
            let mut r = RadioButton::new(0, 25, 100, 40, None);
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
    let mut left_col = Pack::new(0, 0, 220, 640, "");
    left_col.set_type(PackType::Vertical);
    left_col.set_spacing(10);

    let mut file_list = HoldBrowser::new(0, 0, 220, 380, "");
    file_list.set_frame(fltk::enums::FrameType::DownBox);

    let mut text_input = Input::new(0, 0, 220, 30, "");
    text_input.set_value(""); // empty by default
    text_input.set_frame(fltk::enums::FrameType::DownBox); // makes it clearly visible
    // Staff Lookup
    let mut search_input = Input::new(0, 0, 220, 30, "");
    let mut results_list = HoldBrowser::new(0, 0, 220, 170, "");
    results_list.set_frame(fltk::enums::FrameType::DownBox);

    search_input.set_trigger(CallbackTrigger::Changed);
    {
        let cfg_for_search = config.clone();
        let mut results_list_clone = results_list.clone();
        let mut text_input_clone = text_input.clone();

        search_input.set_callback(move |inp| {
            let q = inp.value();

            let csv_path = {
                let cfg = cfg_for_search.borrow();
                cfg.csv_path.clone()
            };

            let employees = load_employees_from_csv(&csv_path).unwrap_or_default();
            results_list_clone.clear();

            if q.trim().is_empty() {
                return; // no query then empty list TODO:MAYBE show all
            }

            for emp in employees.iter().filter(|e| e.matches_query(&q)) {
                let line = format!("{}, {} ({})", emp.last, emp.first, emp.number);
                results_list_clone.add(&line);
            }
        });
    }

    // Clicking a search result fills the staff number textbox
    {
        let mut text_input_clone = text_input.clone();
        results_list.set_callback(move |list| {
            let idx = list.value();
            if idx <= 0 {
                return;
            }
            if let Some(line) = list.text(idx) {
                if let (Some(start), Some(end)) = (line.rfind('('), line.rfind(')')) {
                    if start + 1 < end {
                        let number = &line[start + 1..end];
                        text_input_clone.set_value(number.trim());
                    }
                }
            }
        });
    }
    left_col.end();

    // make file_list resizable
    left_col.resizable(&file_list);

    bottom_row.end();
    vpack.end();
    win.end();
    win.show();

    // Now we have image dir populate.
    {
        let cfg_rc = config.clone();
        let mut file_list_clone = file_list.clone();

        // Set callback for button
        reload_btn.set_callback(move |_| {
            let tiff_dir = {
                let cfg = cfg_rc.borrow();
                cfg.tiff_dir.clone()
            };
            reload_tiff_list(&mut file_list_clone, &tiff_dir);
        });
    }
    // Set callback for menu
    {
        let cfg_rc = config.clone();
        let mut file_list_clone = file_list.clone();
        menubar.add(
            "&File/Reload\t",
            fltk::enums::Shortcut::Ctrl | 'r',
            MenuFlag::Normal,
            move |_| {
                let tiff_dir = {
                    let cfg = cfg_rc.borrow();
                    cfg.tiff_dir.clone()
                };
                reload_tiff_list(&mut file_list_clone, &tiff_dir);
            },
        );
    }
    let image_dir = {
        let cfg = config.borrow();
        cfg.tiff_dir.clone()
    };

    if image_dir.is_empty() {
        dialog::message_default("config.txt missing or empty; defaulting to current directory.");
    }
    // Fill listbox with TIFF filenames
    if let Err(e) = populate_file_list(&mut file_list, &image_dir) {
        dialog::message_default(&format!("Failed to read image directory:\n{e}"));
    }

    // -------------------------------
    // Clear barcodes button - detect + overwrite file
    // -------------------------------
    {
        let mut f_for_clear = img_frame.clone();
        let img_state = Rc::clone(&img_state);
        let cfg_for_clear = config.clone();

        clear_btn.set_callback(move |_| {
            let mut need_save_path: Option<String> = None;
            let mut cleared = 0usize;
            let bar_height_hint = {
                let cfg = cfg_for_clear.borrow();
                cfg.bar_height
            };

            {
                let mut st = img_state.borrow_mut();
                let img = match st.current.as_mut() {
                    Some(img) => img,
                    None => {
                        dialog::message_default("Open or select an image first!");
                        return;
                    }
                };

                match clear_barcodes_with_zxingcpp(img, bar_height_hint) {
                    Ok(count) => {
                        cleared = count;
                        if let Some(ref p) = st.path {
                            need_save_path = Some(p.clone());
                        }
                    }
                    Err(e) => {
                        dialog::message_default(&format!("Failed to clear barcodes:\n{e}"));
                        return;
                    }
                }
            }

            if cleared == 0 {
                dialog::message_default("No barcodes detected to clear.");
                return;
            }

            if let Some(path) = need_save_path {
                let st = img_state.borrow();
                if let Some(ref img) = st.current {
                    if let Err(e) = save_tiff_gray(img, &path) {
                        dialog::message_default(&format!("Failed to save cleared image:\n{e}"));
                        return;
                    }
                    redraw_image(&mut f_for_clear, img);
                }
            }
        });
    }

    // -------------------------------
    // Barcode button - add + overwrite file
    // -------------------------------
    {
        let mut f_for_btn = img_frame.clone();
        let img_state = Rc::clone(&img_state);
        let text_input = text_input.clone();
        let type_choice_cb = type_choice.clone();
        let radios_cb = radios.clone();

        barcode_btn.set_callback(move |_| {
            // Build barcode contents: *EN-MP<textbox>*
            let user_text = text_input.value();
            let main_barcode_text = format!("*EN-MP{}*", user_text.trim());

            // Left barcode *DT-X___*
            let radio_label = selected_radio(&radios_cb).unwrap_or_else(|| "1".to_string());
            let document_code = selected_type(&type_choice_cb).unwrap_or_else(|| "E".to_string());
            let left_barcode_text = if document_code == "None" {
                "".to_string()
            } else {
                format!("*DT-{}{}*", document_code, radio_label)
            };
            // Load barcode font
            let font = match load_barcode_font("barcode.ttf") {
                Some(f) => f,
                None => {
                    dialog::message_default("Could not load barcode.ttf");
                    return;
                }
            };

            // Mutate image in place
            let mut need_save_path: Option<String> = None;
            let bar_height = {
                let cfg = config.borrow();
                cfg.bar_height
            };
            {
                let mut st = img_state.borrow_mut();
                if let Some(ref mut img) = st.current {
                    add_font_barcodes_to_image(
                        img,
                        &font,
                        &main_barcode_text,
                        &left_barcode_text,
                        bar_height,
                    );
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
                    redraw_image(&mut f_for_btn, img);
                }
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
                let full_path = Path::new(&image_dir).join(&filename);
                match load_image(full_path.to_str().unwrap()) {
                    Ok(img) => {
                        let mut st = img_state.borrow_mut();
                        st.original = Some(img.to_rgba8().into());
                        st.current = st.original.clone();
                        st.path = Some(full_path.to_string_lossy().into_owned());

                        if let Some(ref cur) = st.current {
                            redraw_image(&mut img_frame, cur);
                        }
                    }
                    Err(e) => dialog::message_default(&format!("Failed to load image:\n{e}")),
                }
            }
        });
    }

    // Global keyboard shortcuts handled at the window level
    {
        let mut file_list_keys = file_list.clone();
        let mut barcode_btn_keys = barcode_btn.clone();
        let mut clear_btn_keys = clear_btn.clone();
        let mut type_choice_keys = type_choice.clone();
        let mut radios_keys = radios.clone();

        win.handle(move |_, ev| {
            if ev != Event::KeyDown {
                return false;
            }

            let key = app::event_key();
            let state = app::event_state();
            let alt = state.contains(EventState::Alt);
            let ctrl = state.contains(EventState::Ctrl);

            // Down arrow: select next item in the listbox
            if key == Key::Down {
                let size = file_list_keys.size();
                if size > 0 {
                    let current = file_list_keys.value();
                    let next = if current <= 0 {
                        1
                    } else {
                        (current + 1).min(size)
                    };
                    if next != current {
                        file_list_keys.select(next);
                        file_list_keys.do_callback();
                    }
                }
                return true;
            }

            // Space: add barcode
            if key == Key::from_char(' ') {
                barcode_btn_keys.do_callback();
                return true;
            }

            // Ctrl+D: clear barcodes
            if ctrl && (key == Key::from_char('d') || key == Key::from_char('D')) {
                clear_btn_keys.do_callback();
                return true;
            }

            if alt {
                // Alt+letter: set document type dropdown
                if let Some(idx) = match key {
                    k if k == Key::from_char('e') || k == Key::from_char('E') => Some(0),
                    k if k == Key::from_char('p') || k == Key::from_char('P') => Some(1),
                    k if k == Key::from_char('d') || k == Key::from_char('D') => Some(2),
                    k if k == Key::from_char('h') || k == Key::from_char('H') => Some(3),
                    k if k == Key::from_char('w') || k == Key::from_char('W') => Some(4),
                    k if k == Key::from_char('t') || k == Key::from_char('T') => Some(5),
                    k if k == Key::from_char('n') || k == Key::from_char('N') => Some(6),
                    _ => None,
                } {
                    type_choice_keys.set_value(idx);
                    return true;
                }

                // Alt+number: set radio buttons (0 maps to "100")
                if let Some(idx) = match key {
                    k if k == Key::from_char('1') => Some(0),
                    k if k == Key::from_char('2') => Some(1),
                    k if k == Key::from_char('3') => Some(2),
                    k if k == Key::from_char('4') => Some(3),
                    k if k == Key::from_char('5') => Some(4),
                    k if k == Key::from_char('6') => Some(5),
                    k if k == Key::from_char('7') => Some(6),
                    k if k == Key::from_char('0') => Some(7),
                    _ => None,
                } {
                    for (i, r) in radios_keys.iter_mut().enumerate() {
                        r.set_value(i == idx);
                    }
                    return true;
                }
            }

            false
        });
    }

    app.run().unwrap();
}

/* ---------- Helpers ---------- */

/// Get selected radio
fn selected_radio(radios: &Vec<fltk::button::RadioButton>) -> Option<String> {
    for r in radios {
        if r.is_toggled() {
            if r.label() == "100" {
                return Some(r.label());
            } else {
                return Some("00".to_owned() + &r.label());
            }
        }
    }
    None
}

// Return selected doc type (from dropdown menu)
fn selected_type(choice: &Choice) -> Option<String> {
    choice.text(choice.value()).map(|s| s.to_string())
}

fn open_image_dialog(img_state: &Rc<RefCell<ImgState>>, img_frame: &mut Frame) {
    if let Some(path_str) =
        dialog::file_chooser("Select Scan(.tiff/.tif)", "*.tif\t*.tiff", ".", false)
    {
        // Turn the returned string into a PathBuf
        let path = PathBuf::from(&path_str);

        // load_image takes &str, so convert PathBuf → &str
        match load_image(path.to_str().unwrap()) {
            Ok(img) => {
                let mut st = img_state.borrow_mut();
                st.original = Some(img.to_rgb8().into());
                st.current = st.original.clone();

                // Store the path as a String in ImgState
                st.path = Some(path.to_string_lossy().into_owned());

                if let Some(ref cur) = st.current {
                    redraw_image(img_frame, cur);
                }
            }
            Err(e) => dialog::message_default(&format!("Failed to load image:\n{e}")),
        }
    }
}
