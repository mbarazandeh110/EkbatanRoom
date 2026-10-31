---
name: go-sfu-scheduler-and-cascading
description: الگوی زمان‌بندی هوشمند نودهای SFU بر اساس وزن و بار ترکیبی (NodeCapacityMetrics)، مدیریت پویا و سراسری Soft/Hard Threshold، و Cascading اتاق بین چند نود با PipeTransport در signaling-coordinator.
---

# مهارت تخصصی: زمان‌بند هوشمند ناوگان SFU و Cascading بین‌نودی (`signaling-coordinator`)

هنگام پیاده‌سازی انتخاب نود SFU (Node Selection) و مدیریت مقیاس‌پذیری اتاق‌ها در `signaling-coordinator`، اصول زیر را رعایت کن:

## ۱. دریافت و نگهداری بلادرنگ وضعیت ظرفیت نودها (`NodeCapacityRegistry`)
1. در زمان اتصال اولیه به هر `sfu-media-node`، متد gRPC `GetNodeCapacity` را فراخوانی کن و سپس از طریق استریم `WatchNodeEvents`، پیام‌های `NodeCapacityMetrics` را دریافت و در یک جدول Thread-safe (`sync.RWMutex`) ذخیره کن.
2. برای هر نود مقادیر زیر را نگه دار:
   - `TotalCapacityWeight` (وزن کل قدرت نود)
   - `EffectiveLoadPercent` (`max(logical_load, cpu_usage)`)
   - `MaxWorkerLoadPercent`
   - `IsDraining` و `IsHealthy`
3. **فرمول امتیاز ظرفیت خالی وزنی (Available Weighted Capacity Score):**
   ```go
   availableWeight := float64(metrics.TotalCapacityWeight) * (1.0 - (float64(metrics.EffectiveLoadPercent) / 100.0))
   ```

## ۲. مدیریت مرزهای بار (Soft & Hard Threshold)
1. **ناحیه عادی (Normal Zone - بار کمتر از ۷۰٪):** نود مجاز به میزبانی اتاق‌های جدید و کاربران جدید است. انتخاب نود برای اتاق جدید بر مبنای بیشترین امتیاز `availableWeight` صورت می‌گیرد.
2. **مرز نرم (Soft Threshold - بار ۷۰٪ تا ۸۵٪):** نود از اختصاص اتاق‌های جدید خارج می‌شود، اما کاربران جدید برای اتاق‌هایی که از قبل روی این نود دایر هستند، همچنان بدون نیاز به Cascading روی همین نود پذیرفته می‌شوند.
3. **مرز سخت (Hard Threshold - بار ۸۵٪ به بالا):** ایجاد ترنسپورت جدید روی این نود مسدود می‌گردد. کاربران جدید واردشده به اتاق‌های موجود به نود خلوت دیگری در کلاستر سرریز (Spillover) داده می‌شوند و ارتباط رسانه‌ای میان دو نود از طریق `PipeTransport` برقرار می‌گردد.

## ۳. معماری Cascading بین‌نودی با PipeTransport
1. **اتصال دوطرفه Pipe:**
   - روی نود اولیه: فراخوانی `CreatePipeTransport` روی روتر اتاق و دریافت IP، پورت و پارامترهای SRTP.
   - روی نود ثانویه: فراخوانی `CreatePipeTransport` روی روتر ثانویه همان اتاق.
   - تبادل اطلاعات و فراخوانی متقابل `ConnectPipeTransport` روی هر دو نود.
2. **انتشار و دریافت استریم‌ها:**
   - ترک‌های منتشرشده روی نود اولیه با متدهای `PipeProduce` / `PipeConsume` به نود ثانویه منتقل می‌شوند تا کاربران نود ثانویه بتوانند آن‌ها را دریافت کنند و بالعکس.
