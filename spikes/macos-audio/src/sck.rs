//! Hướng (b): ScreenCaptureKit audio (macOS 13+), qua crate `screencapturekit`.
//! Bắt âm thanh hệ thống bằng một stream gắn display; video tắt tối đa (2x2 px, 1 fps) và bị bỏ.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use screencapturekit::cm::CMTime;
use screencapturekit::prelude::*;

use crate::stats::{now_us, Capture};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

/// Quyền Screen Recording hiện tại (không hiện prompt).
pub fn preflight() -> bool {
    // SAFETY: hàm CoreGraphics không tham số, không có tác dụng phụ.
    unsafe { CGPreflightScreenCaptureAccess() }
}

struct Shared {
    cap: Mutex<Option<Capture>>,
    prev_end: Mutex<Option<f64>>,
    video_frames: AtomicU32,
    expect_secs: f64,
}

struct Handler {
    shared: Arc<Shared>,
}

impl SCStreamOutputTrait for Handler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
        let arrival = now_us();
        match output_type {
            SCStreamOutputType::Screen => {
                self.shared.video_frames.fetch_add(1, Ordering::Relaxed);
            }
            SCStreamOutputType::Audio => {
                let Ok(list) = sample.audio_buffer_list() else {
                    return;
                };
                let (rate, ch) = match sample.format_description() {
                    Some(fd) => (
                        fd.audio_sample_rate().unwrap_or(0.0),
                        fd.audio_channel_count().unwrap_or(0),
                    ),
                    None => (0.0, 0),
                };
                let nbuf = list.num_buffers();
                if nbuf == 0 {
                    return;
                }
                let mut mono: Vec<f32> = Vec::new();
                let mut planar_note = "";
                if nbuf > 1 {
                    // Planar: mỗi buffer một kênh.
                    planar_note = "planar";
                    for i in 0..nbuf {
                        let Some(b) = list.buffer(i) else { continue };
                        let samples: Vec<f32> = b
                            .data()
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                        if mono.is_empty() {
                            mono.resize(samples.len(), 0.0);
                        }
                        for (m, s) in mono.iter_mut().zip(&samples) {
                            *m += *s / nbuf as f32;
                        }
                    }
                } else if let Some(b) = list.get(0) {
                    planar_note = "interleaved";
                    let chn = (b.number_channels as usize).max(1);
                    let samples: Vec<f32> = b
                        .data()
                        .chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect();
                    mono.extend(
                        samples
                            .chunks_exact(chn)
                            .map(|f| f.iter().sum::<f32>() / chn as f32),
                    );
                }
                let pts = sample.presentation_timestamp().as_seconds().unwrap_or(0.0);
                let frames = mono.len();
                let Ok(mut guard) = self.shared.cap.lock() else {
                    return;
                };
                let cap = guard.get_or_insert_with(|| {
                    Capture::new(
                        rate,
                        ch,
                        format!("CMSampleBuffer: {rate:.0} Hz, {ch} kênh, Float32 {planar_note}, {nbuf} buffer/callback"),
                        self.shared.expect_secs,
                    )
                });
                if let Ok(mut prev) = self.shared.prev_end.lock() {
                    if let Some(p) = *prev {
                        // Lệch hơn nửa mẫu so với kỳ vọng thì coi là gián đoạn.
                        if (pts - p).abs() > 0.5 / rate.max(1.0) + 1e-4 {
                            cap.discontinuities += 1;
                        }
                    }
                    *prev = Some(pts + frames as f64 / rate.max(1.0));
                }
                cap.push_mono(arrival, mono.into_iter());
            }
            SCStreamOutputType::Microphone => {}
        }
    }
}

pub struct Session {
    stream: SCStream,
    shared: Arc<Shared>,
    pub info: String,
}

impl Session {
    pub fn start(expect_secs: f64) -> Result<Session, String> {
        let preflight_before = preflight();
        let t0 = std::time::Instant::now();
        let content =
            SCShareableContent::get().map_err(|e| format!("SCShareableContent::get: {e}"))?;
        let get_ms = t0.elapsed().as_secs_f64() * 1e3;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or("không có display")?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| format!("filter: {e}"))?;
        // Cho phép thử tần số/kênh khác (SPIKE_SCK_RATE, SPIKE_SCK_CH) để xem SCK có xuất thẳng 16 kHz mono không.
        let req_rate: i32 = std::env::var("SPIKE_SCK_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(48000);
        let req_ch: i32 = std::env::var("SPIKE_SCK_CH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_captures_audio(true)
            .with_excludes_current_process_audio(false)
            .with_sample_rate(req_rate)
            .with_channel_count(req_ch);
        let shared = Arc::new(Shared {
            cap: Mutex::new(None),
            prev_end: Mutex::new(None),
            video_frames: AtomicU32::new(0),
            expect_secs,
        });
        let mut stream =
            SCStream::new(&filter, &config).map_err(|e| format!("SCStream::new: {e}"))?;
        stream
            .add_output_handler(
                Handler {
                    shared: shared.clone(),
                },
                SCStreamOutputType::Screen,
            )
            .map_err(|e| format!("add screen handler: {e}"))?;
        stream
            .add_output_handler(
                Handler {
                    shared: shared.clone(),
                },
                SCStreamOutputType::Audio,
            )
            .map_err(|e| format!("add audio handler: {e}"))?;
        let t1 = std::time::Instant::now();
        stream
            .start_capture()
            .map_err(|e| format!("start_capture: {e}"))?;
        let start_ms = t1.elapsed().as_secs_f64() * 1e3;
        let info = format!(
            "CGPreflightScreenCaptureAccess trước khi chạy = {preflight_before}; SCShareableContent::get mất {get_ms:.0} ms; start_capture mất {start_ms:.0} ms; yêu cầu {req_rate} Hz / {req_ch} kênh"
        );
        Ok(Session {
            stream,
            shared,
            info,
        })
    }

    pub fn finish(self) -> Result<(Capture, u32), String> {
        self.stream
            .stop_capture()
            .map_err(|e| format!("stop_capture: {e}"))?;
        std::thread::sleep(Duration::from_millis(100));
        let video = self.shared.video_frames.load(Ordering::Relaxed);
        let mut guard = self.shared.cap.lock().map_err(|e| format!("mutex: {e}"))?;
        guard
            .take()
            .map(|c| (c, video))
            .ok_or_else(|| "không nhận được callback audio nào".to_string())
    }
}
