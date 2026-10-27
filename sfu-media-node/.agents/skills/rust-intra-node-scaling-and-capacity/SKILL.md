---
name: rust-intra-node-scaling-and-capacity
description: الگوی پیاده‌سازی توزیع شفاف یک اتاق روی چند ورکر داخلی (Intra-Node Multi-Worker Spillover با pipe_to_router) و محاسبه وزن و بار ترکیبی نود (NodeCapacityMetrics) در sfu-media-node.
---

# مهارت تخصصی: مقیاس‌پذیری عمودی شفاف (Multi-Worker Room) و گزارش ظرفیت نود

هنگام ارتقای `sfu-media-node` برای پشتیبانی از اتاق‌های بزرگ و گزارش بار به `signaling-coordinator`، اصول زیر را به صورت سخت‌گیرانه اجرا کن:

## ۱. تفکیک مسئولیت‌ها (Architectural Boundary)
- **مقیاس‌پذیری داخل نود (Intra-Node):** کاملاً بر عهده `sfu-media-node` و **شفاف (Transparent)** از دید `signaling-coordinator` است. Coordinator همچنان برای هر اتاق روی نود فقط یک `logical_router_id` می‌شناسد.
- **تصمیم‌گیری Soft/Hard Threshold:** `sfu-media-node` هرگز خودش درخواست‌های `CreateRouter` یا `CreateTransport` را به خاطر Soft Threshold رد نمی‌کند؛ بلکه وزن کل (`total_capacity_weight`) و درصد پر بودن موثر (`effective_load_percent`) را محاسبه و به `signaling-coordinator` گزارش می‌دهد.

## ۲. معماری Logical Router و سرریز خودکار بین ورکرها (Worker Spillover)
1. **ساختار `LogicalRouter` در `EngineState`:**
   - شناسه روتر اولیه‌ای که در `CreateRouter` ساخته می‌شود، به عنوان `logical_router_id` اتاق عمل می‌کند.
   - هر `LogicalRouter` شامل یک `primary_router: Router` (روی ورکر اولیه) و یک جدول `sub_routers: DashMap<usize, Router>` (نگاشت `worker_index` به روتر فرعی ساخته‌شده برای همان اتاق روی سایر ورکرهای نود) است.
2. **هم‌جواری همزاد (Peer Affinity) در `CreateTransport`:**
   - وقتی `CreateTransport(router_id, peer_id, ...)` فراخوانی می‌شود:
     - اگر برای این `(router_id, peer_id)` قبلاً ترنسپورتی (مثلاً Publisher) ساخته شده است، ترنسپورت دوم (Subscriber) را هم روی **همان ورکر و همان Router فیزیکی** بساز.
     - اگر این اولین ترنسپورتِ این `peer_id` در اتاق است: بررسی کن که آیا بارِ ورکرِ فعلیِ اتاق کمتر از آستانه سرریز ورکر (مثلاً ۸۰٪ ظرفیت یک ورکر) است یا خیر. اگر پر شده بود، کم‌بارترین ورکرِ دیگرِ نود (`worker_pool.get_least_loaded()`) را انتخاب کن؛ اگر برای این اتاق هنوز روتر فرعی روی آن ورکر وجود ندارد، یک `Router` با همان کدک‌های استاندارد روی آن ورکر بساز و در `sub_routers` ثبت کن و ترنسپورت را روی آن ایجاد نما.
3. **پایپینگ خودکار و بدون شرایط مسابقه در `Consume` (Singleflight `pipe_to_router`):**
   - در متد `Consume(router_id, subscriber_transport_id, producer_id)`:
     - روتر فیزیکیِ پرودیوسر ($R_{pub}$) و روتر فیزیکیِ ترنسپورتِ کانسومر ($R_{sub}$) را پیدا کن.
     - اگر $R_{pub}.\text{id}() == R_{sub}.\text{id}()$: مستقیماً روی ترنسپورت `consume` را صدا بزن.
     - اگر $R_{pub}.\text{id}() \neq R_{sub}.\text{id}()$: با استفاده از یک مکانیزم Singleflight (مشابه `IdempotencyCache` روی کلید `format!("{}:{}", producer_id, R_sub.id())`)، پرودیوسر را با متد `R_pub.pipe_to_router(PipeToRouterOptions::new(producer_id, R_sub.clone())).await` به روتر مقصد Pipe کن تا در درخواست‌های همزمان فقط یک بار عملیات Pipe انجام شود.
   - **پاکسازی و انتشار وضعیت:** هنگام `SetMediaPaused` یا `CloseResource` برای یک Producer اصلی، وضعیت Pause/Resume یا Close باید به تمام `PipedProducer`های متناظر در `sub_routers` نیز اعمال شود.

## ۳. فرمول محاسبه وزن و بار ترکیبی نود (`NodeCapacityMetrics`)
1. **وزن کل نود (`total_capacity_weight`):**
   - هر ورکر زنده دارای ظرفیت وزنی پایه (پیش‌فرض قابل تنظیم با `SFU_WORKER_CAPACITY_WEIGHT=1000`) است:
     `total_capacity_weight = alive_workers * worker_capacity_weight`
2. **محاسبه بار منطقی رزروشده (`logical_load_percent`):**
   - وزن منابع فعال روی هر ورکر را به صورت اتمیک نگه دار:
     - `Transport = 1`
     - `AudioProducer = 2`, `VideoProducer = 10`
     - `AudioConsumer = 1`, `VideoConsumer = 2`
   - `logical_load_percent = (sum_of_all_worker_logical_scores / total_capacity_weight) * 100.0` (محدود به بازه `0.0..=100.0`).
3. **محاسبه بار سخت‌افزاری (`cpu_usage_percent`):**
   - در یک تسک پس‌زمینه سبک (هر ۲ ثانیه)، مصرف CPU و RAM را نمونه‌برداری کن.
   - `effective_load_percent = f32::max(logical_load_percent, cpu_usage_percent)`.
4. **انتشار رویداد ظرفیت با Hysteresis:**
   - هرگاه `effective_load_percent` به اندازه حداقل `3.0%` نسبت به آخرین گزارش تغییر کرد (یا هر ۵ ثانیه به صورت Heartbeat)، یک رویداد `NodeEvent { router_id: "".into(), payload: Some(Payload::NodeCapacity(...)) }` در `EventHub` منتشر کن (این رویداد نباید توسط فیلتر Zombie Router حذف شود!).
