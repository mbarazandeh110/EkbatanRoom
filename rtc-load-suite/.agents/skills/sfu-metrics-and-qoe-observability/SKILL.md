---
name: sfu-metrics-and-qoe-observability
description: الگوهای استخراج آمارهای بلادرنگ WebRTC QoE، ریشه‌یابی تفکیکی گلوگاه‌ها (Signaling vs Media SFU) و ذخیره شواهد ساختاریافته در test-results/loadtest-report.json.
---

# مهارت تخصصی: مشاهده‌پذیری، سنجش کیفیت تجربه (QoE) و تفکیک گلوگاه‌ها

در تست‌های لود یکپارچه (`signaling-coordinator` + `sfu-media-node`)، هدف صرفاً مشاهده شکست یا موفقیت نیست، بلکه تشخیص دقیق محل بروز گلوگاه در زمان افت عملکرد یا افزایش خطاها است.

## ۱. درخت تصمیم تفکیک گلوگاه (Bottleneck Attribution Decision Tree)

هنگامی که تست زیر بار بالا با افت فریم، تأخیر یا قطعی مواجه می‌شود، از ماتریس زیر برای تعیین مقصر و سرویس مقصد در فایل RFC استفاده کن:

| نشانه در کلاینت (`lk`) | وضعیت `signaling-coordinator` | وضعیت `sfu-media-node` | تشخیص ریشه و گلوگاه |
| :--- | :--- | :--- | :--- |
| خطای Connection Timeout یا افت نرخ Join | مصرف بالای CPU، صف طولانی Actor یا تأخیر بالا در HTTP/WS | مصرف CPU ورکرها پایین (< ۳۰٪) | **گلوگاه در `signaling-coordinator`:** قفل‌های هم‌زمانی یا سربار JSON/SDP |
| افت شدید فریم، پکت‌لاست بالا، فریز ویدیو | اتصالات پایدار، تأخیر وب‌سوکت طبیعی | مصرف CPU ورکر مدیاسوپ > ۸۰٪ | **گلوگاه در `sfu-media-node`:** اشباع ورکر مدیاسوپ و نیاز به بهینه‌سازی یا سرریز (Spillover) |
| خطای gRPC DeadlineExceeded در لاگ سرور | تأخیر بالا در فراخوانی‌های gRPC Client | پاسخ‌دهی کند سرور gRPC Rust | **گلوگاه در کانال ارتباطی gRPC:** کمبود استخر اتصال یا مسدود شدن ترد ورکر |
| خطای ICE Connection Failed | توکن صادر شده، کاندیدها مبادله شده | پورت‌های UDP پاسخ نمی‌دهند | **گلوگاه در فایروال یا بازه پورت RTC:** بررسی `RTC_MIN_PORT` / `RTC_MAX_PORT` یا بافر سوکت سیستم‌عامل |

## ۲. معیارهای کیفیت تجربه (WebRTC QoE Metrics)
- **نرخ موفقیت اتصال (Connection Success Rate):** نسبت نشست‌های موفق به کل تلاش‌ها (باید $\ge 99\%$ باشد).
- **پایداری بیت‌ریت ارسالی/دریافتی:** سنجش عدم افت ناگهانی پهنای باند تحت تأثیر BWE در شرایط بار سنگین.
- **تعداد فریم‌های گم‌شده و فریز:** رصد از طریق رویدادهای کلاینت و کانتینرهای مدیا.
- **تأخیر برقراری اتصال (Time-to-First-Frame / Join Latency):** مدت‌زمان شروع از اولین پیام سیگنالینگ تا رندر اولین فریم رسانه (P95 باید کمتر از ۱.۵ ثانیه باشد).

## ۳. فرمت گزارش جامع شواهد (`test-results/loadtest-report.json`)
```json
{
  "scenario_name": "broadcast-1-to-100",
  "start_time": "2026-11-06T10:00:00Z",
  "end_time": "2026-11-06T10:03:00Z",
  "duration_seconds": 180,
  "parameters": {
    "url": "ws://localhost:7880",
    "publishers": 1,
    "subscribers": 100,
    "video_resolution": "720p",
    "simulcast": true
  },
  "client_summary": {
    "total_participants": 101,
    "connection_success_rate": 100.0,
    "average_bitrate_kbps": 1250,
    "total_dropped_tracks": 0
  },
  "server_correlation": {
    "signaling_coordinator": {
      "active_websockets": 101,
      "peak_cpu_percent": 24.3,
      "avg_sdp_negotiation_ms": 18.5
    },
    "sfu_media_node": {
      "effective_load_percent": 42.1,
      "max_worker_cpu_percent": 58.7,
      "active_transports": 101,
      "active_consumers": 100,
      "spillover_triggered": false
    }
  },
  "gate_verdict": "PASSED"
}
```
