//! Spike 0.4: bắt âm thanh hệ thống trên macOS bằng Core Audio process tap và ScreenCaptureKit.
//!
//! Cách dùng:
//!   macos-audio tap  [giây]   đo bằng Core Audio process tap
//!   macos-audio sck  [giây]   đo bằng ScreenCaptureKit
//!   macos-audio beep [số bíp] (nội bộ) phát bíp tổng hợp
//! Thêm `--no-beep` để không phát bíp (tự phát nhạc/video bên ngoài).
//! Âm thanh chỉ nằm trong RAM; không ghi file.

mod beep;
mod ca;
mod sck;
mod stats;
mod tap;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct BeepRun {
    child: Child,
    times: Arc<Mutex<Vec<u64>>>,
    outdev: Arc<Mutex<String>>,
    reader: std::thread::JoinHandle<()>,
}

fn spawn_beeper(count: usize) -> Result<BeepRun, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .arg("beep")
        .arg(count.to_string())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = child
        .stdout
        .take()
        .ok_or("không có stdout của tiến trình bíp")?;
    let times = Arc::new(Mutex::new(Vec::new()));
    let outdev = Arc::new(Mutex::new(String::new()));
    let (t2, o2) = (times.clone(), outdev.clone());
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("BEEP ") {
                if let (Ok(us), Ok(mut g)) = (v.trim().parse::<u64>(), t2.lock()) {
                    g.push(us);
                }
            } else if line.starts_with("OUTDEV") {
                if let Ok(mut g) = o2.lock() {
                    *g = line;
                }
            }
        }
    });
    Ok(BeepRun {
        child,
        times,
        outdev,
        reader,
    })
}

fn finish_beeper(mut b: BeepRun) -> (Vec<u64>, String) {
    let _ = b.child.wait();
    let _ = b.reader.join();
    let times = b.times.lock().map(|g| g.clone()).unwrap_or_default();
    let outdev = b.outdev.lock().map(|g| g.clone()).unwrap_or_default();
    (times, outdev)
}

fn run_capture(mode: &str, secs: u64, with_beep: bool) -> Result<(), String> {
    println!("OS: macOS (xem sw_vers); chế độ = {mode}; thời lượng = {secs} s; bíp = {with_beep}");
    let expect = secs as f64;
    let n_beeps = secs.saturating_sub(1) as usize;

    enum S {
        Tap(tap::Session),
        Sck(sck::Session),
    }
    let session = match mode {
        "tap" => {
            let s = tap::Session::start(expect)?;
            println!("tap: {}", s.info);
            S::Tap(s)
        }
        _ => {
            let s = sck::Session::start(expect)?;
            println!("sck: {}", s.info);
            S::Sck(s)
        }
    };

    // Làm nóng 1 s rồi mới phát bíp để loại trừ thời gian khởi động stream.
    std::thread::sleep(Duration::from_secs(1));
    let beeper = if with_beep {
        Some(spawn_beeper(n_beeps)?)
    } else {
        None
    };
    std::thread::sleep(Duration::from_secs(secs + 1));

    let (cap, video) = match session {
        S::Tap(s) => (s.finish()?, None),
        S::Sck(s) => {
            let (c, v) = s.finish()?;
            (c, Some(v))
        }
    };
    let (beeps, outdev) = match beeper {
        Some(b) => finish_beeper(b),
        None => (Vec::new(), String::new()),
    };
    if !outdev.is_empty() {
        println!("{outdev}");
    }
    if let Some(v) = video {
        println!("khung hình video (bị bỏ): {v}");
    }
    print!("{}", cap.report(mode, &beeps));
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let no_beep = args.iter().any(|a| a == "--no-beep");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let mode = pos.first().map(|s| s.as_str()).unwrap_or("");
    let num = pos.get(1).and_then(|s| s.parse::<u64>().ok());
    let result = match mode {
        "tap" | "sck" => run_capture(mode, num.unwrap_or(10), !no_beep),
        "beep" => beep::run(num.unwrap_or(9) as usize),
        "perm" => {
            println!("CGPreflightScreenCaptureAccess = {}", sck::preflight());
            Ok(())
        }
        _ => Err("dùng: macos-audio <tap|sck|beep|perm> [giây] [--no-beep]".to_string()),
    };
    if let Err(e) = result {
        eprintln!("LỖI: {e}");
        std::process::exit(1);
    }
}
