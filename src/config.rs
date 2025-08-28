use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use dirs::home_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub notes_dir: String,
}

impl Config {
    pub fn load_or_create() -> anyhow::Result<Self> {
        let cfg_dir = home_dir().unwrap_or_default().join(".config").join("lazynotes");
        fs::create_dir_all(&cfg_dir)?;
        let cfg_path = cfg_dir.join("config.toml");

        if cfg_path.exists() {
            let s = fs::read_to_string(&cfg_path)?;
            let cfg: Config = toml::from_str(&s)?;
            Ok(cfg)
        } else {
            let default_dir = home_dir()
                .unwrap_or_default()
                .join("Documents")
                .join("Notes");
            let cfg = Config {
                notes_dir: default_dir.to_string_lossy().to_string(),
            };
            let content = toml::to_string_pretty(&cfg)?;
            fs::write(&cfg_path, content)?;
            Ok(cfg)
        }
    }

    pub fn notes_path(&self) -> PathBuf {
        expand_tilde(&self.notes_dir)
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}
