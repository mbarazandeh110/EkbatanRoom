package application

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/uuid"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
	"go.opentelemetry.io/otel/trace"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type noopTelemetryMetrics struct{}

func (noopTelemetryMetrics) IncActiveRooms() {}
func (noopTelemetryMetrics) DecActiveRooms() {}
func (noopTelemetryMetrics) IncGlareQueued() {}
func (noopTelemetryMetrics) IncCascadedRooms() {}
func (noopTelemetryMetrics) DecCascadedRooms() {}
func (noopTelemetryMetrics) IncLicenseQuotaRejection(reason string) {}

// defaultWebRTCRtpCapabilitiesJSON provides fallback standard WebRTC codecs (Opus, VP8, H264)
// with standard RTCP feedback (PLI/FIR/NACK) so mediasoup router.can_consume() validation succeeds
// and keyframe requests are enabled for video consumers.
const defaultWebRTCRtpCapabilitiesJSON = `{"codecs":[{"mimeType":"audio/opus","kind":"audio","clockRate":48000,"channels":2,"preferredPayloadType":111},{"mimeType":"video/VP8","kind":"video","clockRate":90000,"preferredPayloadType":96,"rtcpFeedback":[{"type":"nack"},{"type":"nack","parameter":"pli"},{"type":"ccm","parameter":"fir"},{"type":"goog-remb"},{"type":"transport-cc"}]},{"mimeType":"video/H264","kind":"video","clockRate":90000,"preferredPayloadType":125,"parameters":{"level-asymmetry-allowed":1,"packetization-mode":1,"profile-level-id":"42e01f"},"rtcpFeedback":[{"type":"nack"},{"type":"nack","parameter":"pli"},{"type":"ccm","parameter":"fir"},{"type":"goog-remb"},{"type":"transport-cc"}]}],"headerExtensions":[]}`

type pendingOffer struct {
	responder ports.PeerResponderPort
	slots     []*domain.SDPSlot
	sid       string
}

type pendingQualityUpdate struct {
	responder ports.PeerResponderPort
	trackSID  string
	codec     string
	qualities []domain.SubscribedQuality
}

func defaultSubscribedQualities() []domain.SubscribedQuality {
	return []domain.SubscribedQuality{
		{Quality: domain.VideoQualityLow, Enabled: true},
		{Quality: domain.VideoQualityMedium, Enabled: true},
		{Quality: domain.VideoQualityHigh, Enabled: true},
	}
}

func dispatchQualityUpdates(updates []pendingQualityUpdate) {
	for _, u := range updates {
		if u.responder != nil {
			_ = u.responder.SendSubscribedQualityUpdate(u.trackSID, u.codec, u.qualities)
		}
	}
}

type pendingTrackSubscribed struct {
	responder ports.PeerResponderPort
	trackSID  string
}

func dispatchTrackSubscribed(items []pendingTrackSubscribed) {
	for _, it := range items {
		if it.responder != nil {
			_ = it.responder.SendTrackSubscribed(it.trackSID)
		}
	}
}

func (a *RoomActor) preferredLayersForTrack(trackSID string) (spatialLayer int32, temporalLayer int32) {
	for _, pubSession := range a.sessions {
		if tr, ok := pubSession.Participant().GetTrack(trackSID); ok {
			if len(tr.SimulcastEncodings) > 1 {
				return int32(len(tr.SimulcastEncodings) - 1), 2
			}
			return 0, 2
		}
	}
	return 2, 2
}

// RoomActor owns the room lifecycle, participant roster, and coordinates media resources.
type RoomActor struct {
	mu sync.Mutex

	room             *domain.Room
	sessions         map[string]*ParticipantSession
	participantCount atomic.Int64
	inFlightJoins    atomic.Int64
	routerPort       ports.RouterEnginePort
	transportPort    ports.TransportEnginePort
	mediaStreamPort  ports.MediaStreamPort
	metricsPort      ports.TelemetryMetricsPort
	iceServers       []domain.IceServer
	closed           bool

	// Multi-node cascading support (Enterprise)
	primaryNodeID   string
	nodeRegistry    *NodeRegistry
	nodeRouters     map[string]string // nodeID -> routerID
	pipeTransports  map[string]string // nodeID -> primaryPipeTransportID
	pipePairs       map[string]*nodePipePair // secondaryNodeID -> pipePair
	licenseVerifier ports.LicenseVerifierPort

	evictedParticipants map[string]time.Time
	dataProducers       map[string][]string // label -> []dataProducerID
	dataConsumers       map[string][]string // participantID -> []dataConsumerID
}

type nodePipePair struct {
	PrimaryPipeTransportID   string
	SecondaryPipeTransportID string
	SecondaryNodeID          string
	SecondaryRouterID        string
}

func (a *RoomActor) getRtpCapabilitiesJSON() string {
	caps := a.room.GetRtpCapabilitiesJSON()
	return ensureVideoRtcpFeedback(caps)
}

func ensureVideoRtcpFeedback(capsJSON string) string {
	if capsJSON == "" || capsJSON == `{"codecs":[]}` {
		return defaultWebRTCRtpCapabilitiesJSON
	}

	var caps struct {
		Codecs           []map[string]interface{} `json:"codecs"`
		HeaderExtensions []interface{}            `json:"headerExtensions"`
	}
	if err := json.Unmarshal([]byte(capsJSON), &caps); err != nil {
		return capsJSON
	}

	standardVideoFeedback := []map[string]interface{}{
		{"type": "nack"},
		{"type": "nack", "parameter": "pli"},
		{"type": "ccm", "parameter": "fir"},
		{"type": "goog-remb"},
		{"type": "transport-cc"},
	}

	modified := false
	for _, codec := range caps.Codecs {
		kind, _ := codec["kind"].(string)
		mimeType, _ := codec["mimeType"].(string)
		isVideo := strings.EqualFold(kind, "video") || strings.HasPrefix(strings.ToLower(mimeType), "video/")
		if isVideo {
			fb, hasFB := codec["rtcpFeedback"].([]interface{})
			if !hasFB || len(fb) == 0 {
				codec["rtcpFeedback"] = standardVideoFeedback
				modified = true
			}
		}
	}

	if !modified {
		return capsJSON
	}

	updated, err := json.Marshal(caps)
	if err != nil {
		return capsJSON
	}
	return string(updated)
}

func NewRoomActor(
	roomID string,
	routerPort ports.RouterEnginePort,
	transportPort ports.TransportEnginePort,
	mediaStreamPort ports.MediaStreamPort,
	iceServers []domain.IceServer,
	metricsPort ...ports.TelemetryMetricsPort,
) *RoomActor {
	var mp ports.TelemetryMetricsPort = noopTelemetryMetrics{}
	if len(metricsPort) > 0 && metricsPort[0] != nil {
		mp = metricsPort[0]
	}
	return &RoomActor{
		room:            domain.NewRoom(roomID, ""),
		sessions:        make(map[string]*ParticipantSession),
		routerPort:      routerPort,
		transportPort:   transportPort,
		mediaStreamPort: mediaStreamPort,
		metricsPort:     mp,
		iceServers:      iceServers,
		nodeRouters:     make(map[string]string),
		pipeTransports:      make(map[string]string),
		pipePairs:           make(map[string]*nodePipePair),
		evictedParticipants: make(map[string]time.Time),
		dataProducers:       make(map[string][]string),
		dataConsumers:       make(map[string][]string),
	}
}

func (a *RoomActor) SetLicenseVerifier(lv ports.LicenseVerifierPort) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.licenseVerifier = lv
}

func (a *RoomActor) startSpan(ctx context.Context, name string, participantID string, state domain.NegotiationState) (context.Context, trace.Span) {
	attrs := []attribute.KeyValue{
		attribute.String("room.id", a.room.ID),
	}
	if participantID != "" {
		attrs = append(attrs, attribute.String("participant.id", participantID))
	}
	if state != "" {
		attrs = append(attrs, attribute.String("negotiation.state", string(state)))
	}
	return otel.GetTracerProvider().Tracer("ekbatan-room/signaling-coordinator").Start(
		ctx,
		name,
		trace.WithAttributes(attrs...),
	)
}

