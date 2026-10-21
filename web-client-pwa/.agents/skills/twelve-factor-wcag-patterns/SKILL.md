---
name: twelve-factor-wcag-patterns
description: الگوهای Twelve-Factor App برای استقرار ابری Next.js، مدیریت وضعیت App Router و استانداردهای دسترس‌پذیری WCAG در اتاق کنفرانس ویدیویی.
---

# مهارت تخصصی: اصول Twelve-Factor، الگوهای Next.js App Router و دسترس‌پذیری (WCAG)

در توسعه `web-client-pwa`، علاوه بر معماری هگزاگونال، استانداردهای زیر را رعایت کن:

## ۱. اصول Twelve-Factor App در فرانت‌اند Next.js
1. **پیکربندی محیطی (Config - Factor III):** هیچ آدرس سرور (`LIVEKIT_WS_URL` یا `ORTC_WS_URL`) را در کد Hardcode نکن. تنظیمات را از طریق متغیرهای محیطی تایپ‌شده و یک اندپوینت زمان اجرا (`/api/config`) بارگذاری کن تا یک ایمیج Docker واحد در محیط‌های Dev، Stage و Prod بدون نیاز به Build مجدد کار کند.
2. **خروجی Standalone و Stateless Processes (Factor VI):** در `next.config.mjs` گزینه `output: 'standalone'` را برای ساخت ایمیج Docker سبک فعال کن و یک مسیر سلامت `/api/healthz` برای پروب‌های Kubernetes (`livenessProbe` / `readinessProbe`) بساز.
3. **لاگ و تله‌متری ساختاریافته (Logs - Factor XI):** خطاهای WebRTC و قطع اتصال‌ها را در یک سرویس لاگر متمرکز با فرمت ساختاریافته ثبت کن، نه با `console.log`های پراکنده.

## ۲. الگوهای Next.js App Router (`nextjs-app-router-patterns`)
1. **مرز دقیق Server و Client Components:** صفحات لندینگ، اطلاعات جلسه و متادیتای SEO را به صورت Server Components نگه دار و فقط درخت اتاق جلسه و پیش‌نمایش رسانه (`Lobby` و `RoomStage`) را با `'use client'` علامت‌گذاری کن.
2. **جلوگیری از خطای Hydration در WebRTC:** دسترسی به `navigator.mediaDevices`، `window`، `AudioContext` و `localStorage` باید منحصراً پس از Mount شدن کامپوننت در کلاینت انجام شود.

## ۳. استانداردهای دسترس‌پذیری و تجربه کاربری (`wcag-audit-patterns` & `ui-ux-designer`)
1. **ناوبری کامل با کیبورد (Keyboard Accessibility):** تمام دکمه‌های نوار کنترل (`ControlBar` شامل میکروفون، دوربین، اشتراک صفحه، دست بلند کردن و خروج) باید دارای `aria-label` گویا، وضعیت `aria-pressed` و حلقه فوکوس واضح (`focus-visible:ring-2`) باشند.
2. **اعلان‌های زنده صفحه‌خوان (`ARIA Live Regions`):** یک ناحیه مخفی با `aria-live="polite"` برای اعلام رویدادهای مهم اتاق (ورود/خروج شرکت‌کننده، قطع شدن میکروفون یا افت کیفیت شبکه) قرار بده.
3. **پشتیبانی از کلیدهای میانبر استاندارد:** میانبرهای کیبورد برای قطع/وصل سریع میکروفون و دوربین (در زمانی که فوکوس داخل فیلد متنی چت نیست) تعبیه کن.
