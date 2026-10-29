//! Hướng (a): Core Audio process tap (macOS 14.2+).
//! Tap toàn hệ thống (stereo mixdown, không loại trừ tiến trình nào) gắn vào một aggregate device riêng tư.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceMainSubDeviceKey,
    kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyBufferFrameSize,
    kAudioObjectPropertyScopeGlobal, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    kAudioTapPropertyFormat, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectID, CATapDescription, CATapMuteBehavior,
};
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

use crate::ca;
use crate::stats::{now_us, Capture};

const FLAG_NON_INTERLEAVED: u32 = 1 << 5;
const FLAG_FLOAT: u32 = 1 << 0;

struct Ctx {
    cap: Mutex<Capture>,
    channels: usize,
    interleaved: bool,
    prev_end: Mutex<Option<f64>>,
}

unsafe extern "C-unwind" fn in_proc(
    _dev: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    in_data: NonNull<AudioBufferList>,
    in_time: NonNull<AudioTimeStamp>,
    _out_data: NonNull<AudioBufferList>,
    _out_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    let arrival = now_us();
    // SAFETY: `client` là con trỏ Arc<Ctx> sống đến sau khi dừng IOProc.
    let ctx = unsafe { &*(client as *const Ctx) };
    let list = in_data.as_ptr();
    let nbuf = unsafe { (*list).mNumberBuffers } as usize;
    let first = unsafe { (*list).mBuffers.as_ptr() };
    if nbuf == 0 {
        return 0;
    }
    let sample_time = unsafe { (*in_time.as_ptr()).mSampleTime };

    let frames;
    let mut mono: Vec<f32> = Vec::new();
    if ctx.interleaved {
        let b = unsafe { &*first };
        let ch = (b.mNumberChannels as usize).max(1);
        frames = b.mDataByteSize as usize / 4 / ch;
        let data = unsafe { std::slice::from_raw_parts(b.mData as *const f32, frames * ch) };
        mono.extend(
            data.chunks_exact(ch)
                .map(|f| f.iter().sum::<f32>() / ch as f32),
        );
    } else {
        let b0 = unsafe { &*first };
        frames = b0.mDataByteSize as usize / 4;
        mono.resize(frames, 0.0);
        for i in 0..nbuf {
            let b = unsafe { &*first.add(i) };
            let data = unsafe { std::slice::from_raw_parts(b.mData as *const f32, frames) };
            for (m, s) in mono.iter_mut().zip(data) {
                *m += *s / nbuf as f32;
            }
        }
    }
    let _ = ctx.channels;
    if let Ok(mut cap) = ctx.cap.lock() {
        if let Ok(mut prev) = ctx.prev_end.lock() {
            if let Some(p) = *prev {
                if (sample_time - p).abs() > 0.5 {
                    cap.discontinuities += 1;
                }
            }
            *prev = Some(sample_time + frames as f64);
        }
        cap.push_mono(arrival, mono.into_iter());
    }
    0
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// Dùng `Retained<NSString>` làm giá trị `AnyObject` của từ điển.
fn obj<T: objc2::Message>(r: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: mọi đối tượng ObjC đều là AnyObject.
    unsafe { Retained::cast_unchecked::<AnyObject>(r) }
}

pub struct Session {
    tap_id: AudioObjectID,
    agg_id: AudioObjectID,
    dev_ioproc: AudioDeviceIOProcID,
    ctx: Arc<Ctx>,
    pub info: String,
}

impl Session {
    pub fn start(expect_secs: f64) -> Result<Session, String> {
        let out_dev = ca::default_output_device()?;
        let out_uid = ca::device_uid(out_dev)?;

        // 1) Tap: stereo mixdown toàn hệ thống, không loại tiến trình nào.
        let excluded: Retained<NSArray<NSNumber>> = NSArray::new();
        // SAFETY: gọi init chuẩn của CATapDescription.
        let desc = unsafe {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &excluded,
            )
        };
        unsafe {
            desc.setName(&ns("Offstage spike tap"));
            desc.setPrivate(true);
            desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
        }
        let tap_uuid = unsafe { desc.UUID().UUIDString().to_string() };
        let mut tap_id: AudioObjectID = 0;
        let t0 = std::time::Instant::now();
        ca::status("AudioHardwareCreateProcessTap", unsafe {
            AudioHardwareCreateProcessTap(Some(&desc), &mut tap_id)
        })?;
        let create_tap_ms = t0.elapsed().as_secs_f64() * 1e3;

        let fmt: AudioStreamBasicDescription = ca::get_prop(
            tap_id,
            kAudioTapPropertyFormat,
            kAudioObjectPropertyScopeGlobal,
        )?;
        let interleaved = fmt.mFormatFlags & FLAG_NON_INTERLEAVED == 0;
        let format_note = format!(
            "tap ASBD: {:.0} Hz, {} kênh, {} bit, float={}, {}, {} byte/khung",
            fmt.mSampleRate,
            fmt.mChannelsPerFrame,
            fmt.mBitsPerChannel,
            fmt.mFormatFlags & FLAG_FLOAT != 0,
            if interleaved { "interleaved" } else { "planar" },
            fmt.mBytesPerFrame
        );

        // 2) Aggregate device riêng tư chứa tap (kèm thiết bị xuất mặc định làm đồng hồ chính).
        let sub_dev = NSDictionary::from_retained_objects(
            &[&*ns(kAudioSubTapUIDKey.to_str().unwrap_or("uid"))],
            &[obj(ns(&out_uid))],
        );
        let sub_tap = NSDictionary::from_retained_objects(
            &[
                &*ns(kAudioSubTapUIDKey.to_str().unwrap_or("uid")),
                &*ns(kAudioSubTapDriftCompensationKey.to_str().unwrap_or("drift")),
            ],
            &[obj(ns(&tap_uuid)), obj(NSNumber::new_bool(true))],
        );
        let agg_uid = format!("offstage-spike-agg-{}", now_us());
        let key = |c: &std::ffi::CStr| ns(c.to_str().unwrap_or(""));
        let (k_uid, k_name, k_main, k_subs, k_taps, k_priv, k_auto) = (
            key(kAudioAggregateDeviceUIDKey),
            key(kAudioAggregateDeviceNameKey),
            key(kAudioAggregateDeviceMainSubDeviceKey),
            key(kAudioAggregateDeviceSubDeviceListKey),
            key(kAudioAggregateDeviceTapListKey),
            key(kAudioAggregateDeviceIsPrivateKey),
            key(kAudioAggregateDeviceTapAutoStartKey),
        );
        let subs = NSArray::from_retained_slice(&[obj(sub_dev)]);
        let taps = NSArray::from_retained_slice(&[obj(sub_tap)]);
        let agg_desc = NSDictionary::from_retained_objects(
            &[
                &*k_uid, &*k_name, &*k_main, &*k_subs, &*k_taps, &*k_priv, &*k_auto,
            ],
            &[
                obj(ns(&agg_uid)),
                obj(ns("Offstage spike aggregate")),
                obj(ns(&out_uid)),
                obj(subs),
                obj(taps),
                obj(NSNumber::new_bool(true)),
                obj(NSNumber::new_bool(true)),
            ],
        );
        // SAFETY: NSDictionary được bridge miễn phí sang CFDictionary.
        let cf: &CFDictionary = unsafe { &*(Retained::as_ptr(&agg_desc) as *const CFDictionary) };
        let mut agg_id: AudioObjectID = 0;
        let r = ca::status("AudioHardwareCreateAggregateDevice", unsafe {
            AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut agg_id))
        });
        if let Err(e) = r {
            unsafe { AudioHardwareDestroyProcessTap(tap_id) };
            return Err(e);
        }
        let buf_frames: u32 = ca::get_prop(
            agg_id,
            kAudioDevicePropertyBufferFrameSize,
            kAudioObjectPropertyScopeGlobal,
        )
        .unwrap_or(0);

        // 3) IOProc.
        let ctx = Arc::new(Ctx {
            cap: Mutex::new(Capture::new(
                fmt.mSampleRate,
                fmt.mChannelsPerFrame,
                format_note,
                expect_secs,
            )),
            channels: fmt.mChannelsPerFrame as usize,
            interleaved,
            prev_end: Mutex::new(None),
        });
        let mut proc_id: AudioDeviceIOProcID = None;
        let client = Arc::as_ptr(&ctx) as *mut c_void;
        let r = ca::status("AudioDeviceCreateIOProcID", unsafe {
            AudioDeviceCreateIOProcID(agg_id, Some(in_proc), client, NonNull::from(&mut proc_id))
        })
        .and_then(|_| {
            ca::status("AudioDeviceStart", unsafe {
                AudioDeviceStart(agg_id, proc_id)
            })
        });
        if let Err(e) = r {
            unsafe {
                AudioHardwareDestroyAggregateDevice(agg_id);
                AudioHardwareDestroyProcessTap(tap_id);
            }
            return Err(e);
        }
        let info = format!(
            "output mặc định UID={out_uid}; tạo tap mất {create_tap_ms:.1} ms; buffer aggregate = {buf_frames} khung ({:.1} ms)",
            buf_frames as f64 / fmt.mSampleRate * 1e3
        );
        Ok(Session {
            tap_id,
            agg_id,
            dev_ioproc: proc_id,
            ctx,
            info,
        })
    }

    pub fn finish(self) -> Result<Capture, String> {
        unsafe {
            let _ = AudioDeviceStop(self.agg_id, self.dev_ioproc);
            let _ = AudioDeviceDestroyIOProcID(self.agg_id, self.dev_ioproc);
            let _ = AudioHardwareDestroyAggregateDevice(self.agg_id);
            let _ = AudioHardwareDestroyProcessTap(self.tap_id);
        }
        std::thread::sleep(Duration::from_millis(50));
        let ctx = Arc::try_unwrap(self.ctx).map_err(|_| "Ctx vẫn còn tham chiếu".to_string())?;
        ctx.cap.into_inner().map_err(|e| format!("mutex: {e}"))
    }
}