func (a *RoomActor) RouterID() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.room.RouterID
}

func (a *RoomActor) BeginJoin() {
	a.inFlightJoins.Add(1)
}

func (a *RoomActor) EndJoin() {
	a.inFlightJoins.Add(-1)
}

func (a *RoomActor) IsEmptyAndIdle() bool {
	a.mu.Lock()
	defer a.mu.Unlock()
	return len(a.sessions) == 0 && a.inFlightJoins.Load() == 0
}

func (a *RoomActor) InFlightJoins() int64 {
	return a.inFlightJoins.Load()
}

func (a *RoomActor) SetNodeRegistry(nr *NodeRegistry) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.nodeRegistry = nr
}

func (a *RoomActor) SetPrimaryNodeID(nodeID string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.primaryNodeID = nodeID
}

func (a *RoomActor) collectBroadcastLocked() ([]ports.PeerResponderPort, []*domain.Participant) {
	participants := a.room.ListParticipants()
	responders := make([]ports.PeerResponderPort, 0, len(a.sessions))
	for _, session := range a.sessions {
		if session.responder != nil {
			responders = append(responders, session.responder)
		}
	}
	return responders, participants
}

func dispatchBroadcast(responders []ports.PeerResponderPort, participants []*domain.Participant) {
	for _, r := range responders {
		_ = r.SendParticipantUpdate(participants)
	}
}

func dispatchOffers(offers []pendingOffer) {
	for _, o := range offers {
		if o.responder != nil {
			_ = o.responder.SendSubscriberOffer(o.slots, o.sid)
		}
	}
}

type roomContextKey string

const (
	targetNodeKey  roomContextKey = "targetNodeID"
	previousSIDKey roomContextKey = "previousSID"
)

func WithTargetNode(ctx context.Context, nodeID string) context.Context {
	return context.WithValue(ctx, targetNodeKey, nodeID)
}

func TargetNodeFromContext(ctx context.Context) string {
	if v, ok := ctx.Value(targetNodeKey).(string); ok {
		return v
	}
	return ""
}

func WithPreviousSID(ctx context.Context, sid string) context.Context {
	return context.WithValue(ctx, previousSIDKey, sid)
}

func PreviousSIDFromContext(ctx context.Context) string {
	if v, ok := ctx.Value(previousSIDKey).(string); ok {
		return v
	}
	return ""
}

