# Đo độ trễ và chất lượng nhận dạng giọng nói (task 2.8)

Kết quả đo model `whisper small q5_1` chạy trong `crates/stt`. Công cụ đo nằm ở `spikes/stt-bench/` (crate bỏ đi, không thuộc workspace, không ảnh hưởng `npm run check`).

## Kết luận ngắn

- **Độ trễ STT đạt mục tiêu dưới 1 s trên GPU** với mọi mẫu đã đo: 0,38 đến 0,63 s khi ép tiếng Việt, 0,57 đến 0,88 s khi để tự nhận ngôn ngữ.
- **Người dùng thấy chữ sau khi ngừng nói khoảng 1,0 đến 1,2 s**, vì còn phải chờ 400 ms im lặng để biết câu đã hết. Con số này gần mốc 1 s và nên được xem là giới hạn thật của thiết kế hiện tại.
- **Tiếng Việt: WER 13,6% trên giọng đọc máy sạch**, 17,8% và 19,5% khi thêm nhiễu (SNR 20 dB và 10 dB). Tiếng Anh: 6,5%. Lỗi tiếng Việt điển hình là sai dấu thanh và ghép nhầm âm tiết, ví dụ "ngắn gọn" thành "nắn gọn". Đây chỉ là mẫu nhỏ, giọng máy, nên chưa đủ để kết luận về giọng người thật (xem Giới hạn).
- **PhoWhisper-small chính xác hơn hẳn cho tiếng Việt, nhưng chỉ biết tiếng Việt.** Trên cùng mẫu, WER của nó là 1,7% so với 14,0% của `small` (xem mục PhoWhisper), cùng mức độ trễ. Đổi lại nó không nhận dạng được tiếng Anh, và đầu ra không có hoa, dấu câu hay dấu `?`. Vì vậy nó không thay thế được `small`, mà nên là lựa chọn thứ hai cho cuộc gọi toàn tiếng Việt.
- **Giữ `small` q5 làm mặc định** cho đến khi có chỗ lưu trữ cho bản PhoWhisper đã chuyển đổi và có số đo bằng giọng người thật.

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
- Các bảng trước mục PhoWhisper đo khi engine còn bật dấu thời gian. Sau khi tắt (`set_no_timestamps(true)`), `small` cho WER 14,0%, 17,8%, 19,9% (sạch, 20 dB, 10 dB) và độ trễ 367, 375, 441, 630 ms, tức gần như không đổi. Các số trong mục PhoWhisper đo sau khi tắt.
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

Đã thử. `vinai/PhoWhisper-small` (giấy phép BSD-3-Clause) chỉ có dạng trọng số `transformers`, nên tôi tự chuyển sang ggml và lượng tử hóa q5_1 để so công bằng với `small` q5_1. Các bản ggml của cộng đồng (0 lượt tải, không rõ nguồn) không được dùng.

### Nguồn gốc và cách tái hiện

`spikes/stt-bench/convert-phowhisper.sh <thư-mục>` làm toàn bộ việc và ghim mọi phiên bản:

| Thành phần | Phiên bản ghim |
|---|---|
| Trọng số | `vinai/PhoWhisper-small` commit `a86b604c…`, `pytorch_model.bin` 967.102.729 byte, SHA-256 `9ec6f07c…cbfded` (khớp API Hugging Face) |
| Công cụ chuyển đổi | whisper.cpp `v1.8.3` (commit `2eeeba56…`, đúng bản `whisper-rs` đang dùng) |
| Bộ lọc mel | OpenAI Whisper commit `86098128…` |
| Python | 3.9.6, `torch` 2.8.0, `transformers` 4.57.6, `numpy` 2.0.2 |
| Kết quả | `ggml-phowhisper-small-q5_1.bin`, 190.085.504 byte, SHA-256 `c93fb04f6f586de4eb9b82501f371d4b0bdd033e266264a5ab4f787dea25343e` |

Hai lần chạy cho tệp giống hệt từng byte. Vì `.bin` của PyTorch là định dạng pickle (có thể chạy mã khi nạp), script chỉ mở nó bằng `torch.load(weights_only=True)`. Tệp kết quả **không nằm trong repo** và chưa được đặt ở đâu để tải.

### Kết quả

GPU (Metal), ép tiếng Việt, cùng 13 câu tiếng Việt và cách chuẩn hoá như trên. Cột `small` đo lại bằng đúng engine hiện tại.

| Âm thanh | PhoWhisper WER | PhoWhisper CER | `small` WER | `small` CER |
|---|---|---|---|---|
| Sạch | **1,7%** | 1,0% | 14,0% | 7,8% |
| Nhiễu SNR 20 dB | **2,1%** | 1,4% | 17,8% | 9,9% |
| Nhiễu SNR 10 dB | **1,3%** | 0,8% | 19,9% | 10,5% |

Độ trễ nhận dạng (câu sạch, trung vị của 3 lần), gần như bằng nhau:

| Độ dài câu | PhoWhisper | `small` |
|---|---|---|
| 0 đến 4 s | 380 ms | 367 ms |
| 4 đến 6 s | 398 ms | 375 ms |
| 6 đến 9 s | 482 ms | 441 ms |
| 9 đến 20 s | 675 ms | 630 ms |

Chế độ tự nhận ngôn ngữ cho PhoWhisper cũng đoán đúng tiếng Việt ở cả 13 câu (WER như trên), chậm hơn khoảng 0,24 s (621 ms cho câu ngắn).

Một số khác biệt cụ thể trên âm thanh sạch:

