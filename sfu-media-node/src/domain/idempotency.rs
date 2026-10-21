use crate::domain::error::SfuError;
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

    pub async fn execute_or_get<F, Fut>(
        &self,
        request_id: &str,
        payload: &[u8],
        action: F,
    ) -> Result<T, SfuError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, SfuError>>,
    {
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return Err(SfuError::InvalidArgument(
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
                            return Ok(response.clone());
                        } else {
                            return Err(SfuError::IdempotencyConflict(format!(
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
                            return Err(SfuError::IdempotencyConflict(format!(
                                "request_id '{request_id}' currently in-flight with different payload"
                            )));
                        }
                        return Err(SfuError::InFlight);
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
                // Remove on failure to allow retry
                self.entries.remove(request_id);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_idempotent_execution_success() {
        let cache = IdempotencyCache::<String>::default();
        let payload = b"payload-1";

        let res1 = cache
            .execute_or_get("req-1", payload, || async { Ok("success-1".to_string()) })
            .await
            .unwrap();

        assert_eq!(res1, "success-1");

        // Same request_id, same payload -> returns cached result without running action
        let res2 = cache
            .execute_or_get("req-1", payload, || async {
                panic!("Action should not be called on cache hit");
            })
            .await
            .unwrap();

        assert_eq!(res2, "success-1");
    }

    #[tokio::test]
    async fn test_idempotent_conflict_on_different_payload() {
        let cache = IdempotencyCache::<String>::default();
        cache
            .execute_or_get("req-2", b"payload-a", || async { Ok("ok".to_string()) })
            .await
            .unwrap();

        // Same request_id, different payload -> Conflict error
        let err = cache
            .execute_or_get("req-2", b"payload-b", || async {
                Ok("different".to_string())
            })
            .await
            .unwrap_err();

        match err {
            SfuError::IdempotencyConflict(_) => {}
            other => panic!("Expected IdempotencyConflict, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let cache = IdempotencyCache::<String>::default();
        let payload = b"payload-retry";

        let err = cache
            .execute_or_get("req-retry", payload, || async {
                Err(SfuError::Internal("temporary failure".into()))
            })
            .await
            .unwrap_err();

        assert_eq!(err, SfuError::Internal("temporary failure".into()));

        // Should allow retry after failure
        let res = cache
            .execute_or_get("req-retry", payload, || async {
                Ok("success-on-retry".to_string())
            })
            .await
            .unwrap();

        assert_eq!(res, "success-on-retry");
    }

    #[tokio::test]
    async fn test_empty_request_id_rejected() {
        let cache = IdempotencyCache::<String>::default();
        let err = cache
            .execute_or_get("  ", b"payload", || async { Ok("ok".into()) })
            .await
            .unwrap_err();

        match err {
            SfuError::InvalidArgument(_) => {}
            other => panic!("Expected InvalidArgument, got: {:?}", other),
        }
    }
}
