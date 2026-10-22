---
name: cross-service-rfc
description: پروتکل و قالب استاندارد تبادل نیازمندی و گزارش باگ بین سرویسها بدون دستکاری مستقیم کدهای یکدیگر.
---

# مهارت: پروتکل تبادل نیازمندی بین‌سرویسی (`<source>.requirements.txt`)

## ۱. قالب نوشتن درخواست در پوشه سرویس مقصد
هنگامی که نیاز به تغییر در سرویس دیگری داری، فایلی با نام `<your-service-name>.requirements.txt` در ریشه سرویس مقصد با قالب زیر ایجاد کن:

```text
===================================================================
RFC / REQUIREMENT ID: REQ-001
SOURCE SERVICE: <نام سرویس درخواست‌دهنده، مثلاً rtc-compatibility-suite>
TARGET SERVICE: <نام سرویس مقصد، مثلاً signaling-coordinator>
STATUS: OPEN
===================================================================

1. SUMMARY & TYPE:
   [Bug Fix | Contract Change | New Capability | Behavior Adjustment]

2. REASON & TECHNICAL EVIDENCE:
   - چرا به این تغییر نیاز است؟
   - لاگ دقیق خطا، خروجی تست یا بخش مرتبط از قرارداد که دچار مشکل است.

3. EXPECTED BOUNDARY BEHAVIOR:
   - سرویس مقصد در مرز ورودی/خروجی خود (gRPC / WebSocket / HTTP) چه رفتاری باید نشان دهد؟
   - (توجه: نحوه پیاده‌سازی داخلی بر عهده سرویس مقصد است).

4. RESOLUTION & ARCHITECTURAL TRIAGE (توسط سرویس مقصد پر می‌شود):
   - Decision: [PENDING | ACCEPTED & IMPLEMENTED | REJECTED]
   - Architectural Justification: ...
   - Modified Files & Added Tests: ...