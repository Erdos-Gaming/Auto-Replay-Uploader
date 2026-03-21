use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub watch_dirs: Vec<String>,

    #[serde(default)]
    pub upload_url: String,
    pub discord_id: u64,
    
    #[serde(default)]
    pub delete_after_upload: bool,


    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
    
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
}

fn default_settle_ms() -> u64 {
    500
}

fn default_poll_interval() -> u64 {
    2000
}

pub fn app_data_dir() -> PathBuf {
    let base = dirs::config_dir()
        .unwrap_or_else(|| {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
        });
    let dir = base.join("erdos");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn default_watch_dirs() -> Vec<String> {
    let rl_base = dirs::document_dir()
        .unwrap_or_else(|| {
            let profile = std::env::var("USERPROFILE")
                .unwrap_or_else(|_| "C:\\Users\\Public".to_string());
            PathBuf::from(profile).join("Documents")
        })
        .join("My Games")
        .join("Rocket League")
        .join("TAGame");

    vec![
        rl_base.join("Demos").to_string_lossy().into_owned(),
        rl_base.join("DemosEpic").to_string_lossy().into_owned(),
    ]
}

impl Config {
    pub fn config_path() -> PathBuf {
        app_data_dir().join("config.toml")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let text = fs::read_to_string(&path)?;
            let cfg: Config = toml::from_str(&text)?;
            Ok(cfg)
        } else {
            let default = Config::default();
            default.save()?;
            Ok(default)
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)?;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            watch_dirs: default_watch_dirs(),
            upload_url: "https://alpha.erdosgaming.com".to_string(),
            delete_after_upload: false,
            discord_id: 0,
            settle_ms: default_settle_ms(),
            poll_interval: default_poll_interval(),
        }
    }
}
