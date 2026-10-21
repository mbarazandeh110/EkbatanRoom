package application_test

import (
	"context"
	"testing"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

func TestRoomActor_TrackMutingAndLayers(t *testing.T) {
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-mute", engine, engine, engine, iceServers)
	ctx := context.Background()

	respAlice := newMockResponder()
	respBob := newMockResponder()

	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)
	_, _ = actor.Join(ctx, "bob", "Bob", respBob)

	fingerprints := []domain.DtlsFingerprintInfo{
		{Algorithm: "sha-256", Value: "AA:BB"},
	}
	tracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              5555,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[]}`,
		},
	}

	_, err := actor.PublishTracks(ctx, "alice", "client", fingerprints, tracks)
	if err != nil {
		t.Fatalf("PublishTracks failed: %v", err)
	}

	aliceSession, _ := actor.GetSession("alice")
	aliceTracks := aliceSession.Participant().ListTracks()
	if len(aliceTracks) == 0 {
		t.Fatalf("expected alice to have published track")
	}
	trackSID := aliceTracks[0].SID

	// 1. Mute track
	err = actor.SetTrackMuted(ctx, "alice", trackSID, true)
	if err != nil {
		t.Fatalf("SetTrackMuted failed: %v", err)
	}

	// Verify track is muted in domain
	tObj, _ := aliceSession.Participant().GetTrack(trackSID)
	if !tObj.Paused {
		t.Fatalf("expected track to be paused in domain")
	}

	// Verify Bob received track muted event
	if muted, ok := respBob.mutedTracks[trackSID]; !ok || !muted {
		t.Fatalf("expected Bob to receive muted track event for %s", trackSID)
	}

	// 2. Set consumer layers
	err = actor.SetConsumerLayers(ctx, "bob", "cons-1", 2, 1)
	if err != nil {
		t.Fatalf("SetConsumerLayers failed: %v", err)
	}
}

func TestRoomActor_IceRestart(t *testing.T) {
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-ice", engine, engine, engine, iceServers)
	ctx := context.Background()

	respAlice := newMockResponder()
	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)

	// Publisher ICE Restart
	pubIce, err := actor.RestartPublisherIce(ctx, "alice")
	if err != nil {
		t.Fatalf("RestartPublisherIce failed: %v", err)
	}
	if pubIce.IceUfrag != "ufrag-new" {
		t.Fatalf("expected ufrag-new, got %s", pubIce.IceUfrag)
	}

	// Subscriber ICE Restart
	subIce, err := actor.RestartSubscriberIce(ctx, "alice")
	if err != nil {
		t.Fatalf("RestartSubscriberIce failed: %v", err)
	}
	if subIce.IceUfrag != "ufrag-new" {
		t.Fatalf("expected ufrag-new, got %s", subIce.IceUfrag)
	}

	// Alice must have received a renegotiation offer with the new credentials
	if len(respAlice.offers) != 1 {
		t.Fatalf("expected Alice to receive renegotiation offer after ICE restart, got %d", len(respAlice.offers))
	}
}

func TestRoomActor_SlotInactivationAndRecycling(t *testing.T) {
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-recycle", engine, engine, engine, iceServers)
	ctx := context.Background()

	respAlice := newMockResponder()
	respBob := newMockResponder()

	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)
	_, _ = actor.Join(ctx, "bob", "Bob", respBob)

	fingerprints := []domain.DtlsFingerprintInfo{{Algorithm: "sha-256", Value: "11:22"}}

	// 1. Alice publishes a video track
	_, err := actor.PublishTracks(ctx, "alice", "client", fingerprints, []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              7777,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[]}`,
		},
	})
	if err != nil {
		t.Fatalf("Alice publish failed: %v", err)
	}

	bobSession, _ := actor.GetSession("bob")
	bobSlots := bobSession.SlotTable().ListSlots()
	if len(bobSlots) != 1 || !bobSlots[0].Active || bobSlots[0].MID != "0" {
		t.Fatalf("expected Bob to have 1 active slot with MID 0, got %+v", bobSlots)
	}

	// 2. Bob answers
	_ = actor.HandleSubscriberAnswer(ctx, "bob", "client", fingerprints)

	// 3. Alice leaves -> Alice's video track in Bob's slot table must be inactivated!
	err = actor.Leave(ctx, "alice")
	if err != nil {
		t.Fatalf("Alice leave failed: %v", err)
	}

	bobSlotsAfterLeave := bobSession.SlotTable().ListSlots()
	if len(bobSlotsAfterLeave) != 1 {
		t.Fatalf("expected slot count to remain 1 (never delete m-lines), got %d", len(bobSlotsAfterLeave))
	}
	if bobSlotsAfterLeave[0].Active {
		t.Fatalf("expected slot 0 to be marked inactive after Alice left")
	}

	// 4. Bob answers the inactivation offer
	_ = actor.HandleSubscriberAnswer(ctx, "bob", "client", fingerprints)

	// 5. Charlie joins and publishes a video track -> Slot 0 must be RECYCLED, NOT appended as slot 1!
	respCharlie := newMockResponder()
	_, _ = actor.Join(ctx, "charlie", "Charlie", respCharlie)

	_, err = actor.PublishTracks(ctx, "charlie", "client", fingerprints, []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              8888,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[]}`,
		},
	})
	if err != nil {
		t.Fatalf("Charlie publish failed: %v", err)
	}

	bobSlotsAfterRecycle := bobSession.SlotTable().ListSlots()
	if len(bobSlotsAfterRecycle) != 1 {
		t.Fatalf("expected slot 0 to be recycled! Total slots should still be 1, got %d", len(bobSlotsAfterRecycle))
	}
	if !bobSlotsAfterRecycle[0].Active || bobSlotsAfterRecycle[0].MID != "0" {
		t.Fatalf("expected recycled slot 0 to be active with MID 0, got %+v", bobSlotsAfterRecycle[0])
	}
	if bobSlotsAfterRecycle[0].ProducerID != "prod-charlie" {
		t.Fatalf("expected recycled slot producer to be charlie, got %s", bobSlotsAfterRecycle[0].ProducerID)
	}
}
