//! Tiến trình phụ phát tiếng bíp tổng hợp ra thiết bị xuất mặc định, in mốc thời gian mỗi lần bắt đầu bíp.
//! Không dùng file âm thanh: mẫu sin được sinh trực tiếp trong IOProc.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use objc2_core_audio::{
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyLatency,
    kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertySafetyOffset,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, AudioDeviceCreateIOProcID,
    AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    AudioObjectID,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};

use crate::ca;
use crate::stats::now_us;

const MAX_BEEPS: usize = 64;
const FIRST_AT_S: f64 = 0.5;
const PERIOD_S: f64 = 1.0;
const BURST_S: f64 = 0.15;
const FREQ_HZ: f64 = 1000.0;
const AMPLITUDE: f64 = 0.3;

static POS: AtomicU64 = AtomicU64::new(0);
static RATE_BITS: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static SEEN: AtomicUsize = AtomicUsize::new(0);
static BEEP_US: [AtomicU64; MAX_BEEPS] = [const { AtomicU64::new(0) }; MAX_BEEPS];

unsafe extern "C-unwind" fn out_proc(
    _dev: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    _in_data: NonNull<AudioBufferList>,
    _in_time: NonNull<AudioTimeStamp>,
    out_data: NonNull<AudioBufferList>,
    _out_time: NonNull<AudioTimeStamp>,
    _client: *mut c_void,
) -> i32 {
    let entry_us = now_us();
    let rate = f64::from_bits(RATE_BITS.load(Ordering::Relaxed));
    let total = TOTAL.load(Ordering::Relaxed);
    let list = out_data.as_ptr();
    // SAFETY: HAL cấp danh sách buffer hợp lệ với `mNumberBuffers` phần tử.
    let nbuf = unsafe { (*list).mNumberBuffers } as usize;
    let first = unsafe { (*list).mBuffers.as_mut_ptr() };
    let start = POS.load(Ordering::Relaxed);
    let mut frames_done = 0usize;
    for b in 0..nbuf {
        let buf = unsafe { &mut *first.add(b) };
        let ch = buf.mNumberChannels.max(1) as usize;
        let frames = buf.mDataByteSize as usize / 4 / ch;
        let data = buf.mData as *mut f32;
        if data.is_null() {
            continue;
        }
        frames_done = frames_done.max(frames);
        for f in 0..frames {
            let t = (start + f as u64) as f64 / rate;
            let mut v = 0.0f64;
            let phase = t - FIRST_AT_S;
            if phase >= 0.0 {
                let k = (phase / PERIOD_S).floor() as usize;
                let within = phase - k as f64 * PERIOD_S;
                if k < total && within < BURST_S {
                    // Bao biên 1 ms để tránh tiếng click.
                    let env = (within / 0.001).min(1.0).min((BURST_S - within) / 0.001);
                    v = AMPLITUDE * env.max(0.0) * (2.0 * std::f64::consts::PI * FREQ_HZ * t).sin();
                    if b == 0 && k >= SEEN.load(Ordering::Relaxed) && k < MAX_BEEPS {
                        BEEP_US[k]
                            .store(entry_us + (f as f64 / rate * 1e6) as u64, Ordering::Release);
                        SEEN.store(k + 1, Ordering::Release);
                    }
                }
            }
            for c in 0..ch {
                // SAFETY: f*ch+c < frames*ch = mDataByteSize/4.
                unsafe { *data.add(f * ch + c) = v as f32 };
            }
        }
    }
    POS.store(start + frames_done as u64, Ordering::Relaxed);
    0
}

pub fn run(count: usize) -> Result<(), String> {
    let dev = ca::default_output_device()?;
    let rate = ca::get_prop::<f64>(
        dev,
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
    )?;
    let lat = ca::get_prop::<u32>(
        dev,
        kAudioDevicePropertyLatency,
        kAudioObjectPropertyScopeOutput,
    )
    .unwrap_or(0);
    let safety = ca::get_prop::<u32>(
        dev,
        kAudioDevicePropertySafetyOffset,
        kAudioObjectPropertyScopeOutput,
    )
    .unwrap_or(0);
    let bufsz = ca::get_prop::<u32>(
        dev,
        kAudioDevicePropertyBufferFrameSize,
        kAudioObjectPropertyScopeGlobal,
    )
    .unwrap_or(0);
    RATE_BITS.store(rate.to_bits(), Ordering::Relaxed);
    TOTAL.store(count.min(MAX_BEEPS), Ordering::Relaxed);
    println!(
        "OUTDEV rate={rate:.0} latency_frames={lat} safety_offset_frames={safety} buffer_frames={bufsz} output_latency_ms={:.1}",
        (lat + safety + bufsz) as f64 / rate * 1e3
    );

    let mut proc_id: AudioDeviceIOProcID = None;
    // SAFETY: `out_proc` khớp chữ ký AudioDeviceIOProc, không dùng client data.
    ca::status("AudioDeviceCreateIOProcID", unsafe {
        AudioDeviceCreateIOProcID(
            dev,
            Some(out_proc),
            std::ptr::null_mut(),
            NonNull::from(&mut proc_id),
        )
    })?;
    ca::status("AudioDeviceStart", unsafe {
        AudioDeviceStart(dev, proc_id)
    })?;

    let mut printed = 0usize;
    let deadline = std::time::Instant::now()
        + Duration::from_secs_f64(FIRST_AT_S + count as f64 * PERIOD_S + 0.5);
    while std::time::Instant::now() < deadline {
        let seen = SEEN.load(Ordering::Acquire);
        while printed < seen {
            println!("BEEP {}", BEEP_US[printed].load(Ordering::Acquire));
            printed += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    unsafe {
        let _ = AudioDeviceStop(dev, proc_id);
        let _ = AudioDeviceDestroyIOProcID(dev, proc_id);
    }
    Ok(())
}