func (a *RoomActor) Join(
	ctx context.Context,
	participantID string,
	participantName string,
	responder ports.PeerResponderPort,
	targetNodeID ...string,
) (*domain.JoinResult, error) {
	ctx, span := a.startSpan(ctx, "HandleJoin", participantID, domain.NegotiationStateStable)
	defer span.End()

	a.mu.Lock()

	if a.closed || !a.room.IsActive() {
		a.mu.Unlock()
		span.RecordError(domain.ErrRoomClosed)
		span.SetStatus(codes.Error, domain.ErrRoomClosed.Error())
		return nil, domain.ErrRoomClosed
	}

	if _, evicted := a.evictedParticipants[participantID]; evicted {
		a.mu.Unlock()
		span.RecordError(domain.ErrParticipantEvicted)
		span.SetStatus(codes.Error, domain.ErrParticipantEvicted.Error())
		return nil, domain.ErrParticipantEvicted
	}
	if prevSID := PreviousSIDFromContext(ctx); prevSID != "" {
		if _, evicted := a.evictedParticipants[prevSID]; evicted {
			a.mu.Unlock()
			span.RecordError(domain.ErrParticipantEvicted)
			span.SetStatus(codes.Error, domain.ErrParticipantEvicted.Error())
			return nil, domain.ErrParticipantEvicted
		}
	}

	// Early check if participant already exists before provisioning transports
	if _, exists := a.sessions[participantID]; exists {
		a.mu.Unlock()
		span.RecordError(domain.ErrParticipantAlreadyExists)
		span.SetStatus(codes.Error, domain.ErrParticipantAlreadyExists.Error())
		return nil, domain.ErrParticipantAlreadyExists
	}

	if a.licenseVerifier != nil {
		if err := a.licenseVerifier.CheckCanJoinParticipantToRoom(len(a.sessions)); err != nil {
			a.mu.Unlock()
			if a.metricsPort != nil {
				a.metricsPort.IncLicenseQuotaRejection("max_participants_per_room")
			}
			span.RecordError(err)
			span.SetStatus(codes.Error, err.Error())
			return nil, err
		}
	}

	var assignedNodeID string
	if len(targetNodeID) > 0 && targetNodeID[0] != "" {
		assignedNodeID = targetNodeID[0]
	}
	if a.primaryNodeID == "" && assignedNodeID != "" {
		a.primaryNodeID = assignedNodeID
	}

	// Validate cascading license feature before assigning to secondary node
	if assignedNodeID != "" && a.primaryNodeID != "" && assignedNodeID != a.primaryNodeID {
		if a.licenseVerifier != nil {
			if err := a.licenseVerifier.CheckFeatureAllowed(domain.FeatureCascading); err != nil {
				slog.Warn("multi-node cascading not authorized by license, falling back to primary node",
					"room_id", a.room.ID,
					"assigned_node", assignedNodeID,
					"primary_node", a.primaryNodeID,
				)
				assignedNodeID = a.primaryNodeID
			}
		}
	}

	transportPort := a.transportPort
	activeMediaStreamPort := a.mediaStreamPort
	activeRouterID := a.room.RouterID

	// 1. Lazy initialize media router on the SFU node if not yet created
	if a.room.RouterID == "" {
		routerInfo, err := a.routerPort.CreateRouter(ctx, a.room.ID)
		if err != nil {
			a.mu.Unlock()
			wrappedErr := fmt.Errorf("failed to create media router for room %s: %w", a.room.ID, err)
			span.RecordError(wrappedErr)
			span.SetStatus(codes.Error, wrappedErr.Error())
			return nil, wrappedErr
		}
		a.room.RouterID = routerInfo.RouterID
		a.room.SetRtpCapabilitiesJSON(routerInfo.RtpCapabilitiesJSON)
		activeRouterID = routerInfo.RouterID
	}

	// Multi-node cascading (Enterprise): If assigned to secondary node, provision router & PipeTransport
	if assignedNodeID != "" && a.primaryNodeID != "" && assignedNodeID != a.primaryNodeID && a.nodeRegistry != nil {
		if a.nodeRouters == nil {
			a.nodeRouters = make(map[string]string)
		}
		if a.pipeTransports == nil {
			a.pipeTransports = make(map[string]string)
		}
		if a.pipePairs == nil {
			a.pipePairs = make(map[string]*nodePipePair)
		}

		secNode, err := a.nodeRegistry.GetNode(assignedNodeID)
		if err == nil && secNode != nil {
			secClient := secNode.Client()
			secRouterID, ok := a.nodeRouters[assignedNodeID]
			if !ok {
				if rp, ok := secClient.(ports.RouterEnginePort); ok {
					secRouterInfo, err := rp.CreateRouter(ctx, a.room.ID)
					if err == nil && secRouterInfo != nil {
						secRouterID = secRouterInfo.RouterID
						a.nodeRouters[assignedNodeID] = secRouterID
					}
				}
			}

			if secRouterID != "" {
				activeRouterID = secRouterID
				if tp, ok := secClient.(ports.TransportEnginePort); ok {
					transportPort = tp
				}
				if mp, ok := secClient.(ports.MediaStreamPort); ok {
					activeMediaStreamPort = mp
				}

				if _, hasPipe := a.pipePairs[assignedNodeID]; !hasPipe {
					primNode, pErr := a.nodeRegistry.GetNode(a.primaryNodeID)
					if pErr == nil && primNode != nil {
						primPipePort, ok1 := primNode.Client().(ports.PipeTransportPort)
						secPipePort, ok2 := secClient.(ports.PipeTransportPort)
						if ok1 && ok2 {
							primPipe, err1 := primPipePort.CreatePipeTransport(ctx, &domain.CreatePipeTransportCommand{
								RouterID:   a.room.RouterID,
								EnableSrtp: true,
								EnableRtx:  true,
							})
							secPipe, err2 := secPipePort.CreatePipeTransport(ctx, &domain.CreatePipeTransportCommand{
								RouterID:   secRouterID,
								EnableSrtp: true,
								EnableRtx:  true,
							})
							if err1 == nil && err2 == nil && primPipe != nil && secPipe != nil {
								_ = primPipePort.ConnectPipeTransport(ctx, &domain.ConnectPipeTransportCommand{
									RouterID:                 a.room.RouterID,
									PipeTransportID:          primPipe.PipeTransportID,
									RemoteIP:                 secPipe.IP,
									RemotePort:               secPipe.Port,
									RemoteSrtpParametersJSON: secPipe.SrtpParametersJSON,
								})
								_ = secPipePort.ConnectPipeTransport(ctx, &domain.ConnectPipeTransportCommand{
									RouterID:                 secRouterID,
									PipeTransportID:          secPipe.PipeTransportID,
									RemoteIP:                 primPipe.IP,
									RemotePort:               primPipe.Port,
									RemoteSrtpParametersJSON: primPipe.SrtpParametersJSON,
								})
								pair := &nodePipePair{
									PrimaryPipeTransportID:   primPipe.PipeTransportID,
									SecondaryPipeTransportID: secPipe.PipeTransportID,
									SecondaryNodeID:          assignedNodeID,
									SecondaryRouterID:        secRouterID,
								}
								a.pipePairs[assignedNodeID] = pair
								a.pipeTransports[assignedNodeID] = primPipe.PipeTransportID
								if a.metricsPort != nil {
									a.metricsPort.IncCascadedRooms()
								}

								for _, sess := range a.sessions {
									for _, tr := range sess.Participant().ListTracks() {
										_, _ = primPipePort.PipeProduce(ctx, &domain.PipeProduceCommand{
											RouterID:         a.room.RouterID,
											PipeTransportID:  primPipe.PipeTransportID,
											SourceProducerID: tr.ID,
											Kind:             tr.Kind,
											Paused:           tr.Paused,
										})
									}
								}
							}
						}
					}
				}
			}
		}
	}

	// 2. Provision Publisher WebRTC transport on the SFU node
	pubTransport, err := transportPort.CreateTransport(ctx, &domain.CreateTransportCommand{
		RequestID:   uuid.NewString(),
		RouterID:    activeRouterID,
		PeerID:      participantID,
		IsPublisher: true,
		EnableSctp:  true,
	})
	if err != nil {
		a.mu.Unlock()
		wrappedErr := fmt.Errorf("failed to create publisher transport: %w", err)
		span.RecordError(wrappedErr)
		span.SetStatus(codes.Error, wrappedErr.Error())
		return nil, wrappedErr
	}

	// 3. Provision Subscriber WebRTC transport on the SFU node
	subTransport, err := transportPort.CreateTransport(ctx, &domain.CreateTransportCommand{
		RequestID:   uuid.NewString(),
		RouterID:    activeRouterID,
		PeerID:      participantID,
		IsPublisher: false,
		EnableSctp:  true,
	})
	if err != nil {
		_ = transportPort.CloseTransport(ctx, activeRouterID, pubTransport.TransportID)
		a.mu.Unlock()
		wrappedErr := fmt.Errorf("failed to create subscriber transport: %w", err)
		span.RecordError(wrappedErr)
		span.SetStatus(codes.Error, wrappedErr.Error())
		return nil, wrappedErr
	}

	// 4. Create and enroll participant in domain
	sid := fmt.Sprintf("PA_%s", uuid.NewString()[:8])
	p := domain.NewParticipant(participantID, sid, participantName)
	p.PublisherTransportID = pubTransport.TransportID
	p.SubscriberTransportID = subTransport.TransportID
	p.SetConnected()

	if err := a.room.AddParticipant(p); err != nil {
		_ = transportPort.CloseTransport(ctx, activeRouterID, pubTransport.TransportID)
		_ = transportPort.CloseTransport(ctx, activeRouterID, subTransport.TransportID)
		a.mu.Unlock()
		span.RecordError(err)
		span.SetStatus(codes.Error, err.Error())
		return nil, err
	}

	session := NewParticipantSession(p, pubTransport, subTransport, responder)
	session.SetNodeRouting(assignedNodeID, activeRouterID, transportPort, activeMediaStreamPort)
	a.sessions[participantID] = session
	a.participantCount.Add(1)

	joinRes := &domain.JoinResult{
		Room:                a.room,
		Participant:         p,
		PublisherTransport:  pubTransport,
		SubscriberTransport: subTransport,
		IceServers:          a.iceServers,
	}

	// 5. Establish SCTP DataChannels (REQ-028)
	if pubTransport.SctpPort > 0 {
		relRes, err := activeMediaStreamPort.ProduceData(ctx, &domain.ProduceDataCommand{
			RequestID:                uuid.NewString(),
			RouterID:                 activeRouterID,
			TransportID:              pubTransport.TransportID,
			PeerID:                   participantID,
			SctpStreamParametersJSON: `{"streamId":0,"ordered":true}`,
			Label:                    "_reliable",
		})
		if err == nil && relRes != nil {
			a.dataProducers["_reliable"] = append(a.dataProducers["_reliable"], relRes.DataProducerID)
			for otherID, otherSession := range a.sessions {
				if otherID == participantID {
					continue
				}
				otherSub := otherSession.SubTransport()
				if otherSub != nil && otherSub.SctpPort > 0 {
					otherMP := otherSession.MediaStreamPort()
					if otherMP == nil {
						otherMP = a.mediaStreamPort
					}
					otherRouter := otherSession.RouterID()
					if otherRouter == "" {
						otherRouter = a.room.RouterID
					}
					cRes, cErr := otherMP.ConsumeData(ctx, &domain.ConsumeDataCommand{
						RequestID:      uuid.NewString(),
						RouterID:       otherRouter,
						TransportID:    otherSub.TransportID,
						DataProducerID: relRes.DataProducerID,
					})
					if cErr == nil && cRes != nil {
						a.dataConsumers[otherID] = append(a.dataConsumers[otherID], cRes.DataConsumerID)
					}
				}
			}
		}

		lossRes, err := activeMediaStreamPort.ProduceData(ctx, &domain.ProduceDataCommand{
			RequestID:                uuid.NewString(),
			RouterID:                 activeRouterID,
			TransportID:              pubTransport.TransportID,
			PeerID:                   participantID,
			SctpStreamParametersJSON: `{"streamId":1,"ordered":false,"maxPacketLifeTime":0}`,
			Label:                    "_lossy",
		})
		if err == nil && lossRes != nil {
			a.dataProducers["_lossy"] = append(a.dataProducers["_lossy"], lossRes.DataProducerID)
			for otherID, otherSession := range a.sessions {
				if otherID == participantID {
					continue
				}
				otherSub := otherSession.SubTransport()
				if otherSub != nil && otherSub.SctpPort > 0 {
					otherMP := otherSession.MediaStreamPort()
					if otherMP == nil {
						otherMP = a.mediaStreamPort
					}
					otherRouter := otherSession.RouterID()
					if otherRouter == "" {
						otherRouter = a.room.RouterID
					}
					cRes, cErr := otherMP.ConsumeData(ctx, &domain.ConsumeDataCommand{
						RequestID:      uuid.NewString(),
						RouterID:       otherRouter,
						TransportID:    otherSub.TransportID,
						DataProducerID: lossRes.DataProducerID,
					})
					if cErr == nil && cRes != nil {
						a.dataConsumers[otherID] = append(a.dataConsumers[otherID], cRes.DataConsumerID)
					}
				}
			}
		}
	}

	if subTransport.SctpPort > 0 {
		for _, dpIDs := range a.dataProducers {
			for _, dpID := range dpIDs {
				consRes, err := activeMediaStreamPort.ConsumeData(ctx, &domain.ConsumeDataCommand{
					RequestID:      uuid.NewString(),
					RouterID:       activeRouterID,
					TransportID:    subTransport.TransportID,
					DataProducerID: dpID,
				})
				if err == nil && consRes != nil {
					a.dataConsumers[participantID] = append(a.dataConsumers[participantID], consRes.DataConsumerID)
				}
			}
		}
	}

	// 6. Subscribe the newly joined participant to all already-published tracks in the room (Late Joiner support)
	consumedExistingCount := 0
	var qualityUpdatesToSend []pendingQualityUpdate
	var trackSubscribedToSend []pendingTrackSubscribed
	for otherID, otherSession := range a.sessions {
		if otherID == participantID {
			continue
		}
		otherParticipant := otherSession.Participant()
		for _, t := range otherParticipant.ListTracks() {
			consRes, err := activeMediaStreamPort.Consume(ctx, &domain.ConsumeCommand{
				RequestID:             uuid.NewString(),
				RouterID:              activeRouterID,
				SubscriberTransportID: p.SubscriberTransportID,
				ProducerID:            t.ID,
				RtpCapabilitiesJSON:   a.getRtpCapabilitiesJSON(),
				Paused:                false,
			})
			if err != nil {
				continue
			}

			if t.Kind == domain.TrackKindVideo {
				if otherSession.Responder() != nil {
					qualityUpdatesToSend = append(qualityUpdatesToSend, pendingQualityUpdate{
						responder: otherSession.Responder(),
						trackSID:  t.SID,
						codec:     t.MimeType,
						qualities: defaultSubscribedQualities(),
					})
					trackSubscribedToSend = append(trackSubscribedToSend, pendingTrackSubscribed{
						responder: otherSession.Responder(),
						trackSID:  t.SID,
					})
				}
			}

			pubSID := t.PublisherSID
			if pubSID == "" {
				pubSID = otherParticipant.SID
			}
			cname := t.CName
			if cname == "" {
				cname = pubSID
			}

			session.SlotTable().AllocOrRecycleSlot(
				t.Kind,
				t.SID,
				t.ID,
				consRes.ConsumerID,
				consRes.SSRC,
				consRes.RTXSSRC,
				consRes.PayloadType,
				consRes.RTXPayloadType,
				consRes.ClockRate,
				consRes.CodecMimeType,
				cname,
				pubSID,
			)
			p.AddSubscription(consRes.ConsumerID, t.ID)
			consumedExistingCount++
		}
	}

	var offerToSend *pendingOffer
	hasSubscriberOffer := (consumedExistingCount > 0) || (subTransport.SctpPort > 0)
	if hasSubscriberOffer && session.State() == domain.NegotiationStateStable {
		if err := session.StartOfferTransition(); err == nil {
			if session.Responder() != nil {
				offerToSend = &pendingOffer{
					responder: session.Responder(),
					slots:     session.SlotTable().ListSlots(),
					sid:       p.SID,
				}
			}
		}
	}

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()

	a.mu.Unlock()

	// Notify new participant via responder outside a.mu.Lock()
	if responder != nil {
		if err := responder.SendJoinResponse(joinRes); err != nil {
			_ = a.Leave(ctx, participantID)
			wrappedErr := fmt.Errorf("failed to send join response: %w", err)
			span.RecordError(wrappedErr)
			span.SetStatus(codes.Error, wrappedErr.Error())
			return nil, wrappedErr
		}
	}

	// Broadcast participant joined update to existing participants
	dispatchBroadcast(broadcastResponders, broadcastParts)

	if offerToSend != nil {
		dispatchOffers([]pendingOffer{*offerToSend})
	}
	dispatchQualityUpdates(qualityUpdatesToSend)
	dispatchTrackSubscribed(trackSubscribedToSend)

	return joinRes, nil
}

