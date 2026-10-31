package media_node_grpc

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"google.golang.org/grpc/metadata"

	sfuv1 "ekbatan-room/signaling-coordinator/gen/proto/sfu/v1"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var (
	_ ports.RouterEnginePort    = (*MediaNodeClient)(nil)
	_ ports.TransportEnginePort = (*MediaNodeClient)(nil)
	_ ports.MediaStreamPort     = (*MediaNodeClient)(nil)
	_ ports.NodeLifecyclePort   = (*MediaNodeClient)(nil)
)

// MediaNodeClient implements outbound media engine ports via gRPC.
type MediaNodeClient struct {
	client         sfuv1.SfuMediaEngineServiceClient
	defaultTimeout time.Duration
}

// NewMediaNodeClient creates a new gRPC media node adapter.
func NewMediaNodeClient(client sfuv1.SfuMediaEngineServiceClient, defaultTimeout time.Duration) *MediaNodeClient {
	if defaultTimeout <= 0 {
		defaultTimeout = 5 * time.Second
	}
	return &MediaNodeClient{
		client:         client,
		defaultTimeout: defaultTimeout,
	}
}

func (c *MediaNodeClient) withTimeoutAndReqID(ctx context.Context, reqID string) (context.Context, context.CancelFunc) {
	callCtx := ctx
	var cancel context.CancelFunc = func() {}
	if _, ok := ctx.Deadline(); !ok {
		callCtx, cancel = context.WithTimeout(ctx, c.defaultTimeout)
	}
	if reqID != "" {
		callCtx = metadata.AppendToOutgoingContext(callCtx, "x-request-id", reqID)
	}
	return callCtx, cancel
}

// RouterEnginePort implementation

