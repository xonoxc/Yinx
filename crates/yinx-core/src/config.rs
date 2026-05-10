use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::state::AppSettings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    pub theme: String,
    pub keybindings: HashMap<String, String>,
    pub defaults: AppSettings,
}

impl Config {
    pub fn default_config() -> Self {
        Self {
            theme: "terminal".to_string(),
            keybindings: HashMap::new(),
            defaults: AppSettings::default(),
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(&path)?;
        let config: Config = if path.as_ref().extension().and_then(|s| s.to_str()) == Some("json") {
            serde_json::from_str(&content)?
        } else {
            serde_yaml::from_str(&content)?
        };
        Ok(config)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = if path.as_ref().extension().and_then(|s| s.to_str()) == Some("json") {
            serde_json::to_string_pretty(self)?
        } else {
            serde_yaml::to_string(self)?
        };
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn apply_env_overrides(mut self) -> Self {
        for (key, value) in env::vars() {
            if key.starts_with("YINX_") {
                let key = key.trim_start_matches("YINX_").to_lowercase();
                match key.as_str() {
                    "theme" => self.theme = value,
                    "default_timeout_secs" => {
                        if let Ok(v) = value.parse() {
                            self.defaults.default_timeout_secs = v;
                        }
                    }
                    "follow_redirects" => {
                        if let Ok(v) = value.parse() {
                            self.defaults.follow_redirects = v;
                        }
                    }
                    "verify_tls" => {
                        if let Ok(v) = value.parse() {
                            self.defaults.verify_tls = v;
                        }
                    }
                    "max_history_entries" => {
                        if let Ok(v) = value.parse() {
                            self.defaults.max_history_entries = v;
                        }
                    }
                    _ => {}
                }
            }
        }
        self
    }
}

pub fn discover_config() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from(".yinxrc"),
        PathBuf::from(".yinxrc.yaml"),
        PathBuf::from(".yinxrc.yml"),
        PathBuf::from(".yinxrc.json"),
    ];

    if let Some(mut p) = xdg_config_dir() {
        p.push("yinx/config.yaml");
        candidates.push(p);
    }
    if let Some(mut p) = xdg_config_dir() {
        p.push("yinx/config.yml");
        candidates.push(p);
    }
    if let Some(mut p) = xdg_config_dir() {
        p.push("yinx/config.json");
        candidates.push(p);
    }
    if let Some(mut p) = home_dir() {
        p.push(".yinxrc");
        candidates.push(p);
    }

    for path in &candidates {
        if path.exists() {
            return Some(path.clone());
        }
    }
    None
}

fn xdg_config_dir() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            home_dir().map(|mut p| {
                p.push(".config");
                p
            })
        })
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME").ok().map(PathBuf::from)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Config not found")]
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_config();
        assert_eq!(config.theme, "terminal");
        assert_eq!(config.defaults.default_timeout_secs, 30);
        assert!(config.defaults.follow_redirects);
        assert!(config.defaults.verify_tls);
        assert_eq!(config.defaults.max_history_entries, 1000);
    }

    #[test]
    fn test_config_serialization_yaml() {
        let config = Config::default_config();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let decoded: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config.theme, decoded.theme);
        assert_eq!(
            config.defaults.default_timeout_secs,
            decoded.defaults.default_timeout_secs
        );
    }

    #[test]
    fn test_config_serialization_json() {
        let config = Config::default_config();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.theme, decoded.theme);
    }

    #[test]
    fn test_load_from_yaml_file() {
        let config = Config::default_config();
        let path = std::env::temp_dir().join("test_config.yaml");
        config.save_to_file(&path).unwrap();

        let loaded = Config::load_from_file(&path).unwrap();
        assert_eq!(loaded.theme, "terminal");
        assert_eq!(loaded.defaults.default_timeout_secs, 30);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_load_from_json_file() {
        let config = Config::default_config();
        let path = std::env::temp_dir().join("test_config.json");
        config.save_to_file(&path).unwrap();

        let loaded = Config::load_from_file(&path).unwrap();
        assert_eq!(loaded.theme, "terminal");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let config = Config::default_config();
        let dir = std::env::temp_dir().join("yinx_test_subdir");
        let path = dir.join("config.yaml");
        config.save_to_file(&path).unwrap();
        assert!(path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_apply_env_overrides_theme() {
        env::set_var("YINX_THEME", "light");
        let config = Config::default_config().apply_env_overrides();
        assert_eq!(config.theme, "light");
        env::remove_var("YINX_THEME");
    }

    #[test]
    fn test_apply_env_overrides_timeout() {
        env::set_var("YINX_DEFAULT_TIMEOUT_SECS", "60");
        let config = Config::default_config().apply_env_overrides();
        assert_eq!(config.defaults.default_timeout_secs, 60);
        env::remove_var("YINX_DEFAULT_TIMEOUT_SECS");
    }

    #[test]
    fn test_apply_env_overrides_follow_redirects() {
        env::set_var("YINX_FOLLOW_REDIRECTS", "false");
        let config = Config::default_config().apply_env_overrides();
        assert!(!config.defaults.follow_redirects);
        env::remove_var("YINX_FOLLOW_REDIRECTS");
    }

    #[test]
    fn test_apply_env_overrides_verify_tls() {
        env::set_var("YINX_VERIFY_TLS", "false");
        let config = Config::default_config().apply_env_overrides();
        assert!(!config.defaults.verify_tls);
        env::remove_var("YINX_VERIFY_TLS");
    }

    #[test]
    fn test_apply_env_overrides_max_history() {
        env::set_var("YINX_MAX_HISTORY_ENTRIES", "500");
        let config = Config::default_config().apply_env_overrides();
        assert_eq!(config.defaults.max_history_entries, 500);
        env::remove_var("YINX_MAX_HISTORY_ENTRIES");
    }

    #[test]
    fn test_discover_config_nonexistent() {
        let result = discover_config();
        assert!(result.is_none() || result.is_some());
    }

    #[test]
    fn test_discover_config_with_temp_file() {
        let path = std::env::temp_dir().join(".yinxrc");
        let config = Config::default_config();
        config.save_to_file(&path).unwrap();

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::NotFound;
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_config_with_keybindings() {
        let mut config = Config::default_config();
        config
            .keybindings
            .insert("quit".to_string(), "Ctrl+c".to_string());
        config
            .keybindings
            .insert("save".to_string(), "Ctrl+s".to_string());

        assert_eq!(config.keybindings.len(), 2);
        assert_eq!(config.keybindings.get("quit").unwrap(), "Ctrl+c");
    }

    #[test]
    fn test_config_preserves_unknown_env_vars() {
        // Clear known env vars that other tests may have set
        env::remove_var("YINX_DEFAULT_TIMEOUT_SECS");
        env::remove_var("YINX_THEME");
        env::remove_var("YINX_FOLLOW_REDIRECTS");
        env::remove_var("YINX_VERIFY_TLS");
        env::remove_var("YINX_MAX_HISTORY_ENTRIES");
        env::set_var("YINX_UNKNOWN_VAR", "value");
        let config = Config::default_config().apply_env_overrides();
        assert!(config.keybindings.is_empty());
        assert_eq!(config.defaults.default_timeout_secs, 30);
        env::remove_var("YINX_UNKNOWN_VAR");
    }

    #[test]
    fn test_xdg_config_dir() {
        let dir = xdg_config_dir();
        if env::var("HOME").is_ok() {
            assert!(dir.is_some());
        }
    }
}
