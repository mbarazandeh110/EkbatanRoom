# قوانین معماری و مهندسی سرویس `sfu-media-node`

## ۱. مرزبندی مسئولیت‌ها و اعتبارسنجی مرزی (Boundary Validation)
1. **استقلال کامل از LiveKit/SDP:** این سرویس فقط مفاهیم خالص `mediasoup` و ORTC را می‌شناسد. هیچ وابستگی به SDP، JWT یا پروتکل LiveKit نباید وارد این مخزن شود.
2. **اعتبارسنجی سخت‌گیرانه DTOها در مرز RPC:** رشته‌های JSON ورودی (مانند `rtp_parameters_json` و `rtp_capabilities_json`) نباید به صورت رشته خام وارد لایه دامنه شوند؛ آن‌ها را در مرز ورود gRPC (`src/transport/grpc/`) با `serde_json` به ساختارهای تایپ‌شده `mediasoup` تبدیل و اعتبارسنجی کن و در صورت خطا، کد `tonic::Code::InvalidArgument` با پیام دقیق برگردان.
3. **نسخه‌های Pin شده و تنظیمات محیطی شبکه:** نسخه کریت‌های `mediasoup`، `tonic` و `prost` را در `Cargo.toml` به صورت دقیق (`=x.y.z`) پین کن. آدرس‌های `MEDIASOUP_LISTEN_IP`، `MEDIASOUP_ANNOUNCED_IP` (برای عبور از NAT/Docker) و بازه پورت‌های UDP/TCP را از متغیرهای محیطی بخوان.

## ۲. سیاست دقیق Idempotency و چرخه حیات منابع (Resource Lifecycle)
1. **ماشین وضعیت `request_id`:** برای عملیات ایجاد منبع (`CreateRouter`, `CreateTransport`, `Produce`, `Consume`) یک جدول کش با TTL مشخص (مثلاً ۶۰ ثانیه) نگه دار که شامل هش Payload درخواست باشد:
   - اگر درخواست تکراری با همان `request_id` و همان Payload آمد و قبلاً کامل شده بود: پاسخ ذخیره‌شده قبلی را برگردان.
   - اگر درخواست اول هنوز در حال اجرا (`In-Flight`) بود: منتظر اتمام آن بمان یا کد `Aborted/Unavailable` برگردان تا منبع دو بار ساخته نشود.
   - اگر `request_id` تکراری با **Payload متفاوت** ارسال شد: خطای `AlreadyExists` یا `InvalidArgument` برگردان.
   - اگر اجرای درخواست با خطا مواجه شد: کلید را از کش پاک کن تا Retry مجاز باشد.
2. **پاکسازی آبشاری و جلوگیری از رویدادهای دیررس (Zombie Events):**
   - بستن یک `Router` در `CloseResource` باید تمام `Transport`ها، `Producer`ها و `Consumer`های زیرمجموعه آن را از `DashMap`ها حذف کند.
   - پیش از ارسال هر رویداد در `WatchNodeEvents`، بررسی کن که `Router` مربوطه هنوز فعال باشد تا رویدادهای دیررس باعث زنده شدن وضعیت اتاق بسته شده نشوند.

## ۳. تفکیک تست واحد و تست یکپارچگی
- تست‌های واحد (`unit tests` برای منطق Idempotency و پارس کردن DTOها) را از تست‌های یکپارچگی (`integration tests` در پوشه `tests/` که پروسه واقعی `mediasoup-worker` را بالا می‌آورند) جدا کن.

----------
# قوانین معماری، SOLID و مهندسی سرویس `sfu-media-node`

