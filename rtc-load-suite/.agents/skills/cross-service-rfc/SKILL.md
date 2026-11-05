---
name: cross-service-rfc
description: پروتکل و قالب استاندارد تبادل نیازمندی و گزارش باگ و گلوگاه‌ها بین سرویسها بدون دستکاری مستقیم کدهای یکدیگر.
---

# مهارت: پروتکل تبادل نیازمندی بین‌سرویسی (`<source>.requirements.txt`)

## ۱. قالب نوشتن درخواست در پوشه سرویس مقصد
هنگامی که در اثر لود تست گلوگاهی در `sfu-media-node` یا `signaling-coordinator` کشف شد یا نیاز به تغییر در قرارداد gRPC بود، فایلی با نام `rtc-load-suite.requirements.txt` در ریشه سرویس مقصد با قالب زیر ایجاد یا به‌روزرسانی کن:

```text
===================================================================
RFC / REQUIREMENT ID: REQ-001
SOURCE SERVICE: rtc-load-suite
TARGET SERVICE: sfu-media-node (یا signaling-coordinator)
STATUS: OPEN
===================================================================

1. SUMMARY & TYPE:
   [Bug Fix | Bottleneck Report | Contract Change | Capacity Tuning]

2. REASON & TECHNICAL EVIDENCE:
   - شرح دقیق گلوگاه یا خطای مشاهده شده تحت بار.
   - شواهد عددی: نمودار یا جدول نرخ بسته، درصد پکت‌لاست، تأخیر gRPC، مصرف CPU یا حافظه، و لاگ‌های سرور.

3. EXPECTED BOUNDARY BEHAVIOR:
   - رفتار مورد انتظار در لایه مرزی (gRPC / WebRTC).
   - برای مثال: افزایش ظرفیت صف ورودی، بهبود رفتار الگوریتم سرریز ورکرها، یا تصحیح پاسخ در شرایط اشباع.

4. RESOLUTION & ARCHITECTURAL TRIAGE (توسط سرویس مقصد پر می‌شود):
   - Decision: [PENDING | ACCEPTED & IMPLEMENTED | REJECTED]
   - Architectural Justification: ...
   - Modified Files & Benchmark Verifications: ...
```
