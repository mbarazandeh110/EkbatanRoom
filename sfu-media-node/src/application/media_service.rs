use crate::application::state::{ConsumerItem, EngineState, ProducerItem};
use crate::domain::entities::MediaKind;
use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::ports::inbound::{
    ConsumeParams, ConsumeResult, MediaUseCase, ProduceParams, ProduceResult,
};
use crate::ports::outbound::{EventPublisherPort, MetricsPort};
use async_trait::async_trait;
use dashmap::DashSet;
use mediasoup::consumer::ConsumerRtpStreamStat;
use mediasoup::prelude::*;
use mediasoup::types::scalability_modes::ScalabilityMode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct MediaService {
    state: EngineState,
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    produce_cache: Arc<IdempotencyCache<ProduceResult>>,
    consume_cache: Arc<IdempotencyCache<ConsumeResult>>,
}

impl MediaService {
    pub fn new(
        state: EngineState,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let produce_cache = Arc::new(IdempotencyCache::default());
        let _p_clean = produce_cache.start_periodic_cleanup(Duration::from_secs(30));

        let consume_cache = Arc::new(IdempotencyCache::default());
        let _c_clean = consume_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            state,
            event_publisher,
            metrics,
            produce_cache,
            consume_cache,
        }
    }
}

#[async_trait]
impl MediaUseCase for MediaService {
    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, transport_id = %params.transport_id, peer_id = %params.peer_id, request_id = %params.request_id))]
    async fn produce(&self, params: ProduceParams) -> Result<ProduceResult, SfuDomainError> {
        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;
        validation::validate_not_empty("peer_id", &params.peer_id)?;

        // Include params.paused in payload hash (Logic bug fix)
        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.transport_id,
            params.peer_id,
            params.kind,
            params.raw_rtp_parameters_json,
            params.paused
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.produce_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
                    let transport = {
                        let t_item =
                            self.state
                                .transports
                                .get(&params.transport_id)
                                .ok_or_else(|| {
                                    SfuDomainError::NotFound(format!(
                                        "Transport '{}' not found",
                                        params.transport_id
                                    ))
                                })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' is closed",
                                params.transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' does not belong to router '{}'",
                                params.transport_id, params.router_id
                            )));
                        }

                        t_item.transport.clone()
                    };

                    let ms_kind: mediasoup::prelude::MediaKind = params.kind.into();

                    let mut options = ProducerOptions::new(ms_kind, params.rtp_parameters);
                    options.paused = params.paused;

                    // Await on produce without holding any DashMap guard
                    let producer = transport.produce(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to produce: {e}"))
                    })?;

                    let producer_id = producer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    // Register score callback
                    let event_publisher = self.event_publisher.clone();
                    let r_id = params.router_id.clone();
                    let p_id = producer_id.clone();
                    let peer_id = params.peer_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();

                    producer
                        .on_score(move |scores| {
                            if closed_ref.load(Ordering::SeqCst)
                                || !state_ref.is_router_active(&r_id)
                            {
                                return;
                            }

                            let avg_score = if scores.is_empty() {
                                10
                            } else {
                                (scores.iter().map(|s| s.score as u32).sum::<u32>())
                                    / (scores.len() as u32)
                            };
                            let all_scores = scores.iter().map(|s| s.score as u32).collect();

                            event_publisher
                                .publish_score(&r_id, &p_id, &peer_id, true, avg_score, all_scores);
                        })
                        .detach();

                    let encodings = producer
                        .rtp_parameters()
                        .encodings
                        .iter()
                        .map(|enc| SimulcastEncodingInfo {
                            rid: enc.rid.clone().unwrap_or_default(),
                            ssrc: enc.ssrc.unwrap_or(0),
                            rtx_ssrc: enc.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: enc.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if enc.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                enc.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    let item = ProducerItem {
                        producer,
                        router_id: params.router_id.clone(),
                        transport_id: params.transport_id.clone(),
                        peer_id: params.peer_id.clone(),
                        kind: params.kind,
                        is_closed,
                    };

                    self.state.producers.insert(producer_id.clone(), item);
                    if let Some(set) = self.state.transport_producers.get(&params.transport_id) {
                        set.insert(producer_id.clone());
                    }
                    self.state
                        .producer_consumers
                        .insert(producer_id.clone(), DashSet::new());
                    self.metrics.inc_active_producers(&params.kind.to_string());

                    Ok(ProduceResult {
                        producer_id,
                        kind: params.kind.to_string(),
                        encodings,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, transport_id = %params.subscriber_transport_id, producer_id = %params.producer_id, request_id = %params.request_id))]
    async fn consume(&self, params: ConsumeParams) -> Result<ConsumeResult, SfuDomainError> {
        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("subscriber_transport_id", &params.subscriber_transport_id)?;
        validation::validate_not_empty("producer_id", &params.producer_id)?;

        // Include params.paused in payload hash (Logic bug fix)
        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.subscriber_transport_id,
            params.producer_id,
            params.raw_rtp_capabilities_json,
            params.paused
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.consume_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    // Extract router, verify can_consume, and DROP guard before .await (Deadlock fix)
                    let router = {
                        let r_item =
                            self.state.routers.get(&params.router_id).ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "Router '{}' not found",
                                    params.router_id
                                ))
                            })?;

                        if r_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Router '{}' is closed",
                                params.router_id
                            )));
                        }

                        r_item.router.clone()
                    };

                    let producer_id: ProducerId = params.producer_id.parse().map_err(|e| {
                        SfuDomainError::InvalidArgument(format!("Invalid producer_id: {e}"))
                    })?;

                    // Mandatory can_consume check
                    if !router.can_consume(&producer_id, &params.rtp_capabilities) {
                        return Err(SfuDomainError::IncompatibleCodec(format!(
                            "Router cannot consume producer '{}' with given RTP capabilities",
                            params.producer_id
                        )));
                    }

                    // Extract subscriber transport, verify router ownership, and DROP guard before .await
                    let (transport, peer_id) = {
                        let t_item = self
                            .state
                            .transports
                            .get(&params.subscriber_transport_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "Transport '{}' not found",
                                    params.subscriber_transport_id
                                ))
                            })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' is closed",
                                params.subscriber_transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' does not belong to router '{}'",
                                params.subscriber_transport_id, params.router_id
                            )));
                        }

                        (t_item.transport.clone(), t_item.peer_id.clone())
                    };

                    let mut options = ConsumerOptions::new(producer_id, params.rtp_capabilities);
                    options.paused = params.paused;

                    // Await on consume without holding any DashMap guard
                    let consumer = transport.consume(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to consume: {e}"))
                    })?;

                    let consumer_id = consumer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    // Register callbacks
                    let event_publisher = self.event_publisher.clone();
                    let r_id = params.router_id.clone();
                    let c_id = consumer_id.clone();
                    let p_id_str = params.producer_id.clone();
                    let peer_id_clone = peer_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();

                    consumer
                        .on_score({
                            let r_id = r_id.clone();
                            let c_id = c_id.clone();
                            let peer_id = peer_id_clone.clone();
                            let event_publisher = event_publisher.clone();
                            let state_ref = state_ref.clone();
                            let closed_ref = closed_ref.clone();

                            move |score| {
                                if closed_ref.load(Ordering::SeqCst)
                                    || !state_ref.is_router_active(&r_id)
                                {
                                    return;
                                }

                                event_publisher.publish_score(
                                    &r_id,
                                    &c_id,
                                    &peer_id,
                                    false,
                                    score.score as u32,
                                    score.producer_scores.iter().map(|&s| s as u32).collect(),
                                );
                            }
                        })
                        .detach();

                    let consumer_for_closure = consumer.clone();
                    consumer
                        .on_layers_change({
                            let r_id = r_id.clone();
                            let c_id = c_id.clone();
                            let p_id = p_id_str.clone();
                            let peer_id = peer_id_clone;
                            let event_publisher = event_publisher.clone();
                            let state_ref = state_ref.clone();
                            let closed_ref = closed_ref.clone();

                            move |layers| {
                                if closed_ref.load(Ordering::SeqCst)
                                    || !state_ref.is_router_active(&r_id)
                                {
                                    return;
                                }

                                let pref = consumer_for_closure.preferred_layers();
                                event_publisher.publish_layer_change(
                                    crate::ports::outbound::LayerChangeEventParams {
                                        router_id: &r_id,
                                        consumer_id: &c_id,
                                        producer_id: &p_id,
                                        peer_id: &peer_id,
                                        preferred_spatial: pref
                                            .map(|p| p.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        current_spatial: layers
                                            .map(|l| l.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        preferred_temporal: pref
                                            .and_then(|p| p.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                        current_temporal: layers
                                            .and_then(|l| l.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                    },
                                );
                            }
                        })
                        .detach();

                    let rtp_params = consumer.rtp_parameters();
                    let raw_rtp_json = serde_json::to_string(rtp_params)
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let first_codec = rtp_params.codecs.first();
                    let first_encoding = rtp_params.encodings.first();

                    let ssrc = first_encoding.and_then(|e| e.ssrc).unwrap_or(0);
                    let rtx_ssrc = first_encoding
                        .and_then(|e| e.rtx.as_ref())
                        .map(|r| r.ssrc)
                        .unwrap_or(0);
                    let payload_type = first_codec.map(|c| c.payload_type() as u32).unwrap_or(0);
                    let rtx_payload_type = rtp_params
                        .codecs
                        .iter()
                        .find(|c| c.mime_type().as_str().to_lowercase().ends_with("/rtx"))
                        .map(|c| c.payload_type() as u32)
                        .unwrap_or(0);
                    let codec_mime_type = first_codec
                        .map(|c| c.mime_type().as_str().to_string())
                        .unwrap_or_default();
                    let clock_rate = first_codec.map(|c| c.clock_rate().get()).unwrap_or(0);

                    let encodings = rtp_params
                        .encodings
                        .iter()
                        .map(|enc| SimulcastEncodingInfo {
                            rid: enc.rid.clone().unwrap_or_default(),
                            ssrc: enc.ssrc.unwrap_or(0),
                            rtx_ssrc: enc.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: enc.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if enc.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                enc.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    let current_layers = consumer.current_layers();
                    let initial_spatial_layer =
                        current_layers.map(|l| l.spatial_layer as i32).unwrap_or(-1);
                    let initial_temporal_layer = current_layers
                        .and_then(|l| l.temporal_layer)
                        .map(|t| t as i32)
                        .unwrap_or(-1);

                    let consumer_kind: MediaKind = consumer.kind().into();

                    let item = ConsumerItem {
                        consumer,
                        router_id: params.router_id.clone(),
                        transport_id: params.subscriber_transport_id.clone(),
                        producer_id: params.producer_id.clone(),
                        peer_id,
                        kind: consumer_kind,
                        is_closed,
                    };

                    self.state.consumers.insert(consumer_id.clone(), item);
                    if let Some(set) = self
                        .state
                        .transport_consumers
                        .get(&params.subscriber_transport_id)
                    {
                        set.insert(consumer_id.clone());
                    }
                    if let Some(set) = self.state.producer_consumers.get(&params.producer_id) {
                        set.insert(consumer_id.clone());
                    }
                    self.metrics
                        .inc_active_consumers(&consumer_kind.to_string());

                    Ok(ConsumeResult {
                        consumer_id,
                        producer_id: params.producer_id,
                        kind: consumer_kind.to_string(),
                        rtp_parameters_json: raw_rtp_json,
                        ssrc,
                        rtx_ssrc,
                        payload_type,
                        rtx_payload_type,
                        codec_mime_type,
                        clock_rate,
                        encodings,
                        initial_spatial_layer,
                        initial_temporal_layer,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, resource_id = %resource_id))]
    async fn set_media_paused(
        &self,
        router_id: &str,
        resource_id: &str,
        is_producer: bool,
        paused: bool,
    ) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        if is_producer {
            // Extract producer and DROP guard before .await
            let producer = {
                let p_item = self.state.producers.get(resource_id).ok_or_else(|| {
                    SfuDomainError::NotFound(format!("Producer '{resource_id}' not found"))
                })?;

                if p_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuDomainError::NotFound(format!(
                        "Producer '{resource_id}' is closed"
                    )));
                }

                if p_item.router_id != router_id {
                    return Err(SfuDomainError::NotFound(format!(
                        "Producer '{resource_id}' does not belong to router '{router_id}'"
                    )));
                }

                p_item.producer.clone()
            };

            if paused {
                producer
                    .pause()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            } else {
                producer
                    .resume()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            }
        } else {
            // Extract consumer and DROP guard before .await
            let consumer = {
                let c_item = self.state.consumers.get(resource_id).ok_or_else(|| {
                    SfuDomainError::NotFound(format!("Consumer '{resource_id}' not found"))
                })?;

                if c_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuDomainError::NotFound(format!(
                        "Consumer '{resource_id}' is closed"
                    )));
                }

                if c_item.router_id != router_id {
                    return Err(SfuDomainError::NotFound(format!(
                        "Consumer '{resource_id}' does not belong to router '{router_id}'"
                    )));
                }

                c_item.consumer.clone()
            };

            if paused {
                consumer
                    .pause()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            } else {
                consumer
                    .resume()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, consumer_id = %consumer_id))]
    async fn set_consumer_preferred_layers(
        &self,
        router_id: &str,
        consumer_id: &str,
        layers: ConsumerLayersInfo,
    ) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        // Extract consumer and DROP guard before .await
        let consumer = {
            let c_item = self.state.consumers.get(consumer_id).ok_or_else(|| {
                SfuDomainError::NotFound(format!("Consumer '{consumer_id}' not found"))
            })?;

            if c_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "Consumer '{consumer_id}' is closed"
                )));
            }

            if c_item.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "Consumer '{consumer_id}' does not belong to router '{router_id}'"
                )));
            }

            c_item.consumer.clone()
        };

        consumer
            .set_preferred_layers(ConsumerLayers {
                spatial_layer: layers.spatial_layer,
                temporal_layer: layers.temporal_layer,
            })
            .await
            .map_err(|e| {
                SfuDomainError::Mediasoup(format!("Failed to set preferred layers: {e}"))
            })?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, producer_id = %producer_id))]
    fn close_producer(&self, router_id: &str, producer_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("producer_id", producer_id)?;

        if let Some((_, item)) = self.state.producers.remove(producer_id) {
            if item.router_id != router_id {
                // Restore if router mismatch
                self.state.producers.insert(producer_id.to_string(), item);
                return Err(SfuDomainError::NotFound(format!(
                    "Producer '{producer_id}' does not belong to router '{router_id}'"
                )));
            }

            item.is_closed.store(true, Ordering::SeqCst);
            self.metrics.dec_active_producers(&item.kind.to_string());

            if let Some(set) = self.state.transport_producers.get(&item.transport_id) {
                set.remove(producer_id);
            }

            self.event_publisher.publish_resource_closed(
                router_id,
                "producer",
                producer_id,
                "producer_closed",
            );

            // Cascade close all consumers connected to this producer (Fix orphaned consumers)
            if let Some((_, c_ids)) = self.state.producer_consumers.remove(producer_id) {
                for cid in c_ids {
                    if let Some(c) = self.state.cascade_remove_consumer(&cid) {
                        self.metrics.dec_active_consumers(&c.kind.to_string());
                        self.event_publisher.publish_resource_closed(
                            router_id,
                            "consumer",
                            &cid,
                            "cascade_closed",
                        );
                    }
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, consumer_id = %consumer_id))]
    fn close_consumer(&self, router_id: &str, consumer_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        let router_matches = self
            .state
            .consumers
            .get(consumer_id)
            .map(|c| c.router_id == router_id);

        match router_matches {
            Some(true) => {
                if let Some(item) = self.state.cascade_remove_consumer(consumer_id) {
                    self.metrics.dec_active_consumers(&item.kind.to_string());
                    self.event_publisher.publish_resource_closed(
                        router_id,
                        "consumer",
                        consumer_id,
                        "consumer_closed",
                    );
                }
                Ok(())
            }
            Some(false) => Err(SfuDomainError::NotFound(format!(
                "Consumer '{consumer_id}' does not belong to router '{router_id}'"
            ))),
            None => Ok(()),
        }
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, resource_id = %resource_id))]
    async fn get_stats(
        &self,
        router_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> Result<ResourceStatsInfo, SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        match resource_type.to_lowercase().as_str() {
            "transport" => {
                let transport = {
                    let t_item = self.state.transports.get(resource_id).ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Transport '{resource_id}' not found"))
                    })?;

                    if t_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{resource_id}' is closed"
                        )));
                    }

                    if t_item.router_id != router_id {
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{resource_id}' does not belong to router '{router_id}'"
                        )));
                    }

                    t_item.transport.clone()
                };

                let stats = transport
                    .get_stats()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();

                // Aggregate packet counts from attached producers and consumers
                let mut packets_sent = 0u64;
                let mut packets_received = 0u64;

                let producer_ids: Vec<String> = self
                    .state
                    .transport_producers
                    .get(resource_id)
                    .map(|set| set.iter().map(|id| id.clone()).collect())
                    .unwrap_or_default();

                for pid in producer_ids {
                    let maybe_p = self.state.producers.get(&pid).map(|p| p.producer.clone());
                    if let Some(p) = maybe_p {
                        if let Ok(p_stats) = p.get_stats().await {
                            for s in p_stats {
                                packets_received += s.packet_count;
                            }
                        }
                    }
                }

                let consumer_ids: Vec<String> = self
                    .state
                    .transport_consumers
                    .get(resource_id)
                    .map(|set| set.iter().map(|id| id.clone()).collect())
                    .unwrap_or_default();

                for cid in consumer_ids {
                    let maybe_c = self.state.consumers.get(&cid).map(|c| c.consumer.clone());
                    if let Some(c) = maybe_c {
                        if let Ok(c_stats) = c.get_stats().await {
                            for s in c_stats {
                                if let ConsumerRtpStreamStat::Outbound(out) = s {
                                    packets_sent += out.packet_count;
                                }
                            }
                        }
                    }
                }

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent,
                    bytes_received,
                    packets_sent,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            "producer" => {
                let producer = {
                    let p_item = self.state.producers.get(resource_id).ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Producer '{resource_id}' not found"))
                    })?;

                    if p_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Producer '{resource_id}' is closed"
                        )));
                    }

                    if p_item.router_id != router_id {
                        return Err(SfuDomainError::NotFound(format!(
                            "Producer '{resource_id}' does not belong to router '{router_id}'"
                        )));
                    }

                    p_item.producer.clone()
                };

                let stats = producer
                    .get_stats()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                let (bytes_received, packets_received) =
                    stats.iter().fold((0u64, 0u64), |(b, p), s| {
                        (b + s.byte_count, p + s.packet_count)
                    });

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent: 0,
                    bytes_received,
                    packets_sent: 0,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            "consumer" => {
                let (consumer, current_layers) = {
                    let c_item = self.state.consumers.get(resource_id).ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Consumer '{resource_id}' not found"))
                    })?;

                    if c_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Consumer '{resource_id}' is closed"
                        )));
                    }

                    if c_item.router_id != router_id {
                        return Err(SfuDomainError::NotFound(format!(
                            "Consumer '{resource_id}' does not belong to router '{router_id}'"
                        )));
                    }

                    (c_item.consumer.clone(), c_item.consumer.current_layers())
                };

                let stats = consumer
                    .get_stats()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                // Fix Consumer double counting: Only Outbound goes to bytes_sent / packets_sent!
                let (bytes_sent, packets_sent) =
                    stats.iter().fold((0u64, 0u64), |(b, p), s| match s {
                        ConsumerRtpStreamStat::Outbound(out) => {
                            (b + out.byte_count, p + out.packet_count)
                        }
                        ConsumerRtpStreamStat::Inbound(_) => (b, p),
                    });

                let current_spatial_layer =
                    current_layers.map(|l| l.spatial_layer as i32).unwrap_or(-1);
                let current_temporal_layer = current_layers
                    .and_then(|l| l.temporal_layer)
                    .map(|t| t as i32)
                    .unwrap_or(-1);

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent,
                    bytes_received: 0,
                    packets_sent,
                    packets_received: 0,
                    current_spatial_layer,
                    current_temporal_layer,
                })
            }
            other => Err(SfuDomainError::InvalidArgument(format!(
                "Invalid resource_type '{other}' for get_stats"
            ))),
        }
    }
}
