# SQL: ôn phỏng vấn

## Nền tảng

SQL (Structured Query Language) dùng để truy vấn và thao tác dữ liệu trong cơ sở dữ liệu quan hệ như PostgreSQL, MySQL, SQL Server. Câu lệnh chia thành bốn nhóm:

- DDL: định nghĩa cấu trúc (`CREATE`, `ALTER`, `DROP`).
- DML: thao tác dữ liệu (`INSERT`, `UPDATE`, `DELETE`).
- DQL: truy vấn (`SELECT`).
- DCL/TCL: phân quyền và giao dịch (`GRANT`, `REVOKE`, `COMMIT`, `ROLLBACK`).

## Thứ tự thực thi của một câu SELECT

Thứ tự viết khác thứ tự chạy. Thứ tự chạy logic: `FROM` và `JOIN`, `WHERE`, `GROUP BY`, `HAVING`, `SELECT`, `DISTINCT`, `ORDER BY`, `LIMIT`. Vì vậy không dùng được alias khai báo ở `SELECT` bên trong `WHERE`, nhưng dùng được trong `ORDER BY`.

## WHERE và HAVING khác nhau thế nào

`WHERE` lọc từng dòng trước khi gom nhóm. `HAVING` lọc các nhóm sau khi `GROUP BY` và có thể dùng hàm tổng hợp như `COUNT`, `SUM`.

```sql
SELECT customer_id, COUNT(*) AS orders
FROM orders
WHERE status = 'paid'
GROUP BY customer_id
HAVING COUNT(*) >= 5;
```

## Các loại JOIN

- `INNER JOIN`: chỉ giữ dòng khớp ở cả hai bảng.
- `LEFT JOIN`: giữ mọi dòng bảng trái, cột bảng phải là `NULL` nếu không khớp.
- `RIGHT JOIN`: ngược lại với `LEFT JOIN`.
- `FULL OUTER JOIN`: giữ mọi dòng của cả hai bảng.
- `CROSS JOIN`: tích Descartes, mọi cặp dòng.

Lỗi hay gặp: đặt điều kiện của bảng phải trong `WHERE` khi dùng `LEFT JOIN` làm nó biến thành `INNER JOIN`. Đặt điều kiện đó trong `ON`.

## NULL

`NULL` nghĩa là không có giá trị, không bằng bất cứ thứ gì kể cả chính nó. So sánh bằng `IS NULL` và `IS NOT NULL`, không dùng `= NULL`. Hàm tổng hợp bỏ qua `NULL`, trừ `COUNT(*)`. `COALESCE(a, b)` trả về giá trị khác `NULL` đầu tiên.

## Khóa và ràng buộc

- Primary key: định danh duy nhất, không `NULL`.
- Foreign key: tham chiếu tới khóa của bảng khác, giữ toàn vẹn tham chiếu.
- `UNIQUE`, `NOT NULL`, `CHECK`, `DEFAULT`.

## Chỉ mục (index)

Index là cấu trúc phụ (thường là B-tree) giúp tìm dòng nhanh hơn, đổi lại tốn dung lượng và làm `INSERT`, `UPDATE`, `DELETE` chậm hơn.

- Index nhiều cột tuân theo quy tắc cột trái nhất: index `(a, b)` giúp lọc theo `a` hoặc `a` và `b`, không giúp lọc riêng `b`.
- Bọc cột trong hàm (`WHERE LOWER(email) = ...`) làm index thường không được dùng, trừ khi tạo index theo biểu thức.
- Không nên đánh index cột có rất ít giá trị khác nhau (ví dụ cột giới tính).
- Dùng `EXPLAIN` hoặc `EXPLAIN ANALYZE` để xem kế hoạch thực thi và biết truy vấn có quét toàn bảng (seq scan) hay dùng index.

## Chuẩn hóa

- 1NF: mỗi ô chứa một giá trị nguyên tử.
- 2NF: 1NF và mọi cột không khóa phụ thuộc toàn bộ khóa chính.
- 3NF: 2NF và không có phụ thuộc bắc cầu giữa các cột không khóa.

Phi chuẩn hóa có chủ đích (lưu trùng dữ liệu) đôi khi hợp lý để đọc nhanh hơn, đổi lại phải tự giữ đồng bộ.

## Giao dịch và ACID

Giao dịch là nhóm thao tác chạy trọn vẹn hoặc không chạy gì.

- Atomicity: tất cả hoặc không gì cả.
- Consistency: dữ liệu luôn thỏa ràng buộc.
- Isolation: các giao dịch đồng thời không làm hỏng nhau.
- Durability: đã commit thì không mất.

Mức cô lập từ thấp đến cao: `READ UNCOMMITTED`, `READ COMMITTED`, `REPEATABLE READ`, `SERIALIZABLE`. Mức cao hơn tránh được thêm các hiện tượng dirty read, non-repeatable read, phantom read, đổi lại chậm hơn và dễ xung đột hơn. PostgreSQL mặc định `READ COMMITTED`.

## Window function

Tính toán trên một tập dòng liên quan mà không gộp dòng.

```sql
SELECT name, department, salary,
       RANK() OVER (PARTITION BY department ORDER BY salary DESC) AS rank_in_dept
FROM employees;
```

Hay dùng: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `LAG`, `LEAD`, `SUM() OVER`.

## CTE và subquery

CTE (`WITH`) đặt tên cho một truy vấn con, giúp câu lệnh dễ đọc và cho phép đệ quy (`WITH RECURSIVE`). Subquery tương quan (tham chiếu bảng ngoài) chạy lặp theo từng dòng nên có thể chậm; thường viết lại bằng `JOIN` hoặc window function.

## Một số câu hỏi hay gặp

**Tìm dòng trùng lặp?** Gom nhóm theo các cột cần xét rồi `HAVING COUNT(*) > 1`.

**Lấy lương cao thứ hai?** Dùng `DENSE_RANK() OVER (ORDER BY salary DESC)` rồi lấy hạng 2, hoặc `OFFSET 1 LIMIT 1` trên danh sách giá trị khác nhau.

**DELETE, TRUNCATE, DROP khác nhau?** `DELETE` xóa dòng theo điều kiện và ghi log từng dòng. `TRUNCATE` xóa hết dòng nhanh, giữ cấu trúc bảng. `DROP` xóa cả bảng.

**UNION và UNION ALL?** `UNION` loại dòng trùng nên phải sắp xếp hoặc băm, chậm hơn. `UNION ALL` giữ tất cả.

**Vấn đề N+1 là gì?** Chạy 1 truy vấn lấy danh sách rồi N truy vấn nữa cho từng phần tử. Khắc phục bằng `JOIN`, hoặc lấy gộp theo `WHERE id IN (...)`.

**Phân trang hiệu quả?** `OFFSET` lớn phải bỏ qua nhiều dòng nên chậm dần. Phân trang theo con trỏ (keyset): `WHERE id > :last_id ORDER BY id LIMIT 20`.

**Chống SQL injection?** Luôn dùng câu lệnh tham số hóa (prepared statement), không nối chuỗi từ dữ liệu người dùng.
