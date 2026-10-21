# SFU Media Node (`sfu-media-node`)

هسته رسانه‌ای مستقل (Media Plane) پروژه **EkbatanRoom** مبتنی بر **Rust** و **Mediasoup**، ارائه‌دهنده سرویس gRPC منطبق بر قرارداد `proto/sfu/v1/media_engine.proto`.

---

## ۱. معماری و ویژگی‌های کلیدی

- **طراحی شش‌ضلعی و تفکیک لایه‌ها (Hexagonal / Clean Architecture):** جداسازی کامل لایه‌های دامنه (`domain`)، موتور رسانه (`engine`)، و لایه انتقال ارتباطی (`transport/grpc`).
- **استقلال کامل از سیگنالینگ و لایوکیت:** هسته رسانه تنها مفاهیم استاندارد Mediasoup و ORTC را پردازش می‌کند و هیچ وابستگی به SDP، JWT یا پروتکل‌های سیگنالینگ اختصاصی ندارد.
- **اعتبارسنجی سخت‌گیرانه در مرز RPC (Boundary Validation):** تبدیل و اعتبارسنجی رشته‌های خام JSON ورودی (`rtp_parameters_json` و `rtp_capabilities_json`) در بدو ورود gRPC به ساختارهای تایپ‌شده `mediasoup` و بازگرداندن خطای `InvalidArgument` در صورت بروز هرگونه مغایرت.
- **سیاست دقیق تکرارپذیری (Idempotency State Machine):**
  - جدول کش Thread-Safe با TTL پیش‌فرض ۶۰ ثانیه به همراه هش SHA-256 از بدنه درخواست.
  - پشتیبانی از قفل وضعیت در حال اجرا (`In-Flight`) جهت پیشگیری از ساخت منابع تکراری در درخواست‌های همزمان.
  - تشخیص ناسازگاری Payload با خطای `AlreadyExists` در صورت ارسال داده متفاوت با شناسه یکسان.
  - پاکسازی خودکار کلید در صورت بروز خطا برای مجاز بودن تلاش مجدد (Retry).
- **پاکسازی آبشاری منابع (Cascading Cleanup):** بستن یک `Router` در متد `CloseResource` به صورت خودکار تمام `Transport`ها، `Producer`ها و `Consumer`های وابسته را از حافظه آزاد می‌سازد.
- **جلوگیری از رویدادهای زامبی (Zombie Events Prevention):** بررسی برخط وضعیت فعال بودن اتاق پیش از انتشار هر رویداد در استریم `WatchNodeEvents`.
- **کدک‌های استاندارد رسانه:**
  - **صوتی:** `audio/opus` (48000Hz, 2ch) همراه با بازخورد RTCP (`nack`, `transport-cc`).
  - **تصویری:** `video/VP8` (90000Hz) و `video/H264` (`packetization-mode=1`, `profile-level-id=42e01f`) با فیدبک‌های کامل (`nack`, `nack pli`, `ccm fir`, `goog-remb`, `transport-cc`).
- **مدیریت استخر ورکرها (Worker Pool):** توزیع بار میان پروسه‌های ورکر بر مبنای الگوریتم کم‌بارترین ورکر (Least-Loaded Selection).
- **انطباق با اصول دوازده عاملی (Twelve-Factor App):** خاموش‌سازی نرم و ایمن در برابر سیگنال‌های `SIGINT` و `SIGTERM` و اجرای کانتینر تحت کاربر غیرریشه.

---

## ۲. مشخصات قرارداد gRPC (`rtc.sfu.v1.SfuMediaEngineService`)

