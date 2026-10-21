---
name: test-observability-qoe-correlation
description: الگوی ثبت لاگ‌های ساختاریافته، همبستگی شناسه تریس (Trace Correlation) بین کلاینت و سرور، و استخراج شاخص‌های کیفیت تجربه (QoE) در تست‌های سیاه‌جعبه.
---

# مهارت تخصصی: مشاهده‌پذیری تست، همبستگی سیگنالینگ و سنجش QoE در `rtc-compatibility-suite`

برای اینکه در صورت شکست هر یک از ۵ دروازه فاز صفر بتوان در چند ثانیه ریشه خطا را در سرویس Go یا Rust پیدا کرد، قابلیت‌های زیر را در لایه `src/reporters/` و `src/controllers/` پیاده کن:

## ۱. همبستگی تریس و تایم‌لاین سیگنالینگ (Signaling & Trace Correlation)
1. به ازای هر تست و هر شرکت‌کننده، یک شناسه همبستگی یکتا (`correlationId` / `traceId`) تولید کن و آن را در متادیتای اتصال یا لاگ‌های تست ثبت نما.
2. در `ParticipantClientController`، با استفاده از قابلیت شنود فریم‌های WebSocket در Playwright (`page.on('websocket', ws => ...)`), زمان دقیق ارسال و دریافت فریم‌های سیگنالینگ (`/rtc`)، اندازه بایت و وضعیت اتصال را (بر اساس تنظیم `OBS_LOG_LEVEL`) ثبت کن تا در صورت بروز Timeout در مذاکره SDP، دقیقاً مشخص باشد آخرین پیام از کدام سمت ارسال شده است.

## ۲. استخراج شاخص‌های عمیق کیفیت رسانه (WebRTC QoE Metrics)
در متد استخراج آمار `RTCPeerConnection.getStats()`، علاوه بر `bytesSent` و `bytesReceived`، متریک‌های استاندارد QoE زیر را نیز استخراج و در `test-results/evidence-report.json` ذخیره کن:
- **شبکه و انتقال:** `roundTripTime` (RTT بر حسب میلی‌ثانیه)، `availableOutgoingBitrate`، و وضعیت جفت کاندیدای فعال (`selectedCandidatePair`).
- **کیفیت صوت و تصویر:** `packetsLost`، `jitter`، تعداد درخواست‌های بازپخش و فریم کلیدی (`nackCount`, `pliCount`, `firCount`)، `framesDecoded`، `framesDropped` و رزولوشن فعلی (`frameWidth` x `frameHeight`).

## ۳. لاگینگ قابل کانفیگ در اجرای تست
- اگر متغیر محیطی `OBS_CAPTURE_BROWSER_LOGS=true` فعال بود، لاگ‌های کنسول مرورگر (`page.on('console')` و `page.on('pageerror')`) را با حذف توکن‌های حساس در فایل شواهد همان دروازه ذخیره کن.