| Câu gốc | PhoWhisper | `small` |
|---|---|---|
| ngắn gọn | ngắn gọn | nắn gọn |
| phần trăm, cùng kỳ | đúng | phần chăm, cổng kỳ |
| mười hai phần trăm | mười hai phần trăm | 12 phần trăm |
| hai triệu năm trăm nghìn đồng một tháng | đúng nguyên văn | 2 triệu 500.000 đồng 1 tháng |
| máy tính | mái tính (sai) | máy tiếng (sai) |
| độ trễ | độ trẻ (sai) | độ trẻ (sai) |
| Rust | giúp (sai) | rút (sai) |
| sáu tháng tới | tháng tới (bỏ mất "sáu") | 6 tháng tới |

Một phần khoảng cách là do cách viết số: PhoWhisper viết số thành chữ như câu gốc, còn `small` viết chữ số. Nhưng khoảng cách vẫn lớn khi bỏ các câu có số (`small` vẫn khoảng 10%), nên phần lớn là chênh lệch nhận dạng thật.

### Những đánh đổi

- **Chỉ tiếng Việt.** Với 3 câu tiếng Anh, PhoWhisper trả về chữ Việt vô nghĩa ở chế độ tự nhận (WER 152%), và ép tiếng Anh cũng ra rác. Cuộc gọi lẫn tiếng Anh, đặc biệt thuật ngữ kỹ thuật như "Rust", sẽ sai.
- **Định dạng đầu ra khác.** Chữ thường, gần như không có dấu câu, kể cả dấu `?`, số viết thành chữ. Phần phát hiện câu hỏi ở Phase 3 phải dựa vào từ hỏi tiếng Việt, không dựa vào dấu `?`.
- **Cần cấu hình giải mã đúng.** Với dấu thời gian bật (cấu hình ban đầu của engine), PhoWhisper chen token rác ở đầu câu ("1", "and", "andre") và đôi khi hỏng cả câu: WER 19,1% sạch, 25,4% ở 20 dB, thất thường theo nhiễu. Tắt dấu thời gian (`set_no_timestamps(true)`, đã đưa vào `crates/stt`) sửa hoàn toàn, và `small` gần như không đổi (13,6% thành 14,0%). App không dùng dấu thời gian nên thay đổi này an toàn cho cả hai.
- **Số đo này dựa trên giọng đọc máy**, sạch và dễ hơn giọng người. Phần chênh giữa hai model nhiều khả năng nhỏ hơn với giọng thật, nhưng PhoWhisper được huấn luyện cho tiếng Việt nên khó có chuyện kém hơn.

## Giới hạn của phép đo

- **Giọng đọc máy.** Một giọng duy nhất mỗi ngôn ngữ, sạch, tốc độ đều, không vùng miền, không vấp, không ngắt nghỉ tự nhiên. Giọng người thật qua mic hoặc qua cuộc gọi sẽ khác, và số WER thật nhiều khả năng cao hơn.
- **Nhiễu trắng không phải nhiễu cuộc gọi**: thiếu nén tiếng, mất gói, vọng, nhiều người nói chồng lên nhau, nhạc nền.
- **Mẫu nhỏ**: 13 câu và 236 từ tiếng Việt, 3 câu tiếng Anh. Sai số của WER vì thế lớn, và mẫu 9 đến 20 s chỉ có một câu.
- **Một máy**: Apple M1 Pro. Chưa đo Windows, GPU rời, hay laptop CPU yếu.
- **Model đã nạp sẵn**: số độ trễ không gồm 8,2 s của lần nạp đầu.
- **Chưa chỉnh `end_silence_ms`**: 400 ms là giá trị ban đầu. Rút ngắn sẽ giảm độ trễ nhưng dễ cắt đôi câu có nghỉ giữa chừng, và giọng đọc máy không có nghỉ tự nhiên để kiểm tra điều này.
- Chưa thử mic và loopback trên phần cứng thật, nên không có số đo độ trễ của khâu bắt âm thanh.

## Khuyến nghị

1. Giữ `small` q5 làm mặc định, vì nó là model duy nhất xử lý cả tiếng Anh.
2. **Thêm PhoWhisper-small q5_1 làm lựa chọn cho tiếng Việt**, theo cách dự án đã định: là một `ModelSpec` trong `crates/models`, không phải code riêng. Việc còn thiếu là chỗ lưu trữ: cần đặt tệp đã chuyển đổi ở một địa chỉ ổn định (ví dụ một kho Hugging Face của dự án) để có URL cho preset, kèm SHA-256 ở trên, giấy phép BSD-3-Clause và ghi công VinAI. Việc đăng tệp là quyết định của bạn, tôi chưa làm.
3. Thêm mục chọn ngôn ngữ hoặc model trong cài đặt (ví dụ "Tiếng Việt (PhoWhisper)" và "Đa ngôn ngữ (Whisper small)"). Ép ngôn ngữ nhanh hơn khoảng 0,2 s so với tự nhận.
4. Đo lại bằng giọng người thật, qua mic và qua loopback, trước khi chọn mặc định. Đây vẫn là việc quan trọng nhất còn thiếu.
5. Phát hiện câu hỏi (Phase 3) dùng từ hỏi tiếng Việt, không dựa vào dấu `?`.
6. Cân nhắc rút `end_silence_ms` hoặc hiển thị chữ tạm thời (partial transcript) nếu cần độ trễ cảm nhận dưới 1 s.
7. Theo dõi kích thước binary: còn khoảng 4,4 MiB ngân sách. Model không đóng gói trong app nên PhoWhisper không ảnh hưởng mục này.
