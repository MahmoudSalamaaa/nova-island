//! The default speaker's volume and mute (Audio in NovaHelper.cs). The device is looked up on
//! every call, so switching to headphones is followed without any extra work.
use windows::{
    core::GUID,
    Win32::{
        Media::Audio::{eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{CoCreateInstance, CLSCTX_ALL},
    },
};

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        dev.Activate(CLSCTX_ALL, None)
    }
}

pub fn volume() -> windows::core::Result<f32> {
    unsafe { endpoint()?.GetMasterVolumeLevelScalar() }
}

pub fn muted() -> windows::core::Result<bool> {
    unsafe { Ok(endpoint()?.GetMute()?.as_bool()) }
}

pub fn set_volume(v: f32) -> windows::core::Result<()> {
    unsafe { endpoint()?.SetMasterVolumeLevelScalar(v, &GUID::zeroed()) }
}

pub fn set_mute(m: bool) -> windows::core::Result<()> {
    unsafe { endpoint()?.SetMute(m, &GUID::zeroed()) }
}
