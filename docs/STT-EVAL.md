# Đo độ trễ và chất lượng nhận dạng giọng nói (task 2.8)

Kết quả đo model `whisper small q5_1` chạy trong `crates/stt`. Công cụ đo nằm ở `spikes/stt-bench/` (crate bỏ đi, không thuộc workspace, không ảnh hưởng `npm run check`).

## Kết luận ngắn

- **Độ trễ STT đạt mục tiêu dưới 1 s trên GPU** với mọi mẫu đã đo: 0,38 đến 0,63 s khi ép tiếng Việt, 0,57 đến 0,88 s khi để tự nhận ngôn ngữ.
- **Người dùng thấy chữ sau khi ngừng nói khoảng 1,0 đến 1,2 s**, vì còn phải chờ 400 ms im lặng để biết câu đã hết. Con số này gần mốc 1 s và nên được xem là giới hạn thật của thiết kế hiện tại.
- **Tiếng Việt: WER 13,6% trên giọng đọc máy sạch**, 17,8% và 19,5% khi thêm nhiễu (SNR 20 dB và 10 dB). Tiếng Anh: 6,5%. Lỗi tiếng Việt điển hình là sai dấu thanh và ghép nhầm âm tiết, ví dụ "ngắn gọn" thành "nắn gọn". Đây chỉ là mẫu nhỏ, giọng máy, nên chưa đủ để kết luận về giọng người thật (xem Giới hạn).
- **Giữ `small` q5 làm mặc định.** PhoWhisper chưa được thử, vì không có bản ggml đáng tin để tải (xem mục PhoWhisper).

## Môi trường

| Mục | Giá trị |
|---|---|
| Máy | MacBook Pro, Apple M1 Pro (arm64), macOS 26.6.2 |
| Model | `ggml-small-q5_1.bin`, 190.085.487 byte, SHA-256 `ae85e4a9…11bb` (khớp preset trong `crates/models`) |
| Engine | `whisper-rs` 0.16 qua `crates/stt`, Metal cho GPU, `no_context`, không dịch |
| Build | release (`cargo build --release`) |

## Cách đo

- 16 câu đọc bằng giọng macOS: 13 câu tiếng Việt (giọng Linh, 3,6 đến 13,9 s) và 3 câu tiếng Anh (giọng Samantha, 3,1 đến 3,6 s). Văn bản và cách sinh âm thanh nằm trong `spikes/stt-bench/make-audio.sh`. File âm thanh không được commit.
- Mỗi câu chạy một lần khởi động, rồi 3 lần đo (GPU) hoặc 2 lần (CPU). Báo cáo trung vị. Độ trễ là thời gian `transcribe()` trên model đã nạp, chưa tính nạp model.
- Ba biến thể âm thanh: sạch, thêm nhiễu trắng với SNR 20 dB, và 10 dB. Nhiễu sinh bằng bộ số giả ngẫu nhiên cố định nên lần nào cũng như nhau.
- WER tính theo từ (với tiếng Việt, mỗi từ là một âm tiết), CER theo ký tự. Chuẩn hoá: chữ thường, bỏ dấu câu, không đổi số. Vì vậy "mười hai" thành "12" bị tính là lỗi dù đúng nghĩa. Chạy lại bằng `bench <model> <manifest.tsv> <gpu|cpu> <vi|auto|en> <số lần>`.

## Độ trễ nhận dạng theo độ dài câu

Câu sạch, trung vị. Cột "RTF" là thời gian xử lý chia cho độ dài âm thanh, càng nhỏ càng tốt.

| Độ dài câu | Số câu | GPU, ép tiếng Việt | GPU, tự nhận ngôn ngữ | CPU, ép tiếng Việt |
|---|---|---|---|---|
| 0 đến 4 s (TB 3,5 s) | 7 | 380 ms (RTF 0,11) | 570 ms (0,16) | 646 ms (0,18) |
| 4 đến 6 s (TB 4,6 s) | 6 | 378 ms (0,08) | 619 ms (0,14) | 653 ms (0,14) |
| 6 đến 9 s (TB 7,0 s) | 2 | 440 ms (0,06) | 680 ms (0,10) | 723 ms (0,10) |
| 9 đến 20 s (13,9 s) | 1 | 632 ms (0,05) | 876 ms (0,06) | 844 ms (0,06) |

