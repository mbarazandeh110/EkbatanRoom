---
name: go-open-core-scheduler
description: الگوی معماری Open-Core در signaling-coordinator شامل زمان‌بندی پایه در نسخه متن‌باز (Community) و زمان‌بندی هوشمند مبتنی بر Soft/Hard Threshold همراه با Cascading بین‌نودی در نسخه تجاری (Enterprise).
---

# مهارت تخصصی: زمان‌بند Open-Core و توزیع چند-نودی اتاق (`signaling-coordinator`)

هنگام پیاده‌سازی انتخاب نود SFU و مدیریت ظرفیت در `signaling-coordinator`، معماری Open-Core را بر اساس الگوی استراتژی (Strategy Pattern) و Build Tags زبان Go (`//go:build enterprise` و `//go:build !enterprise`) پیاده‌سازی کن:

## ۱. اینترفیس پورت زمان‌بندی و مقیاس‌پذیری (`RoomPlacementStrategy`)
در لایه Domain/Ports اینترفیس زیر را تعریف کن تا هسته سیگنالینگ هیچ وابستگی مستقیمی به نوع نسخه (Community یا Enterprise) نداشته باشد:
```go
type RoomPlacementStrategy interface {
    // انتخاب نود برای ساخت اتاق جدید
    SelectNodeForNewRoom(ctx context.Context, roomID string) (NodeID, error)
    // انتخاب نود برای ورود کاربر به اتاق موجود (در نسخه Enterprise در صورت عبور از Hard Threshold عملیات Cascading را انجام می‌دهد)
    ResolveNodeForPeerJoin(ctx context.Context, roomID string, peerID string) (NodeID, RouterID, error)
}
