---
name: livekit-sdp-synthesizer
description: راهنمای مبتنی بر شواهد برای پیاده‌سازی پروتکل /rtc در LiveKit و نگاشت SDP به Mediasoup.
---

# مهارت تخصصی: پیاده‌سازی مرحله‌ای و مبتنی بر شواهد `LiveKitAdapter`

به جای فرض گرفتن قواعد اثبات‌نشده، پیاده‌سازی مذاکره WebRTC را بر اساس قرارداد نسخه Pin شده `github.com/livekit/protocol` و بررسی SDP واقعی کلاینت انجام بده:

## ۱. بررسی اولیه قرارداد و نحوه مذاکره کلاینت
1. ابتدا ساختار پیام‌های `SignalRequest` و `SignalResponse` را در نسخه پین‌شده `github.com/livekit/protocol/livekit` بررسی کن (`JoinResponse`, `AddTrackRequest`, `SessionDescription`, `TrickleRequest`, `SubscriptionPermission`, `UpdateTrackSettings`).
2. بررسی کن کلاینت رسمی در هنگام اتصال چه پارامترهایی را در Query String مسیر `/rtc` ارسال می‌کند و چه فیلدهایی در `SignalResponse_Join` (مانند `SubscriberPrimary`، `IceServers`، `Participant` و `Room`) برای شروع کار کلاینت الزامی است.

## ۲. استخراج دقیق نقش DTLS و پارامترهای RTP در مسیر انتشار (Publisher)
1. وقتی کلاینت `SignalRequest_AddTrack` و سپس `SignalRequest_Offer` را می‌فرستد، رشته SDP کلاینت را با `github.com/pion/sdp/v3` پارس کن.
2. **نکته حیاتی DTLS:** در اولین فراخوانی `ConnectTransport` روی Publisher Transport، ویژگی `a=setup` را از SDP کلاینت استخراج کن (کلاینت در Offer معمولاً `a=setup:actpass` می‌فرستد). در این حالت نقش کلاینت را در DTLS مشخص کن تا در SDP Answer تولیدی سرور، مقدار `a=setup:passive` قرار گیرد و Handshake رمزنگاری DTLS بدون خطا برقرار شود.
3. کدک، `PayloadType`، `SSRC`، اکستنشن‌های هدر و خطوط `a=rid` (در صورت فعال بودن Simulcast) را از `MediaDescription` استخراج کرده و به ساختار JSON مورد انتظار `ProduceRequest` تبدیل کن.

## ۳. مدیریت خطوط `m=` در مسیر دریافت (Subscriber Offer)
1. در Unified Plan، ترتیب بخش‌های `m=` و مقادیر `a=mid` در مذاکرات مجدد (Renegotiation) باید پایدار بماند.
2. هنگام بسته شدن یک ترک، به جای حذف خط `m=` که باعث به هم ریختن ایندکس‌ها می‌شود، آن اسلات را با پورت `0` و ویژگی `a=inactive` علامت‌گذاری کن تا در ترک‌های بعدی بازیافت شود.
3. ویژگی `a=msid:{participant_sid} {track_sid}` را مطابق استاندارد درج کن. توجه حیاتی: در وب‌آرتی‌سی مدرن بر پایه Unified Plan و BUNDLE، از درج خطوط صریح `a=ssrc` و `a=ssrc-group` در آفر مشترک (Subscriber Offer) اکیداً خودداری کن؛ وجود SSRC صریح در m-section آفر باعث می‌شود کتابخانه‌هایی مانند Pion WebRTC (در livekit-cli) بسته‌های مدیا با SSRC متفاوت را Drop کنند و رویداد OnTrack هرگز اجرا نشود (REQ-001).
