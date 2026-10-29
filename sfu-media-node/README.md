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
| `CreateTransport` | ساخت `WebRtcTransport` جدید به همراه پارامترهای ICE، کاندیدها، فینگرپرینت‌های DTLS و SCTP (با پشتیبانی از Worker Spillover) |
| `ConnectTransport` | اتصال ترنسپورت سمت SFU به فینگرپرینت‌ها و نقش DTLS کلاینت |
| `CreatePipeTransport` | ایجاد `PipeTransport` جهت توزیع جریان رسانه به روترها/نودهای دیگر (Cascading) |
| `ConnectPipeTransport` | اتصال `PipeTransport` به آدرس IP و پورت مقصد |
| `RestartIce` | تجدید اطلاعات احراز هویت ICE کاندیدها |
| `Produce` | ایجاد پرودیوسر مدیا (صدا یا تصویر) با اعتبارسنجی پارامترهای RTP و اتصال به AudioLevelObserver متناظر |
| `Consume` | ایجاد کانسومر مدیا با بررسی الزامی `router.can_consume` و Pipe خودکار بین ورکرهای داخلی در صورت سرریز |
| `SetMediaPaused` | توقف (`pause`) یا ادامه (`resume`) پرودیوسر یا کانسومر (با همگام‌سازی وضعیت در صورت Pipe) |
| `SetConsumerPreferredLayers` | تنظیم لایه‌های فضایی و زمانی مورد ترجیح در کانسومر |
| `CloseResource` | بستن آبشاری هریک از منابع (`Router`, `Transport`, `Producer`, `Consumer`) و پاکسازی PipeTransportها |
| `GetStats` | استخراج آمار برخط ترافیک، بایت‌ها و بسته‌های ارسالی/دریافتی RTP به صورت موازی |
| `GetNodeCapacity` | استعلام وضعیت وزن، بار منطقی منابع، مصرف واقعی CPU و بار مؤثر کلی نود |
| `WatchNodeEvents` | استریم رویدادهای نود (`MediaScoreEvent`, `LayerChangedEvent`, `ResourceClosedEvent`, `ActiveSpeakerEvent`, `NodeCapacityMetrics`) |

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
| `SFU_WORKER_CAPACITY_WEIGHT` | `1000` | وزن ظرفیت اسمی هر ورکر جهت محاسبه درصد بار منابع فعال |
| `SFU_WORKER_SPILLOVER_THRESHOLD_PERCENT` | `80.0` | درصد آستانه سرریز ورکر جهت ساخت Sub-Router روی کم‌بارترین ورکر دیگر |
| `OBS_LOG_LEVEL` | `info` | سطح فیلتر لاگ‌های ساختاریافته (`trace`, `debug`, `info`, `warn`, `error`) |
| `OBS_LOG_FORMAT` | `json` | قالب خروجی لاگ‌ها (`json` برای لاگ سرورها، `text` برای توسعه محلی) |
| `OBS_TRACING_ENABLED` | `false` | فعال‌سازی ردیابی توزیع‌شده OpenTelemetry OTLP |
| `OBS_OTLP_ENDPOINT` | `http://localhost:4317` | آدرس کالکتور OpenTelemetry OTLP gRPC (پشتیبانی ثانویه از `OBS_TRACING_ENDPOINT`) |
| `OBS_TRACE_SAMPLE_RATE` | `0.1` | نرخ نمونه‌برداری تریسینگ توزیع‌شده (پشتیبانی ثانویه از `OBS_TRACING_SAMPLE_RATE`) |
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

# اجرای تست‌های اعتبارسنجی ریفکتورینگ و پاکسازی آبشاری
make test-refactoring

# اجرای تست‌های سرریز ورکر، توزیع اتاق و ظرفیت نود (Worker Spillover & Capacity)
make test-capacity

# اجرای تست‌های اعتبارسنجی تنظیمات و پارامترهای Mediasoup
make test-config

# اجرای تست‌های معماری Open-Core و توزیع اتاق بین نودها (Enterprise Cascading)
make test-open-core

# اجرای تمامی تست‌ها با تمام فیچرها (حالت تجاری / Enterprise)
make test-all-features

