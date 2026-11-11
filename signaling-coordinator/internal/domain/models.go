package domain

import "time"

type ResourceType int

const (
	ResourceTypeUnspecified ResourceType = 0
	ResourceTypeRouter      ResourceType = 1
	ResourceTypeTransport   ResourceType = 2
	ResourceTypeProducer    ResourceType = 3
	ResourceTypeConsumer    ResourceType = 4
)

type RouterInfo struct {
	RouterID               string
	WorkerIndex            uint32
	RtpCapabilitiesJSON    string
}

type IceCandidateInfo struct {
	Foundation        string
	Priority          uint32
	IP                string
	Protocol          string
	Port              uint32
	Type              string
	TcpType           string
	SDPCandidateLine  string
}

type DtlsFingerprintInfo struct {
	Algorithm string
	Value     string
}

type TransportInfo struct {
	TransportID             string
	IceUfrag                string
	IcePwd                  string
	IceCandidates           []IceCandidateInfo
	DtlsFingerprints        []DtlsFingerprintInfo
	SctpPort                uint32
	RawIceParametersJSON    string
	RawDtlsParametersJSON   string
}

type IceInfo struct {
	IceUfrag             string
	IcePwd               string
	RawIceParametersJSON string
}

type CreateTransportCommand struct {
	RequestID   string
	RouterID    string
	PeerID      string
	IsPublisher bool
	EnableSctp  bool
}

type ConnectTransportCommand struct {
	RouterID           string
	TransportID        string
	DtlsRole           string // "auto", "client", "server"
	RemoteFingerprints []DtlsFingerprintInfo
}

type CreatePipeTransportCommand struct {
	RequestID  string
	RouterID   string
	EnableSrtp bool
	EnableRtx  bool
}

type PipeTransportInfo struct {
	PipeTransportID    string
	IP                 string
	Port               uint32
	SrtpParametersJSON string
}

type ConnectPipeTransportCommand struct {
	RouterID                 string
	PipeTransportID          string
	RemoteIP                 string
	RemotePort               uint32
	RemoteSrtpParametersJSON string
}

type PipeProduceCommand struct {
	RequestID         string
	RouterID          string
	PipeTransportID   string
	SourceProducerID  string
	Kind              TrackKind
	RtpParametersJSON string
	Paused            bool
}

type PipeConsumeCommand struct {
	RequestID       string
	RouterID        string
	PipeTransportID string
	ProducerID      string
}

type ProduceCommand struct {
	RequestID           string
	RouterID            string
	TransportID         string
	PeerID              string
	Kind                TrackKind
	RtpParametersJSON   string
	Paused              bool
}

type ProduceResult struct {
	ProducerID string
	Kind       TrackKind
	Encodings  []SimulcastLayer
}

type ConsumeCommand struct {
	RequestID               string
	RouterID                string
	SubscriberTransportID   string
	ProducerID              string
	RtpCapabilitiesJSON     string
	Paused                  bool
}

type ConsumeResult struct {
	ConsumerID           string
	ProducerID           string
	Kind                 TrackKind
	RtpParametersJSON    string
	SSRC                 uint32
	RTXSSRC              uint32
	PayloadType          uint32
	RTXPayloadType       uint32
	CodecMimeType        string
	ClockRate            uint32
	Encodings            []SimulcastLayer
	InitialSpatialLayer  int32
	InitialTemporalLayer int32
}

type SetMediaPausedCommand struct {
	RouterID     string
	ResourceType ResourceType
	ResourceID   string
	Paused       bool
}

type SetPreferredLayersCommand struct {
	RouterID      string
	ConsumerID    string
	SpatialLayer  int32
	TemporalLayer int32
}

type IceServer struct {
	URLs       []string `json:"urls" yaml:"urls"`
	Username   string   `json:"username,omitempty" yaml:"username,omitempty"`
	Credential string   `json:"credential,omitempty" yaml:"credential,omitempty"`
}

type TrackPublishCommand struct {
	TrackSID          string
	Name              string
	Source            string
	Width             uint32
	Height            uint32
	Kind              TrackKind
	SSRC              uint32
	RTXSSRC           uint32
	PayloadType       uint32
	RTXPayloadType    uint32
	Codec             string
	ClockRate         uint32
	CName             string
	RtpParametersJSON string
	SimulcastRIDs     []string
}

type JoinResult struct {
	Room                *Room
	Participant         *Participant
	PublisherTransport  *TransportInfo
	SubscriberTransport *TransportInfo
	IceServers          []IceServer
}

type NodeCapacityInfo struct {
	TotalCapacityWeight        uint32
	TotalWorkers               uint32
	AliveWorkers               uint32
	LogicalLoadPercent         float32
	CpuUsagePercent            float32
	MemoryUsagePercent         float32
	MaxWorkerLoadPercent       float32
	EffectiveLoadPercent       float32
	ActiveRouters              uint32
	ActiveTransports           uint32
	ActiveProducers            uint32
	ActiveConsumers            uint32
	IsDraining                 bool
	EnterpriseCascadingEnabled bool
	LicenseTier                string
}

