//! User settings, kept in a TOML file.
//!
//! The file lives at `$DD_CONFIG` if that is set, otherwise
//! `$XDG_CONFIG_HOME/dd/settings.toml`, otherwise
//! `~/.config/dd/settings.toml`. Every key is optional.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::look;

/// When block labels are drawn.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Labels {
    Always,
    /// Only while the jump key is held down.
    #[default]
    OnDemand,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// dark, light, escape-dark, escape-light or heirloom.
    pub theme: String,
    /// Texture strength: 0 is smooth, 1 the theme's own, up to 2.
    pub grain: f32,
    pub labels: Labels,
    /// Outline where the key block has to go.
    pub show_goal: bool,
    /// Characters per board cell across: even, 4 or more.
    pub scale: usize,
    /// Play the key block's exit when a puzzle is solved.
    pub animations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "escape-dark".into(),
            grain: 1.0,
            labels: Labels::OnDemand,
            show_goal: false,
            scale: 6,
            animations: true,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        let var = |name| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        if let Some(path) = var("DD_CONFIG") {
            return Some(path);
        }
        let base = var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))?;
        Some(base.join("dd").join("settings.toml"))
    }

    /// Reads the settings file, or the defaults if there is none.
    pub fn load() -> Result<Settings, String> {
        let Some(path) = Settings::path() else {
            return Ok(Settings::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Settings::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn parse(text: &str) -> Result<Settings, String> {
        let settings: Settings = toml::from_str(text).map_err(|e| e.message().to_string())?;
        settings.check()?;
        Ok(settings)
    }

    pub fn check(&self) -> Result<(), String> {
        if look::theme::by_name(&self.theme).is_none() {
            let names: Vec<&str> = look::THEMES.iter().map(|t| t.name).collect();
            return Err(format!("no theme called {:?} (try {})", self.theme, names.join(", ")));
        }
        if !(0.0..=2.0).contains(&self.grain) {
            return Err("grain must be between 0 and 2".into());
        }
        if self.scale < 4 || self.scale % 2 == 1 {
            return Err("scale must be an even number, 4 or more".into());
        }
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("settings serialise")
    }

    // Used by the interactive interface.
    #[allow(dead_code)]
    pub fn theme(&self) -> &'static look::Theme {
        look::theme::by_name(&self.theme).expect("checked on load")
    }

    pub fn paint_options(&self) -> look::PaintOptions {
        look::PaintOptions {
            scale: self.scale,
            show_goal: self.show_goal,
            grain: self.grain,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Settings::parse("").unwrap(), Settings::default());
    }

    #[test]
    fn round_trips_and_validates() {
        let custom = Settings {
            theme: "heirloom".into(),
            grain: 0.4,
            labels: Labels::Always,
            show_goal: true,
            scale: 8,
            animations: false,
        };
        assert_eq!(Settings::parse(&custom.to_toml()).unwrap(), custom);
        assert!(Settings::parse("labels = \"on-demand\"").is_ok());
        assert!(Settings::parse("theme = \"nope\"").is_err());
        assert!(Settings::parse("grain = 3.0").is_err());
        assert!(Settings::parse("scale = 5").is_err());
        assert!(Settings::parse("colour = 1").is_err());
    }
}
