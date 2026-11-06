//! Platform layer: master volume and power actions.

use crate::config::Action;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as imp;

#[cfg(not(any(target_os = "linux", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "linux", windows)))]
use unsupported as imp;

/// Must be called once on every thread that talks to the audio system
/// (needed for COM on Windows, a no-op elsewhere).
pub fn init_thread() {
    imp::init_thread();
}

/// Master volume of the default output device, 0.0..=1.0.
pub fn get_volume() -> Result<f32, String> {
    imp::get_volume()
}

/// Set master volume of the default output device, 0.0..=1.0.
pub fn set_volume(v: f32) -> Result<(), String> {
    imp::set_volume(v.clamp(0.0, 1.0))
}

/// Human-readable name of the audio backend being used.
pub fn volume_backend() -> String {
    imp::volume_backend()
}

/// Shut down / suspend / hibernate. `CloseApp` is handled by the caller.
pub fn power_action(action: Action, force: bool) -> Result<(), String> {
    match action {
        Action::CloseApp => Ok(()),
        a => imp::power_action(a, force),
    }
}
