//! Linux: volume through PipeWire (`wpctl`), PulseAudio (`pactl`) or ALSA (`amixer`);
//! power actions through systemd/logind (`systemctl`), with fallbacks.

use crate::config::Action;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Wpctl,
    Pactl,
    Amixer,
}

static BACKEND: OnceLock<Option<Backend>> = OnceLock::new();

pub fn init_thread() {}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `wpctl get-volume` → "Volume: 0.40" or "Volume: 0.40 [MUTED]"
pub(crate) fn parse_wpctl(s: &str) -> Option<f32> {
    s.split_whitespace().nth(1)?.parse::<f32>().ok()
}

/// First "NN%" occurrence (pactl / amixer output).
pub(crate) fn parse_percent(s: &str) -> Option<f32> {
    let idx = s.find('%')?;
    let digits: String = s[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let n: f32 = digits.chars().rev().collect::<String>().parse().ok()?;
    Some(n / 100.0)
}

fn get_with(b: Backend) -> Result<f32, String> {
    match b {
        Backend::Wpctl => parse_wpctl(&run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?),
        Backend::Pactl => parse_percent(&run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?),
        Backend::Amixer => parse_percent(&run("amixer", &["get", "Master"])?),
    }
    .ok_or_else(|| "Não consegui ler o volume".to_string())
}

fn backend() -> Result<Backend, String> {
    BACKEND
        .get_or_init(|| {
            [Backend::Wpctl, Backend::Pactl, Backend::Amixer].into_iter().find(|b| get_with(*b).is_ok())
        })
        .ok_or_else(|| {
            "Nenhum controlo de volume encontrado (instale wireplumber/wpctl, pulseaudio-utils/pactl ou alsa-utils/amixer)"
                .to_string()
        })
}

pub fn volume_backend() -> String {
    match backend() {
        Ok(Backend::Wpctl) => "PipeWire (wpctl)".into(),
        Ok(Backend::Pactl) => "PulseAudio (pactl)".into(),
        Ok(Backend::Amixer) => "ALSA (amixer)".into(),
        Err(e) => e,
    }
}

pub fn get_volume() -> Result<f32, String> {
    get_with(backend()?)
}

pub fn set_volume(v: f32) -> Result<(), String> {
    let pct = format!("{}%", (v * 100.0).round() as u32);
    match backend()? {
        Backend::Wpctl => run(
            "wpctl",
            &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{v:.3}")],
        ),
        Backend::Pactl => run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &pct]),
        Backend::Amixer => run("amixer", &["-q", "sset", "Master", &pct]),
    }
    .map(|_| ())
}

pub fn power_action(action: Action, force: bool) -> Result<(), String> {
    let verb = match action {
        Action::Shutdown => "poweroff",
        Action::Suspend => "suspend",
        Action::Hibernate => "hibernate",
        Action::CloseApp => return Ok(()),
    };
    let mut errors = Vec::new();

    // 1) systemd (works without root for the active session via polkit)
    let mut args = vec![verb];
    if force {
        args.push("--ignore-inhibitors");
    }
    match run("systemctl", &args) {
        Ok(_) => return Ok(()),
        Err(e) => errors.push(e),
    }
    // 2) logind directly
    match run("loginctl", &[verb]) {
        Ok(_) => return Ok(()),
        Err(e) => errors.push(e),
    }
    // 3) classic shutdown (usually needs root)
    if action == Action::Shutdown {
        match run("shutdown", &["-h", "now"]) {
            Ok(_) => return Ok(()),
            Err(e) => errors.push(e),
        }
    }
    Err(errors.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_output() {
        assert_eq!(parse_wpctl("Volume: 0.40\n"), Some(0.40));
        assert_eq!(parse_wpctl("Volume: 0.15 [MUTED]\n"), Some(0.15));
        let pactl = "Volume: front-left: 26214 /  40% / -23.88 dB,   front-right: 26214 /  40% / -23.88 dB\n        balance 0.00\n";
        assert_eq!(parse_percent(pactl), Some(0.40));
        let amixer = "Simple mixer control 'Master',0\n  Mono: Playback 52 [81%] [-16.50dB] [on]\n";
        assert_eq!(parse_percent(amixer), Some(0.81));
        assert_eq!(parse_percent("nothing"), None);
    }
}
