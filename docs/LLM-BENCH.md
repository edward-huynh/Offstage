# Đo model LLM nhỏ cho câu trả lời RAG

Spike `spikes/llm-bench` (tách khỏi workspace, không ảnh hưởng `npm run check`). Đo ngày 2026-10-31.

## Cách đo

- Máy: Apple M1 Pro, 16 GB RAM, Metal. Chưa đo Windows, GPU rời hay laptop chỉ có CPU.
- Engine: `llama-cpp-2` 0.1.160 với Metal, ngữ cảnh 4096, giải mã tham lam (chạy lại cho cùng kết quả).
- Model: **Qwen3-4B-Q4_K_M** (2,50 GB, Apache-2.0), tải từ `Qwen/Qwen3-4B-GGUF`, đã kiểm tra SHA-256. Chế độ suy nghĩ của Qwen3 được đóng sẵn (`<think></think>`), nếu không model sinh một đoạn nháp dài trước câu trả lời.
- Prompt: system theo mẫu RAG trong `CLAUDE.md` (chỉ trả lời từ tài liệu, 3 đến 5 gạch đầu dòng, cùng ngôn ngữ với câu hỏi, thiếu thì nói "Không có trong tài liệu"), cộng 4 đoạn trích của một CV và một JD mẫu.
- 6 câu hỏi: 3 trả lời được từ tài liệu (2 tiếng Việt, 1 tiếng Anh), 1 cần số liệu cụ thể, 2 tài liệu không có (Kubernetes, mức lương).
- Câu "đúng" chỉ được chấm bằng từ khoá: câu có trong tài liệu phải nhắc một ý đúng, câu không có phải trả đúng "Không có trong tài liệu". Đây là kiểm tra thô, không thay cho bộ 50 câu hỏi của Phase 4.

## Kết quả

| Chỉ số | Giá trị |
|---|---|
| Nạp model | khoảng 2,5 GB trọng số, RSS 1,6 đến 2,5 GB sau khi nạp |
| RSS đỉnh khi trả lời | 1,7 GB (prompt 400 token), 2,4 đến 2,5 GB (prompt 1.000 đến 2.400 token) |
| Tốc độ sinh chữ | 36 đến 45 token/s |
| Đọc prompt | khoảng 400 token/s |
| Trả lời đúng theo kiểm tra từ khoá | 6/6 ở mọi độ dài prompt thử |
| Câu không có trong tài liệu | cả 2 lần trả đúng "Không có trong tài liệu." |

Thời gian đến token đầu theo độ dài prompt (không tính nhận dạng giọng nói):

| Prompt | Đến token đầu |
|---|---|
| 395 token | 0,92 s (lần đầu 1,5 s do Metal biên dịch shader) |
| 962 token | 2,2 s |
| 1.522 token | 3,7 đến 4,4 s |
| 2.363 token | 5,7 đến 6,2 s |

## Điều rút ra

- **Chất lượng không phải vấn đề ở cỡ 4B với đoạn trích đúng**: model trả lời ngắn, đúng tiếng của câu hỏi, và từ chối khi tài liệu thiếu.
- **Độ trễ là vấn đề.** Đọc prompt tốn khoảng 2,5 ms mỗi token. Cộng nhận dạng giọng nói 0,4 đến 0,9 s (`docs/STT-EVAL.md`), mục tiêu "dưới 2 s đến token đầu" chỉ giữ được khi phần tài liệu đưa vào **dưới khoảng 400 token**. Mục RAG trong `CLAUDE.md` đang định 4 đến 6 đoạn, tối đa khoảng 2.000 token, cho token đầu ở khoảng 5 s trên máy này.
- Kích thước batch 512, 1024, 2048 không đổi tốc độ đọc prompt.
- RSS và `footprint` đều **không phản ánh hết bộ nhớ GPU** của Metal (footprint chỉ báo 67 đến 110 MB). Mức dùng thật gần trọng số cộng bộ nhớ ngữ cảnh. Qwen3-4B ở 4096 token ước khoảng 0,6 GB (36 lớp, 8 đầu KV, mỗi đầu 128 chiều, đây là ước lượng từ cấu trúc model, chưa đo).

## Hướng giảm độ trễ (chưa thử)

1. Giảm phần tài liệu: 2 đến 3 đoạn nén ngắn hơn, tổng khoảng 500 đến 800 token.
2. Dùng model nhỏ hơn (Qwen3 1.7B đọc prompt nhanh hơn khoảng gấp đôi) và kiểm tra lại độ đúng.
3. Tái dùng bộ nhớ ngữ cảnh cho phần system và phần tài liệu cố định giữa các câu hỏi.
4. Truy xuất sớm: bắt đầu tìm đoạn trích khi câu hỏi còn đang được nhận dạng.
5. Dùng cloud cho độ trễ, local cho riêng tư (Router đã có trong roadmap).

## Chưa đo

- Gemma 3 4B QAT (2,53 GB, điều khoản Gemma) và Gemma 4 E2B (3,35 GB, Apache-2.0): chưa tải, chưa đo, chưa kiểm tra `llama-cpp-2` 0.1.160 có chạy được kiến trúc Gemma 4 hay không.
- Qwen3 1.7B và 8B.
- Tài liệu tiếng Việt thật và câu hỏi thật. Bộ mẫu ở đây nhỏ, tự viết và dễ.
- Windows, CPU-only.
