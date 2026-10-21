# راهنمای پروژه `sfu-media-node` (Rust Media Plane)

این مخزن هسته رسانه‌ای مستقل مبتنی بر Rust و `mediasoup` است و منبع اصلی قرارداد `proto/sfu/v1/media_engine.proto` محسوب می‌شود.
- **قوانین کامل معماری، اعتبارسنجی مرزی و Idempotency:** حتماً فایل `.agents/rules.md` را مطالعه و رعایت کن.
- **جزئیات فنی Mediasoup:** حتماً فایل `.agents/skills/code-review-and-quality/SKILL.md`, `.agents/skills/code-showcase-systematic-debugging/SKILL.md`, `.agents/skills/rust-mediasoup-sfu/SKILL.md`, `.agents/skills/rust-pro/SKILL.md`, `.agents/skills/tdd-workflows-tdd-cycle/SKILL.md` را مطالعه کن.
- **گردش‌کار اجباری:** پیش از نوشتن کد، مراحل `# Engineering Workflow` در فایل `first-prompt.txt` را مو‌به‌مو اجرا کن.
