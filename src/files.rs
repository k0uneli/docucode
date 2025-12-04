use fltk::browser::HoldBrowser;
use fltk::prelude::*;
use std::path::Path;
use std::{fs, io};
/// Read config file: first non-empty line is the directory path.
/// Returns "." if file can't be read.
pub fn read_config_dir(path: &str) -> String {
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
pub fn populate_file_list(list: &mut HoldBrowser, image_dir: &str) -> io::Result<()> {
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

pub fn reload_tiff_list(list: &mut HoldBrowser, folder: &str) {
    list.clear();

    if let Ok(entries) = fs::read_dir(folder) {
        // Collect just tif/tiff filenames (or full paths)
        let mut files: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter_map(|e| {
                let path = e.path();
                let ext = path.extension()?.to_string_lossy().to_lowercase();
                if ext == "tif" || ext == "tiff" {
                    // only filename:
                    Some(path.file_name()?.to_string_lossy().to_string())
                    // or full path:
                    // Some(path.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();

        files.sort();

        for name in files {
            list.add(&name);
        }
    }
}
