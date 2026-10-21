---
name: frontend-tdd-systematic-debugging
description: گردش‌کار مهندسی مبتنی بر تست (TDD)، عیب‌یابی سیستماتیک و بازبینی کد فرانت‌اند (اقتباس‌شده از مخازن agentic-awesome-skills و antigravity-skills).
---

# مهارت تخصصی: TDD، عیب‌یابی سیستماتیک و بازبینی معماری در فرانت‌اند

بر اساس بهترین الگوهای مهندسی در `agentic-awesome-skills` و `antigravity-skills`[cite: 5]، قوانین زیر را در حین توسعه رعایت کن:

## ۱. توسعه مبتنی بر تست (Frontend TDD Workflow)
1. **تست‌پذیری بدون نیاز به سرور واقعی در Unit Tests:**
   - به لطف معماری هگزاگونال و پورت `RtcEnginePort`، یک کلاس `MockRtcEngineAdapter` بساز که بتواند رویدادهای ورود/خروج شرکت‌کننده، انتشار ترک، تغییر سطح صدا و قطع شبکه را در حافظه شبیه‌سازی کند.
   - برای `RoomSessionController`، `AdaptiveViewportManager` و استورهای Zustand با استفاده از **Vitest** و **React Testing Library** تست واحد بنویس.
2. **ممنوعیت تست‌های نمایشی (No Fake Assertions):**
   - هر تست باید رفتار واقعی ماشین وضعیت (مثلاً تغییر وضعیت از `Lobby` به `Connected`، یا تغییر لایه کیفیت از `HIGH` به `LOW` هنگام تغییر سایز کادر) را بررسی کند.

## ۲. عیب‌یابی سیستماتیک (Systematic Debugging)
در صورت بروز خطا در کامپایل، تایپ‌چکینگ یا تست‌ها:
1. ابتدا پیام خطا و Stack Trace کامل را بخوان؛ هرگز با حدس و گمان یا حذف تایپ‌ها کد را تغییر نده.
2. بررسی کن که آیا خطا ناشی از نقض مرز لایه‌ها، چرخه حیات هوک‌های React (`useEffect` cleanup) یا عدم تطابق نسخه پین‌شده `livekit-client` است.
3. پس از هر اصلاح، دستورات `npm run typecheck`، `npm run lint` و `npm run test` را مجدداً اجرا کن و خروجی واقعی ترمینال را گزارش بده[cite: 5].

## ۳. چک‌لیست بازبینی کد (Architectural Code Review Checklist)
پیش از اتمام هر برش عمودی (Vertical Slice)، کدها را با این چک‌لیست کنترل کن:
- [ ] آیا هیچ ایمپورتی از `livekit-client` یا `mediasoup-client` به داخل `domain`، `store` یا `components` نشت نکرده است؟
- [ ] آیا کامپوننت‌های `VideoTile` از سلکتورهای اتمی Zustand استفاده می‌کنند تا در زمان تغییر `audioLevel` رندر اضافی رخ ندهد؟
- [ ] آیا تمام ترک‌های رسانه و Observerها در زمان Unmount شدن کامپوننت پاکسازی (`disconnect` / `detach`) می‌شوند؟