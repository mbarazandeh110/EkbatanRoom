---
name: livekit-load-testing
description: راهنمای جامع اجرای لود تست با ابزار رسمی livekit-cli (دستور lk load-test) برای سنجش پایداری و مقیاس‌پذیری پشته EkbatanRoom.
---

# مهارت تخصصی: لود تست پیشرفته با `livekit-cli` (`lk load-test`)

این مهارت اصول، فلگ‌های کنترلی، الگوهای سناریوسازی و تنظیمات سیستم‌عامل برای اجرای لود تست‌های سنگین با استفاده از ابزار رسمی `livekit-cli` را علیه سرویس `signaling-coordinator` (که به `sfu-media-node` متصل است) تشریح می‌کند.

## ۱. نصب و استقرار باینری `lk`
ابزار رسمی لایوکیت به صورت خودکار در مسیر محلی `bin/lk` پروژه قابل دریافت است:
```bash
# نصب بدون نیاز به دسترسی روت در پوشه bin
mkdir -p bin
LK_INSTALL_PATH=./bin bash <(curl -sSL https://get.livekit.io/cli)
```

## ۲. مشخصات و فلگ‌های کلیدی `lk load-test`

| فلگ | پیش‌فرض | شرح و کاربرد |
| :--- | :--- | :--- |
| `--url` | - | آدرس وب‌سوکت سیگنالینگ، مثلاً `ws://localhost:7880` |
| `--api-key` | - | کلید دسترسی API برای تولید خودکار توکن JWT (مطابق با کانفیگ `signaling-coordinator`) |
| `--api-secret` | - | کلید محرمانه API جهت امضای توکن |
| `--room` | `load-test` | نام اتاق مقصد برای اتصال شرکت‌کنندگان |
| `--publishers` | `1` | تعداد کلاینت‌های تولیدکننده استریم صوت و تصویر |
| `--subscribers` | `0` | تعداد کلاینت‌های شنونده/بیننده |
| `--duration` | `1m` | مدت زمان اجرای سناریوی بار (مثلاً `2m` یا `10m`) |
| `--video-resolution` | `720p` | رزولوشن ویدیوی ناشران: `1080p`، `720p`، `360p` یا `180p` |
| `--video-codec` | `vp8` | کدک ویدیویی ارسالی (`vp8` یا `h264`) |
| `--simulcast` | `false` | فعال‌سازی لایه‌های سه‌گانه Simulcast (تست انتخاب کیفیت و تطبیق پهنای باند) |
| `--audio-only` | `false` | ارسال فقط صوت Opus بدون تصویر (مناسب تست تالارهای گفتگو / Audio Spaces) |
| `--layout` | - | آرایش شرکت‌کنندگان در اتاق‌ها |

## ۳. الگوهای استاندارد سناریوهای لود تست

### ۳.۱ سناریوی پخش زنده و وبینار (1-to-N Broadcast)
- **هدف:** بررسی توان Fan-out سرور و هزینه پردازش کانسومرهای پرشمار روی یک استریم واحد.
- **دستور نمونه:**
  ```bash
  ./bin/lk load-test \
    --url ws://localhost:7880 \
    --api-key devkey --api-secret secret \
    --room broadcast-room \
    --publishers 1 \
    --subscribers 100 \
    --video-resolution 720p \
    --simulcast \
    --duration 3m
  ```

### ۳.۲ سناریوی جلسه تعاملی گروهی (N-to-N Group Meeting)
- **هدف:** تولید بار دوطرفه صوتی/تصویری، بررسی رقابت داده SDP (Glare) و توان پردازش موازی مدیاسوپ ($N \times (N-1)$ جریان).
- **دستور نمونه (۱۰ شرکت‌کننده تماماً ناشر و گیرنده):**
  ```bash
  ./bin/lk load-test \
    --url ws://localhost:7880 \
    --api-key devkey --api-secret secret \
    --room meeting-room \
    --publishers 10 \
    --subscribers 10 \
    --video-resolution 360p \
    --duration 2m
  ```

### ۳.۳ سناریوی سرریز ظرفیت ورکرها (Worker Spillover & Stress)
- **هدف:** افزایش تدریجی بار تا عبور از آستانه ۸۰٪ ورکر اولیه و تحریک ساخت Sub-Router و PipeTransport بین ورکرهای داخلی `sfu-media-node`.

## ۴. تنظیمات بهینه‌سازی لینوکس جهت تولید بار سنگین
پیش از شروع آزمون‌های فراتر از ۱۰۰ اتصال، سیستم تولید بار باید با تنظیمات زیر آماده شود:
```bash
# افزایش سقف توصیف‌کننده‌های فایل (File Descriptors)
ulimit -n 65535

# افزایش بافرهای سوکت UDP در هسته لینوکس
sudo sysctl -w net.core.rmem_max=26214400
sudo sysctl -w net.core.wmem_max=26214400
sudo sysctl -w net.core.rmem_default=26214400
sudo sysctl -w net.core.wmem_default=26214400
```
