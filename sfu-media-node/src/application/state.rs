use crate::domain::entities::MediaKind;
use dashmap::{DashMap, DashSet};
use mediasoup::prelude::*;
use std::sync::atomic::AtomicBool;
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
    pub is_closed: Arc<AtomicBool>,
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
    pub pipe_consumer: Consumer,
    pub pipe_producer: Producer,
    pub target_router_id: String,
    pub target_worker_index: usize,
}

pub type SubRouterLockMap = DashMap<(String, usize), Arc<tokio::sync::Mutex<()>>>;

pub struct SingleflightKeyGuard<K: std::hash::Hash + Eq + Clone, V: Clone> {
    map: Arc<DashMap<K, V>>,
    key: K,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> SingleflightKeyGuard<K, V> {
    pub fn new(map: Arc<DashMap<K, V>>, key: K) -> Self {
        Self { map, key }
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Drop for SingleflightKeyGuard<K, V> {
    fn drop(&mut self) {
        self.map.remove(&self.key);
    }
}

#[derive(Clone)]
pub struct EngineState {
    pub routers: Arc<DashMap<String, RouterItem>>,
    pub room_to_router: Arc<DashMap<String, String>>,
    pub transports: Arc<DashMap<String, TransportItem>>,
    pub pipe_transports: Arc<DashMap<String, PipeTransportItem>>,
    pub router_transports: Arc<DashMap<String, DashSet<String>>>,
    pub router_pipe_transports: Arc<DashMap<String, DashSet<String>>>,
    pub producers: Arc<DashMap<String, ProducerItem>>,
    pub transport_producers: Arc<DashMap<String, DashSet<String>>>,
    pub producer_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub consumers: Arc<DashMap<String, ConsumerItem>>,
    pub transport_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub peer_worker_affinity: Arc<DashMap<(String, String), usize>>,
    pub piped_producers: Arc<DashMap<String, Vec<PipedProducerEntry>>>,
    pub room_singleflight: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub pipe_singleflight: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub sub_router_singleflight: Arc<SubRouterLockMap>,
    pub is_draining: Arc<AtomicBool>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            routers: Arc::new(DashMap::new()),
            room_to_router: Arc::new(DashMap::new()),
            transports: Arc::new(DashMap::new()),
            pipe_transports: Arc::new(DashMap::new()),
            router_transports: Arc::new(DashMap::new()),
            router_pipe_transports: Arc::new(DashMap::new()),
            producers: Arc::new(DashMap::new()),
            transport_producers: Arc::new(DashMap::new()),
            producer_consumers: Arc::new(DashMap::new()),
            consumers: Arc::new(DashMap::new()),
            transport_consumers: Arc::new(DashMap::new()),
            peer_worker_affinity: Arc::new(DashMap::new()),
            piped_producers: Arc::new(DashMap::new()),
            room_singleflight: Arc::new(DashMap::new()),
            pipe_singleflight: Arc::new(DashMap::new()),
            sub_router_singleflight: Arc::new(DashMap::new()),
            is_draining: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl EngineState {
    pub fn is_router_active(&self, router_id: &str) -> bool {
        self.routers
            .get(router_id)
            .map(|r| !r.is_closed.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn get_room_id_for_router(&self, router_id: &str) -> Option<String> {
        self.routers.get(router_id).map(|r| r.room_id.clone())
    }

    pub fn cascade_remove_consumer(&self, consumer_id: &str) -> Option<ConsumerItem> {
        if let Some((_, item)) = self.consumers.remove(consumer_id) {
            item.is_closed
                .store(true, std::sync::atomic::Ordering::SeqCst);
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
}