## ۱. الزام معماری Hexagonal و اصول SOLID
1. **رعایت اجباری ساختار لایه‌ای:** کدها باید دقیقاً بر اساس معماری هگزاگونال تعریف‌شده در مهارت `rust-hexagonal-solid` در پوشه‌های `domain`، `ports`، `application` و `adapters` تفکیک شوند. نوشتن منطق تجاری داخل هندلرهای gRPC یا قرار دادن کل کدها در یک فایل اکیداً ممنوع است.
2. **استقلال کامل از LiveKit/SDP (اصل DIP):** این سرویس فقط مفاهیم خالص `mediasoup` و ORTC را می‌شناسد. هیچ وابستگی به SDP، JWT، پیام‌های LiveKit یا ساختارهای `prost/tonic` نباید وارد لایه `domain` شود.
3. **اعتبارسنجی سخت‌گیرانه DTOها در مرز RPC (اصل SRP):** رشته‌های JSON ورودی (`rtp_parameters_json` و `rtp_capabilities_json`) باید در مرز ورود gRPC (`adapters/inbound_grpc/mapper.rs`) با `serde_json` به ساختارهای تایپ‌شده `mediasoup` پارس و اعتبارسنجی شوند و در صورت خطا، کد `tonic::Code::InvalidArgument` برگردانده شود[cite: 5].

## ۲. سیاست دقیق Idempotency، چرخه حیات منابع و Best Practices
1. **ماشین وضعیت `request_id`:** برای عملیات ایجاد منبع (`CreateRouter`, `CreateTransport`, `Produce`, `Consume`) یک جدول کش با TTL مشخص نگه دار که شامل هش Payload درخواست باشد[cite: 5]:
   - درخواست تکراری با همان `request_id` و همان Payload (در حالت تکمیل‌شده) $\leftarrow$ بازگرداندن پاسخ قبلی[cite: 5].
   - درخواست تکراری در حالت `In-Flight` $\leftarrow$ انتظار برای اتمام یا بازگرداندن `Aborted` جهت جلوگیری از ساخت منبع تکراری[cite: 5].
   - `request_id` تکراری با Payload متفاوت $\leftarrow$ بازگرداندن خطای `AlreadyExists` / `InvalidArgument`[cite: 5].
   - شکست در اجرای درخواست $\leftarrow$ حذف کلید از کش برای امکان Retry مجدد[cite: 5].
2. **پاکسازی آبشاری و جلوگیری از Zombie Events:**
   - بستن یک `Router` در `CloseResource` باید تمام `Transport`ها، `Producer`ها و `Consumer`های زیرمجموعه را به صورت آبشاری حذف کند[cite: 5].
   - پیش از ارسال هر رویداد در `WatchNodeEvents`، فعال بودن `Router` بررسی شود[cite: 5].
3. **کیفیت کد Rust:**
   - استفاده از `.unwrap()` و `.expect()` در کدهای غیرتست ممنوع است[cite: 5].
   - هرگز یک رفرنس `DashMap` یا `MutexGuard` را در طول یک فراخوانی `.await` نگه ندار.
   - تست‌های واحد (`unit tests` برای منطق دامنه، مپرها و Idempotency) را از تست‌های یکپارچگی (`integration tests` نیازمند `mediasoup-worker`) جدا کن[cite: 5].

## ۳. قوانین Observability، لاگ، APM و مانیتورینگ
1. **پیکربندی‌پذیری کامل:** تمام قابلیت‌های لاگ، تریسینگ (OTLP) و متریک‌ها (`/metrics`) باید از طریق متغیرهای `OBS_*` قابل تنظیم و روشن/خاموش کردن باشند.
2. **محافظت از Hot-Path و امنیت:** ثبت لاگ یا Span در مسیر بسته‌های RTP یا کال‌بک‌های پرتکرار (`on_score`) بدون Throttling ممنوع است. همچنین چاپ کردن کلیدهای خصوصی، `ice_pwd` یا پیلودهای خام JSON در لاگ‌های سطح `info` ممنوع است.
3. **گارد کاردینالیتی متریک‌ها:** قرار دادن شناسه‌های یکتا (`router_id`, `peer_id`, `transport_id`, `request_id`) در لیبل‌های متریک Prometheus/VictoriaMetrics اکیداً ممنوع است (فقط در فیلدهای لاگ و Span قرار گیرند).
