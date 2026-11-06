//! Windows: volume through Core Audio (IAudioEndpointVolume);
//! power actions through `shutdown.exe` and `SetSuspendState`.

use crate::config::Action;
use std::os::windows::process::CommandExt;
use std::process::Command;

use ::windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use ::windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use ::windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use ::windows::Win32::System::Power::SetSuspendState;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn init_thread() {
    // If COM is already initialised on this thread (e.g. as STA by the GUI toolkit)
    // this returns RPC_E_CHANGED_MODE, which is fine: COM is usable either way.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn endpoint() -> Result<IAudioEndpointVolume, String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Core Audio: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("Sem dispositivo de áudio: {e}"))?;
        device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .map_err(|e| format!("Core Audio: {e}"))
    }
}

pub fn volume_backend() -> String {
    "Windows Core Audio".into()
}

pub fn get_volume() -> Result<f32, String> {
    unsafe {
        endpoint()?
            .GetMasterVolumeLevelScalar()
            .map_err(|e| e.to_string())
    }
}

pub fn set_volume(v: f32) -> Result<(), String> {
    unsafe {
        endpoint()?
            .SetMasterVolumeLevelScalar(v, std::ptr::null())
            .map_err(|e| e.to_string())
    }
}

fn shutdown_exe(args: &[&str]) -> Result<(), String> {
    let status = Command::new("shutdown")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("shutdown.exe: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("shutdown {} terminou com {status}", args.join(" ")))
    }
}

pub fn power_action(action: Action, force: bool) -> Result<(), String> {
    match action {
        Action::Shutdown => {
            if force {
                shutdown_exe(&["/s", "/f", "/t", "0"])
            } else {
                // Gentle first; if Windows refuses, fall back to forced (like the original).
                shutdown_exe(&["/s", "/t", "0"]).or_else(|_| shutdown_exe(&["/s", "/f", "/t", "0"]))
            }
        }
        Action::Hibernate => shutdown_exe(&["/h"]),
        Action::Suspend => {
            let ok = unsafe { SetSuspendState(false, force, false) };
            if ok {
                Ok(())
            } else {
                Err("SetSuspendState falhou".into())
            }
        }
        Action::CloseApp => Ok(()),
    }
}
