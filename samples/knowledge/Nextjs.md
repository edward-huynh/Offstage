# Next.js: ôn phỏng vấn

## Next.js là gì

Next.js là framework full-stack xây trên React, cung cấp routing theo thư mục, render phía server, tối ưu ảnh và phông chữ, và API route. Dùng để làm website cần SEO tốt, tải nhanh, hoặc ứng dụng có cả giao diện lẫn backend nhẹ. Hiện nay cách dùng chính là App Router (thư mục `app/`). Pages Router (thư mục `pages/`) cũ hơn nhưng vẫn được hỗ trợ.

## Các kiểu render

- **SSG** (static): trang dựng sẵn lúc build, phục vụ nhanh từ CDN. Hợp với nội dung ít đổi.
- **SSR**: dựng lại ở server cho mỗi request, hợp với dữ liệu phụ thuộc người dùng hay luôn mới.
- **ISR**: trang tĩnh nhưng được dựng lại ở nền theo chu kỳ (`revalidate`) hoặc theo yêu cầu (`revalidatePath`, `revalidateTag`).
- **CSR**: dựng ở trình duyệt, dùng cho phần tương tác nhiều và không cần SEO.

Trong App Router, trang là tĩnh nếu không dùng thứ gì phụ thuộc request (`cookies()`, `headers()`, tham số tìm kiếm, `fetch` không cache). Dùng thứ đó thì trang chuyển sang dựng động.

## Server Component và Client Component

Trong App Router, component mặc định là **Server Component**: chạy ở server, có thể `async` và đọc dữ liệu trực tiếp (gọi cơ sở dữ liệu, `fetch`), mã của nó không gửi xuống trình duyệt nên bundle nhẹ hơn. Không dùng được `useState`, `useEffect`, sự kiện trình duyệt.

Thêm `"use client"` ở đầu tệp để thành **Client Component**: dùng được hook và sự kiện. Chỉ nên đặt ranh giới client ở lá của cây, phần cần tương tác, để phần còn lại vẫn ở server.

Quy tắc: Client Component không import được Server Component, nhưng nhận Server Component qua `children` hoặc props được. Dữ liệu truyền từ server xuống client phải serialize được.

## Routing theo thư mục

- `app/page.tsx`: trang của route.
- `app/layout.tsx`: bố cục bọc các trang con, giữ nguyên khi chuyển giữa các trang con.
- `app/loading.tsx`: giao diện chờ, dựa trên Suspense.
- `app/error.tsx`: ranh giới lỗi cho segment (phải là Client Component).
- `app/not-found.tsx`: trang không tìm thấy.
- `app/blog/[slug]/page.tsx`: route động.
- `app/(marketing)/...`: nhóm route, không ảnh hưởng URL.
- `app/api/.../route.ts`: route handler, xuất các hàm `GET`, `POST`...

## Lấy dữ liệu và cache

Ở Server Component, gọi dữ liệu thẳng bằng `async/await`. Hành vi cache của `fetch` và các hàm liên quan đã thay đổi giữa các phiên bản Next.js, nên cần đọc tài liệu đúng phiên bản đang dùng. Hai công cụ ổn định để làm mới dữ liệu: `revalidatePath` và `revalidateTag`. Tránh chuỗi `await` nối tiếp cho các việc độc lập, hãy chạy song song bằng `Promise.all` để không tạo waterfall.

## Server Actions

Hàm `async` đánh dấu `"use server"`, chạy ở server và gọi được từ form hoặc từ Client Component. Dùng cho thao tác ghi dữ liệu (tạo, sửa, xóa) mà không cần tự viết API. Vẫn phải kiểm tra đầu vào và quyền truy cập bên trong, vì thực chất đó là điểm vào công khai.

## Middleware

Chạy trước khi request tới route, dùng cho chuyển hướng, kiểm tra đăng nhập sơ bộ, đa ngôn ngữ, đặt header. Cần chạy nhanh, không nên làm việc nặng. Kiểm tra quyền thật sự vẫn nên đặt gần nơi truy cập dữ liệu, không chỉ dựa vào middleware.

## Tối ưu

- `next/image`: tự đổi kích thước, định dạng hiện đại, lazy loading, giữ chỗ để tránh nhảy bố cục (CLS).
- `next/font`: tải phông chữ tự lưu trữ, không chặn hiển thị.
- `next/link`: điều hướng phía client và tải trước (prefetch) các route trong vùng nhìn.
- Streaming với `Suspense`: gửi phần có sẵn trước, phần chậm gửi sau.
- Tách mã tự động theo route, `next/dynamic` để nạp muộn component nặng.
- Metadata API (`generateMetadata`) cho SEO: tiêu đề, mô tả, Open Graph.

## Hydration

Sau khi server gửi HTML, trình duyệt "hydrate": gắn sự kiện và state của React vào HTML đó. Lỗi hydration mismatch xảy ra khi HTML ở server khác với những gì client render lần đầu, thường do dùng `Date.now()`, `Math.random()`, `window`, `localStorage` trong lúc render. Khắc phục bằng cách đưa phần đó vào `useEffect` hoặc đánh dấu cho phép lệch có chủ ý.

## Biến môi trường

Biến thường chỉ có ở server. Muốn lộ ra trình duyệt phải đặt tên bắt đầu bằng `NEXT_PUBLIC_`, và khi đó giá trị được nhúng vào mã gửi cho người dùng nên không bao giờ đặt bí mật ở đây.

## Câu hỏi hay gặp

**SSR và SSG khác nhau thế nào?** SSG dựng một lần lúc build, SSR dựng mỗi request. SSG nhanh và rẻ hơn, SSR cho dữ liệu luôn mới.

**Khi nào dùng Client Component?** Khi cần state, hook, sự kiện người dùng hoặc API của trình duyệt. Còn lại giữ ở server.

**Next.js khác React thuần ở đâu?** React chỉ là thư viện giao diện. Next.js thêm routing, render ở server, tối ưu tài nguyên, API route và công cụ build.

**ISR dùng khi nào?** Khi muốn tốc độ của trang tĩnh nhưng nội dung vẫn cập nhật, ví dụ trang bài viết hay sản phẩm.

**Làm sao SEO tốt trong Next.js?** Render HTML có nội dung ở server, dùng Metadata API, đặt `sitemap` và `robots`, tối ưu ảnh và tốc độ tải.

**Route handler và Server Action khác gì?** Route handler là endpoint HTTP, hợp để cho bên ngoài gọi (ứng dụng khác, webhook). Server Action hợp cho thao tác ghi từ chính giao diện của app.

**Làm sao bảo vệ trang cần đăng nhập?** Kiểm tra phiên ở middleware để chuyển hướng sớm, và kiểm tra lại ở nơi đọc dữ liệu (Server Component, Server Action, route handler).
