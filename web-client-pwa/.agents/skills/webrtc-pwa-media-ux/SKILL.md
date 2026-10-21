---
name: webrtc-pwa-media-ux
description: استانداردهای فنی WebRTC در مرورگر، پایداری iOS Safari، پیاده‌سازی Adaptive Stream، Simulcast و پردازش سیگنال سمت کلاینت (DSP).
---

# مهارت تخصصی: مهندسی رسانه WebRTC، پایداری موبایل و Adaptive Stream

هنگام پیاده‌سازی لایه رسانه و کامپوننت‌های ویدیویی، الزامات فنی زیر را رعایت کن:

## ۱. پایداری ۱۰۰٪ روی iOS Safari و مرورگرهای موبایل
1. **فرآیند Pre-Join Lobby و باز کردن قفل صدا:**
   - در رویداد `onClick` دکمه «ورود به اتاق (Join Room)»، ابتدا یک نمونه `AudioContext` را `resume()` کن تا مرورگر اجازه پخش تمام ترک‌های صوتی دریافتی را بدهد.
   - قبل از ورود به اتاق اصلی، ترک‌های پیش‌نمایش محلی (Preview Tracks) را یا به نشست اصلی منتقل کن یا به طور تمیز `stop()` کن تا دوربین در سیستم‌عامل‌های موبایل قفل نشود.
2. **مدیریت WakeLock و تغییر شبکه:**
   - هنگام ورود به اتاق، `navigator.wakeLock.request('screen')` را فعال کن و در رویداد `visibilitychange` (وقتی کاربر به تب برمی‌گردد) آن را مجدداً درخواست نما.

## ۲. پیاده‌سازی دقیق Adaptive Stream (مدیریت مصرف پهنای باند سمت کلاینت)
برای هر `VideoTile` مربوط به شرکت‌کنندگان دیگر، هوک `useAdaptiveVideoViewport` را متصل کن:
1. **`IntersectionObserver`:** اگر کادر ویدیو از دید خارج شد (`isIntersecting === false`، مثلاً در حالت Pagination یا مخفی شدن تب)، پس از یک تأخیر کوتاه (Debounce مثلاً ۳۰۰ میلی‌ثانیه)، متد `remoteSubscriptionPort.setTrackEnabled(trackId, false)` را صدا بزن تا دریافت ویدیو متوقف شود.
2. **`ResizeObserver`:** بر اساس عرض رندرشده کادر ویدیو در DOM (`contentRect.width`):
   - عرض کمتر از `280px` -> درخواست لایه کیفیت `VideoQualityLayer.LOW` (مثلاً 180p).
   - عرض بین `280px` تا `640px` -> درخواست لایه کیفیت `VideoQualityLayer.MEDIUM` (مثلاً 360p).
   - عرض بیشتر از `640px` (یا حالت Pin/Fullscreen) -> درخواست لایه کیفیت `VideoQualityLayer.HIGH` (مثلاً 720p/1080p).
3. در `LiveKitEngineAdapter`، این دستورات مستقیماً به متدهای رسمی `publication.setEnabled()` و `publication.setVideoQuality()` در `livekit-client` نگاشت می‌شوند (و در `MediasoupOrtcEngineAdapter` به پیام‌های سیگنالینگ `pause` و `setPreferredLayers` نگاشت می‌گردند).

## ۳. انتشار با Simulcast و معماری پلاگین‌پذیر DSP
- هنگام روشن کردن دوربین، انتشار Simulcast سه‌لایه را به صورت پیش‌فرض فعال کن.
- در پورت `MediaDevicesPort` یک خط لوله پردازشگر (`MediaTrackProcessorPort`) در نظر بگیر تا بتوان فیلتر حذف نویز (WebAudio / RNNoise AudioWorklet) و پس‌زمینه مجازی را بدون درگیر کردن کدهای UI روی ترک محلی اعمال کرد.
