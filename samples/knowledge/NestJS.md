# NestJS: ôn phỏng vấn

## NestJS là gì

NestJS là framework Node.js viết bằng TypeScript để xây dịch vụ backend có cấu trúc rõ ràng. Mặc định chạy trên Express, có thể đổi sang Fastify. Kiến trúc lấy cảm hứng từ Angular: module, dependency injection, decorator. Phù hợp dự án lớn, nhiều người cùng làm vì ép theo một khuôn tổ chức mã.

## Ba khối chính

- **Module** (`@Module`): đóng gói một nhóm chức năng, khai báo `controllers`, `providers`, `imports`, `exports`. Mọi app có ít nhất một module gốc.
- **Controller** (`@Controller`): nhận request, gọi service, trả phản hồi. Dùng `@Get`, `@Post`, `@Put`, `@Patch`, `@Delete` cho route.
- **Provider** (thường là service, `@Injectable`): chứa logic nghiệp vụ, được tiêm vào nơi cần.

```ts
@Controller('users')
export class UsersController {
  constructor(private readonly users: UsersService) {}

  @Get(':id')
  findOne(@Param('id', ParseIntPipe) id: number) {
    return this.users.findOne(id);
  }
}
```

## Dependency injection

Nest tạo và quản lý đối tượng, rồi tiêm vào constructor theo kiểu dữ liệu. Lợi ích: ít phụ thuộc cứng nên dễ thay thế khi kiểm thử (tiêm bản giả). Muốn module khác dùng được provider của mình thì phải `exports` nó, và module kia phải `imports` module của mình.

Phạm vi sống của provider:

- `DEFAULT`: một thể hiện dùng chung cho cả app (singleton), phổ biến nhất.
- `REQUEST`: tạo mới cho mỗi request.
- `TRANSIENT`: tạo mới cho mỗi nơi tiêm vào.

Phạm vi `REQUEST` làm chậm vì kéo theo cả chuỗi phụ thuộc, chỉ dùng khi thật sự cần.

## Vòng đời của một request

Thứ tự: Middleware, Guard, Interceptor (trước), Pipe, Controller và handler, Interceptor (sau), Exception filter nếu có lỗi.

- **Middleware**: giống Express, chạy sớm nhất, truy cập `req` và `res`.
- **Guard** (`CanActivate`): quyết định cho request đi tiếp hay không, dùng cho xác thực và phân quyền. Trả `false` hoặc ném lỗi thì request bị chặn.
- **Interceptor**: bao quanh handler, dùng để ghi log, đổi dạng phản hồi, cache, đo thời gian, nhờ RxJS.
- **Pipe**: biến đổi và kiểm tra đầu vào (`ParseIntPipe`, `ValidationPipe`).
- **Exception filter**: bắt lỗi và tạo phản hồi lỗi thống nhất.

## Kiểm tra dữ liệu

Dùng DTO (lớp mô tả dữ liệu vào) cùng `class-validator` và `class-transformer`, bật `ValidationPipe` toàn cục.

```ts
export class CreateUserDto {
  @IsEmail() email: string;
  @IsString() @MinLength(8) password: string;
}
```

`whitelist: true` bỏ các trường không khai báo, `forbidNonWhitelisted: true` báo lỗi nếu có, `transform: true` tự đổi kiểu theo DTO.

## Cơ sở dữ liệu

Nest không bó buộc ORM. Hay dùng: TypeORM, Prisma, MikroORM, Mongoose (cho MongoDB). Với TypeORM, `TypeOrmModule.forRoot` cấu hình kết nối và `TypeOrmModule.forFeature([User])` đăng ký entity cho module, sau đó tiêm repository bằng `@InjectRepository(User)`. Giao dịch dùng `DataSource.transaction` hoặc `QueryRunner`.

## Xác thực và phân quyền

- Xác thực: `@nestjs/passport` với chiến lược JWT hoặc local, hoặc tự viết guard kiểm tra token.
- Phân quyền theo vai trò: tạo decorator `@Roles()` gắn metadata bằng `SetMetadata`, rồi `RolesGuard` đọc metadata qua `Reflector`.
- Băm mật khẩu bằng bcrypt hoặc argon2. Access token sống ngắn, refresh token sống dài và có thể thu hồi.

## Cấu hình

Dùng `@nestjs/config` (`ConfigModule.forRoot({ isGlobal: true })`) để đọc biến môi trường, nên kiểm tra tính hợp lệ bằng schema lúc khởi động để thiếu cấu hình thì dừng sớm.

## Kiểm thử

Nest dùng Jest. Unit test dựng `Test.createTestingModule` và ghi đè provider bằng bản giả. Test e2e dùng `supertest` gọi vào ứng dụng thật.

## Các tính năng khác

- Microservices với `@nestjs/microservices` (TCP, Redis, NATS, RabbitMQ, Kafka, gRPC).
- GraphQL qua `@nestjs/graphql`.
- WebSocket gateway cho thời gian thực.
- Hàng đợi với `@nestjs/bullmq`, tác vụ định kỳ với `@nestjs/schedule`.
- Cache với `CacheModule`, giới hạn tần suất với `@nestjs/throttler`.
- Tài liệu API tự sinh với `@nestjs/swagger`.

## Câu hỏi hay gặp

**Guard, Interceptor, Pipe, Middleware khác nhau thế nào?** Middleware chạy sớm và không biết handler nào sẽ xử lý. Guard quyết định cho qua hay chặn. Interceptor bao quanh handler, xử lý cả trước và sau. Pipe biến đổi và kiểm tra tham số vào.

**Vì sao cần `exports` trong module?** Mặc định provider chỉ dùng được trong module chứa nó. `exports` mở nó ra cho module khác import.

**Phụ thuộc vòng tròn giữa hai service xử lý sao?** Nên thiết kế lại để tránh. Nếu bắt buộc thì dùng `forwardRef()` ở cả hai phía, hoặc tách phần dùng chung ra service thứ ba.

**Module toàn cục là gì?** Gắn `@Global()` để provider được export dùng ở mọi nơi mà không cần import. Chỉ nên dùng cho thứ thật sự dùng chung như cấu hình hoặc logger.

**Làm sao xử lý lỗi thống nhất?** Ném `HttpException` hoặc lớp con (`NotFoundException`...) trong service, rồi dùng exception filter toàn cục để định dạng phản hồi.

**Fastify hay Express?** Express quen thuộc và nhiều middleware có sẵn. Fastify nhanh hơn nhưng một số thư viện dành cho Express không dùng được trực tiếp.

**Nest khác Express thuần ở đâu?** Express chỉ cho routing và middleware, cấu trúc tự quyết. Nest thêm khuôn tổ chức, DI, kiểm tra dữ liệu và công cụ kiểm thử, đổi lại phải học nhiều khái niệm hơn.
