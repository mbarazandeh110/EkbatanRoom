# راهنمای پروژه `loadtest-sfu` (E2E WebRTC Load Testing Suite with `livekit-cli`)

این مخزن پلتفرم مستقل ارکستراسیون، سنجش کارایی، بنچ‌مارک و لود تست سرتاسری (E2E) پشته رسانه‌ای EkbatanRoom (شامل `signaling-coordinator` و `sfu-media-node`) مبتنی بر ابزار رسمی **`livekit-cli` (`lk load-test`)** است.

- **قوانین کامل معماری، تست سیاه‌جعبه واقعی و گیت‌بندی ترتیبی:** حتماً فایل `.agents/rules.md` را مطالعه و رعایت کن.
- **مهارت‌های الزامی این پروژه:** پیش از اجرا یا ایجاد سناریو، فایل‌های مهارتی زیر را بررسی کن:
  - `.agents/skills/livekit-load-testing/SKILL.md` (پارامترهای lk load-test، سیمولکست، کدک‌ها و الگوهای بار)
  - `.agents/skills/e2e-load-orchestration-and-harness/SKILL.md` (مدیریت فرآیندها، اجرای ترتیبی گیت‌ها و Fail-Fast)
  - `.agents/skills/sfu-metrics-and-qoe-observability/SKILL.md` (درخت تصمیم تفکیک گلوگاه، شاخص‌های QoE و گزارش شواهد)
  - `.agents/skills/cross-service-rfc/SKILL.md` (پروتکل استاندارد تبادل نیازمندی و گزارش گلوگاه به سرویس‌های دیگر)
  - `.agents/skills/systematic-debugging/SKILL.md` (متدولوژی ۴ مرحله‌ای ریشه‌یابی خطاها)
  - `.agents/skills/twelve-factor-engineering/SKILL.md` (اصول دوازده عاملی مهندسی نرم‌افزار)
  - `.agents/skills/code-reviewer/SKILL.md` (استانداردهای کیفیت کد و ممیزی مهندسی)
- **دستورات کلیدی Makefile:**
  - `make install-lk`: دانلود و استقرار باینری رسمی `lk` در پوشه محلی `bin/`.
  - `make run-baseline`: اجرای سناریوی پایه اعتبارسنجی (گیت ۱).
  - `make run-broadcast`: اجرای سناریوی پخش زنده با ۱ ناشر و ۵۰ تا ۱۰۰ شنونده (گیت ۲).
  - `make run-conference`: اجرای سناریوی جلسه گروهی تعاملی (گیت ۳).
  - `make run-all`: اجرای ترتیبی تمامی گیت‌ها همراه با پایش سرورها و ذخیره شواهد.
- **ذخیره شواهد:** هر اجرای آزمون باید گزارش ساختاریافته نهایی را در `test-results/loadtest-report.json` ذخیره کند.
