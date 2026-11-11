package webhook

import (
	"context"
	"log/slog"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/livekit/protocol/auth"
	livekitproto "github.com/livekit/protocol/livekit"
	lkwebhook "github.com/livekit/protocol/webhook"
	"github.com/prometheus/client_golang/prometheus"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var initStatsOnce sync.Once

// LiveKitWebhookNotifier implements ports.WebhookEventNotifierPort using LiveKit HMAC-SHA256 JWT signatures.
type LiveKitWebhookNotifier struct {
	notifier lkwebhook.QueuedNotifier
	apiKey   string
}

var _ ports.WebhookEventNotifierPort = (*LiveKitWebhookNotifier)(nil)

// NewLiveKitWebhookNotifier constructs a WebhookEventNotifierPort for sending signed LiveKit webhook events.
func NewLiveKitWebhookNotifier(urls []string, apiKey, apiSecret string) (*LiveKitWebhookNotifier, error) {
	if len(urls) == 0 {
		return &LiveKitWebhookNotifier{}, nil
	}
	if apiKey == "" {
		apiKey = "devkey"
	}
	if apiSecret == "" {
		apiSecret = "secret"
	}

	cfg := lkwebhook.WebHookConfig{
		URLs:   urls,
		APIKey: apiKey,
	}
	initStatsOnce.Do(func() {
		lkwebhook.InitWebhookStats(prometheus.Labels{})
	})
	kp := auth.NewSimpleKeyProvider(apiKey, apiSecret)
	notifier, err := lkwebhook.NewDefaultNotifier(cfg, kp)
	if err != nil {
		return nil, err
	}

	return &LiveKitWebhookNotifier{
		notifier: notifier,
		apiKey:   apiKey,
	}, nil
}

func (w *LiveKitWebhookNotifier) dispatch(ctx context.Context, event *livekitproto.WebhookEvent) {
	if w.notifier == nil || event == nil {
		return
	}
	if err := w.notifier.QueueNotify(ctx, event); err != nil {
		slog.WarnContext(ctx, "failed to queue webhook notification",
			"event", event.GetEvent(),
			"event_id", event.GetId(),
			"error", err,
		)
	}
}

func (w *LiveKitWebhookNotifier) NotifyRoomStarted(ctx context.Context, room *domain.Room) {
	if w.notifier == nil || room == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:     lkwebhook.EventRoomStarted,
		Room:      mapRoomToProto(room),
		Id:        uuid.NewString(),
		CreatedAt: time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) NotifyRoomFinished(ctx context.Context, room *domain.Room) {
	if w.notifier == nil || room == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:     lkwebhook.EventRoomFinished,
		Room:      mapRoomToProto(room),
		Id:        uuid.NewString(),
		CreatedAt: time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) NotifyParticipantJoined(ctx context.Context, room *domain.Room, p *domain.Participant) {
	if w.notifier == nil || room == nil || p == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:       lkwebhook.EventParticipantJoined,
		Room:        mapRoomToProto(room),
		Participant: mapParticipantToProto(p),
		Id:          uuid.NewString(),
		CreatedAt:   time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) NotifyParticipantLeft(ctx context.Context, room *domain.Room, p *domain.Participant) {
	if w.notifier == nil || room == nil || p == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:       lkwebhook.EventParticipantLeft,
		Room:        mapRoomToProto(room),
		Participant: mapParticipantToProto(p),
		Id:          uuid.NewString(),
		CreatedAt:   time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) NotifyTrackPublished(ctx context.Context, room *domain.Room, p *domain.Participant, tr *domain.Track) {
	if w.notifier == nil || room == nil || p == nil || tr == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:       lkwebhook.EventTrackPublished,
		Room:        mapRoomToProto(room),
		Participant: mapParticipantToProto(p),
		Track:       mapTrackToProto(tr),
		Id:          uuid.NewString(),
		CreatedAt:   time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) NotifyTrackUnpublished(ctx context.Context, room *domain.Room, p *domain.Participant, tr *domain.Track) {
	if w.notifier == nil || room == nil || p == nil || tr == nil {
		return
	}
	event := &livekitproto.WebhookEvent{
		Event:       lkwebhook.EventTrackUnpublished,
		Room:        mapRoomToProto(room),
		Participant: mapParticipantToProto(p),
		Track:       mapTrackToProto(tr),
		Id:          uuid.NewString(),
		CreatedAt:   time.Now().Unix(),
	}
	w.dispatch(ctx, event)
}

func (w *LiveKitWebhookNotifier) Close() {
	if w.notifier != nil {
		w.notifier.Stop(true)
	}
}

func mapRoomToProto(r *domain.Room) *livekitproto.Room {
	if r == nil {
		return nil
	}
	parts := r.ListParticipants()
	return &livekitproto.Room{
		Sid:             r.ID,
		Name:            r.ID,
		EmptyTimeout:    r.EmptyTimeout,
		MaxParticipants: r.MaxParticipants,
		CreationTime:    r.CreatedAt.Unix(),
		Metadata:        r.GetMetadata(),
		NumParticipants: uint32(len(parts)),
	}
}

func mapParticipantToProto(p *domain.Participant) *livekitproto.ParticipantInfo {
	if p == nil {
		return nil
	}
	return &livekitproto.ParticipantInfo{
		Sid:      p.SID,
		Identity: p.ID,
		Name:     p.Name,
		JoinedAt: p.JoinedAt.Unix(),
		Metadata: p.GetMetadata(),
	}
}

func mapTrackToProto(t *domain.Track) *livekitproto.TrackInfo {
	if t == nil {
		return nil
	}
	trackType := livekitproto.TrackType_AUDIO
	if t.Kind == domain.TrackKindVideo {
		trackType = livekitproto.TrackType_VIDEO
	}
	return &livekitproto.TrackInfo{
		Sid:       t.SID,
		Type:      trackType,
		Name:      t.Name,
		Muted:     t.Paused,
		Width:     t.Width,
		Height:    t.Height,
		Simulcast: len(t.SimulcastEncodings) > 0,
		MimeType:  t.MimeType,
	}
}
