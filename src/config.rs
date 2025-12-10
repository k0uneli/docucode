use std::env;
use std::path::Path;
use std::{fs, io};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub tiff_dir: String,
    pub csv_path: String,
    pub bar_height: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            tiff_dir: default_tiff_dir(),
            csv_path: String::new(),
            bar_height: 200,
        }
    }
}

const CONFIG_PATH: &str = "config.txt";

pub fn load_config() -> AppConfig {
    let contents = match fs::read_to_string(CONFIG_PATH) {
        Ok(s) => s,
        Err(_) => return AppConfig::default(),
    };

    let mut cfg = AppConfig::default();
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("tiff_dir=") {
            cfg.tiff_dir = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("csv_path=") {
            cfg.csv_path = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("bar_height=") {
            if let Ok(parsed) = rest.trim().parse::<u32>() {
                cfg.bar_height = parsed;
            }
        }
    }
    if cfg.tiff_dir.is_empty() {
        cfg.tiff_dir = default_tiff_dir();
    }
    cfg
}

pub fn save_config(cfg: &AppConfig) -> io::Result<()> {
    let data = format!(
        "tiff_dir={}\ncsv_path={}\nbar_height={}\n",
        cfg.tiff_dir, cfg.csv_path, cfg.bar_height
    );
    fs::write(CONFIG_PATH, data)
}

fn default_tiff_dir() -> String {
    let appdata = match env::var("APPDATA") {
        Ok(val) => val,
        Err(_) => return String::new(),
    };

    let base = Path::new(&appdata)
        .join("ELO Digital Office")
        .join("MpsELOStaffDocs");

    let Ok(entries) = fs::read_dir(base) else {
        return String::new();
    };

    let mut intrays = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|f| f.is_dir()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if name.chars().all(|c| c.is_ascii_digit()) {
                let intray_path = entry.path().join("intray");
                if intray_path.is_dir() {
                    return Some(intray_path);
                }
            }
            None
        })
        .collect::<Vec<_>>();

    intrays.sort();
    intrays
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}
