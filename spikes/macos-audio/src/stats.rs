//! Thu thập số đo trong RAM. Mẫu âm thanh chỉ nằm trong một `Vec<f32>` mono, không ghi ra đĩa.

use std::time::{SystemTime, UNIX_EPOCH};

/// Mốc thời gian chung giữa tiến trình đo và tiến trình phát bíp (micro giây từ epoch).
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Ngưỡng biên độ dò điểm bắt đầu tiếng bíp (bíp phát biên độ 0.3, tiếng nền thường < 0.1).
const ONSET_THRESHOLD: f32 = 0.15;
/// Cần im lặng ít nhất khoảng này trước khi một đoạn vượt ngưỡng được tính là tiếng bíp mới.
const QUIET_BEFORE_ONSET_S: f64 = 0.3;

struct Callback {
    arrival_us: u64,
    start_idx: usize,
}

pub struct Capture {
    pub rate: f64,
    pub channels: u32,
    pub format_note: String,
    mono: Vec<f32>,
    callbacks: Vec<Callback>,
    frames_per_cb: Vec<usize>,
    pub discontinuities: u32,
}

impl Capture {
    pub fn new(rate: f64, channels: u32, format_note: String, expect_secs: f64) -> Self {
        let cap = (rate * (expect_secs + 5.0)) as usize;
        Self {
            rate,
            channels,
            format_note,
            mono: Vec::with_capacity(cap),
            callbacks: Vec::with_capacity(4096),
            frames_per_cb: Vec::with_capacity(4096),
            discontinuities: 0,
        }
    }

    /// Thêm một callback: mỗi phần tử của `frames` là một khung (các kênh xen kẽ sẵn, đã trộn mono).
    pub fn push_mono(&mut self, arrival_us: u64, mono: impl Iterator<Item = f32>) {
        let start_idx = self.mono.len();
        self.mono.extend(mono);
        let frames = self.mono.len() - start_idx;
        self.callbacks.push(Callback {
            arrival_us,
            start_idx,
        });
        self.frames_per_cb.push(frames);
    }

    /// Tìm các điểm bắt đầu tiếng bíp; trả về (chỉ số mẫu, thời điểm callback chứa mẫu đó).
    fn onsets(&self) -> Vec<(usize, u64)> {
        let quiet_needed = (self.rate * QUIET_BEFORE_ONSET_S) as usize;
        let mut quiet_run = 0usize;
        let mut out = Vec::new();
        let mut cb_idx = 0usize;
        for (i, s) in self.mono.iter().enumerate() {
            if s.abs() > ONSET_THRESHOLD {
                if quiet_run >= quiet_needed {
                    while cb_idx + 1 < self.callbacks.len()
                        && self.callbacks[cb_idx + 1].start_idx <= i
                    {
                        cb_idx += 1;
                    }
                    out.push((i, self.callbacks[cb_idx].arrival_us));
                }
                quiet_run = 0;
            } else {
                quiet_run += 1;
            }
        }
        out
    }

    pub fn report(&self, label: &str, beeps_us: &[u64]) -> String {
        let mut r = String::new();
        let n = self.mono.len();
        r += &format!("=== {label} ===\n");
        r += &format!("định dạng gốc      : {}\n", self.format_note);
        r += &format!(
            "tần số / kênh      : {:.0} Hz, {} kênh\n",
            self.rate, self.channels
        );
        r += &format!(
            "callback           : {} (khung/callback min {} / TB {:.0} / max {})\n",
            self.callbacks.len(),
            self.frames_per_cb.iter().min().copied().unwrap_or(0),
            if self.frames_per_cb.is_empty() {
                0.0
            } else {
                n as f64 / self.frames_per_cb.len() as f64
            },
            self.frames_per_cb.iter().max().copied().unwrap_or(0),
        );
        if self.callbacks.len() >= 2 {
            let first = self.callbacks[0].arrival_us;
            let last = self.callbacks[self.callbacks.len() - 1].arrival_us;
            let span_s = (last - first) as f64 / 1e6;
            let mut max_gap_ms = 0.0f64;
            let mut sum_gap_ms = 0.0f64;
            for w in self.callbacks.windows(2) {
                let g = (w[1].arrival_us - w[0].arrival_us) as f64 / 1e3;
                sum_gap_ms += g;
                max_gap_ms = max_gap_ms.max(g);
            }
            let mean_gap = sum_gap_ms / (self.callbacks.len() - 1) as f64;
            r += &format!(
                "khoảng cách callback: TB {mean_gap:.2} ms, max {max_gap_ms:.2} ms (span {span_s:.2} s)\n"
            );
            // Số khung nhận được ngoài callback đầu / thời gian tường giữa callback đầu và cuối.
            let first_frames = self.frames_per_cb[0];
            let eff = (n - first_frames) as f64 / span_s;
            r += &format!(
                "tần số hiệu dụng   : {eff:.1} Hz (so với danh định {:.0} Hz, lệch {:+.3}%)\n",
                self.rate,
                (eff / self.rate - 1.0) * 100.0
            );
        }
        r += &format!(
            "tổng khung trong RAM: {n} ({:.2} s)\n",
            n as f64 / self.rate
        );
        let peak = self.mono.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        let rms = if n == 0 {
            0.0
        } else {
            (self.mono.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / n as f64).sqrt()
        };
        let all_zero = self.mono.iter().all(|s| *s == 0.0);
        r += &format!("biên độ            : peak {peak:.4}, rms {rms:.5}, toàn số 0: {all_zero}\n");
        r += &format!("gián đoạn mốc thời gian: {}\n", self.discontinuities);

        if std::env::var_os("SPIKE_ENV").is_some() {
            let win = (self.rate * 0.1) as usize;
            let env: Vec<String> = self
                .mono
                .chunks(win.max(1))
                .map(|c| format!("{:.2}", c.iter().fold(0.0f32, |a, s| a.max(s.abs()))))
                .collect();
            r += &format!("đường bao 100 ms: {}\n", env.join(" "));
        }
        let onsets = self.onsets();
        r += &format!("tiếng bíp dò được  : {}\n", onsets.len());
        if !beeps_us.is_empty() {
            let mut lat_ms: Vec<f64> = Vec::new();
            for (idx, arr) in &onsets {
                // Bíp gần nhất xảy ra trước (hoặc cùng lúc) thời điểm nhận.
                let matched = beeps_us.iter().rev().find(|b| **b <= *arr);
                if let Some(b) = matched {
                    let ms = (*arr - *b) as f64 / 1e3;
                    r += &format!(
                        "  bíp @mẫu {idx:>7}: trễ (callback nhận − lúc phát ghi) = {ms:7.1} ms\n"
                    );
                    lat_ms.push(ms);
                }
            }
            if !lat_ms.is_empty() {
                lat_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mean = lat_ms.iter().sum::<f64>() / lat_ms.len() as f64;
                let median = lat_ms[lat_ms.len() / 2];
                r += &format!(
                    "độ trễ ({} bíp khớp / {} đã phát): min {:.1} / trung vị {:.1} / TB {:.1} / max {:.1} ms\n",
                    lat_ms.len(),
                    beeps_us.len(),
                    lat_ms[0],
                    median,
                    mean,
                    lat_ms[lat_ms.len() - 1]
                );
            } else {
                r += "độ trễ: không khớp được bíp nào\n";
            }
        }
        r
    }
}
