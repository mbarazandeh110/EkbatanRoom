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

    pub fn compute_payload_hash_parts(parts: &[&[u8]]) -> String {
        let mut hasher = Sha256::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                hasher.update(b":");
            }
            hasher.update(part);
        }
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

    pub async fn execute_or_get_parts<F, Fut>(
        &self,
        request_id: &str,
        payload_parts: &[&[u8]],
        action: F,
    ) -> Result<T, SfuDomainError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuDomainError>>,
    {
        self.execute_or_get_validated_parts(request_id, payload_parts, |_| true, action, || {})
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
        self.execute_or_get_validated(request_id, payload, |_| true, action, on_hit)
            .await
    }

    pub async fn execute_or_get_validated<F, Fut, V, H>(
        &self,
        request_id: &str,
        payload: &[u8],
        is_valid: V,
        action: F,
        on_hit: H,
    ) -> Result<T, SfuDomainError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuDomainError>>,
        V: FnOnce(&T) -> bool,
        H: FnOnce(),
    {
        self.execute_or_get_validated_parts(request_id, &[payload], is_valid, action, on_hit)
            .await
    }

    pub async fn execute_or_get_validated_parts<F, Fut, V, H>(
        &self,
        request_id: &str,
        payload_parts: &[&[u8]],
        is_valid: V,
        action: F,
        on_hit: H,
    ) -> Result<T, SfuDomainError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuDomainError>>,
        V: FnOnce(&T) -> bool,
        H: FnOnce(),
    {
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err(SfuDomainError::InvalidArgument(
                "request_id cannot be empty".to_string(),
            ));
        }

        let hash = Self::compute_payload_hash_parts(payload_parts);
        let now = Instant::now();

        // 1. Atomic check and transition to InFlight in an isolated synchronous scope
        {
            use dashmap::mapref::entry::Entry;
            match self.entries.entry(request_id.to_string()) {
                Entry::Occupied(mut occupied) => {
                    match occupied.get() {
                        IdempotencyEntry::Completed {
                            payload_hash,
                            response,
                            completed_at,
                        } => {
                            if now.duration_since(*completed_at) < self.ttl {
                                if payload_hash != &hash {
                                    return Err(SfuDomainError::IdempotencyConflict(format!(
                                        "request_id '{request_id}' already executed with different payload"
                                    )));
                                }
                                if is_valid(response) {
                                    on_hit();
                                    return Ok(response.clone());
                                }
                                // If previous resource is closed or invalid, replace cached entry with fresh InFlight
                            }
                            occupied.insert(IdempotencyEntry::InFlight {
                                payload_hash: hash.clone(),
                                created_at: now,
                            });
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
                            // Timed out in-flight: overwrite with new InFlight
                            occupied.insert(IdempotencyEntry::InFlight {
                                payload_hash: hash.clone(),
                                created_at: now,
                            });
                        }
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(IdempotencyEntry::InFlight {
                        payload_hash: hash.clone(),
                        created_at: now,
                    });
                }
            }
        } // Shard entry lock is dropped immediately here before action().await

        struct InFlightDropGuard<'a, T: Clone> {
            cache: &'a DashMap<String, IdempotencyEntry<T>>,
            request_id: &'a str,
            completed: bool,
        }

        impl<'a, T: Clone> InFlightDropGuard<'a, T> {
            fn new(cache: &'a DashMap<String, IdempotencyEntry<T>>, request_id: &'a str) -> Self {
                Self {
                    cache,
                    request_id,
                    completed: false,
                }
            }

            fn complete(mut self) {
                self.completed = true;
            }
        }

        impl<'a, T: Clone> Drop for InFlightDropGuard<'a, T> {
            fn drop(&mut self) {
                if !self.completed {
                    self.cache.remove(self.request_id);
                }
            }
        }

        let guard = InFlightDropGuard::new(&self.entries, request_id);

        // 3. Execute action
        match action().await {
            Ok(res) => {
                guard.complete();
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
                // On failure, guard.drop() automatically removes in-flight entry to allow retry
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

    #[tokio::test]
    async fn test_cancellation_cleans_inflight_entry() {
        let cache = Arc::new(IdempotencyCache::new(
            Duration::from_secs(60),
            Duration::from_secs(10),
        ));

        let cache_clone = cache.clone();
        let task = tokio::spawn(async move {
            cache_clone
                .execute_or_get("req-cancel", b"payload", || async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Ok("done".to_string())
                })
                .await
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cache.entries.contains_key("req-cancel"));

        task.abort();
        let _ = task.await;

        assert!(
            !cache.entries.contains_key("req-cancel"),
            "InFlight entry must be removed immediately on cancellation"
        );

        let res = cache
            .execute_or_get("req-cancel", b"payload", || async {
                Ok("immediate-retry".to_string())
            })
            .await
            .unwrap();
        assert_eq!(res, "immediate-retry");
    }

    #[tokio::test]
    async fn test_idempotent_reexecution_when_resource_invalid() {
        let cache = IdempotencyCache::new(Duration::from_secs(60), Duration::from_secs(5));
        let call_count = Arc::new(AtomicUsize::new(0));

        let action = || {
            let count = call_count.clone();
            async move {
                let id = count.fetch_add(1, Ordering::SeqCst);
                Ok(format!("resource-{id}"))
            }
        };

        // First call: succeeds and caches response
        let res1 = cache
            .execute_or_get_validated("req-val", b"payload", |_| true, action, || {})
            .await
            .unwrap();
        assert_eq!(res1, "resource-0");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Second call with is_valid returning false (simulating closed resource):
        // It must invalidate cache entry and re-execute action!
        let res2 = cache
            .execute_or_get_validated("req-val", b"payload", |_| false, action, || {})
            .await
            .unwrap();
        assert_eq!(res2, "resource-1");
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_parts_hash_and_execution() {
        let parts: &[&[u8]] = &[b"req1", b"room1", b"audio", b"{\"key\":\"val\"}", b"false"];
        let joined = b"req1:room1:audio:{\"key\":\"val\"}:false";

        assert_eq!(
            IdempotencyCache::<()>::compute_payload_hash_parts(parts),
            IdempotencyCache::<()>::compute_payload_hash(joined)
        );

        let cache = IdempotencyCache::new(Duration::from_secs(60), Duration::from_secs(5));
        let res = cache
            .execute_or_get_validated_parts(
                "req-parts",
                parts,
                |_| true,
                || async { Ok(42) },
                || {},
            )
            .await
            .unwrap();
        assert_eq!(res, 42);

        // Cached hit
        let mut hit = false;
        let res_hit = cache
            .execute_or_get_validated_parts(
                "req-parts",
                parts,
                |_| true,
                || async { Ok(99) },
                || {
                    hit = true;
                },
            )
            .await
            .unwrap();
        assert_eq!(res_hit, 42);
        assert!(hit);
    }
}
