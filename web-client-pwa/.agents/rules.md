# قوانین معماری، SOLID و مهندسی فرانت‌اند (`web-client-pwa`)

## ۱. الزام معماری Clean / Hexagonal و اصول پنج‌گانه SOLID
1. **جداسازی کامل لایه‌ها:** کدها باید دقیقاً بر اساس مهارت `nextjs-clean-hexagonal-solid` در لایه‌های `src/domain`، `src/ports`، `src/application`، `src/adapters`، `src/store` و `src/components` تفکیک شوند.
2. **وارونگی وابستگی و ایزوله‌سازی SDK رسانه (اصل DIP و OCP):**
   - ایمپورت کردن مستقیم `livekit-client` یا `mediasoup-client` در داخل پوشه‌های `src/domain`، `src/store` یا `src/components` اکیداً ممنوع است.
   - تمام کامپوننت‌ها و سرویس‌های کاربردی فقط با اینترفیس‌های تفکیک‌شده در `src/ports/` صحبت می‌کنند و پیاده‌سازی واقعی در `src/adapters/rtc-livekit/` و `src/adapters/rtc-ortc/` قرار می‌گیرد.
3. **تایپ‌اسکریپت سخت‌گیرانه:** فعال بودن `"strict": true` در `tsconfig.json` الزامی است. استفاده از `any`، `@ts-ignore` یا غیرفعال کردن قوانین ESLint ممنوع است.

## ۲. قوانین کارایی بالا در جلسات شلوغ (Zero Re-render Lag)
1. **سلکتورهای اتمی در Zustand:** تغییرات پرتکرار (مانند سطح صدای لحظه‌ای `audioLevel` یا امتیاز کیفیت شبکه `connectionQuality` که چندین بار در ثانیه تغییر می‌کنند) هرگز نباید باعث Re-render شدن کل شبکه ویدیوها (`VideoGrid`) شوند. هر کامپوننت فقط باید با سلکتور اتمی به فیلد خاص همان شرکت‌کننده مشترک شود.
2. **مدیریت المان‌های `<video>` و `<audio>`:** اتصال `MediaStreamTrack` به تگ `<video>` باید در یک هوک اختصاصی (`useMediaTrackAttachment`) با پاکسازی دقیق در زمان Unmount انجام شود تا نشتی حافظه و WebGL/Hardware Decoder در مرورگر رخ ندهد.

## ۳. قوانین پایداری مرورگرهای موبایل (iOS Safari & Android) و بهینه‌سازی شبکه
1. **صفحه اجباری Pre-Join Lobby:** کاربر هرگز نباید بدون کلیک صریح وارد اتاق جلسه شود. در صفحه Pre-Join، پیش‌نمایش دوربین/میکروفون نمایش داده شده و با کلیک کاربر روی دکمه «ورود به جلسه»، `AudioContext.resume()` فراخوانی می‌شود تا قفل Autoplay صدا در iOS Safari باز شود.
2. **ویژگی‌های اجباری تگ ویدیو و قفل صفحه:** تمامی تگ‌های `<video>` باید دارای ویژگی‌های `playsInline` و `autoPlay` باشند (و برای ویدیوی خود کاربر `muted={true}`). همچنین از `navigator.wakeLock` برای جلوگیری از خاموش شدن صفحه موبایل در حین جلسه استفاده شود.
3. **پشتیبانی بومی از Adaptive Stream:** تگ‌های ویدیوی دریافتی باید به `IntersectionObserver` و `ResizeObserver` مجهز باشند تا در صورت خروج از دید (تغییر صفحه Pagination یا اسکرول) یا کوچک شدن کادر، به طور خودکار دستور توقف دریافت (`pause`) یا کاهش لایه کیفیت (`LOW`/`MEDIUM`/`HIGH`) را از طریق پورت رسانه ارسال کنند.
