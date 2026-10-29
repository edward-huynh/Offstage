//! Throwaway benchmark: speech-to-text latency and accuracy on a manifest of WAV files.
//! Usage: bench <model> <manifest.tsv> <gpu|cpu> <vi|auto|en> <reps>
use std::time::Instant;

use offstage_stt::{Language, SpeechToText, WhisperConfig, WhisperStt};

fn read_wav(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read wav");
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            let end = (pos + 8 + size).min(bytes.len());
            return bytes[pos + 8..end]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
                .collect();
        }
        pos += 8 + size + (size % 2);
    }
    panic!("no data chunk");
}

/// Deterministic noise so every run sees the same audio.
struct Rng(u64);
impl Rng {
    fn uniform(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f32) / ((1u64 << 53) as f32)
    }
    fn gauss(&mut self) -> f32 {
        let (a, b) = (self.uniform().max(1e-9), self.uniform());
        (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos()
    }
}

fn with_noise(samples: &[f32], snr_db: f32) -> Vec<f32> {
    let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let noise_rms = (power / 10f32.powf(snr_db / 10.0)).sqrt();
    let mut rng = Rng(0x9E3779B97F4A7C15);
    samples.iter().map(|s| (s + noise_rms * rng.gauss()).clamp(-1.0, 1.0)).collect()
}

fn normalize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn edit_distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(x != y)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (model, manifest, mode, lang, reps) = (&a[1], &a[2], &a[3], &a[4], a[5].parse::<usize>().unwrap());
    let mut config = WhisperConfig::new(model);
    config.use_gpu = mode == "gpu";
    config.language = lang.parse::<Language>().expect("language");
    let t = Instant::now();
    let stt = WhisperStt::load(config).expect("load");
    println!("# mode={mode} lang={lang} reps={reps} load={:.2?}", t.elapsed());

    println!("id\tvariant\tdur_s\tmedian_ms\tp90_ms\trtf\twer\tcer\thyp");
    // (variant, lang) -> (word errs, ref words, char errs, ref chars, n)
    let mut totals: std::collections::BTreeMap<(String, String), (usize, usize, usize, usize, usize)> = Default::default();
    // (variant) -> latencies per file for the bucket summary: (dur, median_ms)
    let mut by_len: Vec<(f64, f64)> = Vec::new();

    for line in std::fs::read_to_string(manifest).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (id, flang, path, reference) = (f[0], f[1], f[2], f[3]);
        let clean = read_wav(path);
        let dur = clean.len() as f64 / 16_000.0;
        for (variant, audio) in [
            ("clean", clean.clone()),
            ("snr20", with_noise(&clean, 20.0)),
            ("snr10", with_noise(&clean, 10.0)),
        ] {
            let hyp = stt.transcribe(&audio).expect("warmup").text; // also warms caches
            let mut ms: Vec<f64> = (0..reps)
                .map(|_| {
                    let t = Instant::now();
                    stt.transcribe(&audio).expect("transcribe");
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            ms.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let (med, p90) = (percentile(&ms, 0.5), percentile(&ms, 0.9));
            let (rw, hw) = (normalize(reference), normalize(&hyp));
            let (rc, hc): (Vec<char>, Vec<char>) = (rw.join(" ").chars().collect(), hw.join(" ").chars().collect());
            let (we, ce) = (edit_distance(&rw, &hw), edit_distance(&rc, &hc));
            println!(
                "{id}\t{variant}\t{dur:.1}\t{med:.0}\t{p90:.0}\t{:.3}\t{:.2}\t{:.2}\t{}",
                med / 1000.0 / dur,
                we as f64 / rw.len() as f64,
                ce as f64 / rc.len() as f64,
                hyp.trim()
            );
            let e = totals.entry((variant.to_owned(), flang.to_owned())).or_default();
            *e = (e.0 + we, e.1 + rw.len(), e.2 + ce, e.3 + rc.len(), e.4 + 1);
            if variant == "clean" {
                by_len.push((dur, med));
            }
        }
    }

    println!("\n# accuracy (corpus level)  variant lang  files  WER  CER");
    for ((variant, lang), (we, rw, ce, rc, n)) in &totals {
        println!("{variant:6} {lang:3} {n:>3}  WER {:5.1}%  CER {:5.1}%", 100.0 * *we as f64 / *rw as f64, 100.0 * *ce as f64 / *rc as f64);
    }
    println!("\n# latency by length (clean audio, median of {reps})");
    for (lo, hi) in [(0.0, 4.0), (4.0, 6.0), (6.0, 9.0), (9.0, 20.0)] {
        let v: Vec<&(f64, f64)> = by_len.iter().filter(|(d, _)| *d >= lo && *d < hi).collect();
        if v.is_empty() { continue; }
        let mean_ms = v.iter().map(|(_, m)| m).sum::<f64>() / v.len() as f64;
        let mean_d = v.iter().map(|(d, _)| d).sum::<f64>() / v.len() as f64;
        println!("{lo:>4.0}-{hi:<3.0}s  n={}  mean audio {mean_d:.1}s  mean latency {mean_ms:.0} ms  rtf {:.3}", v.len(), mean_ms / 1000.0 / mean_d);
    }
}
