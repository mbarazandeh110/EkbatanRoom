use crate::domain::entities::MediaKind;
use dashmap::{DashMap, DashSet};
use mediasoup::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use mediasoup::audio_level_observer::AudioLevelObserver;

#[derive(Clone)]
pub struct RouterItem {
    pub router: Router,
    pub room_id: String,
    pub worker_index: usize,
    pub audio_level_observer: AudioLevelObserver,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct TransportItem {
    pub transport: WebRtcTransport,
    pub router_id: String,
    pub peer_id: String,
    pub is_publisher: bool,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ProducerItem {
    pub producer: Producer,
    pub router_id: String,
    pub transport_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ConsumerItem {
    pub consumer: Consumer,
    pub router_id: String,
    pub transport_id: String,
    pub producer_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct EngineState {
    pub routers: Arc<DashMap<String, RouterItem>>,
    pub room_to_router: Arc<DashMap<String, String>>,
    pub transports: Arc<DashMap<String, TransportItem>>,
    pub router_transports: Arc<DashMap<String, DashSet<String>>>,
    pub producers: Arc<DashMap<String, ProducerItem>>,
    pub transport_producers: Arc<DashMap<String, DashSet<String>>>,
    pub producer_consumers: Arc<DashMap<String, DashSet<String>>>,
    pub consumers: Arc<DashMap<String, ConsumerItem>>,
    pub transport_consumers: Arc<DashMap<String, DashSet<String>>>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            routers: Arc::new(DashMap::new()),
            room_to_router: Arc::new(DashMap::new()),
            transports: Arc::new(DashMap::new()),
            router_transports: Arc::new(DashMap::new()),
            producers: Arc::new(DashMap::new()),
            transport_producers: Arc::new(DashMap::new()),
            producer_consumers: Arc::new(DashMap::new()),
            consumers: Arc::new(DashMap::new()),
            transport_consumers: Arc::new(DashMap::new()),
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