| RPC | شرح عملیات |
| :--- | :--- |
| `CreateRouter` | ساخت روتر جدید روی کم‌بارترین ورکر یا بازگرداندن روتر فعال موجود برای اتاق |
| `CreateTransport` | ساخت `WebRtcTransport` جدید به همراه پارامترهای ICE، کاندیدها، فینگرپرینت‌های DTLS و SCTP |
| `ConnectTransport` | اتصال ترنسپورت سمت SFU به فینگرپرینت‌ها و نقش DTLS کلاینت |
| `RestartIce` | تجدید اطلاعات احراز هویت ICE کاندیدها |
| `Produce` | ایجاد پرودیوسر مدیا (صدا یا تصویر) با اعتبارسنجی پارامترهای RTP |
| `Consume` | ایجاد کانسومر مدیا با بررسی الزامی `router.can_consume` و استخراج لایه‌های تصویر |
| `SetMediaPaused` | توقف (`pause`) یا ادامه (`resume`) پرودیوسر یا کانسومر |
| `SetConsumerPreferredLayers` | تنظیم لایه‌های فضایی و زمانی مورد ترجیح در کانسومر |
| `CloseResource` | بستن آبشاری هریک از منابع (`Router`, `Transport`, `Producer`, `Consumer`) |
| `GetStats` | استخراج آمار برخط ترافیک، بایت‌ها و بسته‌های ارسالی/دریافتی RTP |
| `WatchNodeEvents` | استریم رویدادهای نود (`MediaScoreEvent`, `LayerChangedEvent`, `ResourceClosedEvent`) |

---

## ۳. متغیرهای محیطی و پیکربندی

| نام متغیر | پیش‌فرض | توضیحات |
| :--- | :--- | :--- |
| `MEDIASOUP_LISTEN_IP` | `0.0.0.0` | آدرس IP محلی که مدیاسوپ روی آن شنود می‌کند |
| `MEDIASOUP_ANNOUNCED_IP` | `127.0.0.1` | آدرس IP عمومی برای اعلان در ICE کاندیدها (ضروری برای Docker/NAT) |
| `RTC_MIN_PORT` | `40000` | ابتدای بازه پورت‌های UDP برای ترافیک RTC |
| `RTC_MAX_PORT` | `49999` | انتهای بازه پورت‌های UDP برای ترافیک RTC |
| `GRPC_PORT` | `50051` | پورت گوش فرا دادن سرور gRPC کنترلی |
| `MEDIASOUP_WORKER_COUNT` | تعداد هسته‌های CPU | تعداد پروسه‌های ایزوله `mediasoup-worker` |
| `OBS_LOG_LEVEL` | `info` | سطح فیلتر لاگ‌های ساختاریافته (`trace`, `debug`, `info`, `warn`, `error`) |
| `OBS_LOG_FORMAT` | `json` | قالب خروجی لاگ‌ها (`json` برای لاگ سرورها، `text` برای توسعه محلی) |
| `OBS_TRACING_ENABLED` | `false` | فعال‌سازی ردیابی توزیع‌شده OpenTelemetry OTLP |
| `OBS_TRACING_ENDPOINT` | `http://localhost:4317` | آدرس کالکتور OpenTelemetry OTLP gRPC |
| `OBS_TRACING_SAMPLE_RATE` | `1.0` | نرخ نمونه‌برداری تریسینگ توزیع‌شده (بین 0.0 تا 1.0) |
| `OBS_METRICS_ENABLED` | `true` | فعال‌سازی سرور HTTP سبک پرومتئوس برای اندپوینت `/metrics` |
| `OBS_METRICS_ADDR` | `0.0.0.0:9090` | آدرس و پورت شنود سرور متریک‌های پرومتئوس/ویکتوریا‌متریکس |
| `OBS_LOG_RAW_SDP` | `false` | ثبت خطوط خام کاندیدهای SDP فقط در سطح دیباگ (برای مانیتورینگ امن) |

---

## ۴. پیش‌نیازهای محلی

- **Rust:** نسخه `1.75` یا بالاتر (`rustc` و `cargo`)
- **کامپایلر C/C++:** `gcc`, `g++`, `make`
- **ابزارهای ساخت هسته Mediasoup:** `python3`, `pip`, `meson`, `ninja`

در لینوکس دبیان/اوبونتو:
```bash
sudo apt update
sudo apt install -y build-essential python3 python3-pip meson ninja-build
pip3 install invoke
```

