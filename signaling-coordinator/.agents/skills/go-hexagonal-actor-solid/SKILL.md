---
name: go-hexagonal-actor-solid
description: الگوی پیاده‌سازی معماری Hexagonal، مدل Actor محدود، اصول پنج‌گانه SOLID و Best Practiceهای زبان Go در سرویس سیگنالینگ.
---

# مهارت تخصصی: معماری Hexagonal، Actor Model، اصول SOLID و Best Practiceها در Go

هنگام طراحی و کدنویسی سرویس `signaling-coordinator`، ساختار معماری و اصول زیر را مو‌به‌مو اجرا کن:

## ۱. ساختار دایرکتوری‌ها (Clean / Hexagonal Go Layout)
جهت وابستگی‌ها باید اکیداً رو به داخل (`adapters -> application -> ports -> domain`) باشد:
```text
cmd/
└── coordinator/
    └── main.go                    # Composition Root: سیم‌کشی وابستگی‌ها (DI)، راه‌اندازی HTTP/WS و Graceful Shutdown
internal/
├── domain/                        # لایه هسته ۱۰۰٪ خالص (Pure Go - بدون هیچ ایمپورتی از livekit، pion/sdp یا grpc)
│   ├── room.go                    # موجودیت Room، وضعیت‌ها و قوانین دامنه
│   ├── participant.go             # موجودیت Participant، وضعیت ترک‌ها و کاندیداها
│   ├── sdp_slot.go                # مدل اسلات‌های m-line و ماشین وضعیت مذاکره (Stable, WaitingForAnswer)
│   └── errors.go                  # خطاهای استاندارد دامنه
├── ports/                         # قراردادهای ورودی و خروجی (Interfaces) بر پایه اصل ISP و DIP
│   ├── inbound.go                 # RoomCoordinatorPort, PeerResponderPort
│   └── outbound.go                # RouterEnginePort, TransportEnginePort, MediaStreamPort
├── application/                   # لایه Use Cases و Actorهای مالک وضعیت هم‌زمان
│   ├── room_actor.go              # مالک وضعیت اتاق و ترتیب عملیات ورود/خروج و انتشار ترک
│   ├── participant_session.go     # مالک چرخه حیات نشست شرکت‌کننده و ماشین وضعیت مذاکره (ParticipantSdpActor)
│   └── room_manager.go            # مدیریت ایجاد و یافتن RoomActorها
└── adapters/                      # لایه زیرساخت و آداپتورهای پروتکل
    ├── inbound/
    │   ├── livekit/               # ایزوله‌سازی کامل پروتکل LiveKit (/rtc، Protobuf و SDP Synthesizer)
    │   │   ├── ws_handler.go      # هندلر وب‌سوکت /rtc و مدیریت چرخه اتصال
    │   │   ├── responder.go       # پیاده‌سازی PeerResponderPort برای کلاینت رسمی LiveKit
    │   │   └── sdp_mapper.go      # پارس و تولید SDP با pion/sdp/v3
    │   └── native_ortc/           # اسکلت آداپتور بومی (/ws/ortc) برای اثبات معماری Dual-Protocol
    │       └── ortc_handler.go
    └── outbound/
        └── media_node_grpc/       # آداپتور کلاینت gRPC متصل به سرویس Rust (با تولید request_id و Timeout)
            └── client.go
