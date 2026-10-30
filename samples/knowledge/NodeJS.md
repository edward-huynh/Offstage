# Node.js: ôn phỏng vấn

## Node.js là gì

Node.js là môi trường chạy JavaScript ngoài trình duyệt, xây trên engine V8, dùng cho dịch vụ backend, công cụ dòng lệnh và script. Điểm mạnh là mô hình I/O không chặn, hợp với tác vụ nhiều kết nối đồng thời nhưng ít tính toán nặng (API, chat, streaming).

## Event loop

Node chạy mã JavaScript của bạn trên một luồng chính. Các tác vụ I/O (mạng, đĩa, timer) được giao cho hệ điều hành hoặc thread pool của libuv, xong thì callback được đưa vào hàng đợi để event loop chạy lại.

Các pha chính của một vòng lặp: timers (`setTimeout`, `setInterval`), pending callbacks, poll (nhận I/O), check (`setImmediate`), close callbacks.

Thứ tự ưu tiên giữa các lần chạy:

1. Mã đồng bộ đang chạy xong trước.
2. Microtask: `process.nextTick` trước, rồi callback của Promise (`.then`, `await`).
3. Sau đó mới tới callback của các pha như timers và `setImmediate`.

Vì chỉ có một luồng chạy JavaScript, một đoạn mã tính toán nặng sẽ chặn mọi request khác.

## Bất đồng bộ: callback, Promise, async/await

- Callback kiểu `(err, data)` là cách cũ, dễ lồng sâu.
- Promise biểu diễn kết quả sẽ có sau này, nối bằng `.then`, `.catch`.
- `async/await` là cú pháp viết Promise như mã tuần tự. Dùng `try/catch` để bắt lỗi.
- Chạy song song dùng `Promise.all` (lỗi một cái là lỗi cả nhóm) hoặc `Promise.allSettled` (chờ tất cả, báo từng kết quả).
- `await` trong vòng lặp `for` chạy tuần tự. Nếu các việc độc lập thì gom lại bằng `Promise.all`.

## Xử lý tác vụ nặng

- `worker_threads`: chạy JavaScript tính toán nặng ở luồng riêng.
- `child_process`: chạy tiến trình khác.
- `cluster` hoặc chạy nhiều tiến trình sau load balancer để dùng nhiều lõi CPU.
- Đẩy việc nặng sang hàng đợi (BullMQ, RabbitMQ) và xử lý ở worker riêng.

## Module

- CommonJS: `require` và `module.exports`, nạp đồng bộ.
- ES Modules: `import` và `export`, bật bằng `"type": "module"` trong `package.json` hoặc đuôi `.mjs`.
- Dùng top-level `await` được trong ES Modules.

## Stream và Buffer

Stream xử lý dữ liệu theo từng mảnh thay vì nạp cả vào bộ nhớ, hợp với tệp lớn và dữ liệu mạng. Bốn loại: Readable, Writable, Duplex, Transform. Dùng `pipeline` từ `stream/promises` để nối stream và bắt lỗi đúng cách. Backpressure là cơ chế để bên đọc nhanh không làm tràn bên ghi chậm. `Buffer` là vùng nhớ chứa dữ liệu nhị phân.

## Xử lý lỗi

- Bắt lỗi của `async` bằng `try/catch`, và luôn xử lý Promise bị reject.
- `uncaughtException` và `unhandledRejection` là lưới an toàn cuối cùng: nên ghi log rồi thoát tiến trình để trình giám sát (PM2, Docker, Kubernetes) khởi động lại, không nên tiếp tục chạy ở trạng thái không rõ.
- Phân biệt lỗi vận hành (mạng, đầu vào sai) và lỗi lập trình (bug).

## Express: khái niệm cốt lõi

- Middleware là hàm `(req, res, next)` chạy theo thứ tự đăng ký. Gọi `next()` để đi tiếp, hoặc kết thúc bằng `res.send`.
- Middleware xử lý lỗi có bốn tham số `(err, req, res, next)` và đăng ký sau cùng.
- Thứ tự middleware quan trọng: parse body, xác thực, route, xử lý lỗi.
- Express 5 tự chuyển lỗi của handler `async` vào middleware lỗi. Ở bản 4 phải tự bọc hoặc dùng thư viện.

## Bảo mật

- Kiểm tra và làm sạch đầu vào, dùng thư viện schema như Zod hoặc Joi.
- Truy vấn tham số hóa để chống SQL injection.
- `helmet` để đặt header bảo mật, cấu hình CORS đúng nguồn, giới hạn tần suất request (rate limit).
- Băm mật khẩu bằng bcrypt hoặc argon2, không lưu mật khẩu thô.
- Không đưa bí mật vào mã nguồn, dùng biến môi trường.
- Cập nhật phụ thuộc và chạy `npm audit`.

## Hiệu năng và vận hành

- Cache (Redis, bộ nhớ) cho dữ liệu đọc nhiều.
- Nén phản hồi, phân trang, chỉ chọn cột cần.
- Dùng connection pool cho cơ sở dữ liệu.
- Ghi log có cấu trúc (pino, winston), có mã theo dõi request.
- Tắt êm (graceful shutdown): khi nhận `SIGTERM`, ngừng nhận kết nối mới, chờ request đang chạy xong rồi đóng kết nối.
- Rò rỉ bộ nhớ thường do biến toàn cục phình ra, listener không gỡ, cache không giới hạn. Dùng `--inspect` và heap snapshot để tìm.

## Câu hỏi hay gặp

**Node.js đơn luồng thì sao xử lý nhiều request?** Luồng JavaScript chỉ một, nhưng I/O không chặn nên khi chờ I/O nó phục vụ request khác. Tính toán nặng thì phải đưa ra worker.

**`process.nextTick` và `setImmediate` khác gì?** `nextTick` chạy ngay sau thao tác hiện tại, trước cả microtask của Promise. `setImmediate` chạy ở pha check của vòng lặp, sau pha poll.

**`require` và `import` khác gì?** `require` nạp đồng bộ và là của CommonJS. `import` là chuẩn ES Modules, phân tích tĩnh được và hỗ trợ tree shaking.

**`package.json` và `package-lock.json`?** Cái đầu mô tả phụ thuộc theo khoảng phiên bản. Cái sau khóa phiên bản chính xác để cài đặt lặp lại được, nên được commit.

**`dependencies` và `devDependencies`?** Cái đầu cần khi chạy thật, cái sau chỉ cần lúc phát triển và build (test, linter, TypeScript).

**Làm sao tránh callback hell?** Dùng Promise và `async/await`, tách hàm nhỏ.

**Biến môi trường dùng thế nào?** Đọc qua `process.env`. Giá trị bí mật không commit, nạp từ tệp `.env` khi phát triển và từ hệ thống triển khai khi chạy thật.