func (c *MediaNodeClient) CreateRouter(ctx context.Context, roomID string) (*domain.RouterInfo, error) {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.CreateRouter(callCtx, &sfuv1.CreateRouterRequest{
		RequestId: reqID,
		RoomId:    roomID,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("CreateRouter", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc create router failed (request_id=%s): %w", reqID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("CreateRouter", status, time.Since(start).Seconds())

	return &domain.RouterInfo{
		RouterID:            res.GetRouterId(),
		WorkerIndex:         res.GetWorkerIndex(),
		RtpCapabilitiesJSON: res.GetRouterRtpCapabilitiesJson(),
	}, nil
}

func (c *MediaNodeClient) CloseRouter(ctx context.Context, routerID string) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	_, err := c.client.CloseResource(callCtx, &sfuv1.CloseResourceRequest{
		RouterId:     routerID,
		ResourceType: sfuv1.ResourceType_RESOURCE_TYPE_ROUTER,
		ResourceId:   routerID,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("CloseRouter", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc close router failed (router_id=%s): %w", routerID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("CloseRouter", status, time.Since(start).Seconds())
	return nil
}

// TransportEnginePort implementation

func (c *MediaNodeClient) CreateTransport(ctx context.Context, cmd *domain.CreateTransportCommand) (*domain.TransportInfo, error) {
	start := time.Now()
	reqID := cmd.RequestID
	if reqID == "" {
		reqID = uuid.NewString()
	}
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.CreateTransport(callCtx, &sfuv1.CreateTransportRequest{
		RequestId:   reqID,
		RouterId:    cmd.RouterID,
		PeerId:      cmd.PeerID,
		IsPublisher: cmd.IsPublisher,
		EnableSctp:  cmd.EnableSctp,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("CreateTransport", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc create transport failed (request_id=%s): %w", reqID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("CreateTransport", status, time.Since(start).Seconds())

	candidates := make([]domain.IceCandidateInfo, 0, len(res.GetIceCandidates()))
	for _, cand := range res.GetIceCandidates() {
		candidates = append(candidates, domain.IceCandidateInfo{
			Foundation:       cand.GetFoundation(),
			Priority:         cand.GetPriority(),
			IP:               cand.GetIp(),
			Protocol:         cand.GetProtocol(),
			Port:             cand.GetPort(),
			Type:             cand.GetType(),
			TcpType:          cand.GetTcpType(),
			SDPCandidateLine: cand.GetSdpCandidateLine(),
		})
	}

	fingerprints := make([]domain.DtlsFingerprintInfo, 0, len(res.GetDtlsFingerprints()))
	for _, fp := range res.GetDtlsFingerprints() {
		fingerprints = append(fingerprints, domain.DtlsFingerprintInfo{
			Algorithm: fp.GetAlgorithm(),
			Value:     fp.GetValue(),
		})
	}

	return &domain.TransportInfo{
		TransportID:           res.GetTransportId(),
		IceUfrag:              res.GetIceUfrag(),
		IcePwd:                res.GetIcePwd(),
		IceCandidates:         candidates,
		DtlsFingerprints:      fingerprints,
		SctpPort:              res.GetSctpPort(),
		RawIceParametersJSON:  res.GetRawIceParametersJson(),
		RawDtlsParametersJSON: res.GetRawDtlsParametersJson(),
	}, nil
}

func (c *MediaNodeClient) ConnectTransport(ctx context.Context, cmd *domain.ConnectTransportCommand) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	remoteFps := make([]*sfuv1.DtlsFingerprintInfo, 0, len(cmd.RemoteFingerprints))
	for _, fp := range cmd.RemoteFingerprints {
		remoteFps = append(remoteFps, &sfuv1.DtlsFingerprintInfo{
			Algorithm: fp.Algorithm,
			Value:     fp.Value,
		})
	}

	_, err := c.client.ConnectTransport(callCtx, &sfuv1.ConnectTransportRequest{
		RouterId:           cmd.RouterID,
		TransportId:        cmd.TransportID,
		DtlsRole:           cmd.DtlsRole,
		RemoteFingerprints: remoteFps,
	})
	status := "ok"
	if err != nil {
		// Idempotency: Mediasoup returns an error if connect() is already called on the transport.
		// In WebRTC renegotiation, duplicate connect calls are non-fatal.
		if strings.Contains(err.Error(), "already called") {
			telemetry.DefaultMetrics().RecordGRPC("ConnectTransport", status, time.Since(start).Seconds())
			return nil
		}
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("ConnectTransport", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc connect transport failed (transport_id=%s): %w", cmd.TransportID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("ConnectTransport", status, time.Since(start).Seconds())
	return nil
}

func (c *MediaNodeClient) RestartIce(ctx context.Context, routerID, transportID string) (*domain.IceInfo, error) {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.RestartIce(callCtx, &sfuv1.RestartIceRequest{
		RouterId:    routerID,
		TransportId: transportID,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("RestartIce", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc restart ice failed (transport_id=%s): %w", transportID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("RestartIce", status, time.Since(start).Seconds())

	return &domain.IceInfo{
		IceUfrag:             res.GetIceUfrag(),
		IcePwd:               res.GetIcePwd(),
		RawIceParametersJSON: res.GetRawIceParametersJson(),
	}, nil
}

func (c *MediaNodeClient) CloseTransport(ctx context.Context, routerID, transportID string) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	_, err := c.client.CloseResource(callCtx, &sfuv1.CloseResourceRequest{
		RouterId:     routerID,
		ResourceType: sfuv1.ResourceType_RESOURCE_TYPE_TRANSPORT,
		ResourceId:   transportID,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("CloseTransport", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc close transport failed (transport_id=%s): %w", transportID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("CloseTransport", status, time.Since(start).Seconds())
	return nil
}

// MediaStreamPort implementation

func (c *MediaNodeClient) Produce(ctx context.Context, cmd *domain.ProduceCommand) (*domain.ProduceResult, error) {
	start := time.Now()
	reqID := cmd.RequestID
	if reqID == "" {
		reqID = uuid.NewString()
	}
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.Produce(callCtx, &sfuv1.ProduceRequest{
		RequestId:         reqID,
		RouterId:          cmd.RouterID,
		TransportId:       cmd.TransportID,
		PeerId:            cmd.PeerID,
		Kind:              string(cmd.Kind),
		RtpParametersJson: cmd.RtpParametersJSON,
		Paused:            cmd.Paused,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("Produce", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc produce failed (request_id=%s): %w", reqID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("Produce", status, time.Since(start).Seconds())

	encodings := make([]domain.SimulcastLayer, 0, len(res.GetEncodings()))
	for _, enc := range res.GetEncodings() {
		encodings = append(encodings, domain.SimulcastLayer{
			RID:             enc.GetRid(),
			SSRC:            enc.GetSsrc(),
			RTXSSRC:         enc.GetRtxSsrc(),
			MaxBitrate:      enc.GetMaxBitrate(),
			ScalabilityMode: enc.GetScalabilityMode(),
		})
	}

	return &domain.ProduceResult{
		ProducerID: res.GetProducerId(),
		Kind:       domain.TrackKind(res.GetKind()),
		Encodings:  encodings,
	}, nil
}

func (c *MediaNodeClient) Consume(ctx context.Context, cmd *domain.ConsumeCommand) (*domain.ConsumeResult, error) {
	start := time.Now()
	reqID := cmd.RequestID
	if reqID == "" {
		reqID = uuid.NewString()
	}
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.Consume(callCtx, &sfuv1.ConsumeRequest{
		RequestId:             reqID,
		RouterId:              cmd.RouterID,
		SubscriberTransportId: cmd.SubscriberTransportID,
		ProducerId:            cmd.ProducerID,
		RtpCapabilitiesJson:   cmd.RtpCapabilitiesJSON,
		Paused:                cmd.Paused,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("Consume", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc consume failed (request_id=%s): %w", reqID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("Consume", status, time.Since(start).Seconds())

	encodings := make([]domain.SimulcastLayer, 0, len(res.GetEncodings()))
	for _, enc := range res.GetEncodings() {
		encodings = append(encodings, domain.SimulcastLayer{
			RID:             enc.GetRid(),
			SSRC:            enc.GetSsrc(),
			RTXSSRC:         enc.GetRtxSsrc(),
			MaxBitrate:      enc.GetMaxBitrate(),
			ScalabilityMode: enc.GetScalabilityMode(),
		})
	}

	return &domain.ConsumeResult{
		ConsumerID:           res.GetConsumerId(),
		ProducerID:           res.GetProducerId(),
		Kind:                 domain.TrackKind(res.GetKind()),
		RtpParametersJSON:    res.GetRtpParametersJson(),
		SSRC:                 res.GetSsrc(),
		RTXSSRC:              res.GetRtxSsrc(),
		PayloadType:          res.GetPayloadType(),
		RTXPayloadType:       res.GetRtxPayloadType(),
		CodecMimeType:        res.GetCodecMimeType(),
		ClockRate:            res.GetClockRate(),
		Encodings:            encodings,
		InitialSpatialLayer:  res.GetInitialSpatialLayer(),
		InitialTemporalLayer: res.GetInitialTemporalLayer(),
	}, nil
}

func (c *MediaNodeClient) SetMediaPaused(ctx context.Context, cmd *domain.SetMediaPausedCommand) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	protoResType := sfuv1.ResourceType(cmd.ResourceType)
	_, err := c.client.SetMediaPaused(callCtx, &sfuv1.SetMediaPausedRequest{
		RouterId:     cmd.RouterID,
		ResourceType: protoResType,
		ResourceId:   cmd.ResourceID,
		Paused:       cmd.Paused,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("SetMediaPaused", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc set media paused failed (resource_id=%s): %w", cmd.ResourceID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("SetMediaPaused", status, time.Since(start).Seconds())
	return nil
}

func (c *MediaNodeClient) SetConsumerPreferredLayers(ctx context.Context, cmd *domain.SetPreferredLayersCommand) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	_, err := c.client.SetConsumerPreferredLayers(callCtx, &sfuv1.SetConsumerPreferredLayersRequest{
		RouterId:      cmd.RouterID,
		ConsumerId:    cmd.ConsumerID,
		SpatialLayer:  cmd.SpatialLayer,
		TemporalLayer: cmd.TemporalLayer,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("SetConsumerPreferredLayers", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc set preferred layers failed (consumer_id=%s): %w", cmd.ConsumerID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("SetConsumerPreferredLayers", status, time.Since(start).Seconds())
	return nil
}

func (c *MediaNodeClient) CloseResource(ctx context.Context, routerID string, resourceType domain.ResourceType, resourceID string) error {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	protoResType := sfuv1.ResourceType(resourceType)
	_, err := c.client.CloseResource(callCtx, &sfuv1.CloseResourceRequest{
		RouterId:     routerID,
		ResourceType: protoResType,
		ResourceId:   resourceID,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("CloseResource", status, time.Since(start).Seconds())
		return fmt.Errorf("grpc close resource failed (resource_id=%s): %w", resourceID, err)
	}
	telemetry.DefaultMetrics().RecordGRPC("CloseResource", status, time.Since(start).Seconds())
	return nil
}

// NodeLifecyclePort implementation

func (c *MediaNodeClient) ApplyLicense(ctx context.Context, licenseToken string) (*domain.ApplyLicenseResult, error) {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	res, err := c.client.ApplyLicense(callCtx, &sfuv1.ApplyLicenseRequest{
		LicenseToken: licenseToken,
	})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("ApplyLicense", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc apply license failed: %w", err)
	}
	telemetry.DefaultMetrics().RecordGRPC("ApplyLicense", status, time.Since(start).Seconds())

	return &domain.ApplyLicenseResult{
		IsValid:            res.GetIsValid(),
		LicenseTier:        res.GetLicenseTier(),
		ExpiresAtTimestamp: res.GetExpiresAtTimestamp(),
		Features:           res.GetFeatures(),
		IssuedTo:           res.GetIssuedTo(),
		ErrorMessage:       res.GetErrorMessage(),
	}, nil
}

func (c *MediaNodeClient) GetNodeCapacity(ctx context.Context) (*domain.NodeCapacityInfo, error) {
	start := time.Now()
	reqID := uuid.NewString()
	callCtx, cancel := c.withTimeoutAndReqID(ctx, reqID)
	defer cancel()

	metrics, err := c.client.GetNodeCapacity(callCtx, &sfuv1.GetNodeCapacityRequest{})
	status := "ok"
	if err != nil {
		status = "error"
		telemetry.DefaultMetrics().RecordGRPC("GetNodeCapacity", status, time.Since(start).Seconds())
		return nil, fmt.Errorf("grpc get node capacity failed: %w", err)
	}
	telemetry.DefaultMetrics().RecordGRPC("GetNodeCapacity", status, time.Since(start).Seconds())

	return mapProtoCapacity(metrics), nil
}

func (c *MediaNodeClient) WatchNodeEvents(ctx context.Context, coordinatorID, licenseToken string) (<-chan *domain.NodeEvent, error) {
	stream, err := c.client.WatchNodeEvents(ctx, &sfuv1.WatchNodeEventsRequest{
		CoordinatorId: coordinatorID,
		LicenseToken:  licenseToken,
	})
	if err != nil {
		return nil, fmt.Errorf("grpc watch node events failed: %w", err)
	}

	eventsChan := make(chan *domain.NodeEvent, 64)
	go func() {
		defer close(eventsChan)
		for {
			protoEvt, err := stream.Recv()
			if err != nil {
				return
			}
			if protoEvt == nil {
				continue
			}

			evt := &domain.NodeEvent{
				RouterID:    protoEvt.GetRouterId(),
				TimestampMS: protoEvt.GetTimestampMs(),
			}

			if capMetrics := protoEvt.GetNodeCapacity(); capMetrics != nil {
				evt.NodeCapacity = mapProtoCapacity(capMetrics)
			}
			if as := protoEvt.GetActiveSpeaker(); as != nil {
				evt.ActiveSpeaker = &domain.ActiveSpeakerEvent{
					ProducerID: as.GetProducerId(),
					PeerID:     as.GetPeerId(),
					VolumeDBFS: as.GetVolumeDbfs(),
				}
			}
			if ms := protoEvt.GetMediaScore(); ms != nil {
				evt.MediaScore = &domain.MediaScoreEvent{
					ResourceID:     ms.GetResourceId(),
					PeerID:         ms.GetPeerId(),
					IsProducer:     ms.GetIsProducer(),
					Score:          ms.GetScore(),
					ProducerScores: ms.GetProducerScores(),
				}
			}
			if lc := protoEvt.GetLayerChanged(); lc != nil {
				evt.LayerChanged = &domain.LayerChangedEvent{
					ConsumerID:             lc.GetConsumerId(),
					ProducerID:             lc.GetProducerId(),
					PeerID:                 lc.GetPeerId(),
					PreferredSpatialLayer:  lc.GetPreferredSpatialLayer(),
					CurrentSpatialLayer:    lc.GetCurrentSpatialLayer(),
					PreferredTemporalLayer: lc.GetPreferredTemporalLayer(),
					CurrentTemporalLayer:   lc.GetCurrentTemporalLayer(),
				}
			}
			if rc := protoEvt.GetResourceClosed(); rc != nil {
				evt.ResourceClosed = &domain.ResourceClosedEvent{
					ResourceType: domain.ResourceType(rc.GetResourceType()),
					ResourceID:   rc.GetResourceId(),
					Reason:       rc.GetReason(),
				}
			}

			select {
			case <-ctx.Done():
				return
			case eventsChan <- evt:
			}
		}
	}()

	return eventsChan, nil
}

func mapProtoCapacity(m *sfuv1.NodeCapacityMetrics) *domain.NodeCapacityInfo {
	if m == nil {
		return nil
	}
	return &domain.NodeCapacityInfo{
		TotalCapacityWeight:        m.GetTotalCapacityWeight(),
		TotalWorkers:               m.GetTotalWorkers(),
		AliveWorkers:               m.GetAliveWorkers(),
		LogicalLoadPercent:         m.GetLogicalLoadPercent(),
		CpuUsagePercent:            m.GetCpuUsagePercent(),
		MemoryUsagePercent:         m.GetMemoryUsagePercent(),
		MaxWorkerLoadPercent:       m.GetMaxWorkerLoadPercent(),
		EffectiveLoadPercent:       m.GetEffectiveLoadPercent(),
		ActiveRouters:              m.GetActiveRouters(),
		ActiveTransports:           m.GetActiveTransports(),
		ActiveProducers:            m.GetActiveProducers(),
		ActiveConsumers:            m.GetActiveConsumers(),
		IsDraining:                 m.GetIsDraining(),
		EnterpriseCascadingEnabled: m.GetEnterpriseCascadingEnabled(),
		LicenseTier:                m.GetLicenseTier(),
	}
}
