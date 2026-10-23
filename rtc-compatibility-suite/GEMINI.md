# قوانین پروژه تست سازگاری سیاه‌جعبه (`rtc-compatibility-suite`)

## ۱. تست سیاه‌جعبه واقعی (Strict Black-Box Verification)
1. **نسخه Pin شده و بدون تغییر:** در `package.json` یک نسخه دقیق از `livekit-client` رسمی (مثلاً `2.x.x`) را بدون `^` یا `~` پین کن. هیچ‌گونه Patch یا دستکاری روی SDK کلاینت مجاز نیست.
2. **ممنوعیت قبولی کاذب:** صرفِ اتصال وب‌سوکت برای قبولی تست رسانه کافی نیست؛ باید از طریق `RTCPeerConnection.getStats()` اثبات شود که `bytesSent > 0`، `bytesReceived > 0` و فریم‌های ویدیو در المان `<video>` رندر شده‌اند (`videoWidth > 0`).

## ۲. وابستگی مرحله‌ای تست‌ها (Sequential Gating) و ذخیره شواهد (Evidence Artifacts)
1. **اجرای ترتیبی و توقف در صورت شکست پیش‌نیاز:**
   - تست‌ها باید با `test.describe.configure({ mode: 'serial' })` یا وابستگی پروژه در Playwright تنظیم شوند.
   - اگر **تست مرحله ۱ (اتصال و Join)** شکست خورد، تست‌های مرحله ۲ تا ۵ نباید اجرا شده و خطای گمراه‌کننده تولید کنند؛ وضعیت آن‌ها باید به صورت صریح `skipped / blocked` ثبت شود. اگر مرحله ۲ (عبور پایه RTP) شکست خورد، مراحل ۳ تا ۵ باید `blocked` شوند.
2. **ذخیره اجباری شواهد تست در `test-results/evidence-report.json`:**
   - هر بار اجرای تست باید یک فایل گزارش ساختاریافته تولید کند که شامل موارد زیر باشد:
     - نسخه دقیق `livekit-client`، نسخه مرورگر Chromium، و هش Commit یا وضعیت سرویس‌ها.
     - لاگ‌های سیگنالینگ، وضعیت ICE/DTLS، مقادیر واقعی `bytesSent` / `bytesReceived` و علت دقیق شکست یا مسدود شدن هر دروازه.
3. **مطالعه قواعد**: حتما فایل های `.agents/rules.md`, `SIGNALING_CONTRACT.md`, `.agents/skills/clean-test-architecture-solid/SKILL.md`, `.agents/skills/code-reviewer/SKILL.md`, `.agents/skills/context-driven-development/SKILL.md`, `.agents/skills/e2e-testing-patterns/SKILL.md`, `.agents/skills/microservices-patterns/SKILL.md`, `.agents/skills/playwright-skill/SKILL.md`, `agents/skills/test-observability-qoe-Correlation/SKILL.md`, `.agents/skills/systematic-debugging/SKILL.md`, `.agent/skills/cross-service-rfc/SKILL.md`, `.agents/skills/tdd-workflows-tdd-cycle/SKILL.md`, `.agents/skills/twelve-factor-engineering/SKILL.md`, و `.agents/skills/webrtc-gate-testing/SKILL.md` را مطالعه کن.
