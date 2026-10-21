---
name: rust-hexagonal-solid
description: الگوی پیاده‌سازی عملی معماری Clean/Hexagonal، اصول پنج‌گانه SOLID و Best Practiceهای زبان Rust در سرویس‌های بلادرنگ بدون پیچیدگی زودرس.
---

# مهارت تخصصی: معماری Hexagonal، اصول SOLID و Best Practiceها در Rust

هنگام طراحی و کدنویسی سرویس `sfu-media-node`، ساختار معماری و اصول زیر را به صورت سخت‌گیرانه رعایت کن:

## ۱. ساختار دایرکتوری‌ها (Pragmatic Hexagonal Architecture)
مرزهای معماری را دقیقاً در ۴ لایه زیر با جهت وابستگی رو به داخل (`transport/adapters -> application -> domain`) حفظ کن:
```text
src/
├── main.rs                        # Composition Root: فقط خواندن Config، تزریق وابستگی‌ها (DI) و اجرای سرور
├── config.rs                      # تنظیمات 12-Factor از متغیرهای محیطی با اعتبارسنجی در بدو شروع
├── domain/                        # لایه هسته و قوانین دامنه (بدون وابستگی به tonic یا جزئیات شبکه)
│   ├── mod.rs
│   ├── entities.rs                # مدل‌های تایپ‌شده دامنه (RouterId, TransportId, MediaKind, LayerInfo)
│   ├── idempotency.rs             # ماشین وضعیت کش Idempotency (InFlight, Completed, PayloadHash)
│   └── errors.rs                  # خطاهای دامنه با thiserror (SfuDomainError)
├── ports/                         # مرزهای ورودی و خروجی (Traits) بر اساس اصل ISP و DIP
│   ├── mod.rs
│   ├── inbound.rs                 # Traitهای UseCase برای مدیریت چرخه حیات رسانه
│   └── outbound.rs                # Traitهای انتشار رویداد و مدیریت Workerهای Mediasoup
├── application/                   # لایه هماهنگ‌کننده منطق کاربردی (Use Cases)
│   ├── mod.rs
│   ├── router_service.rs          # پیاده‌سازی ساخت/حذف آبشاری روتر و بررسی can_consume
│   ├── transport_service.rs       # پیاده‌سازی ساخت، اتصال DTLS و بازیابی ICE
│   └── media_service.rs           # پیاده‌سازی Produce, Consume, Pause, PreferredLayers و Stats
└── adapters/                      # لایه زیرساخت و آداپتورهای ورودی/خروجی
    ├── mod.rs
    ├── inbound_grpc/              # آداپتور ورودی Tonic gRPC
    │   ├── server.rs              # پیاده‌سازی SfuMediaEngineService
    │   └── mapper.rs              # پارس، اعتبارسنجی مرزی JSON و تبدیل Protobuf <-> Domain DTO
    └── outbound_mediasoup/        # آداپتور خروجی کریت mediasoup
        ├── worker_pool.rs         # مدیریت WorkerManager و WebRtcServer
        └── event_hub.rs           # کانال توزیع رویدادهای بلادرنگ (tokio::sync::broadcast)
