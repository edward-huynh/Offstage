# Hướng dẫn sử dụng Offstage

Offstage là lớp phủ trong suốt nằm trên mọi ứng dụng và không hiện khi bạn chia sẻ màn hình. Nó đọc kịch bản bằng teleprompter, hiện phụ đề cuộc gọi và gợi ý câu trả lời từ chính tài liệu của bạn. English: [USER-GUIDE.en.md](USER-GUIDE.en.md).

## Lần đầu mở

Màn giới thiệu dẫn bạn qua năm bước, và bước nào cũng bỏ qua được:

1. **Ngôn ngữ** của ứng dụng (Tiếng Việt hoặc English).
2. **Mô hình nhận dạng giọng nói**, tải về một lần (khoảng 190 MB). Âm thanh được xử lý trên máy bạn.
3. **Quyền âm thanh.** Micro là giọng của bạn, âm thanh hệ thống là giọng của người bên kia. macOS chỉ hỏi quyền âm thanh hệ thống khi bạn bắt đầu nghe lần đầu.
4. **Trợ lý AI**: chạy trên máy (riêng tư, cần tải một model), dùng nhà cung cấp qua mạng (nhập khoá API), hoặc để sau.
5. **Sẵn sàng**: chạy thử kịch bản mẫu hoặc vào ứng dụng.

Mọi thứ trong giới thiệu đều chỉnh lại được trong Cài đặt.

## Đọc kịch bản (Content)

Mục **Content** là nơi viết kịch bản. Ngắt đoạn bằng một dòng trống: mỗi đoạn là một khối khi đọc. Chọn một kịch bản ở **Home** rồi bấm **Play**: overlay hiện và cuộn.

| Phím | Tác dụng |
|---|---|
| Space | Chạy / tạm dừng |
| ↑ / ↓ | Tốc độ |
| + / − | Cỡ chữ |
| M | Lật gương |
| E | Sửa kịch bản (Esc để xong) |
| Home | Về đầu |
| Esc | Dừng |

Phím tắt toàn cục hoạt động cả khi Offstage không ở phía trước: `Ctrl/⌘+Alt+Space` chạy/dừng, `Ctrl/⌘+Alt+↑/↓` tốc độ, `Ctrl/⌘+Alt+O` ẩn/hiện, `Ctrl/⌘+Alt+T` chuột xuyên qua.

## Nghe cuộc gọi (Interview)

Ở **Home**, bấm **Nghe và hiện phụ đề**. Trong overlay, phím `1` bật micro, `2` bật âm thanh hệ thống, `T` mở phụ đề. Nguồn mặc định chọn trong Cài đặt, mục Giọng nói.

## Trợ lý AI

Khi đối phương hỏi, trợ lý hiện vài gạch đầu dòng ngay trên overlay, kèm nguồn ("Từ tài liệu: CV.pdf › Kinh nghiệm").

1. Bật **Trợ lý AI** ở Cài đặt, mục Trợ lý AI.
2. Chọn nơi chạy:
   - **Chỉ chạy trên máy**: tải một model (Qwen3 4B khoảng 2,5 GB, hoặc 1.7B khoảng 1,1 GB; "Tự chọn" dựa vào RAM của máy). Không gửi gì đi đâu. Chậm hơn trên máy yếu.
   - **Nhà cung cấp qua mạng**: OpenAI, OpenCode Zen hoặc Go, hoặc địa chỉ tương thích OpenAI. Nhập khoá API, nó được lưu trong Keychain của hệ điều hành. Câu hỏi và vài dòng lời thoại gần đó được gửi dưới dạng chữ, cùng các đoạn tài liệu khớp với câu hỏi.
   - **Dự phòng trên máy**: khi nhà cung cấp lỗi hoặc quá 5 giây chưa viết chữ nào, model trên máy viết thay.
3. Thêm tài liệu ở **Knowledge** (xem dưới).

Đừng coi gợi ý là sự thật: model nhỏ có thể thêm chi tiết không có trong tài liệu. Hãy đọc lại trước khi nói.

## Tài liệu (Knowledge)

Thêm CV, mô tả công việc, tiêu chí đánh giá hay ghi chú: tệp `.pdf`, `.docx`, `.md`, `.txt` (tối đa 20 MB), hoặc tự gõ. Offstage đọc chữ ngay trên máy và lập chỉ mục để tìm đoạn liên quan. Mỗi tài liệu hiện trạng thái: "AI đã đọc", hoặc "Không đọc được" kèm lý do (ví dụ PDF quét từ ảnh không có chữ).

- **Tìm theo nghĩa** (tuỳ chọn): tải một mô hình nhỏ (khoảng 135 MB) để tìm được đoạn liên quan cả khi câu hỏi không dùng đúng từ của tài liệu, kể cả khác ngôn ngữ. Chưa tải thì vẫn tìm theo từ.
- Khi tìm không ra đoạn nào mà tài liệu còn ngắn (một CV và một mô tả công việc), Offstage gửi cả tài liệu.

## Workspace

Mỗi **Workspace** dành cho một tình huống (một buổi phỏng vấn, một cuộc gọi bán hàng): tài liệu riêng, **ghi chú cho AI** (vị trí ứng tuyển, công ty, người nghe), và lựa chọn riêng:

- **Chỉ chạy trên máy** cho riêng workspace đó, kể cả khi cài đặt chung dùng nhà cung cấp qua mạng.
- **Nhà cung cấp riêng**.

AI chỉ tìm trong tài liệu của workspace đang dùng. Chuyển workspace bằng ô chọn ở đầu thanh bên. Xoá một workspace là xoá cả tài liệu của nó.

## Quyền riêng tư và chia sẻ màn hình

- Overlay luôn được loại khỏi luồng chia sẻ màn hình. Cửa sổ chính cũng ẩn mặc định (tắt được trong Cài đặt, Hệ thống), vì nó hiện CV và mô tả công việc.
- Máy ảnh thật và thiết bị thu hình vẫn thấy màn hình.
- Kết quả theo từng ứng dụng: [COMPATIBILITY.md](COMPATIBILITY.md). Hãy thử với người xem thật trước một cuộc họp quan trọng.

## Gỡ rối

| Hiện tượng | Cách xử lý |
|---|---|
| Không có phụ đề | Kiểm tra quyền micro và âm thanh hệ thống (Cài đặt, Giọng nói), và bạn đã tải mô hình giọng nói chưa. |
| "Chưa tải model chạy trên máy" | Tải model ở Cài đặt, Trợ lý AI. Bản chạy từ mã nguồn cần `cargo build -p offstage-llm-host`. |
| Trợ lý không trả lời | Cài đặt, Trợ lý AI: xem dòng trạng thái ("thiếu khoá API", "chưa tải model"). |
| Model local chậm | Chọn Qwen3 1.7B. |
| Tài liệu "Không đọc được" | PDF quét từ ảnh cần một bản có chữ. |

## Sử dụng có trách nhiệm

Bạn chịu trách nhiệm tuân thủ quy định của cuộc họp, buổi phỏng vấn, kỳ thi hay tổ chức nơi bạn dùng Offstage, và các luật về ghi âm, đồng ý và dữ liệu cá nhân. Nhiều nhà tuyển dụng và đơn vị tổ chức thi cấm dùng AI mà không công khai.