type ApplyLicenseResult struct {
	IsValid            bool
	LicenseTier        string
	ExpiresAtTimestamp int64
	Features           []string
	IssuedTo           string
	ErrorMessage       string
}

type ActiveSpeakerEvent struct {
	ProducerID string
	PeerID     string
	VolumeDBFS float32
}

type MediaScoreEvent struct {
	ResourceID     string
	PeerID         string
	IsProducer     bool
	Score          uint32
	ProducerScores []uint32
}

type LayerChangedEvent struct {
	ConsumerID             string
	ProducerID             string
	PeerID                 string
	PreferredSpatialLayer  int32
	CurrentSpatialLayer    int32
	PreferredTemporalLayer int32
	CurrentTemporalLayer   int32
}

type ResourceClosedEvent struct {
	ResourceType ResourceType
	ResourceID   string
	Reason       string
}

type NodeEvent struct {
	RouterID       string
	TimestampMS    int64
	NodeCapacity   *NodeCapacityInfo
	ActiveSpeaker  *ActiveSpeakerEvent
	MediaScore     *MediaScoreEvent
	LayerChanged   *LayerChangedEvent
	ResourceClosed *ResourceClosedEvent
}

type VideoQuality int32

const (
	VideoQualityLow    VideoQuality = 0
	VideoQualityMedium VideoQuality = 1
	VideoQualityHigh   VideoQuality = 2
	VideoQualityOff    VideoQuality = 3
)

type SubscribedQuality struct {
	Quality VideoQuality
	Enabled bool
}

type ConnectionQuality int32

const (
	ConnectionQualityPoor      ConnectionQuality = 0
	ConnectionQualityGood      ConnectionQuality = 1
	ConnectionQualityExcellent ConnectionQuality = 2
	ConnectionQualityLost      ConnectionQuality = 3
)

type ConnectionQualityUpdate struct {
	ParticipantSID string
	Quality        ConnectionQuality
	Score          float32
}

type ActiveSpeakerInfo struct {
	ParticipantSID string
	Level          float32
	Active         bool
}

type DisconnectReason int

const (
	DisconnectReasonUnknown            DisconnectReason = 0
	DisconnectReasonClientInitiated    DisconnectReason = 1
	DisconnectReasonDuplicateIdentity  DisconnectReason = 2
	DisconnectReasonServerShutdown     DisconnectReason = 3
	DisconnectReasonParticipantRemoved DisconnectReason = 4
	DisconnectReasonRoomDeleted        DisconnectReason = 5
	DisconnectReasonStateMismatch      DisconnectReason = 6
	DisconnectReasonJoinFailure        DisconnectReason = 7
	DisconnectReasonMigration          DisconnectReason = 8
	DisconnectReasonSignalClose        DisconnectReason = 9
	DisconnectReasonRoomClosed         DisconnectReason = 10
	DisconnectReasonUserUnavailable    DisconnectReason = 11
	DisconnectReasonUserRejected       DisconnectReason = 12
	DisconnectReasonSipTrunkFailure    DisconnectReason = 13
)

type ProduceDataCommand struct {
	RequestID                string
	RouterID                 string
	TransportID              string
	PeerID                   string
	SctpStreamParametersJSON string
	Label                    string
	Protocol                 string
}

type ProduceDataResult struct {
	DataProducerID           string
	Label                    string
	Protocol                 string
	SctpStreamParametersJSON string
}

type ConsumeDataCommand struct {
	RequestID      string
	RouterID       string
	TransportID    string
	DataProducerID string
}

type ConsumeDataResult struct {
	DataConsumerID           string
	DataProducerID           string
	Label                    string
	Protocol                 string
	SctpStreamParametersJSON string
}

type SendDirectDataCommand struct {
	RouterID string
	Label    string
	Protocol string
	Payload  []byte
}

// RouterPlacementRole defines the functional role of a router in a room topology.
const (
	RouterRolePrimary        = "primary"
	RouterRoleIntraNodePipe  = "intra_node_pipe"
	RouterRoleInterNodePipe  = "inter_node_pipe"
)

// RouterPlacementEntry represents an active router assigned to a room on an SFU node.
type RouterPlacementEntry struct {
	NodeID          string `json:"node_id"`
	RouterID        string `json:"router_id"`
	WorkerIndex     uint32 `json:"worker_index"`
	Role            string `json:"role"` // "primary", "intra_node_pipe", "inter_node_pipe"
	ActiveConsumers uint32 `json:"active_consumers"`
}

// RoomPlacementTopology represents the distributed placement and routing topology of a room.
type RoomPlacementTopology struct {
	RoomID           string                 `json:"room_id"`
	PrimaryNodeID    string                 `json:"primary_node_id"`
	PrimaryRouterID  string                 `json:"primary_router_id"`
	PrimaryWorkerIdx uint32                 `json:"primary_worker_idx"`
	Routers          []RouterPlacementEntry `json:"routers"`
	CreatedAt        time.Time              `json:"created_at"`
	UpdatedAt        time.Time              `json:"updated_at"`
}



