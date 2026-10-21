# راهنمای پروژه `signaling-coordinator` (Go Control Plane & Dual Gateway)

این مخزن مسئول مدیریت اتاق‌ها، نشست‌های شرکت‌کنندگان و آداپتورهای سیگنالینگ (`LiveKitAdapter` روی `/rtc` و اسکلت `NativeOrtcAdapter` روی `/ws/ortc`) است.
- **قوانین معماری هگزاگونال و Actor Model:** فایل `.agents/rules.md` را مطالعه و رعایت کن.
- **راهنمای مذاکره SDP و پروتکل LiveKit:** فایل `.agents/skills/code-reviewer/SKILL.md`, `.agents/skills/error-debugging-error-analysis/SKILL.md`, `.agents/skills/golang-pro/SKILL.md`, `.agents/skills/livekit-sdp-synthesizer/SKILL.md`, `.agents/skills/tdd-workflows-tdd-cycle/SKILL.md` را مطالعه کن.
- **همگام‌سازی قرارداد:** فایل `proto/sfu/v1/media_engine.proto` باید دقیقاً با نسخه منبع در `sfu-media-node` یکسان باشد.
