---
name: rust-mediasoup-sfu
description: راهنمای عملیاتی پیاده‌سازی mediasoup در Rust، تنظیمات شبکه/NAT، مدیریت Worker، اعتبارسنجی کدک‌ها و استخراج آمار RTP.
---

# مهارت تخصصی: پیاده‌سازی گام‌به‌گام Mediasoup در Rust

1. **بررسی API نسخه Pin شده:** پیش از پیاده‌سازی، امضای دقیق متدهای کریت `mediasoup` نصب‌شده در `Cargo.lock` را بررسی کن.
2. **شروع با تک‌ورکر و قابلیت گسترش به Worker-per-Core:**
   - در گام اول (Vertical Slice 1)، راه‌اندازی یک `Worker` و یک `WebRtcServer` پایدار با `ListenInfo` (شامل `ip` و `announced_address` برای محیط Docker/NAT) اولویت دارد. سپس انتخاب کم‌بارترین Worker از استخر Workerها را فعال کن.
3. **پیکربندی کدک‌های استاندارد Router:**
   - کدک‌های `audio/opus` (48000Hz, 2ch)، `video/VP8` (90000Hz) و `video/H264` (`packetization-mode=1`, `profile-level-id=42e01f`) را به همراه بازخوردهای RTCP (`nack`, `nack pli`, `ccm fir`, `transport-cc`) تنظیم کن.
4. **بررسی اجباری `can_consume` و استخراج لایه‌ها:**
   - در متد `Consume`، پس از پارس کردن `RtpCapabilities`، حتماً `router.can_consume(&producer_id, &caps)` را بررسی کن.
   - روی هر `Consumer` و `Producer`، کال‌بک‌های `on_score` و `on_layers_change` را ثبت کن تا رویدادهای `MediaScoreEvent` و `LayerChangedEvent` با جزئیات کامل لایه‌های فعلی و ترجیحی ارسال شوند.
5. **پیاده‌سازی دقیق `GetStats`:**
   - در متد `GetStats`، خروجی `get_stats().await` را از شیء مربوطه بگیر و مقادیر واقعی `bytes_sent`، `bytes_received`، `packets_sent` و `packets_received` را استخراج کن.
