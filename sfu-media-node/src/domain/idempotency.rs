use crate::domain::errors::SfuDomainError;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum IdempotencyEntry<T> {
    InFlight {
        payload_hash: String,
        created_at: Instant,
    },
    Completed {
        payload_hash: String,
        response: T,
        completed_at: Instant,
    },
}

#[derive(Clone)]
pub struct IdempotencyCache<T> {
    entries: Arc<DashMap<String, IdempotencyEntry<T>>>,
    ttl: Duration,
    in_flight_timeout: Duration,
}

impl<T: Clone + Send + Sync + 'static> Default for IdempotencyCache<T> {
    fn default() -> Self {
        Self::new(Duration::from_secs(60), Duration::from_secs(10))
    }
}

impl<T: Clone + Send + Sync + 'static> IdempotencyCache<T> {
    pub fn new(ttl: Duration, in_flight_timeout: Duration) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            ttl,
            in_flight_timeout,
        }
    }

    pub fn compute_payload_hash(payload: impl AsRef<[u8]>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(payload);
        hex::encode(hasher.finalize())
    }

    pub fn clean_expired(&self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| match entry {
            IdempotencyEntry::InFlight { created_at, .. } => {
                now.duration_since(*created_at) < self.in_flight_timeout
            }
            IdempotencyEntry::Completed { completed_at, .. } => {
                now.duration_since(*completed_at) < self.ttl
            }
        });
    }

    pub fn start_periodic_cleanup(
        self: &Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let cache = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Some(c) = cache.upgrade() {
                    c.clean_expired();
                } else {
                    break;
                }
            }
        })
    }

    pub async fn execute_or_get<F, Fut>(
        &self,
        request_id: &str,
        payload: &[u8],
        action: F,
    ) -> Result<T, SfuDomainError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuDomainError>>,
    {
        self.execute_or_get_with_hook(request_id, payload, action, || {})
            .await
    }

    pub async fn execute_or_get_with_hook<F, Fut, H>(
        &self,
        request_id: &str,
        payload: &[u8],
        action: F,
        on_hit: H,
    ) -> Result<T, SfuDomainError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuDomainError>>,
        H: FnOnce(),
    {
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err(SfuDomainError::InvalidArgument(
                "request_id cannot be empty".to_string(),
            ));
        }

        let hash = Self::compute_payload_hash(payload);
        let now = Instant::now();

        // 1. Check existing state
        if let Some(entry) = self.entries.get(request_id) {
            match entry.value() {
                IdempotencyEntry::Completed {
                    payload_hash,
                    response,
                    completed_at,
                } => {
                    if now.duration_since(*completed_at) < self.ttl {
                        if payload_hash == &hash {
                            on_hit();
                            return Ok(response.clone());
                        } else {
                            return Err(SfuDomainError::IdempotencyConflict(format!(
                                "request_id '{request_id}' already executed with different payload"
                            )));
                        }
                    }
                }
                IdempotencyEntry::InFlight {
                    payload_hash,
                    created_at,
                } => {
                    if now.duration_since(*created_at) < self.in_flight_timeout {
                        if payload_hash != &hash {
                            return Err(SfuDomainError::IdempotencyConflict(format!(
                                "request_id '{request_id}' currently in-flight with different payload"
                            )));
                        }
                        return Err(SfuDomainError::InFlight);
                    }
                }
            }
        }

        // 2. Mark as InFlight
        self.entries.insert(
            request_id.to_string(),
            IdempotencyEntry::InFlight {
                payload_hash: hash.clone(),
                created_at: now,
            },
        );

        // 3. Execute action
        match action().await {
            Ok(res) => {
                self.entries.insert(
                    request_id.to_string(),
                    IdempotencyEntry::Completed {
                        payload_hash: hash,
                        response: res.clone(),
                        completed_at: Instant::now(),
                    },
                );
                Ok(res)
            }
            Err(err) => {
                // On failure, remove in-flight entry to allow retry
                self.entries.remove(request_id);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_idempotent_execution_success() {
        let cache = IdempotencyCache::new(Duration::from_secs(60), Duration::from_secs(5));
        let call_count = Arc::new(AtomicUsize::new(0));

        let action = || {
            let count = call_count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok("success_payload".to_string())
            }
        };

        let res1 = cache
            .execute_or_get("req-1", b"payload-A", action)
            .await
            .unwrap();
        assert_eq!(res1, "success_payload");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Second call with same request_id and payload returns cached response
        let res2 = cache
            .execute_or_get("req-1", b"payload-A", action)
            .await
            .unwrap();
        assert_eq!(res2, "success_payload");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_idempotent_conflict_on_different_payload() {
        let cache = IdempotencyCache::new(Duration::from_secs(60), Duration::from_secs(5));

        let action = || async { Ok("res".to_string()) };

        cache
            .execute_or_get("req-1", b"payload-A", action)
            .await
            .unwrap();

        // Same request_id, different payload must return conflict error
        let err = cache
            .execute_or_get("req-1", b"payload-B", action)
            .await
            .unwrap_err();
        match err {
            SfuDomainError::IdempotencyConflict(_) => (),
            other => panic!("Expected IdempotencyConflict, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let cache = IdempotencyCache::new(Duration::from_secs(60), Duration::from_secs(5));
        let attempts = Arc::new(AtomicUsize::new(0));

        let action_fail = || {
            let att = attempts.clone();
            async move {
                att.fetch_add(1, Ordering::SeqCst);
                Err(SfuDomainError::Internal("transient failure".to_string()))
            }
        };

        let err = cache
            .execute_or_get("req-fail", b"payload", action_fail)
            .await
            .unwrap_err();
        assert!(matches!(err, SfuDomainError::Internal(_)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        // Retrying must execute the action again since the failed key was cleared
        let action_ok = || {
            let att = attempts.clone();
            async move {
                att.fetch_add(1, Ordering::SeqCst);
                Ok("recovered".to_string())
            }
        };

        let res = cache
            .execute_or_get("req-fail", b"payload", action_ok)
            .await
            .unwrap();
        assert_eq!(res, "recovered");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let cache = Arc::new(IdempotencyCache::new(
            Duration::from_millis(50),
            Duration::from_millis(10),
        ));

        let _handle = cache.start_periodic_cleanup(Duration::from_millis(20));

        cache
            .execute_or_get("req-exp", b"payload", || async { Ok(123) })
            .await
            .unwrap();
        assert!(cache.entries.contains_key("req-exp"));

        tokio::time::sleep(Duration::from_millis(80)).await;
        cache.clean_expired();
        assert!(!cache.entries.contains_key("req-exp"));
    }
}