Nhận xét:

- **Độ trễ tăng chậm hơn độ dài câu**: câu 13,9 s chỉ chậm hơn câu 3,5 s khoảng 250 ms. Câu bị cắt cứng ở 15 s nên trần của độ trễ này nằm quanh 0,6 đến 0,9 s.
- **Tự nhận ngôn ngữ tốn thêm khoảng 190 đến 240 ms** so với ép ngôn ngữ, vì model phải chạy thêm một bước đoán.
- **CPU chậm hơn GPU khoảng 1,3 đến 1,7 lần** trên máy này. Máy này có 8 nhân hiệu năng cao, nên số CPU **không đại diện cho laptop yếu hơn**, và GPU của Windows (CUDA, Vulkan) chưa đo.
- Mẫu 9 đến 20 s chỉ có một câu, nên dòng đó chỉ mang tính tham khảo.

### Từ lúc ngừng nói đến lúc có chữ

Phát lại theo thời gian thực qua đúng `Pipeline` (segmenter, hàng đợi, whisper), để tự nhận ngôn ngữ:

| Mẫu | Thời lượng nói | Độ trễ nhận dạng | Chữ xuất hiện sau khi bắt đầu phát |
|---|---|---|---|
| Tiếng Việt | 8,0 s | 788 ms | 9,13 s, tức khoảng 1,1 s sau khi ngừng nói |
| Tiếng Anh | 5,4 s | 627 ms | 6,41 s, tức khoảng 1,0 s sau khi ngừng nói |

Khoảng 1,0 đến 1,1 s gồm 400 ms chờ im lặng cuối câu cộng độ trễ nhận dạng. Ép ngôn ngữ sẽ rút khoảng 0,2 s, nhưng cần một mục cài đặt ngôn ngữ mà app chưa có.

## Độ chính xác

Tiếng Việt, 13 câu, 236 từ. Ép tiếng Việt và tự nhận ngôn ngữ cho **kết quả giống hệt nhau**: model đoán đúng tiếng Việt ở cả 13 câu.

| Âm thanh | WER (tất cả câu) | CER | WER (bỏ 5 câu có số) |
|---|---|---|---|
| Sạch | 13,6% | 7,6% | 10,4% |
| Nhiễu SNR 20 dB | 17,8% | 10,0% | 14,9% |
| Nhiễu SNR 10 dB | 19,5% | 10,6% | 16,9% |

Tiếng Anh, 3 câu, tự nhận ngôn ngữ: WER 6,5%, CER 7,5%, không đổi theo nhiễu.

Khoảng 3 điểm WER của tiếng Việt chỉ do cách viết số ("mười hai" thành "12", "hai triệu năm trăm nghìn" thành "2 triệu 500.000"), không phải nghe sai. Phần còn lại là lỗi nhận dạng thật. Một số ví dụ trên âm thanh sạch:

| Câu gốc | Kết quả |
|---|---|
| ngắn gọn | nắn gọn |
| phần trăm | phần chăm |
| cùng kỳ | cổng kỳ |
| máy tính | máy tiếng |
| ngôn ngữ lập trình Rust | ngôn nữ lập trìm rút |
| độ trễ trung bình | độ trẻ chung bình |
| giọng nói | dọng nói |

## Bộ nhớ và kích thước

| Số đo | Giá trị | Mục tiêu |
|---|---|---|
| RAM đỉnh khi nhận dạng, GPU | 359 MB | dưới 5 GB cả pipeline |
| RAM đỉnh khi nhận dạng, CPU | 498 MB | dưới 5 GB cả pipeline |
| RAM lúc nghỉ, tiến trình chính của app, chưa nạp model | 90 MB | dưới 150 MB |
| Binary release `offstage-desktop` | 15,6 MiB | dưới 20 MB |

Con số RAM lúc nghỉ chỉ tính tiến trình chính. Các tiến trình phụ của WebKit (hiển thị, mạng) nằm ngoài và cộng thêm vào mức dùng thật, nên chưa thể khẳng định đã dưới 150 MB cho cả ứng dụng. Binary tăng từ 9,9 lên 15,6 MiB khi thêm whisper, Metal, `cpal`, `rubato`, `reqwest` và các crate mới, tức đã dùng khoảng 78% ngân sách 20 MB.