func (a *RoomActor) leaveLocked(ctx context.Context, participantID string) (*ParticipantSession, *domain.Participant, []pendingOffer, []ports.PeerResponderPort, []*domain.Participant, error) {
	session, exists := a.sessions[participantID]
	if !exists {
		return nil, nil, nil, nil, nil, domain.ErrParticipantNotFound
	}

	p := session.Participant()
	p.SetDisconnected()
	delete(a.sessions, participantID)
	a.participantCount.Add(-1)
	_, _ = a.room.RemoveParticipant(participantID)
	delete(a.dataConsumers, participantID)

	// Inactivate all slots for this participant's tracks on remaining participants in a single batch (REQ-018)
	var offersToSend []pendingOffer
	leavingTracks := p.ListTracks()
	for _, otherSession := range a.sessions {
		inactivatedCount := 0
		for _, track := range leavingTracks {
			if slot, ok := otherSession.SlotTable().InactivateSlotByTrackSID(track.SID); ok {
				inactivatedCount++
				otherSession.Participant().RemoveSubscription(slot.ConsumerID)
			}
		}
		if inactivatedCount > 0 {
			if otherSession.State() == domain.NegotiationStateStable {
				if err := otherSession.StartOfferTransition(); err == nil {
					if otherSession.Responder() != nil {
						offersToSend = append(offersToSend, pendingOffer{
							responder: otherSession.Responder(),
							slots:     otherSession.SlotTable().ListSlots(),
							sid:       otherSession.Participant().SID,
						})
					}
				}
			} else {
				otherSession.MarkPendingRenegotiation()
			}
		}
	}

	if a.room.RouterID != "" {
		if p.PublisherTransportID != "" {
			_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.PublisherTransportID)
		}
		if p.SubscriberTransportID != "" {
			_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.SubscriberTransportID)
		}
	}

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()
	// Include the disconnected participant so remaining clients drop their UI tile (LiveKit contract)
	broadcastParts = append(broadcastParts, p)

	return session, p, offersToSend, broadcastResponders, broadcastParts, nil
}

func (a *RoomActor) Leave(ctx context.Context, participantID string) error {
	a.mu.Lock()

	if a.closed {
		a.mu.Unlock()
		return nil // Ignore late leave after close
	}

	session, _, offersToSend, broadcastResponders, broadcastParts, err := a.leaveLocked(ctx, participantID)
	if err != nil {
		a.mu.Unlock()
		return err
	}
	_ = session.Close()

	a.mu.Unlock()

	dispatchBroadcast(broadcastResponders, broadcastParts)
	dispatchOffers(offersToSend)

	return nil
}

