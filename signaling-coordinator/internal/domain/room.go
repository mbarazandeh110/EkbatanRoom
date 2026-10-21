package domain

import (
	"sync"
	"time"
)

type RoomStatus string

const (
	RoomStatusActive RoomStatus = "active"
	RoomStatusClosed RoomStatus = "closed"
)

type Room struct {
	mu sync.RWMutex

	ID           string
	RouterID     string
	Status       RoomStatus
	CreatedAt    time.Time
	ClosedAt     time.Time
	Participants map[string]*Participant
}

func NewRoom(id, routerID string) *Room {
	return &Room{
		ID:           id,
		RouterID:     routerID,
		Status:       RoomStatusActive,
		CreatedAt:    time.Now(),
		Participants: make(map[string]*Participant),
	}
}

func (r *Room) IsActive() bool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.Status == RoomStatusActive
}

func (r *Room) AddParticipant(p *Participant) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.Status != RoomStatusActive {
		return ErrRoomClosed
	}

	if _, exists := r.Participants[p.ID]; exists {
		return ErrParticipantAlreadyExists
	}

	r.Participants[p.ID] = p
	return nil
}

func (r *Room) GetParticipant(id string) (*Participant, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	p, ok := r.Participants[id]
	return p, ok
}

func (r *Room) RemoveParticipant(id string) (*Participant, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	p, ok := r.Participants[id]
	if !ok {
		return nil, ErrParticipantNotFound
	}
	delete(r.Participants, id)
	return p, nil
}

func (r *Room) ListParticipants() []*Participant {
	r.mu.RLock()
	defer r.mu.RUnlock()

	participants := make([]*Participant, 0, len(r.Participants))
	for _, p := range r.Participants {
		participants = append(participants, p)
	}
	return participants
}

func (r *Room) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.Status == RoomStatusClosed {
		return nil
	}

	r.Status = RoomStatusClosed
	r.ClosedAt = time.Now()
	return nil
}
