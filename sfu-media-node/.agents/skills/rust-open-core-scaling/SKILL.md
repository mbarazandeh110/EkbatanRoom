---
name: rust-open-core-scaling
description: الگوی معماری Open-Core و سیستم لایسنسینگ آفلاین Ed25519 در sfu-media-node شامل توزیع چندورکری در نسخه Community و قفل سخت‌افزاری PipeTransport بین‌نودی در نسخه Enterprise.
---

# مهارت تخصصی: معماری Open-Core مقیاس‌پذیری و لایسنسینگ در `sfu-media-node`

هنگام پیاده‌سازی و نگهداری قابلیت‌های مقیاس‌پذیری و لایسنسینگ در `sfu-media-node`، مرز بین **نسخه متن‌باز (Community - پیش‌فرض)** و **نسخه تجاری (Enterprise - فیچر `enterprise` همراه با لایسنس Ed25519)** را بر اساس اصول زیر رعایت کن:

## ۱. قابلیت‌های نسخه متن‌باز (Community Core - همیشه فعال)
1. **مقیاس‌پذیری شفاف یک اتاق روی چند ورکر داخلی (Intra-Node Multi-Worker Spillover):**
   - هر اتاق با یک `logical_router_id` شناخته می‌شود و مدیریت شفاف زیرروترها و پایپینگ داخلی با `pipe_to_router` کاملاً در هسته متن‌باز قرار دارد.
   - رعایت Peer Affinity و Worker Spillover بر اساس ظرفیت ورکرها.
2. **گزارش وزن و بار ترکیبی نود (`NodeCapacityMetrics`):**
   - محاسبه `total_capacity_weight = alive_workers * worker_capacity_weight`
   - محاسبه `effective_load_percent = f32::max(logical_load_percent, cpu_usage_percent)`
   - بازگرداندن `license_tier: "community"` و `enterprise_cascading_enabled: false` در غیاب لایسنس تجاری.

## ۲. سیستم لایسنسینگ آفلاین (Ed25519 Offline Licensing)
1. **فرمت توکن:** `EKB1.<Base64URL_Payload>.<Base64URL_Signature>` با الگوریتم رمزنگاری نامتقارن Ed25519 و بدون وابستگی به اینترنت یا سرور مرکزی.
2. **پشتیبانی از دوره تنفس (Grace Period):** در صورت پایان تاریخ انقضا و حضور در دوره تنفس، سرویس با صدور لاگ اخطار به کار خود ادامه می‌دهد.
3. **عدم قطعی اتصالات موجود:** انقضای لایسنس هرگز اتصالات فعال را قطع نمی‌کند و فقط مانع ایجاد پایپ‌های جدید می‌شود.
4. **انتشار پویا:** لایسنس از طریق متغیر محیطی `EKBATAN_LICENSE_KEY`، استریم `WatchNodeEventsRequest.license_token` و متد gRPC پویای `ApplyLicense` پشتیبانی می‌شود.

## ۳. قابلیت‌های نسخه تجاری (Enterprise Edition - فیچر `enterprise`)
1. **پورت `InterNodePipeUseCase` در `src/ports/inbound.rs`:**
   - متدهای `create_pipe_transport`, `connect_pipe_transport`, `pipe_produce`, `pipe_consume`.
   - در غیاب فیچر در باینری: خطای `SfuDomainError::EnterpriseFeatureRequired` $\rightarrow$ `tonic::Code::Unimplemented`.
   - در صورت وجود فیچر اما عدم وجود لایسنس معتبر: خطای `SfuDomainError::LicensePermissionDenied` $\rightarrow$ `tonic::Code::PermissionDenied`.
2. **سرویس تجاری (`EnterprisePipeService`):**
   - گیت لایسنسینگ روی تمام متدهای پایپینگ بین‌نودی.
   - فعال شدن پرچم `enterprise_cascading_enabled = true` و `license_tier = "enterprise"` در `NodeCapacityMetrics`.