func (a *RoomActor) RemoveParticipant(ctx context.Context, participantID string) error {
	a.mu.Lock()

	if a.closed {
		a.mu.Unlock()
		return nil
	}

	session, exists := a.sessions[participantID]
	if !exists {
		a.mu.Unlock()
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()
	a.evictedParticipants[participantID] = time.Now()
	if p.SID != "" {
		a.evictedParticipants[p.SID] = time.Now()
	}

	responder := session.Responder()
	if responder != nil {
		_ = responder.SendLeave(false, domain.DisconnectReasonUserRejected)
	}

	_, _, offersToSend, broadcastResponders, broadcastParts, err := a.leaveLocked(ctx, participantID)
	if err != nil {
		a.mu.Unlock()
		return err
	}

	a.mu.Unlock()

	// Wait brief moment so the LeaveRequest packet is flushed over TCP before terminating the websocket
	time.Sleep(10 * time.Millisecond)
	_ = session.Close()

	dispatchBroadcast(broadcastResponders, broadcastParts)
	dispatchOffers(offersToSend)

	return nil
}

func (a *RoomActor) IsEvicted(participantID, sid string) bool {
	a.mu.Lock()
	defer a.mu.Unlock()

	if _, ok := a.evictedParticipants[participantID]; ok {
		return true
	}
	if sid != "" {
		if _, ok := a.evictedParticipants[sid]; ok {
			return true
		}
	}
	return false
}

func (a *RoomActor) ParticipantCount() int {
	return int(a.participantCount.Load())
}

func (a *RoomActor) GetSession(participantID string) (*ParticipantSession, bool) {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, ok := a.sessions[participantID]
	return session, ok
}

func (a *RoomActor) ListSessions() []*ParticipantSession {
	a.mu.Lock()
	defer a.mu.Unlock()

	sessions := make([]*ParticipantSession, 0, len(a.sessions))
	for _, s := range a.sessions {
		sessions = append(sessions, s)
	}
	return sessions
}

func (a *RoomActor) Room() *domain.Room {
	return a.room
}

func (a *RoomActor) UpdateParticipantMetadata(participantID, metadata string) error {
	a.mu.Lock()

	session, ok := a.sessions[participantID]
	if !ok {
		a.mu.Unlock()
		return domain.ErrParticipantNotFound
	}
	session.Participant().SetMetadata(metadata)

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()
	a.mu.Unlock()

	dispatchBroadcast(broadcastResponders, broadcastParts)
	return nil
}

func (a *RoomActor) UpdateParticipant(participantID, metadata, name string) error {
	a.mu.Lock()

	session, ok := a.sessions[participantID]
	if !ok {
		a.mu.Unlock()
		return domain.ErrParticipantNotFound
	}
	if name != "" {
		session.Participant().SetName(name)
	}
	if metadata != "" {
		session.Participant().SetMetadata(metadata)
	}

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()
	a.mu.Unlock()

	dispatchBroadcast(broadcastResponders, broadcastParts)
	return nil
}

func (a *RoomActor) SendData(ctx context.Context, data []byte, topic string, destinationIdentities []string) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.closed || !a.room.IsActive() {
		return domain.ErrRoomClosed
	}

	if a.room.RouterID == "" && a.routerPort != nil {
		routerInfo, err := a.routerPort.CreateRouter(ctx, a.room.ID)
		if err == nil && routerInfo != nil {
			a.room.RouterID = routerInfo.RouterID
			a.room.SetRtpCapabilitiesJSON(routerInfo.RtpCapabilitiesJSON)
		}
	}

	if a.mediaStreamPort != nil && a.room.RouterID != "" {
		_ = a.mediaStreamPort.SendDirectData(ctx, &domain.SendDirectDataCommand{
			RouterID: a.room.RouterID,
			Label:    "_reliable",
			Protocol: "",
			Payload:  data,
		})
	}

	if a.nodeRegistry != nil {
		for secNodeID, secRouterID := range a.nodeRouters {
			if secNode, err := a.nodeRegistry.GetNode(secNodeID); err == nil && secNode != nil {
				if mp, ok := secNode.Client().(ports.MediaStreamPort); ok && mp != nil {
					_ = mp.SendDirectData(ctx, &domain.SendDirectDataCommand{
						RouterID: secRouterID,
						Label:    "_reliable",
						Protocol: "",
						Payload:  data,
					})
				}
			}
		}
	}

	return nil
}

func (a *RoomActor) UpdateRoomMetadata(metadata string) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	a.room.SetMetadata(metadata)
	return nil
}

func (a *RoomActor) PublishTracks(
	ctx context.Context,
	participantID string,
	dtlsRole string,
	fingerprints []domain.DtlsFingerprintInfo,
	tracks []domain.TrackPublishCommand,
) (*domain.TransportInfo, error) {
	ctx, span := a.startSpan(ctx, "HandleOffer", participantID, domain.NegotiationStateStable)
	defer span.End()

	a.mu.Lock()

	session, exists := a.sessions[participantID]
	if !exists {
		a.mu.Unlock()
		span.RecordError(domain.ErrParticipantNotFound)
		span.SetStatus(codes.Error, domain.ErrParticipantNotFound.Error())
		return nil, domain.ErrParticipantNotFound
	}
	p := session.Participant()

	// 1. Connect Publisher Transport on the SFU (once per transport lifecycle)
	if !session.PubTransportConnected() {
		err := a.transportPort.ConnectTransport(ctx, &domain.ConnectTransportCommand{
			RouterID:           a.room.RouterID,
			TransportID:        p.PublisherTransportID,
			DtlsRole:           dtlsRole,
			RemoteFingerprints: fingerprints,
		})
		if err != nil {
			a.mu.Unlock()
			wrappedErr := fmt.Errorf("failed to connect publisher transport: %w", err)
			span.RecordError(wrappedErr)
			span.SetStatus(codes.Error, wrappedErr.Error())
			return nil, wrappedErr
		}
		session.SetPubTransportConnected(true)
	}

	publishedTracks := make([]*domain.Track, 0, len(tracks))

	// 2. Produce each NEW track on SFU node
	for _, tc := range tracks {
		var alreadyPublished *domain.Track
		for _, existing := range p.ListTracks() {
			if (tc.SSRC > 0 && existing.SSRC == tc.SSRC) || (tc.TrackSID != "" && existing.SID == tc.TrackSID) {
				alreadyPublished = existing
				break
			}
		}
		if alreadyPublished != nil {
			continue
		}

		trackSID := tc.TrackSID
		if trackSID == "" {
			trackSID = fmt.Sprintf("TR_%s", uuid.NewString()[:8])
		}
		reqID := uuid.NewString()

		prodRes, err := a.mediaStreamPort.Produce(ctx, &domain.ProduceCommand{
			RequestID:         reqID,
			RouterID:          a.room.RouterID,
			TransportID:       p.PublisherTransportID,
			PeerID:            participantID,
			Kind:              tc.Kind,
			RtpParametersJSON: tc.RtpParametersJSON,
			Paused:            false,
		})
		if err != nil {
			a.mu.Unlock()
			wrappedErr := fmt.Errorf("failed to produce track %s: %w", tc.Kind, err)
			span.RecordError(wrappedErr)
			span.SetStatus(codes.Error, wrappedErr.Error())
			return nil, wrappedErr
		}

		trackName := tc.Name
		if trackName == "" {
			trackName = prodRes.ProducerID
		}

		trackMime := tc.Codec
		if trackMime == "" {
			if tc.Kind == domain.TrackKindVideo {
				trackMime = "video/vp8"
			} else {
				trackMime = "audio/opus"
			}
		} else {
			// REQ-005: Dynamically normalize published track MIME type (e.g. video/vp8 or video/h264)
			if tc.Kind == domain.TrackKindVideo && !strings.HasPrefix(strings.ToLower(trackMime), "video/") {
				trackMime = "video/" + trackMime
			} else if tc.Kind == domain.TrackKindAudio && !strings.HasPrefix(strings.ToLower(trackMime), "audio/") {
				trackMime = "audio/" + trackMime
			}
		}

		encodings := prodRes.Encodings
		if len(encodings) == 0 && len(tc.SimulcastRIDs) > 0 {
			encodings = make([]domain.SimulcastLayer, 0, len(tc.SimulcastRIDs))
			for _, rid := range tc.SimulcastRIDs {
				encodings = append(encodings, domain.SimulcastLayer{RID: rid})
			}
		}

		domainTrack := &domain.Track{
			ID:                 prodRes.ProducerID,
			SID:                trackSID,
			Name:               trackName,
			Source:             tc.Source,
			Width:              tc.Width,
			Height:             tc.Height,
			PublisherID:        participantID,
			PublisherSID:       p.SID,
			Kind:               tc.Kind,
			SSRC:               tc.SSRC,
			RTXSSRC:            tc.RTXSSRC,
			PayloadType:        tc.PayloadType,
			RTXPayloadType:     tc.RTXPayloadType,
			MimeType:           trackMime,
			ClockRate:          tc.ClockRate,
			CName:              tc.CName,
			SimulcastEncodings: encodings,
		}
		_ = p.AddTrack(domainTrack)
		publishedTracks = append(publishedTracks, domainTrack)

		// Bi-directional multi-node cascading track forwarding
		if len(a.pipePairs) > 0 && a.nodeRegistry != nil {
			pubNodeID := a.primaryNodeID
			if pubSess, ok := a.sessions[participantID]; ok && pubSess.NodeID() != "" {
				pubNodeID = pubSess.NodeID()
			}

			if pubNodeID == a.primaryNodeID || pubNodeID == "" {
				// Published on Primary Node -> Forward to all Secondary Nodes
				if primNode, err := a.nodeRegistry.GetNode(a.primaryNodeID); err == nil && primNode != nil {
					if pipePort, ok := primNode.Client().(ports.PipeTransportPort); ok {
						for _, pair := range a.pipePairs {
							_, _ = pipePort.PipeProduce(ctx, &domain.PipeProduceCommand{
								RouterID:         a.room.RouterID,
								PipeTransportID:  pair.PrimaryPipeTransportID,
								SourceProducerID: prodRes.ProducerID,
								Kind:             tc.Kind,
								Paused:           false,
							})
						}
					}
				}
			} else {
				// Published on Secondary Node -> Forward to Primary Node
				if pair, ok := a.pipePairs[pubNodeID]; ok {
					if secNode, err := a.nodeRegistry.GetNode(pubNodeID); err == nil && secNode != nil {
						if secPipePort, ok := secNode.Client().(ports.PipeTransportPort); ok {
							_, _ = secPipePort.PipeProduce(ctx, &domain.PipeProduceCommand{
								RouterID:         pair.SecondaryRouterID,
								PipeTransportID:  pair.SecondaryPipeTransportID,
								SourceProducerID: prodRes.ProducerID,
								Kind:             tc.Kind,
								Paused:           false,
							})
						}
					}
					// Forward from Primary Node to any other Secondary Nodes in the room
					if primNode, err := a.nodeRegistry.GetNode(a.primaryNodeID); err == nil && primNode != nil {
						if primPipePort, ok := primNode.Client().(ports.PipeTransportPort); ok {
							for secID, otherPair := range a.pipePairs {
								if secID != pubNodeID {
									_, _ = primPipePort.PipeProduce(ctx, &domain.PipeProduceCommand{
										RouterID:         a.room.RouterID,
										PipeTransportID:  otherPair.PrimaryPipeTransportID,
										SourceProducerID: prodRes.ProducerID,
										Kind:             tc.Kind,
										Paused:           false,
									})
								}
							}
						}
					}
				}
			}
		}
	}

	var offersToSend []pendingOffer

	if len(publishedTracks) > 0 {
		// 3. Coordinate subscriptions on all OTHER participants in the room
		for _, otherSession := range a.sessions {
			if otherSession.Participant().ID == participantID {
				continue
			}

			if otherSession.State() == domain.NegotiationStateWaitingForAnswer {
				// Concurrency / Glare handling: queue tracks until current negotiation completes
				if a.metricsPort != nil {
					a.metricsPort.IncGlareQueued()
				}
				for _, t := range publishedTracks {
					otherSession.QueuePendingTrack(t)
				}
				continue
			}

			subRouterID := otherSession.RouterID()
			if subRouterID == "" {
				subRouterID = a.room.RouterID
			}
			subMediaPort := otherSession.MediaStreamPort()
			if subMediaPort == nil {
				subMediaPort = a.mediaStreamPort
			}

			// Other session is Stable -> consume immediately
			for _, t := range publishedTracks {
				consRes, err := subMediaPort.Consume(ctx, &domain.ConsumeCommand{
					RequestID:             uuid.NewString(),
					RouterID:              subRouterID,
					SubscriberTransportID: otherSession.Participant().SubscriberTransportID,
					ProducerID:            t.ID,
					RtpCapabilitiesJSON:   a.getRtpCapabilitiesJSON(),
					Paused:                false,
				})
				if err != nil {
					continue
				}

				cname := t.CName
				if cname == "" {
					cname = p.SID
				}

				otherSession.SlotTable().AllocOrRecycleSlot(
					t.Kind,
					t.SID,
					t.ID,
					consRes.ConsumerID,
					consRes.SSRC,
					consRes.RTXSSRC,
					consRes.PayloadType,
					consRes.RTXPayloadType,
					consRes.ClockRate,
					consRes.CodecMimeType,
					cname,
					p.SID,
				)
				otherSession.Participant().AddSubscription(consRes.ConsumerID, t.ID)

				if t.Kind == domain.TrackKindVideo && otherSession.SubTransportConnected() && subMediaPort != nil {
					spatial, temporal := a.preferredLayersForTrack(t.SID)
					_ = subMediaPort.SetConsumerPreferredLayers(ctx, &domain.SetPreferredLayersCommand{
						RouterID:      subRouterID,
						ConsumerID:    consRes.ConsumerID,
						SpatialLayer:  spatial,
						TemporalLayer: temporal,
					})
				}
			}

			if err := otherSession.StartOfferTransition(); err == nil {
				if otherSession.Responder() != nil {
					offersToSend = append(offersToSend, pendingOffer{
						responder: otherSession.Responder(),
						slots:     otherSession.SlotTable().ListSlots(),
						sid:       otherSession.Participant().SID,
					})
				}
			}
		}
	}

	var qualityUpdatesToSend []pendingQualityUpdate
	if session.Responder() != nil {
		for _, t := range publishedTracks {
			if t.Kind == domain.TrackKindVideo {
				qualityUpdatesToSend = append(qualityUpdatesToSend, pendingQualityUpdate{
					responder: session.Responder(),
					trackSID:  t.SID,
					codec:     t.MimeType,
					qualities: defaultSubscribedQualities(),
				})
			}
		}
	}

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()

	pubTrans := session.PubTransport()
	a.mu.Unlock()

	dispatchBroadcast(broadcastResponders, broadcastParts)
	dispatchOffers(offersToSend)
	dispatchQualityUpdates(qualityUpdatesToSend)

	return pubTrans, nil
}

