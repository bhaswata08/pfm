use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub id: String,
    pub host: String,
    pub local_port: u16,
    pub remote_port: u16,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub forwards: HashMap<String, PortForward>,
}

impl Config {
    pub fn new() -> Self {
        Config {
            forwards: HashMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return Ok(Config::new());
        }
        let contents = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        let config: Config = serde_json::from_str(&contents)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let parent = config_path
            .parent()
            .context("Config path has no parent directory")?;
        fs::create_dir_all(parent)
            .context("Failed to create config directory")?;

        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;

        let temp_path = parent.join(format!(".config.json.tmp.{}", std::process::id()));
        fs::write(&temp_path, contents)
            .context("Failed to write temporary config file")?;

        if let Err(e) = fs::rename(&temp_path, &config_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(e).context("Failed to atomically replace config file");
        }

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("pfm");

        Ok(config_dir.join("config.json"))
    }

    pub fn add_forward(&mut self, forward: PortForward) {
        self.forwards.insert(forward.id.clone(), forward);
    }

    pub fn get_sorted_forwards(&self) -> Vec<&PortForward> {
        let mut forwards: Vec<&PortForward> = self.forwards.values().collect();
        forwards.sort_by_key(|f| &f.id);
        forwards
    }

    pub fn get_forward_by_index(&self, index: usize) -> Option<&PortForward> {
        self.get_sorted_forwards().get(index).copied()
    }

    pub fn remove_forward(&mut self, id: &str) -> Option<PortForward> {
        self.forwards.remove(id)
    }
}
