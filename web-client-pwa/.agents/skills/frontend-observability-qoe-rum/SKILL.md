---
name: frontend-observability-qoe-rum
description: الگوی پیاده‌سازی تله‌متری سمت کلاینت، گزارش‌دهنده هوشمند کیفیت تجربه WebRTC (QoE)، سانسور داده‌های حساس و مانیتورینگ SSR در Next.js بر پایه معماری هگزاگونال.
---

# مهارت تخصصی: Observability، RUM و پایش هوشمند QoE در فرانت‌اند (`web-client-pwa`)

در طراحی و پیاده‌سازی `web-client-pwa`، لایه مشاهده‌پذیری و تله‌متری را مطابق معماری هگزاگونال و اصول زیر پیاده کن:

## ۱. پورت انتزاعی تله‌متری (`src/ports/telemetry_port.ts` - رعایت اصل DIP و SRP)
1. هیچ کامپوننت یا کلاسی نباید مستقیماً از `console.log` یا SDKهای خاص مانیتورینگ استفاده کند. یک پورت تمیز به نام `TelemetryPort` تعریف کن:
   - `log(level, eventName, attributes)`
   - `startSpan(operationName, attributes)`
   - `recordQoEMetric(snapshot: WebRtcQoESnapshot)`
2. در `src/adapters/telemetry/` یک پیاده‌سازی قابل کانفیگ بساز که رفتار خود را از متغیرهای محیطی (`NEXT_PUBLIC_OBS_ENABLED`, `NEXT_PUBLIC_OBS_LOG_LEVEL`, `NEXT_PUBLIC_OBS_SAMPLE_RATE`) بخواند.

## ۲. ثبت انتخابی و نمونه‌برداری ناهنجاری‌های کیفیت رسانه (Anomaly-Driven QoE Reporting)
برای جلوگیری از مصرف CPU مرورگر و ترافیک شبکه کاربر:
1. وضعیت کیفیت شبکه و رسانه را در فواصل زمانی قابل تنظیم (مثلاً هر ۵ یا ۱۰ ثانیه) بررسی کن.
2. **ثبت مبتنی بر آستانه خطا (Threshold/Anomaly Logging):** در حالت عادی که تماس بدون مشکل برقرار است، لاگ‌های تکراری نفرست! فقط زمانی لاگ هشدار/متریک کامل ارسال کن که یکی از شرایط زیر رخ دهد:
   - نرخ گم شدن بسته‌ها (`packetLossRate`) از ۲٪ عبور کند.
   - تأخیر (`RTT`) بالای ۲۵۰ میلی‌ثانیه برود یا فریز شدن تصویر (`freezeCount`) رخ دهد.
   - تغییر وضعیت اتصال (`Reconnecting`, `IceRestart`, `PermissionDenied`, `AudioContextBlocked`) اتفاق بیفتد.

## ۳. امنیت و حریم خصوصی در لاگ‌های مرورگر (PII & Token Sanitization)
- آداپتور تله‌متری باید به صورت خودکار تمام پارامترهای Query String حاوی `access_token`، هدرهای `Authorization` و کلیدهای محلی را قبل از ثبت در کنسول یا ارسال به سرور مانیتورینگ حذف (`[REDACTED]`) کند.

## ۴. مشاهده‌پذیری سمت سرور Next.js (SSR & Health Endpoint)
- در مسیرهای سرور (`/api/token` و `/api/healthz`)، لاگ‌های JSON ساختاریافته به همراه هدر `X-Request-ID` / `traceparent` ثبت کن تا درخواست‌های صدور توکن و وضعیت سلامت کانتینر در Kubernetes و Elasticsearch قابل ردیابی باشند[cite: 6].