---

## ۵. دستورات بیلد و اجرای آزمون‌ها

این مخزن مجهز به یک `Makefile` با تارگت‌های از پیش تعریف‌شده است:

```bash
# نمایش لیست کامل تارگت‌ها
make help

# اجرای سریع اعتبارسنجی کامپایل
make check

# بررسی فرمت کد
make fmt-check

# اجرای لینتر سخت‌گیرانه Clippy
make lint

# اجرای تمامی تست‌های واحد و تست‌های یکپارچگی
make test

# اجرای مجزای تست‌های واحد
make test-unit

# اجرای تست‌های یکپارچگی چرخه حیات (Router & Transport Lifecycle)
make test-lifecycle

# اجرای تست‌های یکپارچگی انتشار و مصرف رسانه (Produce & Consume)
make test-media

# اجرای تست‌های اعتبارسنجی مشاهده‌پذیری، استخراج W3C TraceContext و متریک‌ها
make test-observability

# اجرای سرور به صورت محلی
make run
```

---

## ۶. مشاهده‌پذیری، APM، متریک‌ها و گارد کاردینالیتی (Observability)

سیستم مشاهده‌پذیری مطابق استانداردهای `rust-observability-otel` و با رعایت چهار اصل بنیادین پیاده‌سازی شده است:

1. **پیکربندی ۱۰۰٪ پویا با `OBS_*`:**
   - فیلتر سطوح لاگ از طریق `OBS_LOG_LEVEL`.
   - قالب‌بندی ساختاریافته JSON با `OBS_LOG_FORMAT=json` برای محیط سرور.
   - ردیابی توزیع‌شده با صادرکننده استاندارد OTLP gRPC (`OBS_TRACING_ENABLED=true` و `OBS_TRACING_ENDPOINT`). در حالت غیرفعال، لایه تریسینگ به رجیستری متصل نمی‌شود تا سربار پردازشی صفر باشد.
   - اندپوینت مستقل پرومتئوس `/metrics` روی `OBS_METRICS_ADDR` (پیش‌فرض `0.0.0.0:9090`).
2. **محافظت از مسیر پرتکرار رسانه (Media Hot-Path Protection):**
   - هیچ‌گونه لاگ یا Span به ازای هر بسته RTP یا کال‌بک‌های سطح صدا تولید نمی‌شود.
3. **گارد سخت‌گیرانه کاردینالیتی متریک‌ها (Metric Cardinality Guard):**
   - متریک‌های استاندارد ثبت‌شده:
     - `sfu_active_routers` (Gauge)
     - `sfu_active_transports{direction="pub|sub"}` (Gauge)
     - `sfu_active_producers{kind="audio|video"}` (Gauge)
     - `sfu_active_consumers{kind="audio|video"}` (Gauge)
     - `sfu_grpc_requests_total{method="...", status="..."}` (Counter)
     - `sfu_grpc_request_duration_seconds{method="...", status="..."}` (Histogram)
     - `sfu_idempotency_hits_total` (Counter)
   - **خط قرمز:** شناسه‌های با کاردینالیتی بالا (`room_id`, `peer_id`, `transport_id`, `request_id`) **هرگز** در برچسب‌های متریک قرار نمی‌گیرند و منحصراً در فیلدهای لاگ JSON و اتریبیوت‌های Span تریسینگ ثبت می‌شوند.
4. **پالایش و امنیت اطلاعات حساس (Security Redaction):**
   - گذرواژه‌های ICE (`ice_pwd`) و توکن‌ها در لاگ‌ها با مقدار `[REDACTED]` ماسک می‌شوند.
   - خطوط خام کاندیدهای SDP فقط در صورت فعال‌سازی `OBS_LOG_RAW_SDP=true` و در سطح `debug` لاگ می‌شوند.
5. **استخراج W3C `traceparent` و `x-request-id`:**
   - استخراج خودکار متادیتاهای W3C از هدرهای درخواست gRPC در بدو ورود و اتصال به زنجیره ردیابی توزیع‌شده سرتاسری.

