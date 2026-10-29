//! Tiện ích nhỏ đọc thuộc tính Core Audio.

use std::mem::MaybeUninit;
use std::ptr::NonNull;

use objc2_core_audio::{
    kAudioDevicePropertyDeviceUID, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
};
use objc2_core_foundation::{CFRetained, CFString};

pub fn status(what: &str, st: i32) -> Result<(), String> {
    if st == 0 {
        Ok(())
    } else {
        let bytes = st.to_be_bytes();
        let fourcc: String = bytes
            .iter()
            .map(|b| {
                if b.is_ascii_graphic() {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect();
        Err(format!("{what}: OSStatus {st} ('{fourcc}')"))
    }
}

pub fn get_prop<T: Copy>(obj: AudioObjectID, selector: u32, scope: u32) -> Result<T, String> {
    let addr = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut size = std::mem::size_of::<T>() as u32;
    let mut val = MaybeUninit::<T>::uninit();
    // SAFETY: `val` có đủ chỗ cho `size` byte, `addr` và `size` là con trỏ hợp lệ.
    let st = unsafe {
        AudioObjectGetPropertyData(
            obj,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(val.as_mut_ptr().cast()),
        )
    };
    status("AudioObjectGetPropertyData", st)?;
    // SAFETY: HAL đã ghi `T` khi trả về 0.
    Ok(unsafe { val.assume_init() })
}

pub fn default_output_device() -> Result<AudioObjectID, String> {
    get_prop::<AudioObjectID>(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
    )
}

pub fn device_uid(dev: AudioObjectID) -> Result<String, String> {
    let raw = get_prop::<*const CFString>(
        dev,
        kAudioDevicePropertyDeviceUID,
        kAudioObjectPropertyScopeGlobal,
    )?;
    let nn = NonNull::new(raw.cast_mut()).ok_or("UID null")?;
    // SAFETY: thuộc tính trả về CFString đã retain (+1), `CFRetained` nhận quyền sở hữu.
    let s: CFRetained<CFString> = unsafe { CFRetained::from_raw(nn) };
    Ok(s.to_string())
}
