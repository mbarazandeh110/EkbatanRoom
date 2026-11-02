package domain

import (
	"sync"
	"time"
)

type TrackKind string

const (
	TrackKindAudio TrackKind = "audio"
	TrackKindVideo TrackKind = "video"
)

type ParticipantState string

const (
	ParticipantStateJoining      ParticipantState = "joining"
	ParticipantStateConnected    ParticipantState = "connected"
	ParticipantStateDisconnected ParticipantState = "disconnected"
)

type SimulcastLayer struct {
	RID             string
	SSRC            uint32
	RTXSSRC         uint32
	MaxBitrate      uint32
	ScalabilityMode string
}

type Track struct {
	ID                 string
	SID                string
	Name               string
	Source             string
	Width              uint32
	Height             uint32
	PublisherID        string
	PublisherSID       string
	Kind               TrackKind
	SSRC               uint32
	RTXSSRC            uint32
	PayloadType        uint32
	RTXPayloadType     uint32
	MimeType           string
	ClockRate          uint32
	CName              string
	Paused             bool
	SimulcastEncodings []SimulcastLayer
}

type Participant struct {
	mu sync.RWMutex

	ID                    string
	SID                   string
	Name                  string
	State                 ParticipantState
	PublisherTransportID  string
	SubscriberTransportID string
	JoinedAt              time.Time

	// PublishedTracks maps track SID (or ID) to Track
	PublishedTracks map[string]*Track

	// SubscribedConsumerIDs maps consumerID to producerID
	SubscribedConsumerIDs map[string]string
}

func NewParticipant(id, sid, name string) *Participant {
	return &Participant{
		ID:                    id,
		SID:                   sid,
		Name:                  name,
		State:                 ParticipantStateJoining,
		JoinedAt:              time.Now(),
		PublishedTracks:       make(map[string]*Track),
		SubscribedConsumerIDs: make(map[string]string),
	}
}

func (p *Participant) SetConnected() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.State = ParticipantStateConnected
}

func (p *Participant) SetDisconnected() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.State = ParticipantStateDisconnected
}

func (p *Participant) GetState() ParticipantState {
	p.mu.RLock()
	defer p.mu.RUnlock()
	return p.State
}

func (p *Participant) AddTrack(t *Track) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if _, exists := p.PublishedTracks[t.SID]; exists {
		return ErrTrackAlreadyPublished
	}
	p.PublishedTracks[t.SID] = t
	return nil
}

func (p *Participant) GetTrack(sid string) (*Track, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()

	t, ok := p.PublishedTracks[sid]
	if !ok || t == nil {
		return nil, false
	}
	cp := *t
	if len(t.SimulcastEncodings) > 0 {
		cp.SimulcastEncodings = append([]SimulcastLayer(nil), t.SimulcastEncodings...)
	}
	return &cp, true
}

func (p *Participant) RemoveTrack(sid string) (*Track, error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	t, ok := p.PublishedTracks[sid]
	if !ok {
		return nil, ErrTrackNotFound
	}
	delete(p.PublishedTracks, sid)
	return t, nil
}

func (p *Participant) SetTrackMuted(sid string, muted bool) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	t, ok := p.PublishedTracks[sid]
	if !ok {
		return ErrTrackNotFound
	}
	t.Paused = muted
	return nil
}

func (p *Participant) ListTracks() []*Track {
	p.mu.RLock()
	defer p.mu.RUnlock()

	tracks := make([]*Track, 0, len(p.PublishedTracks))
	for _, t := range p.PublishedTracks {
		if t == nil {
			continue
		}
		cp := *t
		if len(t.SimulcastEncodings) > 0 {
			cp.SimulcastEncodings = append([]SimulcastLayer(nil), t.SimulcastEncodings...)
		}
		tracks = append(tracks, &cp)
	}
	return tracks
}

func (p *Participant) AddSubscription(consumerID, producerID string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.SubscribedConsumerIDs[consumerID] = producerID
}

func (p *Participant) RemoveSubscription(consumerID string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	delete(p.SubscribedConsumerIDs, consumerID)
}