---

## ۷. ساخت و استقرار با Docker

ایمیج داکر این پروژه به صورت چندمرحله‌ای (Multi-stage) و سبک طراحی شده و تحت کاربر غیرریشه اجرا می‌شود:

```bash
# بیلد ایمیج داکر
make docker-build

# اجرای کانتینر به همراه نگاشت شبکه
make docker-run
```

یا مستقیماً با دستور داکر:
```bash
docker run --rm -it \
  --network host \
  -e MEDIASOUP_LISTEN_IP=0.0.0.0 \
  -e MEDIASOUP_ANNOUNCED_IP=<PUBLIC_OR_LAN_IP> \
  -e RTC_MIN_PORT=40000 \
  -e RTC_MAX_PORT=49999 \
  -e GRPC_PORT=50051 \
  -e OBS_LOG_LEVEL=info \
  -e OBS_LOG_FORMAT=json \
  -e OBS_METRICS_ENABLED=true \
  -e OBS_METRICS_ADDR=0.0.0.0:9090 \
  ekbatan/sfu-media-node:latest
```

---

## ۸. ساختار درختی پروژه

```text
.
├── Cargo.toml                  # تعریف وابستگی‌های پین‌شده (mediasoup, tonic, opentelemetry, metrics)
├── build.rs                    # کامپایل خودکار protobuf با protoc-bin-vendored
├── Dockerfile                  # بیلد امن چندمرحله‌ای و اجرای بهینه
├── Makefile                    # اتوماسیون بیلد، تست، لینت و داکر
├── proto/
│   └── sfu/v1/
│       └── media_engine.proto  # قرارداد اصلی سرویس gRPC رسانه
├── src/
│   ├── config.rs               # تنظیمات شبکه، پورت‌ها، ورکرها و مشاهده‌پذیری
│   ├── domain/
│   │   ├── error.rs            # خطاهای تایپ‌شده دامنه و نگاشت به tonic::Status
│   │   ├── idempotency.rs      # ماشین وضعیت کش Idempotency با هش SHA-256
│   │   └── validation.rs       # اعتبارسنجی مرزی پارامترهای RTP و DTLS
│   ├── engine/
│   │   ├── codecs.rs           # پیکربندی کدک‌های استاندارد Opus, VP8, H.264
│   │   ├── media_node.rs       # هسته رسانه، پاکسازی آبشاری و پخش رویدادها
│   │   └── worker_pool.rs      # استخر ورکرها و تخصیص با کم‌ترین بار
│   ├── observability/
│   │   ├── config.rs           # تنظیمات OBS_* مشاهده‌پذیری
│   │   ├── init.rs             # راه‌اندازی Tracing، OpenTelemetry OTLP و سرور /metrics
│   │   ├── metrics.rs          # توابع ثبت متریک با رعایت کامل گارد کاردینالیتی
│   │   ├── redaction.rs        # ماسک‌سازی گذرواژه‌های ICE و داده‌های حساس
│   │   ├── tracing_extractor.rs# استخراج W3C traceparent و x-request-id
│   │   └── mod.rs              # ماژول مشاهده‌پذیری
│   ├── proto.rs                # ماژول تولیدشده از protobuf
│   ├── transport/
│   │   └── grpc/
│   │       └── service.rs      # پیاده‌سازی اینترفیس gRPC با Instrumentation و متریک
│   ├── lib.rs                  # اکسپورت کتابخانه هسته SFU
│   └── main.rs                 # نقطه ورودی باینری و Graceful Shutdown
└── tests/
    ├── api_verification.rs     # راستی‌آزمایی API هسته mediasoup
    ├── lifecycle_tests.rs      # آزمون‌های چرخه حیات Router و Transport و ICE Restart
    ├── media_tests.rs          # آزمون‌های Produce/Consume، لایه‌ها، آمار و رویدادها
    └── observability_tests.rs  # آزمون‌های استخراج W3C TraceContext و متریک‌های پرومتئوس
```
