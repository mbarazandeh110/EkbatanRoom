package domain

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
	URLs       []string
	Username   string
	Credential string
}

type TrackPublishCommand struct {
	Kind              TrackKind
	SSRC              uint32
	RTXSSRC           uint32
	PayloadType       uint32
	RTXPayloadType    uint32
	Codec             string
	ClockRate         uint32
	CName             string
	RtpParametersJSON string
}

type JoinResult struct {
	Room                *Room
	Participant         *Participant
	PublisherTransport  *TransportInfo
	SubscriberTransport *TransportInfo
	IceServers          []IceServer
}
