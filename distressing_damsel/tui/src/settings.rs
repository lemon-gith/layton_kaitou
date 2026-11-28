//! User settings, kept in `settings.toml` in the data folder (see
//! [`Home`]). Every key is optional.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::{fsio, Home};
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

/// Texture multiplier for each grain level. Level 2 is each theme as
/// designed; past 1.5 times the patterns start to break up.
pub const GRAIN_LEVELS: [f32; 4] = [0.0, 0.5, 1.0, 1.5];

/// The format the picture view starts in.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PictureFormat {
    /// An image of the board.
    #[default]
    Png,
    /// The whole screen as text with colour codes, for a terminal.
    Ansi,
}

/// Which best result the side panel shows.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BestShown {
    /// The fewest moves, however they were reached.
    #[default]
    Overall,
    /// The fewest moves without hints or viewing the solution, falling back
    /// to the overall best when there is none.
    NoHintsFirst,
}

pub fn grain_strength(level: u8) -> f32 {
    GRAIN_LEVELS[(level as usize).min(3)]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// dark, light, escape-dark, escape-light or liberation.
    pub theme: String,
    /// Texture, from 0 (smooth) to 3 (strongest); 2 is the theme as
    /// designed. Only textured themes use it.
    pub grain: u8,
    pub labels: Labels,
    /// Outline where the key block has to go.
    pub show_goal: bool,
    /// Outline empty cells.
    pub grid: bool,
    /// Characters per board cell across: even, 4 or more.
    pub scale: usize,
    /// Slide moved blocks and fade the key block out when solved.
    pub animations: bool,
    /// Ask before giving a hint.
    pub confirm_hints: bool,
    /// Show the shortest solution's length in the side panel.
    pub show_solver_length: bool,
    /// Mark the cells a grabbed block can reach.
    pub mark_reachable: bool,
    pub best: BestShown,
    /// The format the picture view starts in.
    pub picture_format: PictureFormat,
    /// Where puzzles, progress and records live. A relative path counts
    /// from the data folder; unset means the data folder itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "escape-dark".into(),
            grain: 1,
            labels: Labels::OnDemand,
            show_goal: false,
            grid: true,
            scale: 6,
            animations: true,
            confirm_hints: true,
            show_solver_length: false,
            mark_reachable: false,
            best: BestShown::Overall,
            picture_format: PictureFormat::Png,
            data_dir: None,
        }
    }
}

impl Settings {
    /// Reads the settings file, or the defaults if there is none.
    pub fn load(home: &Home) -> Result<Settings, String> {
        let path = home.settings_file();
        match fsio::read_if_exists(&path) {
            Ok(Some(text)) => Settings::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Ok(None) => Ok(Settings::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, home: &Home) -> Result<(), String> {
        let path = home.settings_file();
        fsio::write_atomic(&path, &self.to_toml()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Where puzzles, progress and records live.
    pub fn data_dir(&self, home: &Home) -> PathBuf {
        home.data_dir(self.data_dir.as_deref())
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
        if self.grain > 3 {
            return Err("grain must be 0, 1, 2 or 3".into());
        }
        if self.scale < 4 || self.scale % 2 == 1 {
            return Err("scale must be an even number, 4 or more".into());
        }
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("settings serialise")
    }

    pub fn theme(&self) -> &'static look::Theme {
        look::theme::by_name(&self.theme).expect("checked on load")
    }

    pub fn paint_options(&self) -> look::PaintOptions {
        look::PaintOptions {
            scale: self.scale,
            show_goal: self.show_goal,
            grid: self.grid,
            grain: grain_strength(self.grain),
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
            theme: "liberation".into(),
            grain: 0,
            labels: Labels::Always,
            show_goal: true,
            grid: false,
            scale: 8,
            animations: false,
            confirm_hints: false,
            show_solver_length: true,
            mark_reachable: true,
            best: BestShown::NoHintsFirst,
            picture_format: PictureFormat::Ansi,
            data_dir: Some("../elsewhere".into()),
        };
        assert_eq!(Settings::parse(&custom.to_toml()).unwrap(), custom);
        assert!(Settings::parse("labels = \"on-demand\"").is_ok());
        assert!(Settings::parse("theme = \"nope\"").is_err());
        assert!(Settings::parse("grain = 3").is_ok());
        assert!(Settings::parse("grain = 4").is_err());
        assert!(Settings::parse("grain = 1.5").is_err());
        assert!(Settings::parse("scale = 5").is_err());
        assert!(Settings::parse("colour = 1").is_err());
    }

    #[test]
    fn saves_and_loads_in_the_data_folder() {
        let tmp = crate::data::home::testing::TempDir::new();
        let home = Home::at(&tmp.0);
        assert_eq!(Settings::load(&home).unwrap(), Settings::default());
        let custom = Settings { theme: "light".into(), best: BestShown::NoHintsFirst, ..Settings::default() };
        custom.save(&home).unwrap();
        assert!(home.settings_file().exists());
        assert_eq!(Settings::load(&home).unwrap(), custom);
        assert_eq!(custom.data_dir(&home), home.root());
        std::fs::write(home.settings_file(), "theme = 3").unwrap();
        assert!(Settings::load(&home).unwrap_err().contains("settings.toml"));
    }
}
