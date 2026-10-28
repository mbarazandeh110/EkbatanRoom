---
name: rust-open-core-scaling
description: الگوی معماری Open-Core در sfu-media-node شامل توزیع شفاف اتاق روی چند ورکر در نسخه متن‌باز (Community) و جداسازی PipeTransport بین‌نودی برای نسخه تجاری (Enterprise).
---

# مهارت تخصصی: معماری Open-Core مقیاس‌پذیری در `sfu-media-node`

هنگام پیاده‌سازی قابلیت‌های مقیاس‌پذیری در `sfu-media-node`، مرز بین **نسخه متن‌باز (Community - پیش‌فرض)** و **نسخه تجاری (Enterprise - فیچر `enterprise`)** را دقیقاً بر اساس اصول Hexagonal و SOLID زیر رعایت کن:

## ۱. قابلیت‌های نسخه متن‌باز (Community Core - همیشه فعال)
1. **مقیاس‌پذیری شفاف یک اتاق روی چند ورکر داخلی (Intra-Node Multi-Worker Spillover):**
   - هر اتاق با یک `logical_router_id` (شناسه روتر اولیه ساخته‌شده در `CreateRouter`) شناخته می‌شود.
   - در `EngineState`، هر اتاق می‌تواند علاوه بر روتر اصلی، مجموعه‌ای از روترهای فرعی (`sub_routers: DashMap<usize, Router>`) روی سایر ورکرهای همان نود داشته باشد.
   - **منطق `CreateTransport`:**
     - **Peer Affinity:** اگر برای `(router_id, peer_id)` قبلاً ترنسپورتی در این اتاق وجود دارد، ترنسپورت جدید همان کاربر را روی **همان ورکر و همان روتر فیزیکی** بساز.
     - **Worker Spillover:** اگر کاربر جدید است و بار ورکرِ فعلیِ اتاق از مرز سرریز ورکر (مثلاً ۸۰٪ ظرفیت یک ورکر) عبور کرده، کم‌بارترین ورکرِ نود را از `WorkerPool` بگیر؛ در صورت نبود روتر برای این اتاق روی آن ورکر، یک Sub-Router بساز و ترنسپورت کاربر را روی آن ایجاد کن.
   - **منطق `Consume` (پایپینگ داخلی خودکار با `pipe_to_router`):**
     - اگر روتر فیزیکیِ `producer_id` ($R_{pub}$) با روتر فیزیکیِ `subscriber_transport_id` ($R_{sub}$) متفاوت بود، به صورت خودکار و Thread-safe (با قفل Singleflight/Idempotency روی کلید `(producer_id, R_sub.id())`)، پرودیوسر را با `R_pub.pipe_to_router(...)` به $R_{sub}$ منتقل کن و سپس Consumer را روی $R_{sub}$ بساز.
     - عملیات `SetMediaPaused` و `CloseResource` روی Producer اصلی باید به صورت آبشاری روی تمام Producerهای Pipeشده در روترهای فرعی همان اتاق نیز اعمال شود.
2. **گزارش وزن و بار ترکیبی نود (`NodeCapacityMetrics`):**
   - محاسبه `total_capacity_weight = alive_workers * 1000`.
   - محاسبه `logical_load_percent` بر اساس امتیاز وزنی منابع فعال (`Transport=1, AudioProducer=2, VideoProducer=10, AudioConsumer=1, VideoConsumer=2`).
   - محاسبه `effective_load_percent = f32::max(logical_load_percent, cpu_usage_percent)`.
   - انتشار در استریم `WatchNodeEvents` (هنگام تغییر بار بیش از ۳٪ یا هر ۵ ثانیه) و ارائه در RPC `GetNodeCapacity`.

## ۲. قابلیت‌های نسخه پولی (Enterprise Edition - تحت فیچر `#[cfg(feature = "enterprise")]`)
1. **پورت `InterNodePipeUseCase` در `src/ports/inbound.rs`:**
   - متدهای `create_pipe_transport`, `connect_pipe_transport`, `pipe_produce`, `pipe_consume` را در تریت `InterNodePipeUseCase` تعریف کن.
   - در `SfuDomainError` خطای `EnterpriseFeatureRequired(String)` را اضافه کن که در `mapper.rs` به `tonic::Status::unimplemented(msg)` نگاشت می‌شود.
2. **آداپتور نسخه متن‌باز (`CommunityNoOpPipeService`):**
   - وقتی فیچر `enterprise` فعال نیست، این پیاده‌سازی به `MediaNode` تزریق می‌شود، فیلد `enterprise_cascading_enabled: false` را در `NodeCapacityMetrics` برمی‌گرداند و برای تمام RPCهای Pipe بین‌نودی خطای `SfuDomainError::EnterpriseFeatureRequired("Inter-node room cascading is an Enterprise feature")"` برمی‌گرداند.
3. **سرویس نسخه تجاری (`EnterprisePipeService` - فعال با `--features enterprise`):**
   - ساخت `PipeTransport` واقعی با پشتیبانی از `enable_srtp` و `enable_rtx` روی `listen_ip` و `announced_ip` نود.
   - اتصال دو نود با `connect_pipe_transport` و امکان انتقال Producer بین دو سرور مجزا.
