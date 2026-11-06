//! Settings, presets and persistence (JSON file in the user's config folder).

use crate::curve::Profile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Unit {
    Seconds,
    #[default]
    Minutes,
    Hours,
}

impl Unit {
    pub const ALL: [Unit; 3] = [Unit::Seconds, Unit::Minutes, Unit::Hours];
    pub fn label(self) -> &'static str {
        match self {
            Unit::Seconds => "segundo(s)",
            Unit::Minutes => "minuto(s)",
            Unit::Hours => "hora(s)",
        }
    }
    pub fn to_duration(self, amount: u32) -> Duration {
        let secs = amount as u64
            * match self {
                Unit::Seconds => 1,
                Unit::Minutes => 60,
                Unit::Hours => 3600,
            };
        Duration::from_secs(secs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// Act after a countdown.
    #[default]
    Countdown,
    /// Act at a given clock time (next occurrence).
    AtTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Action {
    #[default]
    Shutdown,
    Suspend,
    Hibernate,
    /// Don't touch the computer, just close this program.
    CloseApp,
}

impl Action {
    pub const ALL: [Action; 4] = [
        Action::Shutdown,
        Action::Suspend,
        Action::Hibernate,
        Action::CloseApp,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Action::Shutdown => "Desligar o computador",
            Action::Suspend => "Suspender (dormir)",
            Action::Hibernate => "Hibernar",
            Action::CloseApp => "Não desligar (só fechar o programa)",
        }
    }
    pub fn countdown_verb(self) -> &'static str {
        match self {
            Action::Shutdown => "A desligar daqui a",
            Action::Suspend => "A suspender daqui a",
            Action::Hibernate => "A hibernar daqui a",
            Action::CloseApp => "A fechar o programa daqui a",
        }
    }
    pub fn is_power_action(self) -> bool {
        !matches!(self, Action::CloseApp)
    }
}

/// Everything that describes one "goodnight" (what gets saved as a preset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimerSettings {
    pub mode: Mode,
    pub amount: u32,
    pub unit: Unit,
    pub at_hour: u32,
    pub at_minute: u32,
    pub action: Action,
    pub fade_enabled: bool,
    pub fade_amount: u32,
    pub fade_unit: Unit,
    pub profile: Profile,
    /// 0..=100
    pub start_volume: u8,
    /// 0..=100
    pub end_volume: u8,
}

impl Default for TimerSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Countdown,
            amount: 45,
            unit: Unit::Minutes,
            at_hour: 0,
            at_minute: 30,
            action: Action::Shutdown,
            fade_enabled: true,
            fade_amount: 15,
            fade_unit: Unit::Minutes,
            profile: Profile::Linear,
            start_volume: 40,
            end_volume: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Force applications to quit when shutting down (unsaved work is lost).
    pub force_quit: bool,
    /// Put the volume back to `restore_volume` right before the power action,
    /// so the computer isn't silent the next time it's used.
    pub restore_enabled: bool,
    pub restore_volume: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            force_quit: false,
            restore_enabled: true,
            restore_volume: 40,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub settings: TimerSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub last: TimerSettings,
    pub options: Options,
    pub presets: Vec<Preset>,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "goodnight")
            .map(|d| d.config_dir().join("config.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Config {
                version: 1,
                ..Default::default()
            })
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Não encontrei a pasta de configuração")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("Não consegui criar {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Não consegui gravar {}: {e}", path.display()))
    }

    /// Insert or replace a preset by name.
    pub fn upsert_preset(&mut self, name: &str, settings: TimerSettings) {
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == name) {
            p.settings = settings;
        } else {
            self.presets.push(Preset {
                name: name.to_string(),
                settings,
            });
            self.presets.sort_by_key(|p| p.name.to_lowercase());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tolerates_missing_fields() {
        let mut c = Config::default();
        c.upsert_preset(
            "Filme",
            TimerSettings {
                amount: 120,
                ..Default::default()
            },
        );
        c.upsert_preset(
            "Filme",
            TimerSettings {
                amount: 90,
                ..Default::default()
            },
        );
        assert_eq!(c.presets.len(), 1);
        let s = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
        let partial: Config = serde_json::from_str(r#"{"last":{"amount":5}}"#).unwrap();
        assert_eq!(partial.last.amount, 5);
        assert_eq!(
            partial.last.fade_amount,
            TimerSettings::default().fade_amount
        );
    }
}
