---
name: rust-open-core-scaling-and-licensing
description: الگوی معماری Open-Core و سیستم لایسنسینگ آفلاین Ed25519 در sfu-media-node شامل توزیع چندورکری در نسخه Community و قفل سخت‌افزاری PipeTransport بین‌نودی در نسخه Enterprise.
---

# مهارت تخصصی: معماری Open-Core، مقیاس‌پذیری و سیستم لایسنسینگ آفلاین در `sfu-media-node`

این مهارت راهنمای جامع معماری، طراحی دامنه، امنیت رمزنگاری و پیاده‌سازی مدل **Open-Core** و سیستم **لایسنسینگ آفلاین مبتنی بر الگوریتم Asymmetric Ed25519** در سرویس مدیا پلین `sfu-media-node` (Rust) است.

---

## ۱. مدل تفکیک قابلیت‌ها (The Open-Core Architecture Model)

مرز قابلیت‌های نسخه متن‌باز و تجاری در لایه مدیا پلین به گونه‌ای طراحی شده که هسته متن‌باز حداکثر توان پردازشی سرور تک‌نوده را در اختیار جامعه توسعه‌دهندگان قرار دهد و نسخه تجاری قابلیت‌های کلاسترینگ توزیع‌شده را فعال سازد:

| قابلیت / ویژگی | نسخه متن‌باز (Community Edition) | نسخه تجاری (Enterprise Edition) |
| :--- | :--- | :--- |
| **توزیع اتاق روی چند ورکر داخلی** | ✅ بله (شفاف و نامحدود روی همه هسته‌ها) | ✅ بله (شامل تمام قابلیت‌های Community) |
| **محاسبه بار ترکیبی نود (`NodeCapacityMetrics`)** | ✅ بله (Logical + Reactive CPU Load) | ✅ بله (ارسال بلادرنگ در `WatchNodeEvents`) |
| **Singleflight و Idempotency کامل** | ✅ بله (جلوگیری از Race Condition) | ✅ بله |
| **اتصال PipeTransport بین دو سرور مجزا** | ❌ خیر (خطای `UNIMPLEMENTED`) | ✅ بله (نیاز به فیچر `enterprise` و لایسنس معتبر) |
| **سیستم لایسنسینگ آفلاین (Ed25519)** | ❌ خیر (پیش‌فرض روی `license_tier: "community"`) | ✅ بله (اعتبارسنجی آفلاین بدون وابستگی خارجی) |
| **اعمال لایسنس بدون Downtime** | ❌ ندارد | ✅ بله (از طریق متغیر محیطی، RPC یا استریم رویداد) |

---

## ۲. قابلیت‌های هسته متن‌باز (Community Core)

۱. **توزیع شفاف اتاق روی چند ورکر داخلی (Intra-Node Multi-Worker Spillover):**
   - هر اتاق با یک `logical_router_id` شناخته می‌شود.
   - روتر اولیه روی ورکر با کمترین بار ساخته می‌شود.
   - **Peer Affinity:** ترنسپورت‌های یک کاربر (`peer_id`) در صورت وجود روی همان ورکر و روتر فیزیکی قبلی ایجاد می‌شوند تا از پایپینگ غیرضروری جلوگیری شود.
   - **Worker Spillover:** در صورت عبور بار ورکر فعلی از آستانه سرریز (مثلاً ۸۰٪)، ورکر خلوت دیگری انتخاب شده و در صورت نبود Sub-Router برای اتاق روی آن ورکر، به صورت خودکار و Thread-safe (با قفل `SubRouterLockMap`) ساخته می‌شود.
   - **Internal Pipe-to-Router:** اگر ناشر و مشترک روی دو روتر فیزیکی مجزا باشند، پرودیوسر با `pub_router.pipe_to_router` به روتر ساب اسکرایبر پایپ می‌شود.
   - **Cascade Close:** با بسته شدن پرودیوسر اصلی یا ورکر، تمام پایپ‌های زیرمجموعه به صورت آبشاری تمیز می‌شوند.

۲. **گزارش وزن و بار ترکیبی (`NodeCapacityMetrics`):**
   - وزن کل: `total_capacity_weight = alive_workers * worker_capacity_weight`
   - بار منطقی (Predictive): محاسبه بر مبنای جدول اوزان استاندارد:
     - `WebRtcTransport = 1`
     - `AudioProducer = 2`
     - `VideoProducer = 10`
     - `AudioConsumer = 1`
     - `VideoConsumer = 2`
   - بار موثر: `effective_load_percent = max(logical_load_percent, cpu_usage_percent)`
   - فیلد `license_tier`: در نسخه متن‌باز مقدار `"community"` دارد.
   - فیلد `enterprise_cascading_enabled`: در غیاب لایسنس تجاری مقدار `false` دارد.

---

