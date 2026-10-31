# Spike 0.4: bắt âm thanh hệ thống trên macOS

Kết quả spike task 0.4. Quyết định ở đây chặn task 2.1 phía macOS (`crates/audio/macos_loopback.rs`, speaker `Remote`).

Mã spike nằm ở `spikes/macos-audio/` (crate throwaway, có `[workspace]` rỗng riêng, không thuộc workspace gốc, không ảnh hưởng `npm run check`). Âm thanh chỉ nằm trong RAM (một `Vec<f32>` mono), không ghi file, không có file âm thanh nào trong repo.

## Kết luận ngắn

**Dùng Core Audio process tap làm đường chính (macOS 14.2+), ScreenCaptureKit (SCK) làm dự phòng cho macOS 13 đến 14.1.** Giả thuyết ban đầu được số đo ủng hộ: tap trễ thấp hơn, nhẹ hơn, không phụ thuộc màn hình, quyền hẹp hơn. Hai điều cần kiểm chứng tiếp trước khi chốt hẳn (mục [Chưa đo được](#chưa-đo-được-hoặc-chưa-kết-luận-được)): hành vi quyền của tap trong app đã đóng gói/ký (có thể treo vô hạn khi chưa cấp quyền) và hành vi khi đổi thiết bị xuất.

## Môi trường đo

| Mục | Giá trị |
|---|---|
| Máy | MacBook Pro, Apple M1 Pro (arm64) |
| OS | macOS 26.6.2 (build 25G83), Darwin 25.6 |
| Rust | cargo 1.93, build `--release` |
| Thiết bị xuất mặc định | màn hình MSI MP251 E2 qua HDMI, 48 kHz, 2 kênh, buffer HAL 512 khung, latency 88 + safety offset 320 khung (≈ 19,2 ms ước tính phía xuất) |
| Binary | chưa ký (chỉ chữ ký ad-hoc `linker-signed` do toolchain), không `Info.plist`, không entitlement, 1,7 MB |
| Crate | `objc2-core-audio` 0.3 (tap), `screencapturekit` 11.1 (SCK) |
| Tiến trình cha khi đo | Claude.app, rồi `claude-code`, rồi `zsh`, rồi spike. TCC gán quyền cho `com.anthropic.claude-code`. Lần thử riêng từ Terminal.app: xem bên dưới |

## Cách đo

- `tap` / `sck`: mở luồng bắt âm thanh hệ thống, giữ mẫu trong RAM (mono trộn), đo trong 10 hoặc 60 giây.
- Độ trễ: một tiến trình phụ (`beep`, chính binary này) phát bíp 1 kHz biên độ 0,3, dài 150 ms, mỗi giây một lần, sinh trực tiếp trong IOProc ra thiết bị xuất mặc định (không dùng file). Nó in `epoch_us` lúc IOProc của nó ghi mẫu bíp. Tiến trình đo ghi `epoch_us` lúc callback chứa điểm bắt đầu bíp (ngưỡng biên độ 0,15) đến nơi. Độ trễ = hiệu hai mốc này, cùng một đồng hồ hệ thống.
- Ổn định: số callback, khoảng cách giữa các callback, tần số hiệu dụng (số khung nhận được chia cho thời gian tường), số lần mốc thời gian của luồng bị đứt (`mSampleTime` cho tap, PTS cho SCK).
- Lệnh tái lập: `cargo build --release` trong `spikes/macos-audio`, rồi `./target/release/offstage-macos-audio-spike tap 10`, `... sck 10`, thêm `--no-beep` để tự phát nhạc. `SPIKE_ENV=1` in đường bao biên độ. `SPIKE_SCK_RATE` và `SPIKE_SCK_CH` đổi định dạng SCK yêu cầu.

**Giới hạn của phép đo trễ.** Mốc "phát" là lúc callback xuất của tiến trình phát chạy (HAL gọi trước thời điểm loa thật sự phát), mốc "nhận" là lúc callback của ta chạy. Số đo gồm buffering phía phát, độ chi tiết của callback (10,7 ms với tap, 20 ms với SCK) và sai số ngưỡng vài ms. Không gồm độ trễ âm thanh ra loa thật hay Bluetooth. Hiệu số giữa hai hướng đáng tin hơn giá trị tuyệt đối.

## Số đo

Cả hai hướng chạy được trên máy này khi tiến trình cha đã có quyền. Bíp dò được 9 trên 9 (chạy 10 s) và 59 trên 59 (chạy 60 s), trừ ghi chú ở bảng.

| | Core Audio tap | SCK (yêu cầu 48 kHz stereo) | SCK (yêu cầu 16 kHz mono) |
|---|---|---|---|
| Định dạng gốc | 48000 Hz, 2 kênh, Float32 interleaved (theo thiết bị xuất) | 48000 Hz, 2 kênh, Float32 planar (đúng như yêu cầu) | 16000 Hz, 1 kênh, Float32 interleaved (SCK tự resample) |
| Khung mỗi callback | 512 (10,67 ms) | 960 (20 ms) | 320 (20 ms) |
| Khoảng cách callback TB / max | 10,67 / 10,96 ms | 20,0 / 32,0 ms | 19,9 / 67,3 ms |
| Độ trễ, chạy 60 s (59 bíp) min / trung vị / TB / max | 57,9 / 63,2 / 62,0 / 66,1 ms | 73,1 / 78,5 / 77,6 / 82,2 ms | không đo 60 s |
| Độ trễ, chạy 10 s | 63,2 ms trung vị (2 lần: 62,1 TB cả hai) | 76,3 và 85,1 ms trung vị (2 lần) | 110,7 ms trung vị, min 105,4, max 114,0 (khớp 8 trên 9 bíp, chưa rõ vì sao 1 bíp không khớp) |
| Tần số hiệu dụng, 60 s | 47999,9 Hz (0,000%) | 47991,8 Hz (−0,017%) | không đo |
| Mốc thời gian bị đứt | 0 trong 62 s | 0 trong 62 s | 0 trong 10 s |
| CPU, 20 s không phát bíp | 0,09 s CPU / 22 s (≈ 0,4%) | 0,23 s CPU / 22 s (≈ 1,0%) | không đo |
| RSS tối đa (đã dành sẵn vùng đệm 25 s) | 21,9 MB | 24,4 MB | không đo |
| Khởi động | tạo tap 3 đến 4,4 ms (chỉ đo bước này; chưa đo thời gian tới callback đầu tiên) | `SCShareableContent::get` 45 đến 52 ms, `start_capture` 106 đến 193 ms | như cột giữa |
| Thu được âm thanh khác trên máy (không phải bíp) | có (nền 0,01 đến 0,08 từ trình duyệt đang phát) | có | có |

Ghi chú:

- Tap luôn giao 512 khung mỗi 10,67 ms, rất đều, tần số hiệu dụng khớp danh định ở mức làm tròn. SCK giao 20 ms mỗi lần với jitter tới 32 ms (67 ms một lần ở 16 kHz), tần số hiệu dụng lệch trong khoảng ±0,4% ở lần chạy 10 s (nhiễu do callback đầu tiên đến dồn, không phải trôi đồng hồ: lần 60 s chỉ −0,017%).
- SCK trễ hơn tap khoảng 15 ms ở 48 kHz và khoảng 47 ms ở 16 kHz (có thêm bước resample trong SCK). Điều này đáng cân nhắc: yêu cầu 16 kHz mono trực tiếp tiết kiệm một resampler nhưng tốn thêm độ trễ.
- SCK vẫn phải mở luồng video (spike để 2x2 px, 1 fps, nhận 13 khung hình trong 12 s rồi bỏ). Đây là chi phí CPU nhỏ nhưng có thật.
- Cả hai chỉ cần một đường xử lý mono 16 kHz cho pipeline STT. Tap giao 48 kHz stereo (hoặc tần số của thiết bị xuất, xem [chưa đo](#chưa-đo-được-hoặc-chưa-kết-luận-được)), nên `crates/audio` cần bộ resample + trộn mono như dự kiến ở task 2.1.

## Quyền và cách prompt hiện ra

| | Core Audio tap | ScreenCaptureKit |
|---|---|---|
| Dịch vụ TCC | `kTCCServiceAudioCapture` ("System Audio Recording") | `kTCCServiceScreenCapture` ("Screen Recording") |
| Khoá `Info.plist` | `NSAudioCaptureUsageDescription` (theo tài liệu Apple; spike không có `Info.plist` nên chưa kiểm chứng tác động) | không cần khoá riêng cho system audio |
| Kiểm tra trước, không hiện prompt | không có API công khai (đã thấy trong log chỉ `coreaudiod` tự hỏi tccd) | `CGPreflightScreenCaptureAccess()` (đã dùng, hoạt động) |
| Khi nào prompt hiện | khi bắt đầu chạy thiết bị aggregate (`AudioDeviceStart`), không phải lúc tạo tap: tạo tap và aggregate thành công trong vài ms không cần quyền (xem bên dưới) | khi gọi `SCShareableContent::get` lần đầu |
| Chưa có quyền | **không có lỗi trả về.** `AudioDeviceStart` có thể chặn rất lâu (xem dưới), rồi không có callback nào | trả lỗi ngay: `The user declined TCCs for application, window, display capture` |
| Gán quyền cho ai | tiến trình "responsible": app cha (Claude.app / Terminal.app / IDE), không phải binary vô danh | như tap |
| Chạy với binary chưa ký từ terminal | có, khi tiến trình cha đã được cấp quyền | có, khi tiến trình cha đã được cấp quyền |

Ba điểm quan sát được:

1. **Tiến trình cha quyết định.** Binary không có định danh riêng: TCC gán cho `com.anthropic.claude-code` khi chạy từ Claude.app, và cho `com.apple.Terminal` khi mở từ Terminal.app. Trong `tauri dev` quyền sẽ gắn với terminal/IDE; bản đóng gói mới có định danh riêng cần cấp riêng.
2. **Từ Terminal.app, không có quyền.** `CGPreflightScreenCaptureAccess` = false, SCK lỗi ngay như bảng trên, tức SCK từ chối ngay lập tức (lần chạy thứ hai có hai mốc thời gian trùng nhau để xác nhận). Tap từ Terminal: dòng log `AUTHREQ_PROMPTING` cho `com.apple.Terminal` có xuất hiện (15:08:10), tiến trình treo ở `AudioDeviceStart` khoảng 3 phút rồi trả 0 callback, không lỗi. Không ai trả lời hộp thoại trong thời gian đó (không xác minh được bằng mắt, xem dưới), nên kết quả này nên đọc là "chưa có quyền thì kết thúc im lặng bằng 0 mẫu sau thời gian chờ dài", chưa phải kết luận về trường hợp cấp quyền.
3. **Treo kéo dài sau đó.** Sau lần thử Terminal, mọi lần chạy tap (kể cả dưới Claude.app, vốn chạy tốt trước đó) đều treo ở `AudioDeviceStart`. Log `coreaudiod` cho thấy tiến trình dừng ngay sau dòng `(TCC) TCCAccessRequest() IPC`, không có phản hồi từ `tccd` ghi lại. Giả thuyết hợp lý nhất: hộp thoại quyền của Terminal vẫn đang chờ trên màn hình (máy đang khoá) và hàng đợi prompt TCC bị chặn tới khi nó được trả lời hoặc huỷ. Chưa xác nhận được vì không nhìn thấy hộp thoại. SCK cũng bị ảnh hưởng ở thời điểm đó nhưng vì lý do khác (xem phần SCK phụ thuộc màn hình).

**Hệ quả thiết kế cho `crates/audio` (tap):** vì `AudioDeviceStart` có thể chặn vô hạn khi quyền chưa quyết, luôn chạy khởi động trên một thread riêng có timeout, và coi "không nhận callback nào sau khoảng 2 s" là tín hiệu chưa có quyền để dẫn người dùng tới onboarding (task 2.7), thay vì chờ lỗi.

### Tap không gửi buffer khi không có âm thanh nào phát

Đo ngày 2026-10-31 (macOS 26, loa trong): `tap 5 --no-beep` khi máy im lặng cho 0 callback, 0 khung. Có bíp thì 823 callback trong 8 s. Nên "không có buffer nào" không chứng minh thiếu quyền. `CpalSource::start` từng coi 3 s không có buffer là `permission_denied`, nên bấm nguồn âm thanh hệ thống lúc máy im lặng hiện banner xin quyền sai. Nay loopback không chờ buffer đầu tiên (xem `waits_for_first_data` trong `crates/audio/src/capture.rs`). Hệ quả: chưa biết quyền bị từ chối biểu hiện thế nào ở tap (lỗi khi mở stream hay buffer toàn số 0), cần kiểm lại trên bản đóng gói.

### Chưa giải thích được

Log tccd cho `com.anthropic.claude-code` đối với `kTCCServiceAudioCapture` ghi `AUTHREQ_RESULT authValue=0, authReason=8` ngay (vài ms, không hộp thoại) ở các lần chạy tap thành công. Nhưng tap vẫn nhận âm thanh thật (bíp đúng thời điểm). Mình không xác minh được ý nghĩa chính xác của hai mã đó (nghi `authReason=8` liên quan thiếu chuỗi mô tả sử dụng, nhưng chưa kiểm chứng) và vì sao dữ liệu vẫn đến. Vì vậy: **đừng coi việc "chạy được từ terminal" là bằng chứng quyền đã đúng.** Phải kiểm lại trên bản đóng gói có `NSAudioCaptureUsageDescription` (task 2.7).

## SCK phụ thuộc màn hình đang hoạt động

SCK gắn vào một display. Khi màn hình ngủ (máy khoá, màn hình rời tắt), `SCShareableContent::get` trả danh sách display rỗng và spike báo "không có display". Đã gặp nhiều lần: `CGDisplayIsAsleep` = true, active displays = 0. Đánh thức màn hình bằng `caffeinate -u` thì SCK chạy lại bình thường. Tap không dùng display nên không bị ràng buộc này, đây là một lý do nữa để chọn tap làm đường chính (cuộc gọi có thể kéo dài khi màn hình tắt).

## So sánh và khuyến nghị

| Tiêu chí | Core Audio tap | SCK | Hơn |
|---|---|---|---|
| macOS tối thiểu | 14.2 | 13.0 | SCK |
| Độ trễ đo được (trung vị, 48 kHz) | ≈ 63 ms | ≈ 78 ms (≈ 111 ms nếu xin 16 kHz) | tap |
| Độ đều của callback | rất đều (512 khung) | jitter tới 32 ms | tap |
| CPU | ≈ 0,4% | ≈ 1,0% (có luồng video) | tap |
| Quyền | System Audio Recording (hẹp, chỉ âm thanh) | Screen Recording (rộng; theo mô tả Apple macOS mới hay nhắc cấp lại, chưa quan sát ở đây) | tap |
| Phụ thuộc màn hình | không | có (display ngủ thì lỗi) | tap |
| Cho ra 16 kHz mono sẵn | không (tần số thiết bị xuất, 2 kênh) | có, nhưng +47 ms trễ | SCK (nhỏ) |
| Độ chín của API Rust | `objc2-core-audio` bindings thô, phải tự dựng aggregate device (khoảng 250 dòng, phần lớn unsafe, trong spike) | `screencapturekit` có API an toàn, nhưng kéo cầu nối Swift và cần rpath `/usr/lib/swift` (xem `build.rs`) | SCK |
| Chọn tiến trình cụ thể / loại trừ tiến trình | API có (`initStereoMixdownOfProcesses`, `...ButExcludeProcesses`); chưa thử | có lọc theo app/cửa sổ; chưa thử | hoà |

**Khuyến nghị:**

1. `crates/audio/macos_loopback.rs`: cài Core Audio tap (tap toàn hệ thống, stereo mixdown, aggregate riêng tư) làm đường chính cho macOS 14.2+.
2. Giữ SCK (`screencapturekit`, yêu cầu 48 kHz stereo rồi tự resample thống nhất với tap) làm dự phòng cho macOS 13 đến 14.1, chọn lúc chạy theo phiên bản OS. Nếu Phase 5 quyết định tối thiểu macOS 14.2 thì có thể bỏ hẳn SCK và tránh cầu nối Swift.
3. Khi cài tap: IOProc không được cấp phát hay khoá mutex (spike còn làm cả hai để lấy số đo, không dùng nguyên văn): sao chép vào ring buffer rồi xử lý ở thread khác. Khởi động trên thread riêng với timeout; "0 callback sau ~2 s" nghĩa là thiếu quyền.
4. Onboarding quyền (2.7): tap cần `NSAudioCaptureUsageDescription` trong `Info.plist` của bundle và có thể phải kiểm lại trên bản đóng gói; SCK cần `CGPreflightScreenCaptureAccess`/`CGRequestScreenCaptureAccess` và có thể phải mở System Settings.
5. Không phụ thuộc vào kết quả "chạy từ terminal được" để suy ra bản đóng gói chạy được.

## Chưa đo được hoặc chưa kết luận được

- **Giao diện prompt.** Không có ai trước màn hình và không có công cụ chụp màn hình desktop nên chưa thấy nội dung hộp thoại, số lần bấm, hay chúng hiện ở đâu. Thông tin về prompt dựa vào log `tccd`/`coreaudiod`, không phải quan sát trực tiếp.
- **Tap từ Terminal.app sau khi được cấp quyền.** Hộp thoại của Terminal chưa được trả lời nên chưa có số liệu cho trường hợp "đã cấp quyền cho terminal". Nếu người dùng trả lời hộp thoại, chạy lại `./target/release/offstage-macos-audio-spike tap 10` từ Terminal.app để bổ sung. Có thể phải trả lời hoặc huỷ hộp thoại đang chờ thì tap mới chạy lại được cả dưới Claude.app (xem điểm 3 ở phần quyền).
- **Bản đóng gói/ký/notarize.** Chưa thử app có `Info.plist`, entitlement (`com.apple.security.device.audio-input`) hay hardened runtime. Chưa rõ thiếu `NSAudioCaptureUsageDescription` sẽ cho hành vi gì với bundle thật.
- **macOS 13, 14.0 đến 14.1, 14.2 đến 15.x.** Chỉ có macOS 26.6.2. Các ngưỡng phiên bản (13.0 cho SCK, 14.2 cho tap) là theo tài liệu, chưa kiểm trên máy thật. Hành vi macOS 15 (nhắc cấp lại Screen Recording định kỳ) chưa quan sát.
- **Thiết bị xuất khác.** Chỉ đo với màn hình HDMI 48 kHz. Chưa đo loa trong, USB, Bluetooth (A2DP/HFP, tần số có thể khác 48 kHz), đổi thiết bị xuất giữa chừng, ngủ/thức. Aggregate device của spike dùng thiết bị xuất mặc định làm thiết bị chính nên cần xem tap có đi theo khi đổi thiết bị không.
- **Tap theo tiến trình và loại trừ tiến trình của chính app.** API có nhưng chưa thử. SCK: tham số `excludesCurrentProcessAudio` mới thử ở giá trị `false`.
- **Chạy dài.** Tối đa 62 s mỗi hướng. Chưa kiểm tra trôi đồng hồ hay rò bộ nhớ trong nhiều giờ.
- **Độ trễ tuyệt đối thật sự** (từ loa/Bluetooth đến mẫu) và độ trễ khởi động tới mẫu đầu tiên chưa đo; spike chỉ đo từ callback phát tới callback nhận.
- **Một bíp không khớp** ở lần chạy SCK 16 kHz (8 trên 9) chưa rõ nguyên nhân (bíp đầu bị bỏ lỡ hoặc nằm dưới ngưỡng khi stream còn khởi động).
- **Kích thước binary Tauri và RAM nhàn rỗi** khi thêm hai dependency: chưa đo trong app thật (spike release 1,7 MB, không đại diện).
