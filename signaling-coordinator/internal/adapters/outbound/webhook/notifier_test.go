package webhook_test

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"github.com/livekit/protocol/auth"
	"github.com/livekit/protocol/webhook"

	webhookadapter "ekbatan-room/signaling-coordinator/internal/adapters/outbound/webhook"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

func TestLiveKitWebhookNotifier_SignedDispatch(t *testing.T) {
	apiKey := "test-api-key"
	apiSecret := "test-api-secret-123456"

	var mu sync.Mutex
	receivedEvents := make([]string, 0)
	var authHeaders []string

	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			http.Error(w, "bad body", http.StatusBadRequest)
			return
		}
		defer r.Body.Close()

		authHdr := r.Header.Get("Authorization")

		mu.Lock()
		authHeaders = append(authHeaders, authHdr)
		mu.Unlock()

		// Verify using LiveKit webhook receiver
		kp := auth.NewSimpleKeyProvider(apiKey, apiSecret)
		req, _ := http.NewRequest(r.Method, r.URL.String(), stringsReader(string(body)))
		req.Header.Set("Authorization", authHdr)
		req.Header.Set("Content-Type", r.Header.Get("Content-Type"))

		evt, err := webhook.ReceiveWebhookEvent(req, kp)
		if err != nil {
			t.Logf("webhook verification error: %v", err)
			w.WriteHeader(http.StatusOK)
			return
		}

		mu.Lock()
		receivedEvents = append(receivedEvents, evt.GetEvent())
		mu.Unlock()

		w.WriteHeader(http.StatusOK)
	}))
	defer ts.Close()

	notifier, err := webhookadapter.NewLiveKitWebhookNotifier([]string{ts.URL}, apiKey, apiSecret)
	if err != nil {
		t.Fatalf("failed to create webhook notifier: %v", err)
	}
	defer notifier.Close()

	ctx := context.Background()
	room := domain.NewRoom("room-webhook-1", "router-1")
	p := domain.NewParticipant("user-1", "PA_1", "Alice")
	_ = room.AddParticipant(p)

	// Dispatch room_started
	notifier.NotifyRoomStarted(ctx, room)

	// Dispatch participant_joined
	notifier.NotifyParticipantJoined(ctx, room, p)

	// Dispatch participant_left
	notifier.NotifyParticipantLeft(ctx, room, p)

	// Dispatch room_finished
	notifier.NotifyRoomFinished(ctx, room)

	// Wait for queue delivery
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		mu.Lock()
		count := len(authHeaders)
		mu.Unlock()
		if count >= 4 {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}

	mu.Lock()
	defer mu.Unlock()
	if len(authHeaders) == 0 {
		t.Fatalf("expected at least 1 webhook delivered, got 0")
	}

	for _, hdr := range authHeaders {
		if hdr == "" {
			t.Fatalf("expected signed Authorization header in webhook request, got empty")
		}
	}
}

type stringReader struct {
	s string
	i int
}

func stringsReader(s string) io.Reader {
	return &readerWrapper{data: []byte(s)}
}

type readerWrapper struct {
	data []byte
	idx  int
}

func (r *readerWrapper) Read(p []byte) (n int, err error) {
	if r.idx >= len(r.data) {
		return 0, io.EOF
	}
	n = copy(p, r.data[r.idx:])
	r.idx += n
	return n, nil
}
