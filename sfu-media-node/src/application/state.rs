use crate::domain::entities::MediaKind;
use dashmap::{DashMap, DashSet};
use mediasoup::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use mediasoup::audio_level_observer::AudioLevelObserver;
use mediasoup::pipe_transport::PipeTransport;

#[derive(Clone)]
pub struct SubRouterItem {
    pub router: Router,
    pub worker_index: usize,
    pub audio_level_observer: AudioLevelObserver,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct RouterItem {
    pub router: Router,
    pub room_id: String,
    pub worker_index: usize,
    pub audio_level_observer: AudioLevelObserver,
    pub is_closed: Arc<AtomicBool>,
    pub sub_routers: Arc<DashMap<usize, SubRouterItem>>,
}

#[derive(Clone)]
pub struct TransportItem {
    pub transport: WebRtcTransport,
    pub router_id: String,
    pub physical_router_id: String,
    pub worker_index: usize,
    pub peer_id: String,
    pub is_publisher: bool,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct PipeTransportItem {
    pub transport: PipeTransport,
    pub router_id: String,
    pub worker_index: usize,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct PlainTransportItem {
    pub transport: mediasoup::plain_transport::PlainTransport,
    pub router_id: String,
    pub worker_index: usize,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct DataProducerItem {
    pub data_producer: mediasoup::data_producer::DataProducer,
    pub router_id: String,
    pub physical_router_id: String,
    pub worker_index: usize,
    pub transport_id: String,
    pub peer_id: String,
    pub label: String,
    pub protocol: String,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct DataConsumerItem {
    pub data_consumer: mediasoup::data_consumer::DataConsumer,
    pub router_id: String,
    pub transport_id: String,
    pub data_producer_id: String,
    pub label: String,
    pub protocol: String,
    pub is_closed: Arc<AtomicBool>,
    pub worker_index: usize,
}

#[derive(Clone)]
pub struct PipedDataProducerEntry {
    pub router_id: String,
    pub pipe_data_consumer: mediasoup::data_consumer::DataConsumer,
    pub pipe_data_producer: mediasoup::data_producer::DataProducer,
    pub target_router_id: String,
}

#[derive(Clone)]
pub struct ProducerItem {
    pub producer: Producer,
    pub router_id: String,
    pub physical_router_id: String,
    pub worker_index: usize,
    pub transport_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ConsumerItem {
    pub consumer: Consumer,
    pub router_id: String,
    pub physical_router_id: String,
    pub worker_index: usize,
    pub transport_id: String,
    pub producer_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct PipedProducerEntry {
    pub router_id: String,
    pub pipe_consumer: Consumer,
    pub pipe_producer: Producer,
    pub source_worker_index: usize,
    pub target_router_id: String,
    pub target_worker_index: usize,
    pub in_flight_consumes: Arc<AtomicUsize>,
}

/// RAII guard to decrement in_flight_consumes counter upon completion, failure, or cancellation of Consume
pub struct PipedConsumeInFlightGuard {
    counter: Option<Arc<AtomicUsize>>,
}

impl PipedConsumeInFlightGuard {
    pub fn new(counter: Option<Arc<AtomicUsize>>) -> Self {
        Self { counter }
    }
}

impl Drop for PipedConsumeInFlightGuard {
    fn drop(&mut self) {
        if let Some(ref c) = self.counter {
            let mut cur = c.load(Ordering::SeqCst);
            while cur > 0 {
                match c.compare_exchange_weak(cur, cur - 1, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => break,
                    Err(actual) => cur = actual,
                }
            }
        }
    }
}

pub type SubRouterLockMap = DashMap<(String, usize), Arc<tokio::sync::Mutex<()>>>;

pub struct SingleflightKeyGuard<K: std::hash::Hash + Eq + Clone, V> {
    map: Arc<DashMap<K, Arc<V>>>,
    key: K,
}

impl<K: std::hash::Hash + Eq + Clone, V> SingleflightKeyGuard<K, V> {
    pub fn new(map: Arc<DashMap<K, Arc<V>>>, key: K) -> Self {
        Self { map, key }
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> Drop for SingleflightKeyGuard<K, V> {
    fn drop(&mut self) {
        // 1 reference inside DashMap + 1 reference in local `lock` variable = 2.
        // If > 2, another concurrent task is waiting on `lock.lock().await`.
        self.map
            .remove_if(&self.key, |_, v| Arc::strong_count(v) <= 2);
    }
}

#[derive(Clone)]
pub struct EngineState {
    pub routers: Arc<DashMap<String, RouterItem>>,
    pub room_to_router: Arc<DashMap<String, String>>,
    pub transports: Arc<DashMap<String, TransportItem>>,
    pub pipe_transports: Arc<DashMap<String, PipeTransportItem>>,
    pub plain_transports: Arc<DashMap<String, PlainTransportItem>>,
    pub direct_transports: Arc<DashMap<String, mediasoup::direct_transport::DirectTransport>>,
    pub direct_data_producers:
        Arc<DashMap<(String, String, String), mediasoup::data_producer::DirectDataProducer>>,
    pub router_transports: Arc<DashMap<String, DashSet<String>>>,
    pub router_pipe_transports: Arc<DashMap<String, DashSet<String>>>,
    pub router_plain_transports: Arc<DashMap<String, DashSet<String>>>,
    pub producers: Arc<DashMap<(String, String), ProducerItem>>,
    pub transport_producers: Arc<DashMap<String, DashSet<String>>>,
    pub producer_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub consumers: Arc<DashMap<String, ConsumerItem>>,
    pub worker_consumer_counts: Arc<DashMap<usize, AtomicUsize>>,
    pub transport_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub data_producers: Arc<DashMap<(String, String), DataProducerItem>>,
    pub data_consumers: Arc<DashMap<String, DataConsumerItem>>,
    pub transport_data_producers: Arc<DashMap<String, DashSet<String>>>,
    pub transport_data_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub piped_data_producers: Arc<DashMap<String, Vec<PipedDataProducerEntry>>>,
    pub peer_worker_affinity: Arc<DashMap<(String, String), usize>>,
    pub piped_producers: Arc<DashMap<String, Vec<PipedProducerEntry>>>,
    pub dynacast_tracker: Arc<crate::domain::dynacast::DynacastTracker>,
    pub room_singleflight: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub pipe_singleflight: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub sub_router_singleflight: Arc<SubRouterLockMap>,
    pub is_draining: Arc<AtomicBool>,
    pub active_license: Arc<parking_lot::RwLock<Option<crate::domain::license::LicenseClaims>>>,
    pub license_public_key: Arc<parking_lot::RwLock<[u8; 32]>>,
}

impl Default for EngineState {
    fn default() -> Self {
        let default_pubkey = crate::domain::license::parse_public_key_hex(
            crate::domain::license::DEFAULT_PUBLIC_KEY_HEX,
        )
        .unwrap_or([0u8; 32]);
        Self {
            routers: Arc::new(DashMap::new()),
            room_to_router: Arc::new(DashMap::new()),
            transports: Arc::new(DashMap::new()),
            pipe_transports: Arc::new(DashMap::new()),
            plain_transports: Arc::new(DashMap::new()),
            direct_transports: Arc::new(DashMap::new()),
            direct_data_producers: Arc::new(DashMap::new()),
            router_transports: Arc::new(DashMap::new()),
            router_pipe_transports: Arc::new(DashMap::new()),
            router_plain_transports: Arc::new(DashMap::new()),
            producers: Arc::new(DashMap::new()),
            transport_producers: Arc::new(DashMap::new()),
            producer_consumers: Arc::new(DashMap::new()),
            consumers: Arc::new(DashMap::new()),
            worker_consumer_counts: Arc::new(DashMap::new()),
            transport_consumers: Arc::new(DashMap::new()),
            data_producers: Arc::new(DashMap::new()),
            data_consumers: Arc::new(DashMap::new()),
            transport_data_producers: Arc::new(DashMap::new()),
            transport_data_consumers: Arc::new(DashMap::new()),
            piped_data_producers: Arc::new(DashMap::new()),
            peer_worker_affinity: Arc::new(DashMap::new()),
            piped_producers: Arc::new(DashMap::new()),
            dynacast_tracker: Arc::new(crate::domain::dynacast::DynacastTracker::new()),
            room_singleflight: Arc::new(DashMap::new()),
            pipe_singleflight: Arc::new(DashMap::new()),
            sub_router_singleflight: Arc::new(SubRouterLockMap::new()),
            is_draining: Arc::new(AtomicBool::new(false)),
            active_license: Arc::new(parking_lot::RwLock::new(None)),
            license_public_key: Arc::new(parking_lot::RwLock::new(default_pubkey)),
        }
    }
}

impl EngineState {
    pub fn inc_worker_consumers(&self, worker_index: usize) {
        self.worker_consumer_counts
            .entry(worker_index)
            .or_insert_with(|| AtomicUsize::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_worker_consumers(&self, worker_index: usize) {
        if let Some(counter) = self.worker_consumer_counts.get(&worker_index) {
            let mut cur = counter.load(Ordering::Relaxed);
            while cur > 0 {
                match counter.compare_exchange_weak(
                    cur,
                    cur - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => cur = actual,
                }
            }
        }
    }

    pub fn get_worker_consumers(&self, worker_index: usize) -> usize {
        self.worker_consumer_counts
            .get(&worker_index)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
    pub fn get_active_license(&self) -> Option<crate::domain::license::LicenseClaims> {
        self.active_license.read().clone()
    }

    pub fn set_active_license(&self, claims: crate::domain::license::LicenseClaims) {
        *self.active_license.write() = Some(claims);
    }

    pub fn set_license_public_key(&self, key: [u8; 32]) {
        *self.license_public_key.write() = key;
    }

    pub fn get_license_public_key(&self) -> [u8; 32] {
        *self.license_public_key.read()
    }

    pub fn is_enterprise_license_valid(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.active_license
            .read()
            .as_ref()
            .map(|lic| lic.is_enterprise_cascading_allowed(now))
            .unwrap_or(false)
    }

    pub fn get_license_tier(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(ref lic) = *self.active_license.read() {
            if lic.is_valid_at(now) && lic.tier.eq_ignore_ascii_case("enterprise") {
                return "enterprise".to_string();
            }
        }
        "community".to_string()
    }

    pub fn is_router_active(&self, router_id: &str) -> bool {
        self.routers
            .get(router_id)
            .map(|r| !r.is_closed.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn get_producer(
        &self,
        router_id: &str,
        producer_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, (String, String), ProducerItem>> {
        self.producers
            .get(&(router_id.to_string(), producer_id.to_string()))
    }

    pub fn get_room_id_for_router(&self, router_id: &str) -> Option<String> {
        self.routers.get(router_id).map(|r| r.room_id.clone())
    }

    /// Removes and returns only the consumer IDs belonging to `(router_id, producer_id)`,
    /// preserving consumers on other cascaded routers that share `producer_id`.
    pub fn take_consumers_for_producer(&self, router_id: &str, producer_id: &str) -> Vec<String> {
        let matching_cids: Vec<String> = if let Some(set) = self.producer_consumers.get(producer_id)
        {
            set.iter()
                .filter(|cid| {
                    self.consumers
                        .get(cid.as_str())
                        .map(|c| c.router_id == router_id)
                        .unwrap_or(true)
                })
                .map(|cid| cid.clone())
                .collect()
        } else {
            return Vec::new();
        };

        if let Some(set) = self.producer_consumers.get(producer_id) {
            for cid in &matching_cids {
                set.remove(cid);
            }
        }

        let has_other_router_producer = self
            .producers
            .iter()
            .any(|entry| entry.key().1 == producer_id && entry.key().0 != router_id);

        if !has_other_router_producer {
            self.producer_consumers
                .remove_if(producer_id, |_, set| set.is_empty());
        }

        matching_cids
    }

    /// Removes and returns only the intra-node `PipedProducerEntry` items belonging to `(router_id, producer_id)`.
    pub fn take_piped_producers_for_router(
        &self,
        router_id: &str,
        producer_id: &str,
    ) -> Vec<PipedProducerEntry> {
        let mut removed = Vec::new();
        if let Some(mut entries) = self.piped_producers.get_mut(producer_id) {
            let mut i = 0;
            while i < entries.len() {
                if entries[i].router_id == router_id {
                    removed.push(entries.swap_remove(i));
                } else {
                    i += 1;
                }
            }
        }
        self.piped_producers
            .remove_if(producer_id, |_, entries| entries.is_empty());
        // SingleflightKeyGuard automatically cleans up keys when the last waiter drops
        removed
    }

    pub fn cascade_remove_consumer(&self, consumer_id: &str) -> Option<ConsumerItem> {
        if let Some((_, item)) = self.consumers.remove(consumer_id) {
            item.is_closed.store(true, Ordering::SeqCst);
            self.dec_worker_consumers(item.worker_index);
            if let Some(set) = self.transport_consumers.get(&item.transport_id) {
                set.remove(consumer_id);
            }
            if let Some(set) = self.producer_consumers.get(&item.producer_id) {
                set.remove(consumer_id);
            }
            Some(item)
        } else {
            None
        }
    }

    pub fn cleanup_piped_producer_if_last_consumer(
        &self,
        router_id: &str,
        producer_id: &str,
        target_router_id: &str,
    ) -> Option<(PipedProducerEntry, usize)> {
        if let Some(p) = self.get_producer(router_id, producer_id) {
            if p.physical_router_id == target_router_id {
                return None;
            }
        }

        let has_remaining = self
            .producer_consumers
            .get(producer_id)
            .map(|c_ids| {
                c_ids.iter().any(|cid| {
                    self.consumers
                        .get(cid.as_str())
                        .map(|c| {
                            c.router_id == router_id && c.physical_router_id == target_router_id
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if has_remaining {
            return None;
        }

        // Only cleanup if no in-flight consume operations are currently running on this pipe
        let removed = self
            .piped_producers
            .get_mut(producer_id)
            .and_then(|mut entries| {
                entries
                    .iter()
                    .position(|e| {
                        e.router_id == router_id
                            && e.target_router_id == target_router_id
                            && e.in_flight_consumes.load(Ordering::SeqCst) == 0
                    })
                    .map(|pos| entries.swap_remove(pos))
            });

        if let Some(entry) = removed {
            let source_worker_index = entry.source_worker_index;
            self.piped_producers
                .remove_if(producer_id, |_, e| e.is_empty());

            Some((entry, source_worker_index))
        } else {
            None
        }
    }

    pub fn cleanup_dead_worker(
        &self,
        worker_index: usize,
        worker_pool: &dyn crate::ports::outbound::WorkerPoolPort,
        metrics: &dyn crate::ports::outbound::MetricsPort,
        event_publisher: &dyn crate::ports::outbound::EventPublisherPort,
    ) {
        self.worker_consumer_counts.remove(&worker_index);

        // 1. Identify and close all WebRtcTransports on this worker
        let dead_transports: Vec<(String, String, bool)> = self
            .transports
            .iter()
            .filter(|entry| entry.value().worker_index == worker_index)
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().router_id.clone(),
                    entry.value().is_publisher,
                )
            })
            .collect();

        for (tid, router_id, is_publisher) in dead_transports {
            if let Some((_, t)) = self.transports.remove(&tid) {
                t.is_closed.store(true, Ordering::SeqCst);
                metrics.dec_active_transports(is_publisher);
                if let Some(set) = self.router_transports.get_mut(&router_id) {
                    set.remove(&tid);
                }
                self.transport_producers.remove(&tid);
                self.transport_consumers.remove(&tid);

                if let Some((_, dp_ids)) = self.transport_data_producers.remove(&tid) {
                    for dpid in dp_ids {
                        if let Some((_, dp)) = self
                            .data_producers
                            .remove(&(router_id.clone(), dpid.clone()))
                        {
                            dp.is_closed.store(true, Ordering::SeqCst);
                            self.piped_data_producers.remove(&dpid);
                            event_publisher.publish_resource_closed(
                                &router_id,
                                "data_producer",
                                &dpid,
                                "worker_died",
                            );
                        }
                    }
                }
                if let Some((_, dc_ids)) = self.transport_data_consumers.remove(&tid) {
                    for dcid in dc_ids {
                        if let Some((_, dc)) = self.data_consumers.remove(&dcid) {
                            dc.is_closed.store(true, Ordering::SeqCst);
                            event_publisher.publish_resource_closed(
                                &router_id,
                                "data_consumer",
                                &dcid,
                                "worker_died",
                            );
                        }
                    }
                }

                event_publisher.publish_resource_closed(
                    &router_id,
                    "transport",
                    &tid,
                    "worker_died",
                );
            }
        }

        // 1b. Identify and close all PipeTransports on this worker
        let dead_pipe_transports: Vec<(String, String)> = self
            .pipe_transports
            .iter()
            .filter(|entry| entry.value().worker_index == worker_index)
            .map(|entry| (entry.key().clone(), entry.value().router_id.clone()))
            .collect();

        for (ptid, router_id) in dead_pipe_transports {
            if let Some((_, pt)) = self.pipe_transports.remove(&ptid) {
                pt.is_closed.store(true, Ordering::SeqCst);
                metrics.dec_active_transports(false);
                metrics.dec_active_pipe_transports();
                if let Some(set) = self.router_pipe_transports.get_mut(&router_id) {
                    set.remove(&ptid);
                }
                self.transport_producers.remove(&ptid);
                self.transport_consumers.remove(&ptid);
                event_publisher.publish_resource_closed(
                    &router_id,
                    "transport",
                    &ptid,
                    "worker_died",
                );
            }
        }

        // 1c. Identify and close all PlainTransports on this worker
        let dead_plain_transports: Vec<(String, String)> = self
            .plain_transports
            .iter()
            .filter(|entry| entry.value().worker_index == worker_index)
            .map(|entry| (entry.key().clone(), entry.value().router_id.clone()))
            .collect();

        for (ptid, router_id) in dead_plain_transports {
            if let Some((_, pt)) = self.plain_transports.remove(&ptid) {
                pt.is_closed.store(true, Ordering::SeqCst);
                metrics.dec_active_transports(false);
                if let Some(set) = self.router_plain_transports.get_mut(&router_id) {
                    set.remove(&ptid);
                }
                self.transport_producers.remove(&ptid);
                self.transport_consumers.remove(&ptid);
                self.transport_data_producers.remove(&ptid);
                self.transport_data_consumers.remove(&ptid);
                event_publisher.publish_resource_closed(
                    &router_id,
                    "transport",
                    &ptid,
                    "worker_died",
                );
            }
        }

        // 2. Identify and close all producers on this worker
        let dead_producers: Vec<((String, String), String, MediaKind)> = self
            .producers
            .iter()
            .filter(|entry| entry.value().worker_index == worker_index)
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().router_id.clone(),
                    entry.value().kind,
                )
            })
            .collect();

        for (key, router_id, kind) in dead_producers {
            let pid = key.1.clone();
            if let Some((_, p)) = self.producers.remove(&key) {
                p.is_closed.store(true, Ordering::SeqCst);
                metrics.dec_active_producers(&kind.to_string());
                if let Some(set) = self.transport_producers.get(&p.transport_id) {
                    set.remove(&pid);
                }
                let piped_entries = self.take_piped_producers_for_router(&router_id, &pid);
                for entry in piped_entries {
                    worker_pool.sub_resource_score(entry.target_worker_index, 1);
                }

                event_publisher.publish_resource_closed(
                    &router_id,
                    "producer",
                    &pid,
                    "worker_died",
                );

                // Cascade close consumers connected to this producer on this router
                let c_ids = self.take_consumers_for_producer(&router_id, &pid);
                for cid in c_ids {
                    if let Some(c) = self.cascade_remove_consumer(&cid) {
                        let c_weight = if c.kind == MediaKind::Audio { 1 } else { 2 };
                        worker_pool.sub_resource_score(c.worker_index, c_weight);
                        metrics.dec_active_consumers(&c.kind.to_string());
                        event_publisher.publish_resource_closed(
                            &router_id,
                            "consumer",
                            &cid,
                            "worker_died",
                        );
                    }
                }
            }
        }

        // 3. Identify and close all consumers on this worker
        let dead_consumers: Vec<(String, String, MediaKind)> = self
            .consumers
            .iter()
            .filter(|entry| entry.value().worker_index == worker_index)
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().router_id.clone(),
                    entry.value().kind,
                )
            })
            .collect();

        for (cid, router_id, kind) in dead_consumers {
            if let Some(c) = self.cascade_remove_consumer(&cid) {
                metrics.dec_active_consumers(&kind.to_string());
                if let Some((_piped_entry, pub_worker_idx)) = self
                    .cleanup_piped_producer_if_last_consumer(
                        &c.router_id,
                        &c.producer_id,
                        &c.physical_router_id,
                    )
                {
                    worker_pool.sub_resource_score(pub_worker_idx, 1);
                }
                event_publisher.publish_resource_closed(
                    &router_id,
                    "consumer",
                    &cid,
                    "worker_died",
                );
            }
        }

        // 3b. Clean up any remaining piped_producers entries targeting the dead worker
        let mut empty_piped_keys = Vec::new();
        for mut entry in self.piped_producers.iter_mut() {
            let pid = entry.key().clone();
            let vec = entry.value_mut();
            let mut i = 0;
            while i < vec.len() {
                if vec[i].target_worker_index == worker_index {
                    let removed = vec.swap_remove(i);
                    if let Some(pub_item) = self.get_producer(&removed.router_id, &pid) {
                        worker_pool.sub_resource_score(pub_item.worker_index, 1);
                    }
                } else {
                    i += 1;
                }
            }
            if vec.is_empty() {
                empty_piped_keys.push(pid);
            }
        }
        for k in empty_piped_keys {
            self.piped_producers.remove(&k);
        }

        // 4. Clean up sub-routers on this worker, and identify primary routers living on this worker
        let mut dead_primary_routers = Vec::new();
        for r_entry in self.routers.iter() {
            if let Some((_, sub)) = r_entry.value().sub_routers.remove(&worker_index) {
                sub.is_closed.store(true, Ordering::SeqCst);
            }
            if r_entry.value().worker_index == worker_index {
                dead_primary_routers.push(r_entry.key().clone());
            }
        }

        for router_id in dead_primary_routers {
            if let Some((_, r_item)) = self.routers.remove(&router_id) {
                r_item.is_closed.store(true, Ordering::SeqCst);
                self.room_to_router.remove(&r_item.room_id);
                self.room_singleflight.remove(&r_item.room_id);

                for sub_entry in r_item.sub_routers.iter() {
                    sub_entry.value().is_closed.store(true, Ordering::SeqCst);
                    worker_pool.decrement_load(sub_entry.value().worker_index);
                }
                r_item.sub_routers.clear();

                // Clean up peer affinity and singleflights for this router
                self.peer_worker_affinity.retain(|k, _| k.0 != router_id);
                self.sub_router_singleflight.retain(|k, _| k.0 != router_id);

                // Cascade close any remaining transports associated with this router
                // (e.g. transports and consumers living on sub-routers on alive workers)
                if let Some((_, transport_ids)) = self.router_transports.remove(&router_id) {
                    for tid in transport_ids {
                        if let Some((_, t_item)) = self.transports.remove(&tid) {
                            t_item.is_closed.store(true, Ordering::SeqCst);
                            worker_pool.sub_resource_score(t_item.worker_index, 1);
                            metrics.dec_active_transports(t_item.is_publisher);
                            event_publisher.publish_resource_closed(
                                &router_id,
                                "transport",
                                &tid,
                                "worker_died",
                            );
                            crate::application::transport_service::cascade_close_producers_and_consumers(
                                self,
                                worker_pool,
                                metrics,
                                event_publisher,
                                &router_id,
                                &tid,
                            );
                        }
                    }
                }

                // Cascade close any remaining pipe transports for this router
                if let Some((_, pipe_ids)) = self.router_pipe_transports.remove(&router_id) {
                    for pid in pipe_ids {
                        if let Some((_, pt)) = self.pipe_transports.remove(&pid) {
                            pt.is_closed.store(true, Ordering::SeqCst);
                            worker_pool.sub_resource_score(pt.worker_index, 1);
                            metrics.dec_active_transports(false);
                            metrics.dec_active_pipe_transports();
                            event_publisher.publish_resource_closed(
                                &router_id,
                                "transport",
                                &pid,
                                "worker_died",
                            );
                            crate::application::transport_service::cascade_close_producers_and_consumers(
                                self,
                                worker_pool,
                                metrics,
                                event_publisher,
                                &router_id,
                                &pid,
                            );
                        }
                    }
                }

                // Cascade close any remaining plain transports for this router
                if let Some((_, plain_ids)) = self.router_plain_transports.remove(&router_id) {
                    for ptid in plain_ids {
                        if let Some((_, pt)) = self.plain_transports.remove(&ptid) {
                            pt.is_closed.store(true, Ordering::SeqCst);
                            worker_pool.sub_resource_score(pt.worker_index, 1);
                            metrics.dec_active_transports(false);
                            event_publisher.publish_resource_closed(
                                &router_id,
                                "transport",
                                &ptid,
                                "worker_died",
                            );
                            crate::application::transport_service::cascade_close_producers_and_consumers(
                                self,
                                worker_pool,
                                metrics,
                                event_publisher,
                                &router_id,
                                &ptid,
                            );
                        }
                    }
                }

                self.direct_data_producers.retain(|k, _| k.0 != router_id);
                self.direct_transports.remove(&router_id);

                metrics.dec_active_routers();
                event_publisher.publish_resource_closed(
                    &router_id,
                    "router",
                    &router_id,
                    "worker_died",
                );
            }
        }

        // 5. Clean up peer affinity and sub-router singleflights for this worker
        self.peer_worker_affinity
            .retain(|_, &mut w| w != worker_index);
        self.sub_router_singleflight
            .retain(|k, _| k.1 != worker_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_singleflight_key_guard_drop_preserves_queued_waiters() {
        let map = Arc::new(DashMap::<String, Arc<tokio::sync::Mutex<()>>>::new());
        let key = "room_123".to_string();

        // Task 1 simulates entering critical section
        let lock1 = map
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let guard1 = lock1.lock().await;
        let key_guard1 = SingleflightKeyGuard::new(map.clone(), key.clone());

        // Task 2 simulates queuing up on the same singleflight entry
        let lock2 = map
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        // When Task 1 finishes and drops key_guard1, key should NOT be removed because Task 2 holds lock2
        drop(key_guard1);
        assert!(
            map.contains_key(&key),
            "Key must remain in map while Task 2 is queued"
        );

        // Task 1 releases Mutex and drops its local lock
        drop(guard1);
        drop(lock1);

        // Task 2 acquires lock and sets up its own key guard
        let guard2 = lock2.lock().await;
        let key_guard2 = SingleflightKeyGuard::new(map.clone(), key.clone());

        // Now, no other tasks are waiting. When Task 2 drops key_guard2, key must be removed
        drop(key_guard2);
        assert!(
            !map.contains_key(&key),
            "Key must be removed when no concurrent tasks wait"
        );

        drop(guard2);
        drop(lock2);
    }

    #[tokio::test]
    async fn test_singleflight_key_guard_cancellation_before_lock_cleans_map() {
        let map = Arc::new(DashMap::<String, Arc<tokio::sync::Mutex<()>>>::new());
        let key = "room_cancel".to_string();

        let lock1 = map
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let key_guard1 = SingleflightKeyGuard::new(map.clone(), key.clone());
        let guard1 = lock1.lock().await;

        // Task 2 sets up key_guard before awaiting lock
        let map_clone = map.clone();
        let key_clone = key.clone();
        let handle = tokio::spawn(async move {
            let lock2 = map_clone
                .entry(key_clone.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone();
            let _key_guard2 = SingleflightKeyGuard::new(map_clone.clone(), key_clone.clone());
            let _guard2 = lock2.lock().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Task 1 finishes and releases
        drop(guard1);
        drop(key_guard1);
        drop(lock1);

        // Cancel Task 2
        handle.abort();
        let _ = handle.await;

        assert!(
            !map.contains_key(&key),
            "Key must be cleaned up when waiting task is cancelled"
        );
    }
}
