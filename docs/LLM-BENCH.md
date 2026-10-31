# Đo model LLM nhỏ cho câu trả lời RAG

Spike `spikes/llm-bench` (tách khỏi workspace, không ảnh hưởng `npm run check`). Đo ngày 2026-10-31.

## Cách đo

- Máy: Apple M1 Pro, 16 GB RAM, Metal. Chưa đo Windows, GPU rời hay laptop chỉ có CPU.
- Engine: `llama-cpp-2` 0.1.160 với Metal, ngữ cảnh 4096, giải mã tham lam (chạy lại cho cùng kết quả).
- Bốn model, tất cả bản 4 bit, đã kiểm tra SHA-256 với giá trị Hugging Face báo:

| Model | File | Dung lượng | Giấy phép |
|---|---|---|---|
| Qwen3 4B | `Qwen3-4B-Q4_K_M.gguf` (`Qwen/Qwen3-4B-GGUF`) | 2,50 GB | Apache-2.0 |
| Qwen3 1.7B | `Qwen3-1.7B-Q4_K_M.gguf` (`unsloth/Qwen3-1.7B-GGUF`) | 1,11 GB | Apache-2.0 |
| Gemma 3 4B QAT | `gemma-3-4b-it-qat-Q4_0.gguf` (`ggml-org/gemma-3-4b-it-qat-GGUF`) | 2,53 GB | điều khoản Gemma |
| Gemma 4 E2B QAT | `gemma-4-E2B_q4_0-it.gguf` (`google/gemma-4-E2B-it-qat-q4_0-gguf`) | 3,35 GB | Apache-2.0 |

- Chế độ suy nghĩ của Qwen3 được đóng sẵn (`<think></think>`), nếu không model sinh một đoạn nháp dài trước câu trả lời. Gemma 3 dùng mẫu chat của llama.cpp. Gemma 4 chưa có trong các mẫu có sẵn của llama.cpp (hàm áp dụng mẫu báo lỗi), nên spike dựng prompt bằng tay theo mẫu nằm trong file model (`<|turn>system … <turn|>`, `<|turn>user … <turn|>`, `<|turn>model`), tắt chế độ suy nghĩ.
- Prompt: system theo mẫu RAG trong `CLAUDE.md` (chỉ trả lời từ tài liệu, 3 đến 5 gạch đầu dòng, cùng ngôn ngữ với câu hỏi, thiếu thì nói "Không có trong tài liệu"), cộng 4 đoạn trích của một CV và một JD mẫu. `--repeat 5` lặp các đoạn trích để có prompt khoảng 1.500 token.
- 6 câu hỏi: 3 trả lời được từ tài liệu (2 tiếng Việt, 1 tiếng Anh), 1 cần số liệu cụ thể, 2 tài liệu không có (Kubernetes, mức lương).
- Phép kiểm tra từ khoá chỉ xem câu trả lời có nhắc ý đúng hoặc từ chối đúng không. Nó **không bắt được** bịa chi tiết, sai ngôi, sai ngôn ngữ hay lặp ý, nên các câu trả lời đã được đọc lại bằng tay (xem bên dưới). Mỗi cấu hình chỉ chạy một lần, 6 câu, nên đây là dấu hiệu chứ chưa phải kết luận thống kê.

## Số đo

| | Qwen3 4B | Qwen3 1.7B | Gemma 3 4B | Gemma 4 E2B |
|---|---|---|---|---|
| Đến token đầu, prompt khoảng 400 token | 0,92 s | 0,40 đến 0,43 s | 0,78 s | 0,53 đến 0,66 s |
| Đến token đầu, prompt khoảng 1.520 token | 3,7 đến 4,4 s | 1,54 đến 1,61 s | 2,9 s | 2,2 s |
| Đọc prompt (token/s) | khoảng 400 | khoảng 950 | khoảng 520 | khoảng 690 |
| Sinh chữ (token/s) | 43 | 71 đến 85 | 43 đến 47 | 51 đến 61 |
| RSS đỉnh, prompt 400 / 1.520 token | 1,7 / 2,5 GB | 1,2 / 1,2 GB | 2,5 / 2,5 GB | 2,65 / 3,3 GB |
| Keyword check (400 / 1.520 token) | 6/6 / 6/6 | 6/6 / 6/6 | 6/6 / 6/6 | 5/6 / 6/6 |

Thời gian đến token đầu chưa tính nhận dạng giọng nói (0,4 đến 0,9 s, `docs/STT-EVAL.md`) và lần chạy đầu tiên luôn chậm hơn khoảng 0,5 s vì Metal biên dịch shader. Kích thước batch 512, 1024, 2048 không đổi tốc độ đọc prompt (đo trên Qwen3 4B).

RSS và `footprint` không phản ánh hết bộ nhớ GPU của Metal (footprint chỉ báo 67 đến 160 MB). Mức dùng thật gần dung lượng trọng số cộng bộ nhớ ngữ cảnh.