func (a *RoomActor) HandleSubscriberAnswer(ctx context.Context, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo) error {
	ctx, span := a.startSpan(ctx, "TriggerSubscriberNegotiation", participantID, domain.NegotiationStateWaitingForAnswer)
	defer span.End()

	a.mu.Lock()

	session, exists := a.sessions[participantID]
	if !exists {
		a.mu.Unlock()
		span.RecordError(domain.ErrParticipantNotFound)
		span.SetStatus(codes.Error, domain.ErrParticipantNotFound.Error())
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()

	subRouterID := session.RouterID()
	if subRouterID == "" {
		subRouterID = a.room.RouterID
	}
	subTransportPort := session.TransportPort()
	if subTransportPort == nil {
		subTransportPort = a.transportPort
	}
	subMediaPort := session.MediaStreamPort()
	if subMediaPort == nil {
		subMediaPort = a.mediaStreamPort
	}

	// Connect Subscriber Transport on the SFU (once per transport lifecycle)
	if len(fingerprints) > 0 && !session.SubTransportConnected() {
		role := dtlsRole
		if role == "" {
			role = "client"
		}
		err := subTransportPort.ConnectTransport(ctx, &domain.ConnectTransportCommand{
			RouterID:           subRouterID,
			TransportID:        p.SubscriberTransportID,
			DtlsRole:           role,
			RemoteFingerprints: fingerprints,
		})
		if err == nil {
			session.SetSubTransportConnected(true)
		}
	}

	if err := session.AnswerReceivedTransition(); err != nil {
		a.mu.Unlock()
		span.RecordError(err)
		span.SetStatus(codes.Error, err.Error())
		return err
	}

	// Drain any queued pending tracks or deferred ICE restart offer or pending renegotiation
	pending := session.PopPendingTracks()
	needsIceOffer := session.ConsumePendingIceRestart()
	needsRenegotiation := session.ConsumePendingRenegotiation()

	if len(pending) > 0 {
		for _, t := range pending {
			consRes, err := subMediaPort.Consume(ctx, &domain.ConsumeCommand{
				RequestID:             uuid.NewString(),
				RouterID:              subRouterID,
				SubscriberTransportID: p.SubscriberTransportID,
				ProducerID:            t.ID,
				RtpCapabilitiesJSON:   a.getRtpCapabilitiesJSON(),
				Paused:                false,
			})
			if err != nil {
				continue
			}

			pubSID := t.PublisherSID
			if pubSID == "" {
				if pubSess, ok := a.sessions[t.PublisherID]; ok {
					pubSID = pubSess.Participant().SID
				} else {
					pubSID = p.SID
				}
			}
			cname := t.CName
			if cname == "" {
				cname = pubSID
			}

			session.SlotTable().AllocOrRecycleSlot(
				t.Kind,
				t.SID,
				t.ID,
				consRes.ConsumerID,
				consRes.SSRC,
				consRes.RTXSSRC,
				consRes.PayloadType,
				consRes.RTXPayloadType,
				consRes.ClockRate,
				consRes.CodecMimeType,
				cname,
				pubSID,
			)
			p.AddSubscription(consRes.ConsumerID, t.ID)
		}
	}

	var qualityUpdatesToSend []pendingQualityUpdate
	var trackSubscribedToSend []pendingTrackSubscribed
	seenTracks := make(map[string]bool)

	for _, slot := range session.SlotTable().ListSlots() {
		if !slot.Active || slot.Kind != domain.TrackKindVideo || slot.ConsumerID == "" {
			continue
		}

		// REQ-004: Activate optimal preferred spatial and temporal layers on SFU consumer to unlock media flow
		spatial, temporal := a.preferredLayersForTrack(slot.TrackSID)
		if subMediaPort != nil {
			if err := subMediaPort.SetConsumerPreferredLayers(ctx, &domain.SetPreferredLayersCommand{
				RouterID:      subRouterID,
				ConsumerID:    slot.ConsumerID,
				SpatialLayer:  spatial,
				TemporalLayer: temporal,
			}); err != nil {
				slog.WarnContext(ctx, "failed to set consumer preferred layers on answer",
					"room_id", a.room.ID,
					"consumer_id", slot.ConsumerID,
					"spatial_layer", spatial,
					"temporal_layer", temporal,
					"error", err,
				)
			}
		}

		if !seenTracks[slot.TrackSID] {
			seenTracks[slot.TrackSID] = true
			for _, pubSession := range a.sessions {
				if tr, ok := pubSession.Participant().GetTrack(slot.TrackSID); ok {
					if pubSession.Responder() != nil {
						qualityUpdatesToSend = append(qualityUpdatesToSend, pendingQualityUpdate{
							responder: pubSession.Responder(),
							trackSID:  slot.TrackSID,
							codec:     tr.MimeType,
							qualities: defaultSubscribedQualities(),
						})
						trackSubscribedToSend = append(trackSubscribedToSend, pendingTrackSubscribed{
							responder: pubSession.Responder(),
							trackSID:  slot.TrackSID,
						})
					}
					break
				}
			}
		}
	}

	var offerToSend *pendingOffer
	if len(pending) > 0 || needsIceOffer || needsRenegotiation {
		if session.State() == domain.NegotiationStateStable {
			if err := session.StartOfferTransition(); err == nil {
				if session.Responder() != nil {
					offerToSend = &pendingOffer{
						responder: session.Responder(),
						slots:     session.SlotTable().ListSlots(),
						sid:       p.SID,
					}
				}
			}
		}
	}

	a.mu.Unlock()

	if offerToSend != nil {
		dispatchOffers([]pendingOffer{*offerToSend})
	}
	dispatchQualityUpdates(qualityUpdatesToSend)
	dispatchTrackSubscribed(trackSubscribedToSend)

	return nil
}

func (a *RoomActor) HandleTrickle(ctx context.Context, participantID string, candidateJSON string, target int) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	_, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}
	return nil
}

