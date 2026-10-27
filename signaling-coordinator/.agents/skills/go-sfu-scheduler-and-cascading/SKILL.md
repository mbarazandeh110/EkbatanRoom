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
   availableWeight := float64(metrics.TotalCapacityWeight) * (1.0 - ( float64(metrics.EffectiveLoadPercent) / 100.0 ))
