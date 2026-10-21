# قوانین معماری و مهندسی سرویس `signaling-coordinator` (Go)

## ۱. معماری هگزاگونال و ایزوله‌سازی پروتکل
1. **استقلال لایه دامنه (`internal/domain/` و `internal/ports/`):** هیچ پکیجی از `github.com/livekit/protocol`، `github.com/pion/sdp/v3` یا `google.golang.org/grpc` نباید وارد لایه دامنه شود.
2. **محدود کردن دامنه فاز صفر (جلوگیری از Over-Engineering):**
   - در فاز صفر، تمرکز صرفاً روی اتصال `/rtc`، فرآیند Join و عبور واقعی صدا و تصویر (Publish/Subscribe) و سپس مدیریت هم‌زمانی و Simulcast است.
   - پیاده‌سازی کامل سرویس‌های جانبی مثل Twirp API، وب‌هوک‌ها یا چندمستأجری را تا پیش از عبور از دروازه فاز صفر وارد کد نکن.
   - برای `NativeOrtcAdapter` (`/ws/ortc`) فقط یک اسکلت ساده در کنار `LiveKitAdapter` قرار بده.

## ۲. ماشین وضعیت صریح مذاکره و Actorهای محدود
1. **فقط دو سطح مالکیت وضعیت هم‌زمان:**
   - `RoomActor`: مالک وضعیت اتاق، لیست شرکت‌کنندگان و ترتیب عملیات. نادیده گرفتن رویدادهای دیررس پس از بسته شدن اتاق.
   - `ParticipantSession` (`ParticipantSdpActor`): مالک وضعیت اتصال هر شرکت‌کننده و ماشین وضعیت مذاکره SDP.
2. **ماشین وضعیت صریح مذاکره SDP:**
   - وضعیت‌ها را به صورت صریح تعریف و تست کن: `Stable` $\rightarrow$ `WaitingForAnswer` $\rightarrow$ `Stable` (به همراه حالت‌های `TimedOut` و `Closed`).
   - هرگز در حالت `WaitingForAnswer` یک Offer جدید ارسال نکن؛ تغییرات جدید ترک‌ها را در صف (`pendingChanges`) نگه دار و پس از دریافت Answer و تکمیل اتصال، مذاکره بعدی را آغاز کن.

-----
# قوانین معماری، SOLID و مهندسی سرویس `signaling-coordinator` (Go)

## ۱. الزام معماری Hexagonal و اصول پنج‌گانه SOLID
1. **رعایت اجباری ساختار لایه‌ای:** کدها باید دقیقاً مطابق مهارت `go-hexagonal-actor-solid` در لایه‌های `internal/domain`، `internal/ports`، `internal/application` و `internal/adapters` تفکیک شوند.
2. **استقلال لایه دامنه (اصل DIP):** هیچ پکیجی از `github.com/livekit/protocol`، `github.com/pion/sdp/v3`، `gorilla/websocket` یا `google.golang.org/grpc` نباید در `internal/domain` یا `internal/ports` ایمپورت شود[cite: 3, 5].
3. **ایزوله‌سازی کامل پروتکل LiveKit (اصل SRP و OCP):** تمام منطق مربوط به Protobufهای LiveKit و پارس/تولید SDP باید منحصراً داخل `internal/adapters/inbound/livekit/` قرار گیرد و از طریق پورت `PeerResponderPort` با هسته دامنه ارتباط برقرار کند[cite: 3, 5].
4. **محدود کردن دامنه فاز صفر:** در فاز صفر تمرکز روی `/rtc`، فرآیند Join و عبور واقعی رسانه (Publish/Subscribe)، هم‌زمانی، Simulcast و ICE Restart است. سرویس‌های جانبی (مانند Twirp کامل یا چندمستأجری) را وارد فاز صفر نکن و برای `/ws/ortc` یک اسکلت تمیز قرار بده[cite: 3, 5].

## ۲. ماشین وضعیت صریح مذاکره، Actorها و Best Practices
1. **دو سطح مالکیت وضعیت هم‌زمان:**
   - `RoomActor`: مالک وضعیت اتاق و ترتیب عملیات. رویدادهای دیررس پس از بسته شدن اتاق باید نادیده گرفته شوند[cite: 3, 5].
   - `ParticipantSession` (`ParticipantSdpActor`): مالک وضعیت اتصال شرکت‌کننده و ماشین وضعیت مذاکره SDP[cite: 3, 5].
2. **ماشین وضعیت صریح مذاکره SDP:**
   - وضعیت‌های `Stable` $\rightarrow$ `WaitingForAnswer` $\rightarrow$ `Stable` (به همراه `TimedOut` و `Closed`) را به صورت صریح پیاده‌سازی و با Unit Test راستی‌آزمایی کن[cite: 5].
   - در حالت `WaitingForAnswer` هرگز Offer جدید نفرست؛ تغییرات جدید را صف‌بندی/ادغام کن تا پس از دریافت Answer اعمال شوند[cite: 5].
3. **کیفیت کد Go:**
   - اجرای تمام تست‌ها با فلگ `-race` (`go test -race ./...`) برای اثبات نبودن Race Condition الزامی است.
   - تمام فراخوانی‌های gRPC به نود Rust باید دارای `context.WithTimeout` و `request_id` یکتا باشند[cite: 3, 5].