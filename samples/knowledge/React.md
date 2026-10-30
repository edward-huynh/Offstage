# React: ôn phỏng vấn

## React là gì

React là thư viện JavaScript để dựng giao diện từ các component. Giao diện là hàm của state: cùng state và props thì cho cùng kết quả hiển thị. React so sánh cây mô tả giao diện mới với cây cũ (reconciliation) rồi chỉ cập nhật phần DOM thật sự thay đổi.

## Component, props, state

- Component là hàm trả về JSX.
- Props là dữ liệu cha truyền xuống, chỉ đọc.
- State là dữ liệu riêng của component, đổi state thì component render lại.
- Không sửa trực tiếp state hay props. Luôn tạo giá trị mới (`setItems([...items, item])`).

## JSX

JSX là cú pháp mô tả giao diện bên trong JavaScript, được biên dịch thành lời gọi hàm. Dùng `className` thay cho `class`, `htmlFor` thay cho `for`. Biểu thức JavaScript đặt trong `{}`.

## Key trong danh sách

`key` giúp React nhận ra phần tử nào đổi, thêm hay xóa. Key phải ổn định và duy nhất giữa các phần tử anh em, thường là id dữ liệu. Dùng chỉ số mảng làm key gây lỗi khi danh sách đổi thứ tự hoặc có phần tử chèn vào, vì state của component gắn nhầm phần tử.

## Hook thường dùng

- `useState`: lưu state. Cập nhật theo giá trị trước đó dùng dạng hàm: `setCount(c => c + 1)`.
- `useEffect`: chạy tác dụng phụ sau khi render (gọi API, đăng ký sự kiện, timer). Mảng phụ thuộc quyết định khi nào chạy lại. Hàm trả về là cleanup, chạy trước lần chạy kế tiếp và khi unmount.
- `useRef`: giữ giá trị đổi được mà không gây render lại, hoặc tham chiếu tới phần tử DOM.
- `useMemo`: ghi nhớ kết quả tính toán nặng.
- `useCallback`: ghi nhớ một hàm để giữ nguyên tham chiếu giữa các lần render.
- `useContext`: đọc giá trị context mà không cần truyền props qua nhiều tầng.
- `useReducer`: quản lý state phức tạp theo kiểu reducer.

Quy tắc hook: chỉ gọi ở cấp cao nhất của component hoặc custom hook, không gọi trong điều kiện, vòng lặp hay hàm lồng.

## useEffect: các lỗi hay gặp

- Thiếu phụ thuộc làm effect dùng giá trị cũ (stale closure).
- Quên cleanup gây rò rỉ listener hoặc timer.
- Dùng effect để tính dữ liệu có thể suy ra từ state hoặc props. Nên tính thẳng trong lúc render, hoặc `useMemo` nếu nặng.
- Gọi API trong effect mà không xử lý race condition: đặt cờ `cancelled` hoặc dùng `AbortController` để bỏ kết quả của lần gọi cũ.
- Trong Strict Mode ở môi trường phát triển, effect chạy hai lần để lộ lỗi cleanup. Đó là chủ ý.

## Khi nào component render lại

Khi state của nó đổi, khi component cha render lại, hoặc khi context mà nó đọc đổi. Cha render lại thì mặc định con cũng render lại dù props không đổi. `React.memo` bỏ qua render của con nếu props giống nhau theo so sánh nông. Đo trước khi tối ưu: dùng React DevTools Profiler.

## Controlled và uncontrolled

Controlled: giá trị input do state điều khiển (`value` và `onChange`). Uncontrolled: DOM giữ giá trị, đọc qua `ref` hoặc `FormData`. Form lớn thường dùng thư viện như React Hook Form để tránh render lại cả form mỗi lần gõ.

## Quản lý state

- State cục bộ: `useState` hoặc `useReducer`.
- Dùng chung giữa vài component gần nhau: nâng state lên component cha chung.
- Dùng chung toàn app: Context cho dữ liệu ít đổi (theme, người dùng), hoặc thư viện như Redux Toolkit, Zustand.
- Dữ liệu từ server: dùng TanStack Query hoặc SWR để có cache, tải lại và trạng thái loading, thay vì tự giữ trong Redux.

Context không phù hợp cho giá trị đổi liên tục, vì mọi component dùng context đều render lại.

## Custom hook

Hàm bắt đầu bằng `use` và dùng các hook khác, để tái sử dụng logic có state (không phải giao diện). Ví dụ `useDebounce`, `useLocalStorage`, `useFetch`.

## Hiệu năng

- Tách component để state nằm sát nơi dùng.
- `React.memo`, `useMemo`, `useCallback` khi đã đo thấy cần.
- Danh sách dài: ảo hóa (react-window, TanStack Virtual).
- Tách mã theo route với `React.lazy` và `Suspense`.
- Tránh tạo object hoặc hàm mới trong props của component đã `memo` nếu không cần.

## Một số khái niệm của React 19

- Actions: hàm bất đồng bộ dùng trong chuyển trạng thái, kèm `useActionState`, `useFormStatus`, `useOptimistic`.
- Hook `use` để đọc Promise hoặc context, kết hợp với `Suspense`.
- `ref` truyền thẳng như prop cho function component, không cần `forwardRef`.
- Server Components: component chạy trên server, không gửi mã của nó xuống trình duyệt. Dùng trong framework hỗ trợ như Next.js.

## Câu hỏi hay gặp

**Virtual DOM là gì?** Bản mô tả giao diện bằng đối tượng JavaScript. React so sánh bản mới và cũ để biết cần đổi gì ở DOM thật.

**State và props khác nhau?** Props truyền từ ngoài vào và chỉ đọc. State do component tự quản và đổi được.

**Vì sao `setState` không đổi giá trị ngay?** Cập nhật state được gom lại và áp dụng ở lần render kế tiếp. Muốn dựa vào giá trị mới nhất thì dùng dạng hàm.

**Lifting state up là gì?** Chuyển state lên component cha chung gần nhất để nhiều con cùng dùng.

**Error Boundary là gì?** Component bắt lỗi xảy ra khi render của cây con và hiện giao diện dự phòng. Hiện chỉ làm được bằng class component hoặc thư viện như react-error-boundary.

**Controlled component có nhược điểm gì?** Mỗi lần gõ đều render lại, với form lớn có thể chậm.

**Khi nào dùng `useMemo`?** Khi tính toán nặng, hoặc cần giữ nguyên tham chiếu của object truyền cho component đã `memo` hay mảng phụ thuộc của hook khác. Không dùng tràn lan vì bản thân nó cũng có chi phí.
