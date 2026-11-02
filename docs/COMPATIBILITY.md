# Tương thích chia sẻ màn hình

Offstage hứa một điều: overlay và cửa sổ chính **không hiện** khi bạn chia sẻ màn hình. Tài liệu này ghi cách kiểm tra và kết quả thật. Mọi ô còn trống là **chưa thử**, không phải "chạy tốt".

## Cơ chế

| Hệ điều hành | Cách ẩn | Ghi chú |
|---|---|---|
| Windows 10 2004 trở lên, Windows 11 | `WDA_EXCLUDEFROMCAPTURE` (Tauri `contentProtected`) | Cửa sổ biến mất hẳn khỏi ảnh chụp |
| Windows 10 cũ hơn 2004 | `WDA_MONITOR` | Cửa sổ thành khung đen. App đã cảnh báo ở Cài đặt |
| macOS | `NSWindow.sharingType = .none` (Tauri `contentProtected`) | Từ macOS 15, ứng dụng chia sẻ bằng ScreenCaptureKit có thể vẫn thấy cửa sổ. Chưa kiểm tra |

Cả overlay lẫn cửa sổ chính đều bật `contentProtected`, trong `tauri.conf.json` và lúc chạy. Cửa sổ chính có thể tắt trong Cài đặt → Hệ thống ("Ẩn cửa sổ chính khỏi chia sẻ màn hình").

## Cách kiểm tra

Cần hai người hoặc hai thiết bị: một bên chia sẻ, một bên xem. Tự xem lại bản ghi của chính mình cũng được nếu ứng dụng có ghi hình.

1. Chạy bản cài đặt thật (`npm run build`), không phải `npm run dev`, vì bản dev có thể khác về quyền và ký.
2. Mở Offstage, chọn "Kịch bản mẫu" và bấm Play để overlay hiện, chữ đang cuộn. Giữ cửa sổ chính mở ở chỗ khác.
3. Vào cuộc họp thử với ứng dụng cần kiểm tra. Chia sẻ lần lượt:
   - **Toàn màn hình** (kiểm tra quan trọng nhất)
   - **Một cửa sổ khác** (overlay không được lộ ra dù nằm đè lên cửa sổ đó)
   - **Một tab trình duyệt**, nếu ứng dụng cho chọn
4. Người xem xác nhận: overlay có hiện không? Cửa sổ chính có hiện không? Có khung đen hay vệt gì ở chỗ overlay không?
5. Bật "Nghe âm thanh hệ thống" và kiểm tra thêm: tiếng của cuộc họp vẫn được nhận dạng khi đang chia sẻ.
6. Ghi vào bảng bên dưới: ngày, phiên bản hệ điều hành, phiên bản ứng dụng, kết quả.

## Kết quả

Ký hiệu: ✅ không hiện, ❌ vẫn hiện, ⬛ hiện khung đen, ➖ chưa thử.

### macOS

| Ứng dụng | Toàn màn hình | Cửa sổ khác | Tab | Ghi chú |
|---|---|---|---|---|
| Google Meet (Chrome) | ➖ | ➖ | ➖ | |
| Microsoft Teams | ➖ | ➖ | | |
| Zoom | ➖ | ➖ | | |
| OBS (Display Capture) | ➖ | ➖ | | OBS 30 trở lên dùng ScreenCaptureKit |
| QuickTime / `screencapture` | ➖ | ➖ | | |
| Slack huddle | ➖ | ➖ | | |

Hệ điều hành đã thử: chưa có. Máy phát triển chạy macOS 26.

### Windows

| Ứng dụng | Toàn màn hình | Cửa sổ khác | Tab | Ghi chú |
|---|---|---|---|---|
| Google Meet (Chrome) | ➖ | ➖ | ➖ | |
| Microsoft Teams | ➖ | ➖ | | |
| Zoom | ➖ | ➖ | | |
| OBS (Display Capture, Window Capture) | ➖ | ➖ | | |
| Windows 10 cũ hơn 2004 | | | | Chờ khung đen, kiểm tra cảnh báo |

Hệ điều hành đã thử: chưa có.

## Khi thấy overlay lộ ra

1. Ghi lại ứng dụng, chế độ chia sẻ, phiên bản hệ điều hành và phiên bản ứng dụng.
2. Thử chia sẻ **một cửa sổ** thay vì toàn màn hình: với ScreenCaptureKit, chia sẻ từng cửa sổ thường đáng tin hơn.
3. Ghi vào cột "Ghi chú" để README nói rõ ứng dụng nào không an toàn, thay vì hứa chung chung.