## ۳. سیستم لایسنسینگ آفلاین (Ed25519 Offline Licensing)

### ۳.۱. مشخصات فنی توکن
- **الگوریتم رمزنگاری:** امضای نامتقارن **Ed25519** (۶۴ بایت امضا، ۳۲ بایت کلید عمومی).
- **عدم وابستگی:** بدون استفاده از کتابخانه‌های حجیم JWT یا سرورهای مرکزی (کاملاً Air-Gapped).
- **فرمت توکن:**
  ```text
  EKB1.<Base64URL_Payload>.<Base64URL_Ed25519_Signature>
  ```
- **داده‌های امضاشده (Signed Message):**
  رشته `format!("EKB1.{}", b64_payload).as_bytes()` امضا می‌شود تا از هرگونه دستکاری هدر پیشگیری شود.

### ۳.۲. ساختار کلیم‌ها (`LicenseClaims`)
```rust
pub struct LicenseClaims {
    pub license_id: String,
    pub customer_id: String,
    pub tier: String, // "enterprise" | "community"
    pub features: Vec<String>, // ["inter_node_cascading", ...]
    pub cluster_limits: ClusterLimits,
    pub allowed_domains: Vec<String>,
    pub issued_at: u64, // Unix timestamp in seconds
    pub expires_at: u64, // 0 = Perpetual
    pub grace_period_days: u32,
}
```

### ۳.۳. منطق دوره تنفس (Grace Period) و عدم قطعی اتصالات
- **دوره تنفس:** اگر `now > expires_at` باشد ولی هنوز در بازه `expires_at + (grace_period_days * 86400)` باشیم:
  - لایسنس همچنان معتبر شناخته می‌شود.
  - لاگ هشدار `WARN` با سطح بالا صادر می‌شود.
- **پس از اتمام دوره تنفس:**
  - ساخت منابع بین‌نودی جدید با خطای `LicensePermissionDenied` مسدود می‌شود.
  - **قانون طلایی پایداری:** اتصالات و ترنسپورت‌های پایپ جاری که قبلاً ساخته شده‌اند، هرگز قطع نمی‌شوند تا تماس کاربران فعال خراب نشود.

---

## ۴. انتشار و اعمال لایسنس (Control-Plane Propagation)

برای حفظ استقلال و راحتی استقرار، ۳ مسیر مستقل برای اعمال لایسنس پیاده‌سازی شده است:

۱. **متغیر محیطی در زمان راه‌اندازی:**
   - `EKBATAN_LICENSE_KEY="EKB1..."`
   - `EKBATAN_LICENSE_PUBLIC_KEY="<32-byte-hex>"` (اختیاری جهت بازنویسی کلید ریشه)
۲. **انتشار از طریق استریم رویدادها (`WatchNodeEvents`):**
   - سرویس `signaling-coordinator` توکن را در پیام `WatchNodeEventsRequest.license_token` ارسال می‌کند.
   - مدیا نود در بدو اتصال استریم، توکن را در پس‌زمینه اعتبارسنجی و لود می‌کند.
۳. **فراخوانی RPC پویا (`ApplyLicense`):**
   - برای تمدید آنی لایسنس بدون نیاز به Restart پروسه نود (Zero-Downtime License Refresh).

---

## ۵. نگاشت خطاهای دامنه در مرز gRPC (Hexagonal Error Mapping)

- عدم کامپایل فیچر `enterprise` در نسخه Community:
  - `SfuDomainError::EnterpriseFeatureRequired(msg)` $\rightarrow$ `tonic::Code::Unimplemented`
- وجود فیچر `enterprise` ولی عدم ارائه یا انقضای لایسنس:
  - `SfuDomainError::LicensePermissionDenied(msg)` $\rightarrow$ `tonic::Code::PermissionDenied`
- توکن لایسنس با فرمت خراب یا امضای دستکاری‌شده:
  - `SfuDomainError::InvalidLicense(msg)` $\rightarrow$ `tonic::Code::InvalidArgument`

---

## ۶. الزامات لاگ و متریک‌ها (Observability Guards)

۱. **ماسک کردن توکن لایسنس:**
   - توکن لایسنس جزو داده‌های حساس است و هرگز نباید به صورت خام در لاگ‌ها چاپ شود. همواره از برچسب `[REDACTED]` استفاده شود.
۲. **گارد کاردینالیتی متریک‌ها:**
   - متریک‌های اختصاصی لایسنس:
     - `sfu_license_valid` (مقدار ۱ یا ۰)
     - `sfu_license_expiry_timestamp_seconds` (یونیکس تایم‌استمپ بر حسب ثانیه)
   - قرار دادن هرگونه برچسب با کاردینالیتی بالا نظیر `license_id` یا `customer_id` در متریک‌های پرومتئوس اکیداً ممنوع است.
