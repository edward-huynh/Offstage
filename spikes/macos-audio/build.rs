//! `screencapturekit` dùng cầu nối Swift; binary cần rpath tới runtime Swift có sẵn trên macOS.
fn main() {
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
