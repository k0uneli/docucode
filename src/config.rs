use std::path::Path;
use std::{fs, io};

#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub tiff_dir: String,
    pub csv_path: String,
	pub bar_height: u32,
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
            cfg.bar_height = rest.trim().parse::<u32>()
			.expect("Invalid bar_height in config");
        }
    }
    cfg
}

pub fn save_config(cfg: &AppConfig) -> io::Result<()> {
    let data = format!("tiff_dir={}\ncsv_path={}\nbar_height={}\n", cfg.tiff_dir, cfg.csv_path, cfg.bar_height);
    fs::write(CONFIG_PATH, data)
}
