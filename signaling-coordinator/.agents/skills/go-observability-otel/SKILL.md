---
name: go-observability-otel
description: الگوی پیاده‌سازی لاگ ساختاریافته (log/slog با سانسور امنیتی)، تریسینگ توزیع‌شده OpenTelemetry با انتشار خودکار gRPC، و متریک‌های VictoriaMetrics در Go.
---

# مهارت تخصصی: Observability، APM و مانیتورینگ قابل کانفیگ در Go (`signaling-coordinator`)

در پیاده‌سازی سرویس `signaling-coordinator`، معماری مشاهده‌پذیری را در پکیج زیرساختی `internal/adapters/outbound/telemetry/` (یا `pkg/observability/`) بر اساس اصول زیر پیاده کن:

## ۱. لاگر ساختاریافته، انتخابی و امن با `log/slog`
1. یک `slog.Handler` سفارشی روی `slog.NewJSONHandler` (یا `NewTextHandler` بر اساس `OBS_LOG_FORMAT`) بساز که سطح آن از `OBS_LOG_LEVEL` خوانده شود.
2. **تزریق خودکار TraceContext به لاگ‌ها:** هندلر `slog` باید از `ctx` ورودی، مقادیر `trace_id` و `span_id` فعال در OpenTelemetry را استخراج کرده و به صورت خودکار به رکورد JSON لاگ اضافه کند.
3. **سانسور داده‌های حساس (`ReplaceAttr`):**
   - هر فیلدی با نام `token`, `access_token`, `ice_pwd`, `secret` باید به صورت خودکار با `"[REDACTED]"` جایگزین شود.
   - رشته‌های `sdp` فقط زمانی لاگ شوند که هم `OBS_LOG_LEVEL=debug` و هم `OBS_LOG_RAW_SDP=true` باشد؛ در غیر این صورت فقط طول رشته SDP، تعداد خطوط `m=` و وضعیت ماشین حالت (`Stable`/`WaitingForAnswer`) لاگ شود.

## ۲. ردیابی توزیع‌شده (APM) و انتشار به Rust از طریق gRPC
1. اگر `OBS_TRACING_ENABLED=true` بود، `TracerProvider` را با صادرکننده OTLP و نرخ نمونه‌برداری `OBS_TRACE_SAMPLE_RATE` راه‌اندازی کن و `propagation.TraceContext{}` (استاندارد W3C) را ثبت نما.
2. **اتصال خودکار تریس Go به Rust:** هنگام ساخت کلاینت gRPC در `internal/adapters/outbound/media_node_grpc/`، از `otelgrpc.NewClientHandler()` استفاده کن تا هر زمان `RoomActor` یا `ParticipantSession` متدی را روی نود Rust صدا می‌زند، هدر `traceparent` به صورت خودکار به Rust ارسال شود.
3. برای عملیات کلیدی چرخه حیات (`HandleJoin`, `HandleOffer`, `TriggerSubscriberNegotiation`, `HandleIceRestart`) یک Span با ویژگی‌های `room.id`، `participant.id` و `negotiation.state` باز کن و در صورت بروز خطا، `span.RecordError(err)` و `span.SetStatus(codes.Error, ...)` را فراخوانی نما.

## ۳. متریک‌های کم‌کاردینالیتی برای VictoriaMetrics (`/metrics`)
در صورت فعال بودن `OBS_METRICS_ENABLED`، متریک‌های زیر را ارائه بده:
- `signaling_active_rooms` و `signaling_active_participants{protocol="livekit|ortc"}` (Gauge)
- `signaling_ws_messages_total{protocol, type, status}` (Counter)
- `signaling_sdp_negotiation_duration_seconds{role="publisher|subscriber"}` (Histogram)
- `signaling_sdp_glare_queued_total` (Counter - تعداد دفعاتی که به لطف `ParticipantSdpActor` از برخورد همزمان Offer جلوگیری شده است)
- `signaling_grpc_client_duration_seconds{method, status}` (Histogram)