func (a *RoomActor) SetTrackMuted(ctx context.Context, participantID, trackSID string, muted bool) error {
	a.mu.Lock()

	session, exists := a.sessions[participantID]
	if !exists {
		a.mu.Unlock()
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()
	if err := p.SetTrackMuted(trackSID, muted); err != nil {
		a.mu.Unlock()
		return err
	}

	track, ok := p.GetTrack(trackSID)
	routerID := a.room.RouterID
	mediaPort := a.mediaStreamPort
	if session != nil {
		if rID := session.RouterID(); rID != "" {
			routerID = rID
		}
		if mp := session.MediaStreamPort(); mp != nil {
			mediaPort = mp
		}
	}

	if ok && track.ID != "" && mediaPort != nil {
		_ = mediaPort.SetMediaPaused(ctx, &domain.SetMediaPausedCommand{
			RouterID:     routerID,
			ResourceType: domain.ResourceTypeProducer,
			ResourceID:   track.ID,
			Paused:       muted,
		})
	}

	type trackMuteItem struct {
		responder ports.PeerResponderPort
		trackSID  string
		muted     bool
	}
	var trackMutes []trackMuteItem
	for _, s := range a.sessions {
		if s.Responder() != nil {
			trackMutes = append(trackMutes, trackMuteItem{
				responder: s.Responder(),
				trackSID:  trackSID,
				muted:     muted,
			})
		}
	}

	broadcastResponders, broadcastParts := a.collectBroadcastLocked()

	a.mu.Unlock()

	for _, tm := range trackMutes {
		_ = tm.responder.SendTrackMuted(tm.trackSID, tm.muted)
	}
	dispatchBroadcast(broadcastResponders, broadcastParts)

	return nil
}

func (a *RoomActor) SetConsumerPaused(ctx context.Context, participantID, trackSID string, paused bool) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	actualConsumerID := trackSID
	if session != nil && session.SlotTable() != nil {
		actualConsumerID, _ = session.SlotTable().SetSlotPaused(trackSID, paused)
	}

	if actualConsumerID == "" {
		return domain.ErrTrackNotFound
	}

	routerID := session.RouterID()
	if routerID == "" {
		routerID = a.room.RouterID
	}
	mediaPort := session.MediaStreamPort()
	if mediaPort == nil {
		mediaPort = a.mediaStreamPort
	}

	return mediaPort.SetMediaPaused(ctx, &domain.SetMediaPausedCommand{
		RouterID:     routerID,
		ResourceType: domain.ResourceTypeConsumer,
		ResourceID:   actualConsumerID,
		Paused:       paused,
	})
}

func (a *RoomActor) SetConsumerLayers(ctx context.Context, participantID, consumerID string, spatial, temporal int32) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	actualConsumerID := consumerID
	if session != nil && session.SlotTable() != nil {
		actualConsumerID = session.SlotTable().ResolveConsumerID(consumerID)
	}

	actualTemporal := temporal
	if actualTemporal < 0 {
		actualTemporal = 2
	}

	routerID := session.RouterID()
	if routerID == "" {
		routerID = a.room.RouterID
	}
	mediaPort := session.MediaStreamPort()
	if mediaPort == nil {
		mediaPort = a.mediaStreamPort
	}

	return mediaPort.SetConsumerPreferredLayers(ctx, &domain.SetPreferredLayersCommand{
		RouterID:      routerID,
		ConsumerID:    actualConsumerID,
		SpatialLayer:  spatial,
		TemporalLayer: actualTemporal,
	})
}

func (a *RoomActor) RestartPublisherIce(ctx context.Context, participantID string) (*domain.IceInfo, error) {
	ctx, span := a.startSpan(ctx, "HandleIceRestart.Publisher", participantID, domain.NegotiationStateStable)
	defer span.End()

	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		span.RecordError(domain.ErrParticipantNotFound)
		span.SetStatus(codes.Error, domain.ErrParticipantNotFound.Error())
		return nil, domain.ErrParticipantNotFound
	}

	p := session.Participant()
	pubTransport := session.PubTransport()

	routerID := session.RouterID()
	if routerID == "" {
		routerID = a.room.RouterID
	}
	transportPort := session.TransportPort()
	if transportPort == nil {
		transportPort = a.transportPort
	}

	iceInfo, err := transportPort.RestartIce(ctx, routerID, p.PublisherTransportID)
	if err != nil {
		span.RecordError(err)
		span.SetStatus(codes.Error, err.Error())
		return nil, err
	}
	if pubTransport != nil {
		pubTransport.IceUfrag = iceInfo.IceUfrag
		pubTransport.IcePwd = iceInfo.IcePwd
	}
	return iceInfo, nil
}

func (a *RoomActor) RestartSubscriberIce(ctx context.Context, participantID string) (*domain.IceInfo, error) {
	ctx, span := a.startSpan(ctx, "HandleIceRestart.Subscriber", participantID, domain.NegotiationStateStable)
	defer span.End()

	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		span.RecordError(domain.ErrParticipantNotFound)
		span.SetStatus(codes.Error, domain.ErrParticipantNotFound.Error())
		return nil, domain.ErrParticipantNotFound
	}

	p := session.Participant()

	routerID := session.RouterID()
	if routerID == "" {
		routerID = a.room.RouterID
	}
	transportPort := session.TransportPort()
	if transportPort == nil {
		transportPort = a.transportPort
	}

	iceInfo, err := transportPort.RestartIce(ctx, routerID, p.SubscriberTransportID)
	if err != nil {
		span.RecordError(err)
		span.SetStatus(codes.Error, err.Error())
		return nil, err
	}

	subTransport := session.SubTransport()
	if subTransport != nil {
		subTransport.IceUfrag = iceInfo.IceUfrag
		subTransport.IcePwd = iceInfo.IcePwd
	}

	// Send updated subscriber offer with new ICE credentials or queue if WaitingForAnswer
	if session.State() == domain.NegotiationStateStable {
		if err := session.StartOfferTransition(); err == nil {
			if session.Responder() != nil {
				_ = session.Responder().SendSubscriberOffer(session.SlotTable().ListSlots(), p.SID)
			}
		}
	} else if session.State() == domain.NegotiationStateWaitingForAnswer {
		session.MarkPendingIceRestart()
	}

	return iceInfo, nil
}

