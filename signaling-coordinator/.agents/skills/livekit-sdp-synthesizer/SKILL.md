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
2. **رفع بن‌بست نقشی DTLS و آغازگری فعال (REQ-005):** نبود صفت `a=setup` در سطح خطوط مدیا (`m=`) باعث می‌شود کلاینت‌هایی مانند Pion WebRTC دچار ابهام نقشی شده و هر دو سمت به عنوان DTLS Server منتظر دیگری بمانند. در متد `GeneratePublisherAnswer`، صفت `a=setup:active` را هم در سطح سشن و هم مستقیماً درون تک‌تک خطوط مدیا (`m=application`، `m=video`، `m=audio`) درج کن. همچنین هنگام فراخوانی `ConnectTransport` روی نود مدیا، نقش `role: client` را ارسال کن تا Mediasoup به عنوان آغازگر فعال پیام `DTLS ClientHello` را Proactive ارسال کند.
3. **استخراج هدرهای اکستنشن RTP:** خطوط `a=extmap` اعلام‌شده توسط کلاینت (به‌ویژه MID، RID سیمولکست، و Transport-CC) را استخراج کرده و در ساختار JSON پارامترهای RTP (`ProduceRequest.rtp_parameters_json`) در هر دو فیلد `headerExtensions` و `header_extensions` با الزام `"encrypt": false` قرار بده تا BWE و مدیریت لایه‌های سیمولکست به درستی عمل کند.
4. کدک، `PayloadType`، `SSRC`، اکستنشن‌های هدر و خطوط `a=rid` (در صورت فعال بودن Simulcast) را از `MediaDescription` استخراج کرده و به ساختار JSON مورد انتظار `ProduceRequest` تبدیل کن.

## ۳. مدیریت خطوط `m=` در مسیر دریافت (Subscriber Offer)
1. در Unified Plan، ترتیب بخش‌های `m=` و مقادیر `a=mid` در مذاکرات مجدد (Renegotiation) باید پایدار بماند.
2. هنگام بسته شدن یک ترک، به جای حذف خط `m=` که باعث به هم ریختن ایندکس‌ها می‌شود، آن اسلات را با پورت `0` و ویژگی `a=inactive` علامت‌گذاری کن تا در ترک‌های بعدی بازیافت شود.
3. ویژگی `a=msid:{participant_sid} {track_sid}` را مطابق استاندارد درج کن. توجه حیاتی: در وب‌آرتی‌سی مدرن بر پایه Unified Plan و BUNDLE، از درج خطوط صریح `a=ssrc` و `a=ssrc-group` در آفر مشترک (Subscriber Offer) اکیداً خودداری کن؛ وجود SSRC صریح در m-section آفر باعث می‌شود کتابخانه‌هایی مانند Pion WebRTC (در livekit-cli) بسته‌های مدیا با SSRC متفاوت را Drop کنند و رویداد OnTrack هرگز اجرا نشود (REQ-001).

## ۴. مذاکره Simulcast در پاسخ ناشر و هماهنگ‌سازی کیفیت لایه‌ها (REQ-002 و REQ-005)
1. **پذیرش صریح سیمولکست در Publisher Answer (RFC 8853):** هنگامی که ناشر استریم‌های سیمولکست را پیشنهاد می‌دهد (`a=simulcast:send ...` و `a=rid:<id> send`)، سرور به عنوان دریافت‌کننده رسانه باید صفت‌های پاسخ را به صورت `a=simulcast:recv ...` و `a=rid:<id> recv` بازگرداند تا کلاینت‌هایی مانند `livekit-cli` (Pion WebRTC) سرور را به عنوان گیرنده معتبر بشناسند و بسته‌های فیدبک RTCP PLI/FIR به لایه‌های ارسالی ناشر متصل شوند.
2. **ارسال پیام `SubscribedQualityUpdate` و هماهنگی پویای کدک (REQ-005):** در پروتکل LiveKit، انکودر ناشر لایه‌های با کیفیت بالا را تا زمان اعلام نیاز فعال نمی‌کند یا کلیدفریم نمی‌فرستد. هنگام انتشار ترک ویدیویی، پیوستن مشترکین جدید و نهایی شدن Answer مشترک، باید پیام `SubscribedQualityUpdate` با فعال بودن کیفیت‌های HIGH، MEDIUM و LOW برای ناشر ارسال شود. فیلد `codec` در این پیام باید بر اساس کدک واقعی ترک منتشرشده (مثلاً `video/vp8` یا `video/h264`) مقداردهی شود تا کلاینت‌هایی مانند `livekit-cli` که با کدک پیش‌فرض H.264 کار می‌کنند دچار عدم تطابق کدک و خاموش ماندن ترک نشوند.