## Chất lượng, đọc từng câu trả lời

- **Qwen3 4B**: ngắn (35 đến 60 token), đúng ý, dùng ngôi "tôi", **trả lời câu tiếng Anh bằng tiếng Anh**, không thấy bịa. Từ chối đúng khi tài liệu thiếu.
- **Qwen3 1.7B**: dài dòng (90 đến 180 token), **lẫn yêu cầu của JD vào kinh nghiệm của ứng viên** ("tôi đã triển khai mô hình nhận dạng giọng nói bằng Rust"), bịa chi tiết không có trong tài liệu ("tối ưu hóa thuật toán"), trả lời câu tiếng Anh bằng tiếng Việt, đổi ngôi lung tung ("Bạn đã…"). Ở prompt dài còn **lặp một ý ba lần**. Từ chối đúng khi tài liệu thiếu.
- **Gemma 3 4B**: nói sai ngôi ("Bạn đã…", "ứng viên có…") thay vì giọng của người dùng, thêm ý thừa cho đủ 4 gạch ("Bạn có kỹ năng Rust trong hồ sơ của mình"), trả lời câu tiếng Anh bằng tiếng Việt, cũng lẫn yêu cầu JD vào kinh nghiệm. Từ chối đúng.
- **Gemma 4 E2B**: ngôi không ổn định, thêm ý lạc đề ("kinh nghiệm với whisper.cpp và llama.cpp" cho câu hỏi về Rust), trả lời câu tiếng Anh bằng tiếng Việt. **Ở prompt ngắn, câu "mô tả dự án gần đây nhất" bị trả lời bằng công việc ở Công ty Âm Sắc, không phải dự án Offstage** (chọn nhầm đoạn trích). Từ chối đúng.

## Điều rút ra

- **Chỉ Qwen3 4B làm tốt cả ngôi, ngôn ngữ và bám tài liệu.** Ba model còn lại đều trả lời câu tiếng Anh bằng tiếng Việt, và 1.7B với Gemma lẫn yêu cầu JD vào kinh nghiệm của ứng viên, đây là kiểu bịa nguy hiểm nhất trong buổi phỏng vấn.
- **Nhưng Qwen3 4B là model chậm nhất đến token đầu.** Cộng nhận dạng giọng nói 0,4 đến 0,9 s, mục tiêu "dưới 2 giây" chỉ giữ được khi phần tài liệu đưa vào khoảng **400 đến 600 token**. Với 2.000 token như `CLAUDE.md` định, token đầu ở khoảng 5 s trên máy này.
- Ngân sách tài liệu theo mục tiêu 2 s (còn khoảng 1,1 đến 1,6 s cho việc đọc prompt): Qwen3 1.7B khoảng 1.000 đến 1.500 token, Gemma 4 E2B khoảng 700 đến 1.000, Gemma 3 4B khoảng 500 đến 800, Qwen3 4B khoảng 400 đến 600.
- Gemma 4 chạy được trong `llama-cpp-2` 0.1.160 nhưng cần tự viết mẫu chat. Gemma 3 vướng điều khoản giấy phép Gemma. Hai điểm này cộng với chất lượng kém hơn làm Gemma không có lý do để thay Qwen3 4B.
- Không có model nào trong bốn model cho cả chất lượng lẫn độ trễ với 2.000 token tài liệu. Đây là quyết định thiết kế, không phải việc chọn thêm model.

## Hướng xử lý (chưa thử)

1. Giảm phần tài liệu: 2 đến 3 đoạn nén ngắn hơn (khoảng 500 token) với Qwen3 4B.
2. Hiện nguồn tìm được ngay khi có câu hỏi, trong lúc model còn đọc prompt (người dùng thấy gợi ý có ích trước khi chữ đầu tiên xuất hiện).
3. Tái dùng bộ nhớ ngữ cảnh cho phần system và phần tài liệu cố định giữa các câu hỏi.
4. Truy xuất sớm: bắt đầu tìm đoạn trích khi câu hỏi còn đang được nhận dạng.
5. Dùng cloud khi cần nhanh, local khi cần riêng tư (Router đã có trong roadmap).
6. Sửa prompt để ép ngôi "tôi" và ngôn ngữ của câu hỏi, rồi đo lại: lỗi ngôn ngữ của ba model còn lại có thể do prompt tiếng Việt.

## Chưa đo

- Qwen3 8B (cỡ 5,5 GB), chỉ hợp máy từ 16 GB.
- Tài liệu tiếng Việt thật và câu hỏi thật. Bộ mẫu ở đây nhỏ, tự viết và dễ.
- Windows, máy chỉ có CPU.
- Prompt tiếng Anh cho cả system và tài liệu, để xem lỗi ngôn ngữ có biến mất không.