Model được nạp khi bắt đầu nghe và giải phóng khi nguồn cuối dừng. Lần nạp đầu tiên mất khoảng 8,2 s (Metal biên dịch shader), các lần sau 0,13 đến 0,17 s.

## PhoWhisper

Chưa thử. Khảo sát trên Hugging Face (chỉ đọc danh sách, không tải gì):

- Bản chính thức `vinai/PhoWhisper-{tiny,base,small,medium,large}` (giấy phép BSD-3-Clause) chỉ có dạng trọng số `transformers`, không phải ggml.
- Các bản ggml chỉ có ở kho cộng đồng (`dongxiat/ggml-PhoWhisper-*`, `Briansss/ggml-PhoWhisper-large`, `hoangecg/phowhisper-large-ggml`), đều 0 lượt tải, không rõ nguồn gốc chuyển đổi. Không nên đưa vào preset mà chưa kiểm chứng.

Hai hướng khả thi, cả hai cần bạn quyết định vì phải tải tệp lớn:

1. **Tự chuyển đổi bản chính thức** `vinai/PhoWhisper-small` sang ggml bằng script của whisper.cpp. Cần tải khoảng 1 GB trọng số chính thức và cài thêm Python với PyTorch, nhưng có nguồn gốc rõ ràng và tự tính được SHA-256 cho preset.
2. **Dùng tạm bản cộng đồng** chỉ để đo thử, không đưa vào app. Rủi ro là tệp không rõ nguồn.

Nhóm tác giả PhoWhisper công bố cải thiện so với Whisper trên các bộ dữ liệu tiếng Việt. Tôi chưa kiểm chứng điều đó ở đây, nên chưa có cơ sở nói PhoWhisper tốt hơn trên mẫu này.

## Giới hạn của phép đo

- **Giọng đọc máy.** Một giọng duy nhất mỗi ngôn ngữ, sạch, tốc độ đều, không vùng miền, không vấp, không ngắt nghỉ tự nhiên. Giọng người thật qua mic hoặc qua cuộc gọi sẽ khác, và số WER thật nhiều khả năng cao hơn.
- **Nhiễu trắng không phải nhiễu cuộc gọi**: thiếu nén tiếng, mất gói, vọng, nhiều người nói chồng lên nhau, nhạc nền.
- **Mẫu nhỏ**: 13 câu và 236 từ tiếng Việt, 3 câu tiếng Anh. Sai số của WER vì thế lớn, và mẫu 9 đến 20 s chỉ có một câu.
- **Một máy**: Apple M1 Pro. Chưa đo Windows, GPU rời, hay laptop CPU yếu.
- **Model đã nạp sẵn**: số độ trễ không gồm 8,2 s của lần nạp đầu.
- **Chưa chỉnh `end_silence_ms`**: 400 ms là giá trị ban đầu. Rút ngắn sẽ giảm độ trễ nhưng dễ cắt đôi câu có nghỉ giữa chừng, và giọng đọc máy không có nghỉ tự nhiên để kiểm tra điều này.
- Chưa thử mic và loopback trên phần cứng thật, nên không có số đo độ trễ của khâu bắt âm thanh.

## Khuyến nghị

1. Giữ `small` q5 làm mặc định.
2. Thêm lựa chọn ngôn ngữ (Tự động, Tiếng Việt, English). Tự động cho kết quả đúng ở mọi mẫu nhưng chậm hơn khoảng 0,2 s. Ép tiếng Việt nhanh hơn nhưng biến tiếng Anh thành rác (WER 90% trở lên), nên không nên là mặc định cho cuộc gọi lẫn lộn.
3. Đo lại bằng giọng người thật, qua mic và qua loopback, trước khi kết luận về chất lượng. Đây là việc quan trọng nhất còn thiếu.
4. Xem xét PhoWhisper nếu số đo giọng thật cho thấy tiếng Việt chưa đủ tốt, bắt đầu bằng cách tự chuyển đổi bản chính thức.
5. Cân nhắc rút `end_silence_ms` hoặc hiển thị chữ tạm thời (partial transcript) nếu cần độ trễ cảm nhận dưới 1 s.
6. Theo dõi kích thước binary: còn khoảng 4,4 MiB ngân sách.
