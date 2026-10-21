---
name: rust-observability-otel
description: الگوی پیاده‌سازی لاگ ساختاریافته (tracing)، ردیابی توزیع‌شده OpenTelemetry (APM) و متریک‌های VictoriaMetrics/Prometheus در سرویس Rust با حفظ معماری هگزاگونال.
---

# مهارت تخصصی: Observability، APM و مانیتورینگ قابل کانفیگ در Rust (`sfu-media-node`)

هنگام پیاده‌سازی سرویس `sfu-media-node`، زیرساخت مشاهده‌پذیری را بر اساس اصول زیر پیاده کن:

## ۱. کتابخانه‌ها و پیکربندی پویا (`src/config.rs` و `src/adapters/outbound_telemetry/`)
1. از کریت‌های استاندارد اکوسیستم Rust استفاده کن:
   - `tracing` و `tracing-subscriber` (با فیچرهای `env-filter` و `json`).
   - `opentelemetry`، `opentelemetry_sdk`، `opentelemetry-otlp` و `tracing-opentelemetry`.
   - `metrics` و `metrics-exporter-prometheus` برای ارائه اندپوینت `/metrics`.
2. **ثبت انتخابی بر اساس کانفیگ (`ObservabilityConfig`):**
   - فیلتر سطح لاگ (`EnvFilter`) را از روی `OBS_LOG_LEVEL` بساز تا در سطح `info` هیچ لاگ اضافه‌ای از لایه‌های داخلی تولید نشود.
   - اگر `OBS_TRACING_ENABLED=false` بود، لایه صادرکننده OTLP را به `Registry` اضافه نکن تا سربار پردازشی صفر باشد. اگر `true` بود، از `Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(sample_rate)))` استفاده کن.
   - اگر `OBS_METRICS_ENABLED=true` بود، سرور HTTP سبک متریک‌ها را روی `OBS_METRICS_ADDR` بالا بیاور.

## ۲. استخراج W3C `traceparent` از درخواست‌های gRPC (`tonic`)
- در لایه `adapters/inbound_grpc/`، یک Interceptor یا تابع استخراج‌کننده بنویس که هدر `traceparent` و `x-request-id` را از `tonic::Request::metadata()` بخواند و به عنوان Parent Context روی Span فعلی `tracing` تنظیم کند.
- روی متدهای لایه `application` (مانند `create_router`, `create_transport`, `produce`, `consume`, `restart_ice`) از ماکروی `#[tracing::instrument(skip(self, req), fields(room_id, router_id, peer_id, request_id))]` استفاده کن؛ با استفاده از `skip` هرگز رشته‌های بزرگ JSON (`rtp_parameters_json`) را داخل Span یا لاگ چاپ نکن!

## ۳. متریک‌های استاندارد و قانون Cardinality Guard
فقط متریک‌های زیر را با لیبل‌های کم‌کاردینالیتی (`worker_index`, `kind`) ثبت کن:
- `sfu_active_routers` (Gauge)
- `sfu_active_transports` (Gauge - برچسب: `direction="pub|sub"`)
- `sfu_active_producers` و `sfu_active_consumers` (Gauge - برچسب: `kind="audio|video"`)
- `sfu_grpc_requests_total` و `sfu_grpc_request_duration_seconds` (Counter & Histogram - برچسب: `method`, `status`)
- `sfu_idempotency_hits_total` (Counter)
- **خط قرمز:** هرگز `router_id`، `transport_id`، `peer_id` یا `request_id` را در لیبل متریک‌ها قرار نده!
