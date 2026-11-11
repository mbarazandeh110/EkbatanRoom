package telemetry

import (
	"net/http"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

// Metrics holds Prometheus collectors with strict low-cardinality label guards.
// NEVER include high-cardinality keys (room_id, peer_id, transport_id, request_id) in labels.
type Metrics struct {
	ActiveRooms                 prometheus.Gauge
	ActiveParticipants          *prometheus.GaugeVec
	CascadedRooms               prometheus.Gauge
	LicenseQuotaRejectionsTotal *prometheus.CounterVec
	WSMessagesTotal             *prometheus.CounterVec
	SDPNegotiationDuration      *prometheus.HistogramVec
	SDPGlareQueuedTotal         prometheus.Counter
	GRPCClientDuration          *prometheus.HistogramVec
	ErrorsTotal                 *prometheus.CounterVec

	registry *prometheus.Registry
}

var globalMetrics *Metrics

func init() {
	globalMetrics = NewMetrics()
}

func DefaultMetrics() *Metrics {
	return globalMetrics
}

func NewMetrics() *Metrics {
	reg := prometheus.NewRegistry()

	m := &Metrics{
		registry: reg,
		ActiveRooms: prometheus.NewGauge(prometheus.GaugeOpts{
			Name: "signaling_active_rooms",
			Help: "Current number of active rooms.",
		}),
		ActiveParticipants: prometheus.NewGaugeVec(prometheus.GaugeOpts{
			Name: "signaling_active_participants",
			Help: "Current number of active participants partitioned by protocol.",
		}, []string{"protocol"}),
		CascadedRooms: prometheus.NewGauge(prometheus.GaugeOpts{
			Name: "signaling_cascaded_rooms_total",
			Help: "Current number of cascaded multi-node rooms.",
		}),
		LicenseQuotaRejectionsTotal: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "signaling_license_quota_rejections_total",
			Help: "Total number of connection or creation requests rejected due to license limits.",
		}, []string{"reason"}),
		WSMessagesTotal: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "signaling_ws_messages_total",
			Help: "Total count of WebSocket signaling messages processed.",
		}, []string{"protocol", "type", "status"}),
		SDPNegotiationDuration: prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Name:    "signaling_sdp_negotiation_duration_seconds",
			Help:    "Duration of SDP negotiation cycles in seconds.",
			Buckets: prometheus.DefBuckets,
		}, []string{"role"}),
		SDPGlareQueuedTotal: prometheus.NewCounter(prometheus.CounterOpts{
			Name: "signaling_sdp_glare_queued_total",
			Help: "Total number of SDP renegotiations queued to avoid glare collisions.",
		}),
		GRPCClientDuration: prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Name:    "signaling_grpc_client_duration_seconds",
			Help:    "Duration of outbound gRPC calls to media node.",
			Buckets: prometheus.DefBuckets,
		}, []string{"method", "status"}),
		ErrorsTotal: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "signaling_errors_total",
			Help: "Total count of signaling errors partitioned by type.",
		}, []string{"type"}),
	}

	reg.MustRegister(
		m.ActiveRooms,
		m.ActiveParticipants,
		m.CascadedRooms,
		m.LicenseQuotaRejectionsTotal,
		m.WSMessagesTotal,
		m.SDPNegotiationDuration,
		m.SDPGlareQueuedTotal,
		m.GRPCClientDuration,
		m.ErrorsTotal,
	)

	return m
}

func (m *Metrics) Handler() http.Handler {
	return promhttp.HandlerFor(m.registry, promhttp.HandlerOpts{
		EnableOpenMetrics: true,
	})
}

func (m *Metrics) IncActiveRooms() {
	m.ActiveRooms.Inc()
}

func (m *Metrics) DecActiveRooms() {
	m.ActiveRooms.Dec()
}

func (m *Metrics) IncCascadedRooms() {
	m.CascadedRooms.Inc()
}

func (m *Metrics) DecCascadedRooms() {
	m.CascadedRooms.Dec()
}

func (m *Metrics) IncLicenseQuotaRejection(reason string) {
	m.LicenseQuotaRejectionsTotal.WithLabelValues(reason).Inc()
}

func (m *Metrics) IncActiveParticipants(protocol string) {
	m.ActiveParticipants.WithLabelValues(protocol).Inc()
}

func (m *Metrics) DecActiveParticipants(protocol string) {
	m.ActiveParticipants.WithLabelValues(protocol).Dec()
}

func (m *Metrics) RecordWSMessage(protocol, msgType, status string) {
	m.WSMessagesTotal.WithLabelValues(protocol, msgType, status).Inc()
}

func (m *Metrics) RecordSDPNegotiation(role string, durationSeconds float64) {
	m.SDPNegotiationDuration.WithLabelValues(role).Observe(durationSeconds)
}

func (m *Metrics) IncGlareQueued() {
	m.SDPGlareQueuedTotal.Inc()
}

func (m *Metrics) RecordGRPC(method, status string, durationSeconds float64) {
	m.GRPCClientDuration.WithLabelValues(method, status).Observe(durationSeconds)
}

func (m *Metrics) RecordError(errType string) {
	m.ErrorsTotal.WithLabelValues(errType).Inc()
}
