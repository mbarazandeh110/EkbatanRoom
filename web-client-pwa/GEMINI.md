# راهنمای پروژه `web-client-pwa` (Next.js Enterprise WebRTC PWA)

این مخزن کلاینت وب و PWA رسمی پلتفرم کنفرانس ویدیویی است که با **Next.js (App Router)، TypeScript، Zustand، Tailwind CSS و shadcn/ui** ساخته می‌شود.
1. **معماری دوگانه (Dual-Protocol Hexagonal Frontend):** رابط کاربری و استورهای وضعیت کاملاً مستقل از SDK رسانه هستند و از طریق پورت `RtcEnginePort` می‌توانند هم با آداپتور `livekit-client` (`/rtc`) و هم با آداپتور `mediasoup-client` (`/ws/ortc`) کار کنند.
2. **مطالعه اجباری پیش از هرگونه کدنویسی:**
   - قوانین و خط قرمزهای معماری و کارایی: `.agents/rules.md`
   - مهارت معماری Clean/Hexagonal و SOLID در فرانت‌اند: `.agents/skills/nextjs-clean-hexagonal-solid/SKILL.md`
   - مهارت مهندسی WebRTC، موبایل/Safari و Adaptive Stream: `.agents/skills/webrtc-pwa-media-ux/SKILL.md`
   - مهارت TDD، دیباگ سیستماتیک و بازبینی کد: `.agents/skills/frontend-tdd-systematic-debugging/SKILL.md`
   - گردش‌کار مرحله‌به‌مرحله: `first-prompt.txt`
3. حتماً فایل های ` .agents/skills/frontend-observability-qoe-rum/SKILL.md`, `SIGNALING_CONTRACT.md`, ` .agents/skills/frontend-tdd-systematic-debugging/SKILL.md`, `.agents/skills/cross-service-rfc/SKILL.md`, ` .agents/skills/nextjs-app-router-patterns/SKILL.md`, ` .agents/skills/nextjs-clean-hexagonal-solid/SKILL.md`, ` .agents/skills/react-state-management/SKILL.md`, ` .agents/skills/senior-frontend/SKILL.md`, ` .agents/skills/twelve-factor-wcag-patterns/SKILL.md`, ` .agents/skills/ui-ux-designer/SKILL.md`, ` .agents/skills/wcag-audit-patterns/SKILL.md`, ` .agents/skills/webrtc-pwa-media-ux/SKILL.md` را مطالعه کن.