func (a *RoomActor) Close(ctx context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.closed {
		return nil
	}
	a.closed = true
	a.participantCount.Store(0)

	for pid, session := range a.sessions {
		p := session.Participant()
		_ = session.Close()

		routerID := session.RouterID()
		if routerID == "" {
			routerID = a.room.RouterID
		}
		transportPort := session.TransportPort()
		if transportPort == nil {
			transportPort = a.transportPort
		}

		if routerID != "" {
			if p.PublisherTransportID != "" {
				_ = transportPort.CloseTransport(ctx, routerID, p.PublisherTransportID)
			}
			if p.SubscriberTransportID != "" {
				_ = transportPort.CloseTransport(ctx, routerID, p.SubscriberTransportID)
			}
		}
		delete(a.sessions, pid)
	}

	// Close secondary routers if cascaded
	if a.nodeRegistry != nil {
		for secNodeID, secRouterID := range a.nodeRouters {
			if secNode, err := a.nodeRegistry.GetNode(secNodeID); err == nil && secNode != nil {
				if rp, ok := secNode.Client().(ports.RouterEnginePort); ok {
					_ = rp.CloseRouter(ctx, secRouterID)
				}
			}
		}
	}
	if a.metricsPort != nil && len(a.pipePairs) > 0 {
		a.metricsPort.DecCascadedRooms()
	}
	a.nodeRouters = make(map[string]string)
	a.pipeTransports = make(map[string]string)
	a.pipePairs = make(map[string]*nodePipePair)

	_ = a.room.Close()
	if a.room.RouterID != "" {
		_ = a.routerPort.CloseRouter(ctx, a.room.RouterID)
	}
	return nil
}

// HandleNodeEvent processes media engine events (ActiveSpeaker, MediaScore, LayerChanged, ResourceClosed) for this room.
func (a *RoomActor) HandleNodeEvent(ctx context.Context, evt *domain.NodeEvent) {
	a.mu.Lock()

	if a.closed || evt == nil {
		a.mu.Unlock()
		return
	}

	if evt.ActiveSpeaker != nil {
		slog.Debug("active speaker detected in room",
			"room_id", a.room.ID,
			"peer_id", evt.ActiveSpeaker.PeerID,
			"producer_id", evt.ActiveSpeaker.ProducerID,
			"volume_dbfs", evt.ActiveSpeaker.VolumeDBFS,
		)
		var activeSpeakers []*domain.ActiveSpeakerInfo
		for _, sess := range a.sessions {
			if sess.Participant().ID == evt.ActiveSpeaker.PeerID {
				activeSpeakers = append(activeSpeakers, &domain.ActiveSpeakerInfo{
					ParticipantSID: sess.Participant().SID,
					Level:          evt.ActiveSpeaker.VolumeDBFS,
					Active:         true,
				})
				break
			}
		}
		if len(activeSpeakers) > 0 {
			for _, sess := range a.sessions {
				if sess.Responder() != nil {
					_ = sess.Responder().SendActiveSpeakers(activeSpeakers)
				}
			}
		}
	}

	if evt.MediaScore != nil {
		slog.Debug("media score updated",
			"room_id", a.room.ID,
			"peer_id", evt.MediaScore.PeerID,
			"resource_id", evt.MediaScore.ResourceID,
			"score", evt.MediaScore.Score,
		)
		quality := domain.ConnectionQualityGood
		if evt.MediaScore.Score >= 8 {
			quality = domain.ConnectionQualityExcellent
		} else if evt.MediaScore.Score <= 3 {
			quality = domain.ConnectionQualityPoor
		}
		var updates []*domain.ConnectionQualityUpdate
		for _, sess := range a.sessions {
			if sess.Participant().ID == evt.MediaScore.PeerID {
				updates = append(updates, &domain.ConnectionQualityUpdate{
					ParticipantSID: sess.Participant().SID,
					Quality:        quality,
					Score:          float32(evt.MediaScore.Score),
				})
				break
			}
		}
		if len(updates) > 0 {
			for _, sess := range a.sessions {
				if sess.Responder() != nil {
					_ = sess.Responder().SendConnectionQuality(updates)
				}
			}
		}
	}

	if evt.LayerChanged != nil {
		slog.Debug("consumer layer changed",
			"room_id", a.room.ID,
			"peer_id", evt.LayerChanged.PeerID,
			"consumer_id", evt.LayerChanged.ConsumerID,
			"spatial_layer", evt.LayerChanged.CurrentSpatialLayer,
			"temporal_layer", evt.LayerChanged.CurrentTemporalLayer,
		)
		for _, sess := range a.sessions {
			for _, tr := range sess.Participant().ListTracks() {
				if tr.ID == evt.LayerChanged.ProducerID {
					qualities := []domain.SubscribedQuality{
						{Quality: domain.VideoQualityLow, Enabled: evt.LayerChanged.CurrentSpatialLayer >= 0},
						{Quality: domain.VideoQualityMedium, Enabled: evt.LayerChanged.CurrentSpatialLayer >= 1},
						{Quality: domain.VideoQualityHigh, Enabled: evt.LayerChanged.CurrentSpatialLayer >= 2},
					}
					if sess.Responder() != nil {
						_ = sess.Responder().SendSubscribedQualityUpdate(tr.SID, tr.MimeType, qualities)
					}
					break
				}
			}
		}
	}

	var offersToSend []pendingOffer
	var shouldBroadcast bool

	if evt.ResourceClosed != nil {
		slog.Warn("media engine resource closed unexpectedly",
			"room_id", a.room.ID,
			"resource_type", evt.ResourceClosed.ResourceType,
			"resource_id", evt.ResourceClosed.ResourceID,
			"reason", evt.ResourceClosed.Reason,
		)
		if evt.ResourceClosed.ResourceType == domain.ResourceTypeProducer {
			producerID := evt.ResourceClosed.ResourceID
			var targetTrackSID string
			for _, session := range a.sessions {
				p := session.Participant()
				for _, tr := range p.ListTracks() {
					if tr.ID == producerID {
						targetTrackSID = tr.SID
						_, _ = p.RemoveTrack(tr.SID)
						break
					}
				}
				if targetTrackSID != "" {
					break
				}
			}

			if targetTrackSID != "" {
				for _, otherSession := range a.sessions {
					if slot, ok := otherSession.SlotTable().InactivateSlotByTrackSID(targetTrackSID); ok {
						otherSession.Participant().RemoveSubscription(slot.ConsumerID)
						if otherSession.State() == domain.NegotiationStateStable {
							if err := otherSession.StartOfferTransition(); err == nil {
								if otherSession.Responder() != nil {
									offersToSend = append(offersToSend, pendingOffer{
										responder: otherSession.Responder(),
										slots:     otherSession.SlotTable().ListSlots(),
										sid:       otherSession.Participant().SID,
									})
								}
							}
						} else {
							otherSession.MarkPendingRenegotiation()
						}
					}
				}
				shouldBroadcast = true
			}
		}
	}

	var broadcastResponders []ports.PeerResponderPort
	var broadcastParts []*domain.Participant
	if shouldBroadcast {
		broadcastResponders, broadcastParts = a.collectBroadcastLocked()
	}

	a.mu.Unlock()

	if shouldBroadcast {
		dispatchBroadcast(broadcastResponders, broadcastParts)
	}
	dispatchOffers(offersToSend)
}
