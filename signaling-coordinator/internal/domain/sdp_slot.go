package domain

import (
	"fmt"
	"sync"
)

type NegotiationState string

const (
	NegotiationStateStable           NegotiationState = "stable"
	NegotiationStateWaitingForAnswer NegotiationState = "waiting_for_answer"
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
	SSRC           uint32
	RTXSSRC        uint32
	PayloadType    uint32
	RTXPayloadType uint32
	MimeType       string
	ClockRate      uint32
	CName          string
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
) *SDPSlot {
	t.mu.Lock()
	defer t.mu.Unlock()

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
			slot.SSRC = ssrc
			slot.RTXSSRC = rtxSSRC
			slot.PayloadType = pt
			slot.RTXPayloadType = rtxPT
			slot.ClockRate = clockRate
			slot.MimeType = mimeType
			slot.CName = cname
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
			slot.Active = false
			slot.ConsumerID = ""
			slot.TrackSID = ""
			slot.ProducerID = ""
			return slot, true
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
			slot.Active = false
			slot.ConsumerID = ""
			slot.TrackSID = ""
			slot.ProducerID = ""
			return slot, true
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
