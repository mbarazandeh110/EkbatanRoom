---
name: clean-test-architecture-solid
description: الگوی معماری تمیز تست (Clean Test Architecture)، اصول SOLID و تفکیک لایه‌های Harness، Controller، استخراج آمار WebRTC و گزارش شواهد در Playwright.
---

# مهارت تخصصی: معماری تمیز و اصول SOLID در مجموعه تست E2E WebRTC

در طراحی و پیاده‌سازی `rtc-compatibility-suite`، از نوشتن منطق مستقیم مرورگر، تولید توکن یا پارس کردن آمار `getStats()` در داخل فایل‌های `.spec.ts` خودداری کن و معماری ۴ لایه‌ای زیر را بر اساس اصول SOLID پیاده کن:

## ۱. ساختار دایرکتوری‌ها (Clean Test Architecture)
```text
rtc-compatibility-suite/
├── harness-app/                   # لایه ۱: اپلیکیشن وب سیاه‌جعبه (Vite + Official Pinned livekit-client)
│   ├── index.html
│   └── src/
│       └── main.ts                # فقط اتصال کلاینت رسمی LiveKit و قرار دادن کنترلر استاندارد روی window
├── src/
│   ├── domain/                    # لایه ۲: مدل‌های تایپ‌شده شواهد، آمار RTP و وضعیت دروازه‌ها
│   │   └── types.ts               # RtpMetricsSnapshot, GateEvidence, GateStatus ('PASSED'|'FAILED'|'BLOCKED')
│   ├── controllers/               # لایه ۳: کنترلرهای انتزاعی تعامل با کلاینت (Page/Participant Object Model)
│   │   ├── participant_client.ts  # کلاس کنترل یک شرکت‌کننده در Playwright (join, publish, mute, setQuality, getStats)
│   │   └── token_factory.ts       # تولیدکننده مستقل توکن JWT استاندارد LiveKit
│   └── reporters/                 # لایه ۴: جمع‌آوری شواهد و تولید گزارش ساختاریافته
│       └── evidence_recorder.ts   # ثبت لاگ‌های سیگنالینگ، آمار بایت‌های RTP و ذخیره در evidence-report.json
└── tests/                         # لایه ۵: سناریوهای تست خوانا و مرحله‌ای (Declarative Gate Specs)
    ├── 01-connection.spec.ts
    ├── 02-bidirectional-rtp.spec.ts
    ├── 03-concurrency-glare.spec.ts
    ├── 04-simulcast-layers.spec.ts
    └── 05-ice-restart.spec.ts