# اجرای سرور به صورت محلی
make run
```

---

## ۶. مشاهده‌پذیری، APM، متریک‌ها و گارد کاردینالیتی (Observability)

سیستم مشاهده‌پذیری مطابق استانداردهای `rust-observability-otel` و با رعایت چهار اصل بنیادین پیاده‌سازی شده است:

1. **پیکربندی ۱۰۰٪ پویا با `OBS_*`:**
   - فیلتر سطوح لاگ از طریق `OBS_LOG_LEVEL`.
   - قالب‌بندی ساختاریافته JSON با `OBS_LOG_FORMAT=json` برای محیط سرور.
   - ردیابی توزیع‌شده با صادرکننده استاندارد OTLP gRPC (`OBS_TRACING_ENABLED=true` و `OBS_OTLP_ENDPOINT`). در حالت غیرفعال، لایه تریسینگ به رجیستری متصل نمی‌شود تا سربار پردازشی صفر باشد.
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
     - `sfu_node_capacity_weight` (Gauge)
     - `sfu_node_logical_load_percent` (Gauge)
     - `sfu_node_cpu_usage_percent` (Gauge)
     - `sfu_node_effective_load_percent` (Gauge)
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

## ۸. ساختار درختی پروژه (Hexagonal Architecture)

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
│   ├── adapters/               # آداپتورهای ورودی و خروجی (Hexagonal Adapters)
│   │   ├── inbound_grpc/
│   │   │   ├── mapper.rs       # نگاشت و اعتبارسنجی DTO مرز Protobuf به Domain
│   │   │   ├── mod.rs
│   │   │   └── server.rs       # پیاده‌سازی سرویس gRPC و توزیع Use-Caseها
│   │   ├── outbound_mediasoup/
│   │   │   ├── event_hub.rs    # آداپتور انتشار برادکست رویدادها (ActiveSpeaker, Score, etc.)
│   │   │   ├── mod.rs
│   │   │   └── worker_pool.rs  # استخر پروسه‌های mediasoup-worker و بالانس بار
│   │   └── mod.rs
│   ├── application/            # لایه هماهنگ‌کننده یوزکیس‌ها (Application Layer)
│   │   ├── media_node.rs       # Facade و Composition Root سرور مدیا
│   │   ├── media_service.rs    # منطق تولید، مصرف و لایه‌بندی مدیا
│   │   ├── pipe_service.rs     # مدیریت PipeTransport بین نودها (Open-Core & Cascading)
│   │   ├── router_service.rs   # مدیریت روترها و پیشگیری از شرایط مسابقه
│   │   ├── state.rs            # نگهداری ایزوله و بدون ددلاک وضعیت نود
│   │   ├── system_sampler.rs   # نمونه‌برداری بلادرنگ CPU و بار سیستم
│   │   ├── transport_service.rs# مدیریت انتقال WebRTC و تجدید ICE
│   │   └── mod.rs
│   ├── config.rs               # تنظیمات شبکه، پورت‌ها، ورکرها و مقادیر پیش‌فرض امن
│   ├── domain/                 # موجودیت‌ها، خطاها و منطق محض دامنه
│   │   ├── codecs.rs           # پیکربندی کدک‌های استاندارد Opus, VP8, H.264
│   │   ├── entities.rs         # مدل‌ها و تایپ‌های مستقل دامنه
│   │   ├── errors.rs           # خطاهای تایپ‌شده SfuDomainError
│   │   ├── idempotency.rs      # ماشین وضعیت کش Idempotency اتمیک با هش SHA-256
│   │   ├── mod.rs
│   │   └── validation.rs       # اعتبارسنجی مرزی پارامترهای RTP و DTLS
│   ├── lib.rs                  # اکسپورت کتابخانه هسته SFU
│   ├── main.rs                 # نقطه ورودی باینری و Graceful Shutdown
│   ├── observability/          # ماژول مشاهده‌پذیری، تریسینگ توزیع‌شده و لاگ
│   │   ├── config.rs           # تنظیمات OBS_* مشاهده‌پذیری
│   │   ├── init.rs             # راه‌اندازی Tracing، OpenTelemetry OTLP و سرور /metrics
│   │   ├── metrics.rs          # توابع ثبت متریک با رعایت کامل گارد کاردینالیتی
│   │   ├── redaction.rs        # ماسک‌سازی گذرواژه‌های ICE و داده‌های حساس
│   │   ├── tracing_extractor.rs# استخراج W3C traceparent و x-request-id
│   │   └── mod.rs
│   ├── ports/                  # اینترفیس‌های مرزی هگزاگونال (Ports)
│   │   ├── inbound.rs          # Use-Case Traitها (RouterUseCase, TransportUseCase, MediaUseCase, InterNodePipeUseCase)
│   │   ├── mod.rs
│   │   └── outbound.rs         # اینترفیس‌های آداپتورهای خروجی (WorkerPoolPort, EventPublisherPort, MetricsPort)
│   └── proto.rs                # ماژول تولیدشده از protobuf
└── tests/
    ├── api_verification.rs     # راستی‌آزمایی API هسته mediasoup
    ├── config_and_mediasoup_tuning_tests.rs # آزمون‌های بارگذاری پیکربندی و پارامترهای mediasoup
    ├── lifecycle_tests.rs      # آزمون‌های چرخه حیات Router و Transport و ICE Restart
    ├── media_tests.rs          # آزمون‌های Produce/Consume، لایه‌ها، آمار و رویدادها
    ├── multi_worker_capacity_tests.rs # آزمون‌های Multi-Worker Spillover، سرریز، Pipe و ظرفیت نود
    ├── observability_tests.rs  # آزمون‌های استخراج W3C TraceContext و متریک‌های پرومتئوس
    ├── open_core_tests.rs      # آزمون‌های معماری Open-Core، سرریز و انتقال رسانه بین‌نودی
    └── refactoring_verification_tests.rs # آزمون‌های حذف آبشاری، رویدادهای زامبی و ایمنی ورکر
```

