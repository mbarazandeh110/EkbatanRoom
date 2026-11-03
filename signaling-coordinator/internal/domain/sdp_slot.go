package domain

import (
	"fmt"
	"sync"
)

type NegotiationState string

const (
	NegotiationStateStable           NegotiationState = "stable"
	NegotiationStateWaitingForAnswer NegotiationState = "waiting_for_answer"
	NegotiationStateTimedOut         NegotiationState = "timed_out"
	NegotiationStateClosed           NegotiationState = "closed"
)

type SDPSlot struct {
	Index          int
	MID            string
	Kind           TrackKind
	Active         bool
	TrackSID       string
	ProducerID     string
	ConsumerID     string
	PublisherSID   string
	SSRC           uint32
	RTXSSRC        uint32
	PayloadType    uint32
	RTXPayloadType uint32
	MimeType       string
	ClockRate      uint32
	CName          string
	Paused         bool
}

type SDPSlotTable struct {
	mu    sync.RWMutex
	slots []*SDPSlot
}

func NewSDPSlotTable() *SDPSlotTable {
	return &SDPSlotTable{
		slots: make([]*SDPSlot, 0),
	}
}

// AllocOrRecycleSlot allocates an inactive slot of the same kind or appends a new slot to preserve Unified Plan ordering.
func (t *SDPSlotTable) AllocOrRecycleSlot(
	kind TrackKind,
	trackSID string,
	producerID string,
	consumerID string,
	ssrc uint32,
	rtxSSRC uint32,
	pt uint32,
	rtxPT uint32,
	clockRate uint32,
	mimeType string,
	cname string,
	publisherSID ...string,
) *SDPSlot {
	t.mu.Lock()
	defer t.mu.Unlock()

	pubSID := cname
	if len(publisherSID) > 0 && publisherSID[0] != "" {
		pubSID = publisherSID[0]
	}

	// 1. Look for existing slot for this consumer/track to avoid duplicate slots
	for _, slot := range t.slots {
		if slot.ConsumerID == consumerID && slot.Active {
			slot.SSRC = ssrc
			slot.RTXSSRC = rtxSSRC
			slot.PayloadType = pt
			slot.RTXPayloadType = rtxPT
			slot.ClockRate = clockRate
			slot.MimeType = mimeType
			slot.CName = cname
			slot.PublisherSID = pubSID
			return slot
		}
	}

	// 2. Recycle an inactive slot of the same kind if available
	for _, slot := range t.slots {
		if !slot.Active && slot.Kind == kind {
			slot.Active = true
			slot.TrackSID = trackSID
			slot.ProducerID = producerID
			slot.ConsumerID = consumerID
			slot.PublisherSID = pubSID
			slot.SSRC = ssrc
			slot.RTXSSRC = rtxSSRC
			slot.PayloadType = pt
			slot.RTXPayloadType = rtxPT
			slot.ClockRate = clockRate
			slot.MimeType = mimeType
			slot.CName = cname
			slot.Paused = false
			return slot
		}
	}

	// 3. Append new slot
	idx := len(t.slots)
	slot := &SDPSlot{
		Index:          idx,
		MID:            fmt.Sprintf("%d", idx),
		Kind:           kind,
		Active:         true,
		TrackSID:       trackSID,
		ProducerID:     producerID,
		ConsumerID:     consumerID,
		PublisherSID:   pubSID,
		SSRC:           ssrc,
		RTXSSRC:        rtxSSRC,
		PayloadType:    pt,
		RTXPayloadType: rtxPT,
		MimeType:       mimeType,
		ClockRate:      clockRate,
		CName:          cname,
	}
	t.slots = append(t.slots, slot)
	return slot
}

// InactivateSlot marks the slot for the given consumerID as inactive (for recycling).
func (t *SDPSlotTable) InactivateSlot(consumerID string) (*SDPSlot, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()

	for _, slot := range t.slots {
		if slot.ConsumerID == consumerID && slot.Active {
			snapshot := *slot
			slot.Active = false
			slot.ConsumerID = ""
			slot.TrackSID = ""
			slot.ProducerID = ""
			slot.Paused = false
			return &snapshot, true
		}
	}
	return nil, false
}

// InactivateSlotByTrackSID marks the slot for the given trackSID as inactive.
func (t *SDPSlotTable) InactivateSlotByTrackSID(trackSID string) (*SDPSlot, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()

	for _, slot := range t.slots {
		if slot.TrackSID == trackSID && slot.Active {
			snapshot := *slot
			slot.Active = false
			slot.ConsumerID = ""
			slot.TrackSID = ""
			slot.ProducerID = ""
			slot.Paused = false
			return &snapshot, true
		}
	}
	return nil, false
}

func (t *SDPSlotTable) ListSlots() []*SDPSlot {
	t.mu.RLock()
	defer t.mu.RUnlock()

	copied := make([]*SDPSlot, len(t.slots))
	for i, s := range t.slots {
		slotCopy := *s
		copied[i] = &slotCopy
	}
	return copied
}

func (t *SDPSlotTable) ActiveSlots() []*SDPSlot {
	t.mu.RLock()
	defer t.mu.RUnlock()

	active := make([]*SDPSlot, 0, len(t.slots))
	for _, s := range t.slots {
		if s.Active {
			slotCopy := *s
			active = append(active, &slotCopy)
		}
	}
	return active
}

// SetSlotPaused sets the paused state for a slot matching trackSID, producerID, or consumerID.
// It returns the resolved consumerID and whether a matching slot was found.
func (t *SDPSlotTable) SetSlotPaused(id string, paused bool) (string, bool) {
	t.mu.Lock()
	defer t.mu.Unlock()

	for _, slot := range t.slots {
		if slot.TrackSID == id || slot.ProducerID == id || slot.ConsumerID == id {
			slot.Paused = paused
			consumerID := slot.ConsumerID
			if consumerID == "" {
				consumerID = id
			}
			return consumerID, true
		}
	}
	return id, false
}

// ResolveConsumerID finds the consumerID for a given trackSID, producerID, or consumerID.
func (t *SDPSlotTable) ResolveConsumerID(id string) string {
	t.mu.RLock()
	defer t.mu.RUnlock()

	for _, slot := range t.slots {
		if slot.TrackSID == id || slot.ProducerID == id || slot.ConsumerID == id {
			if slot.ConsumerID != "" {
				return slot.ConsumerID
			}
			break
		}
	}
	return id
}
