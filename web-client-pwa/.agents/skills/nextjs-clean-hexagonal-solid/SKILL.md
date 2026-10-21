---
name: nextjs-clean-hexagonal-solid
description: الگوی پیاده‌سازی معماری Clean/Hexagonal، اصول پنج‌گانه SOLID، مدیریت وضعیت Zustand و آداپتور دوگانه WebRTC در Next.js و TypeScript.
---

# مهارت تخصصی: معماری Clean/Hexagonal و اصول SOLID در Next.js

هنگام طراحی و کدنویسی `web-client-pwa`، ساختار دایرکتوری و اصول زیر را به صورت دقیق اجرا کن:

## ۱. ساختار دایرکتوری‌ها (Hexagonal Frontend Layout)
جهت وابستگی‌ها همیشه رو به داخل است (`components/adapters -> application/store -> ports -> domain`):
```text
src/
├── domain/                        # لایه ۱: مدل‌های خالص دامنه (Pure TypeScript - بدون وابستگی به React یا SDKهای WebRTC)
│   ├── models/
│   │   ├── room.ts                # RoomState, ConnectionStatus, LayoutMode ('grid' | 'speaker' | 'presentation')
│   │   ├── participant.ts         # ParticipantModel, ParticipantRole, ConnectionQualityLevel
│   │   └── media.ts               # TrackDescriptor, TrackSource ('camera'|'microphone'|'screen'), VideoQualityLayer
│   └── errors.ts                  # خطاهای تایپ‌شده دامنه (MediaPermissionError, SignalingError)
├── ports/                         # لایه ۲: قراردادهای ورودی و خروجی بر پایه اصل ISP و DIP
│   ├── rtc_engine_port.ts         # RtcConnectionPort, LocalMediaControlPort, RemoteSubscriptionPort, RtcEventsPort
│   └── media_devices_port.ts      # قرارداد مدیریت سخت‌افزار دوربین/میکروفون/بلندگو و پردازش DSP
├── application/                   # لایه ۳: منطق کاربردی و هماهنگ‌کننده‌ها (Use Cases)
│   ├── room_session_controller.ts # هماهنگ‌کننده اتصال، Pre-Join، مدیریت خطاها و همگام‌سازی رویدادهای پورت با Store
│   └── adaptive_viewport_manager.ts # منطق محاسبه لایه کیفیت بهینه بر اساس ابعاد و وضعیت دید در صفحه (Debounce شده)
├── adapters/                      # لایه ۴: پیاده‌سازی‌های زیرساخت و SDKهای خارجی
│   ├── rtc_livekit/               # آداپتور مسیر اول: پیاده‌سازی پورت‌ها با پکیج رسمی `livekit-client` (مسیر /rtc)
│   │   └── livekit_adapter.ts
│   ├── rtc_ortc/                  # آداپتور مسیر دوم: پیاده‌سازی پورت‌ها با `mediasoup-client` (مسیر /ws/ortc)
│   │   └── mediasoup_ortc_adapter.ts
│   ├── browser_media/             # آداپتور Web APIs (WakeLock, AudioContext Unlocker, Device Enumerator, DSP)
│   └── factory.ts                 # کارخانه ساخت آداپتور بر اساس متغیر محیطی NEXT_PUBLIC_RTC_PROTOCOL
├── store/                         # لایه ۵: مدیریت وضعیت واکنش‌گرا با Zustand (تفکیک‌شده برای جلوگیری از Re-render)
│   ├── use_room_store.ts          # وضعیت کلی اتاق، چیدمان و لیست شناسه‌های شرکت‌کنندگان
│   ├── use_participants_store.ts  # نقشه شرکت‌کنندگان و ترک‌ها (با سلکتورهای اتمی به ازای هر participantId)
│   └── use_telemetry_store.ts     # استور مجزا برای تغییرات فرکانس بالا (Audio Levels و Active Speakers)
├── components/                    # لایه ۶: رابط کاربری (Tailwind CSS + shadcn/ui)
│   ├── lobby/                     # کامپوننت‌های Pre-Join Lobby، انتخاب دوربین/میکروفون و تست صدا
│   ├── stage/                     # VideoGrid، VideoTile (مجهز به Adaptive Observer)، ScreenShareStage
│   ├── controls/                  # ControlBar (Mute/Unmute، اشتراک صفحه، تنظیمات کیفیت، خروج)
│   └── ui/                        # کامپوننت‌های پایه و اتمی shadcn/ui
└── app/                           # لایه ۷: Next.js App Router (SSR برای صفحات ورود/رویداد و Client برای اتاق جلسه)
