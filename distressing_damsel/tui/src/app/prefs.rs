//! The preferences overlay: settings worth changing in the middle of a
//! puzzle. Changes apply at once and are saved straight away.

use crate::look::THEMES;
use crate::settings::{Labels, Settings};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pref {
    Theme,
    Grain,
    Goal,
    Grid,
    Labels,
    Reach,
    SolverLength,
}

/// The rows shown, in order. Grain only for themes with texture.
pub fn rows(settings: &Settings) -> Vec<Pref> {
    let mut rows = vec![Pref::Theme];
    if settings.theme().textured() {
        rows.push(Pref::Grain);
    }
    rows.extend([Pref::Goal, Pref::Grid, Pref::Labels, Pref::Reach, Pref::SolverLength]);
    rows
}

pub fn name(pref: Pref) -> &'static str {
    match pref {
        Pref::Theme => "theme",
        Pref::Grain => "grain",
        Pref::Goal => "goal outline",
        Pref::Grid => "grid",
        Pref::Labels => "block labels",
        Pref::Reach => "mark reachable",
        Pref::SolverLength => "solver length",
    }
}

fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.into()
}

/// The value as shown. Grain is a four-stop slider.
pub fn value(settings: &Settings, pref: Pref) -> String {
    match pref {
        Pref::Theme => settings.theme.clone(),
        Pref::Grain => (0..4).map(|i| if i == settings.grain { '●' } else { '○' }).collect(),
        Pref::Goal => on_off(settings.show_goal),
        Pref::Grid => on_off(settings.grid),
        Pref::Labels => match settings.labels {
            Labels::Always => "always",
            Labels::OnDemand => "on demand",
            Labels::Off => "off",
        }
        .into(),
        Pref::Reach => on_off(settings.mark_reachable),
        Pref::SolverLength => on_off(settings.show_solver_length),
    }
}

/// Changes a setting one step forwards or back. Lists wrap round; grain
/// stops at its ends.
pub fn adjust(settings: &mut Settings, pref: Pref, forward: bool) {
    let step = |i: usize, n: usize| if forward { (i + 1) % n } else { (i + n - 1) % n };
    match pref {
        Pref::Theme => {
            let i = THEMES.iter().position(|t| t.name == settings.theme).unwrap_or(0);
            settings.theme = THEMES[step(i, THEMES.len())].name.to_string();
        }
        Pref::Grain => {
            settings.grain = if forward { (settings.grain + 1).min(3) } else { settings.grain.saturating_sub(1) };
        }
        Pref::Goal => settings.show_goal = !settings.show_goal,
        Pref::Grid => settings.grid = !settings.grid,
        Pref::Labels => {
            let all = [Labels::Always, Labels::OnDemand, Labels::Off];
            let i = all.iter().position(|l| *l == settings.labels).unwrap_or(1);
            settings.labels = all[step(i, all.len())];
        }
        Pref::Reach => settings.mark_reachable = !settings.mark_reachable,
        Pref::SolverLength => settings.show_solver_length = !settings.show_solver_length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grain_is_offered_only_for_textured_themes() {
        let mut s = Settings::default();
        assert!(rows(&s).contains(&Pref::Grain));
        s.theme = "dark".into();
        assert!(!rows(&s).contains(&Pref::Grain));
    }

    #[test]
    fn values_step_and_wrap() {
        let mut s = Settings { theme: "dark".into(), grain: 1, ..Settings::default() };
        adjust(&mut s, Pref::Theme, false);
        assert_eq!(s.theme, "liberation", "back from the first theme wraps to the last");
        adjust(&mut s, Pref::Theme, true);
        assert_eq!(s.theme, "dark");
        adjust(&mut s, Pref::Grain, true);
        adjust(&mut s, Pref::Grain, true);
        adjust(&mut s, Pref::Grain, true);
        assert_eq!(s.grain, 3, "grain stops at 3");
        assert_eq!(value(&s, Pref::Grain), "○○○●");
        adjust(&mut s, Pref::Labels, true);
        assert_eq!(s.labels, Labels::Off);
        adjust(&mut s, Pref::Labels, true);
        assert_eq!(s.labels, Labels::Always);
        adjust(&mut s, Pref::Goal, true);
        assert!(s.show_goal);
        assert_eq!(value(&s, Pref::Goal), "on");
    }
}
