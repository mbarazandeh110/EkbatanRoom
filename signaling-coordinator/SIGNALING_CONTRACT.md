## ۵. قرارداد مشترک تنظیمات Observability (Logs, Traces, APM & Metrics)
تمامی سرویس‌ها باید رفتار ثبت لاگ، تریسینگ و متریک‌های خود را بر اساس متغیرهای محیطی زیر تنظیم کنند (ثبت انتخابی و قابل پیکربندی):
- `OBS_LOG_LEVEL`: سطح ثبت لاگ (`trace` | `debug` | `info` | `warn` | `error` - پیش‌فرض: `info`).
- `OBS_LOG_FORMAT`: فرمت خروجی لاگ (`json` برای محیط عملیاتی/Elasticsearch | `pretty` برای توسعه محلی - پیش‌فرض: `json`).
- `OBS_LOG_RAW_SDP`: آیا متن کامل SDP در سطح `debug` لاگ شود؟ (`true` | `false` - پیش‌فرض: `false`).
- `OBS_METRICS_ENABLED`: فعال‌سازی اندپوینت `/metrics` با فرمت Prometheus برای VictoriaMetrics (`true` | `false` - پیش‌فرض: `true`).
- `OBS_METRICS_ADDR`: آدرس و پورت سرور متریک‌ها (`0.0.0.0:9090` برای Rust و `0.0.0.0:9091` برای Go).
- `OBS_TRACING_ENABLED`: فعال‌سازی ارسال تریس‌های OpenTelemetry (OTLP) به APM (`true` | `false` - پیش‌فرض: `false` در تست محلی).
- `OBS_OTLP_ENDPOINT`: آدرس کلکتور OpenTelemetry / APM (مثال: `http://localhost:4317`).
- `OBS_TRACE_SAMPLE_RATE`: نرخ نمونه‌برداری تریس‌ها بین `0.0` تا `1.0` (پیش‌فرض: `0.1` در پروداکشن؛ خطاها همیشه ۱۰۰٪ ثبت می‌شوند).

### استاندارد انتشار Context بین سرویس‌ها:
- سرویس `signaling-coordinator` (Go) در تمام فراخوانی‌های gRPC به `sfu-media-node` (Rust) باید هدر استاندارد W3C (`traceparent`) و `x-request-id` را در Metadata قرار دهد.
- تمام لاگ‌های JSON در هر ۴ پروژه باید در صورت وجود تریس فعال، فیلدهای استاندارد `trace_id`، `span_id`، `room_id` و `peer_id` را در خروجی JSON درج کنند.