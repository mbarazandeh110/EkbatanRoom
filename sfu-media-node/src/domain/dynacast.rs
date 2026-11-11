use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynacastTransition {
    pub producer_id: String,
    pub spatial_layer: i32,
    pub is_active: bool,
    pub active_consumers: u32,
}

#[derive(Debug, Default)]
struct ProducerLayerCounters {
    layers: [AtomicUsize; 3],
}

impl ProducerLayerCounters {
    fn new() -> Self {
        Self {
            layers: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
        }
    }

    fn increment(&self, layer: usize) -> (usize, usize) {
        if layer >= 3 {
            return (0, 0);
        }
        let prev = self.layers[layer].fetch_add(1, Ordering::SeqCst);
        (prev, prev + 1)
    }

    fn decrement(&self, layer: usize) -> (usize, usize) {
        if layer >= 3 {
            return (0, 0);
        }
        let mut cur = self.layers[layer].load(Ordering::SeqCst);
        while cur > 0 {
            match self.layers[layer].compare_exchange_weak(
                cur,
                cur - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return (cur, cur - 1),
                Err(actual) => cur = actual,
            }
        }
        (0, 0)
    }

    fn get(&self, layer: usize) -> usize {
        if layer < 3 {
            self.layers[layer].load(Ordering::SeqCst)
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DynacastTracker {
    producers: Arc<DashMap<String, Arc<ProducerLayerCounters>>>,
    consumer_states: Arc<DashMap<String, (String, Option<usize>)>>, // consumer_id -> (producer_id, active_layer)
}

impl DynacastTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or updates a consumer's active layer for a producer.
    /// If paused, active_layer is None.
    pub fn update_consumer(
        &self,
        consumer_id: &str,
        producer_id: &str,
        spatial_layer: Option<i32>,
        paused: bool,
    ) -> Vec<DynacastTransition> {
        let mut transitions = Vec::new();
        let target_layer = if paused {
            None
        } else {
            spatial_layer.map(|l| (l.clamp(0, 2)) as usize)
        };

        let counters = self
            .producers
            .entry(producer_id.to_string())
            .or_insert_with(|| Arc::new(ProducerLayerCounters::new()))
            .clone();

        let old_state = self.consumer_states.insert(
            consumer_id.to_string(),
            (producer_id.to_string(), target_layer),
        );

        let old_layer = old_state.and_then(|(_, l)| l);

        if old_layer == target_layer {
            return transitions;
        }

        // Decrement old layer if previously active
        if let Some(old_l) = old_layer {
            let (prev, new_val) = counters.decrement(old_l);
            if prev == 1 && new_val == 0 {
                transitions.push(DynacastTransition {
                    producer_id: producer_id.to_string(),
                    spatial_layer: old_l as i32,
                    is_active: false,
                    active_consumers: 0,
                });
            }
        }

        // Increment new layer if currently active
        if let Some(new_l) = target_layer {
            let (prev, new_val) = counters.increment(new_l);
            if prev == 0 && new_val == 1 {
                transitions.push(DynacastTransition {
                    producer_id: producer_id.to_string(),
                    spatial_layer: new_l as i32,
                    is_active: true,
                    active_consumers: new_val as u32,
                });
            }
        }

        transitions
    }

    /// Removes a consumer upon closure and updates layer counts.
    pub fn remove_consumer(&self, consumer_id: &str) -> Vec<DynacastTransition> {
        let mut transitions = Vec::new();
        if let Some((_, (producer_id, Some(old_l)))) = self.consumer_states.remove(consumer_id) {
            if let Some(counters) = self.producers.get(&producer_id) {
                let (prev, new_val) = counters.decrement(old_l);
                if prev == 1 && new_val == 0 {
                    transitions.push(DynacastTransition {
                        producer_id: producer_id.clone(),
                        spatial_layer: old_l as i32,
                        is_active: false,
                        active_consumers: 0,
                    });
                }
            }
        }
        transitions
    }

    /// Removes a producer when it closes.
    pub fn remove_producer(&self, producer_id: &str) {
        self.producers.remove(producer_id);
        self.consumer_states
            .retain(|_, (p_id, _)| p_id != producer_id);
    }

    /// Gets active consumer count for a producer's layer.
    pub fn get_active_count(&self, producer_id: &str, layer: usize) -> usize {
        self.producers
            .get(producer_id)
            .map(|c| c.get(layer))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynacast_layer_activation_and_deactivation() {
        let tracker = DynacastTracker::new();
        let pid = "prod_video_1";

        // Consumer 1 joins on layer 2 (720p)
        let t1 = tracker.update_consumer("cons_1", pid, Some(2), false);
        assert_eq!(t1.len(), 1);
        assert_eq!(
            t1[0],
            DynacastTransition {
                producer_id: pid.to_string(),
                spatial_layer: 2,
                is_active: true,
                active_consumers: 1,
            }
        );

        // Consumer 2 joins on layer 2 -> active_consumers goes from 1 to 2 -> No transition event (already active)
        let t2 = tracker.update_consumer("cons_2", pid, Some(2), false);
        assert!(t2.is_empty());
        assert_eq!(tracker.get_active_count(pid, 2), 2);

        // Consumer 1 pauses -> active count 2 -> 1 -> No deactivation event
        let t3 = tracker.update_consumer("cons_1", pid, Some(2), true);
        assert!(t3.is_empty());
        assert_eq!(tracker.get_active_count(pid, 2), 1);

        // Consumer 2 pauses -> active count 1 -> 0 -> Deactivation event emitted!
        let t4 = tracker.update_consumer("cons_2", pid, Some(2), true);
        assert_eq!(t4.len(), 1);
        assert_eq!(
            t4[0],
            DynacastTransition {
                producer_id: pid.to_string(),
                spatial_layer: 2,
                is_active: false,
                active_consumers: 0,
            }
        );

        // Consumer 1 unpauses and switches to layer 0 (180p) -> Layer 0 active event
        let t5 = tracker.update_consumer("cons_1", pid, Some(0), false);
        assert_eq!(t5.len(), 1);
        assert_eq!(
            t5[0],
            DynacastTransition {
                producer_id: pid.to_string(),
                spatial_layer: 0,
                is_active: true,
                active_consumers: 1,
            }
        );

        // Consumer 1 closes -> Layer 0 deactivation event
        let t6 = tracker.remove_consumer("cons_1");
        assert_eq!(t6.len(), 1);
        assert_eq!(
            t6[0],
            DynacastTransition {
                producer_id: pid.to_string(),
                spatial_layer: 0,
                is_active: false,
                active_consumers: 0,
            }
        );
    }
}
