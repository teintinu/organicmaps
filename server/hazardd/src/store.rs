//! Storage interface and the in-memory backend.
//!
//! `HazardStore` is the seam: the Postgres backend (`pg.rs`) and the in-memory
//! backend used by tests and single-node demos implement the same contract.

use crate::domain::{distance_m, Hazard, HazardDraft, HazardType};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::RwLock;

const DEDUPE_RADIUS_M: f64 = 100.0;
const DEVICE_TTL: Duration = Duration::days(90);

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vote {
    Confirm,
    Deny,
    Resolved,
}

#[derive(Debug)]
pub enum CreateOutcome {
    Created(Hazard),
    Merged(Hazard),
}

#[derive(Debug, Default, Clone)]
pub struct ListQuery {
    pub bbox: Option<(f64, f64, f64, f64)>,
    pub since_ms: Option<i64>,
    pub types: Vec<HazardType>,
    pub limit: usize,
}

/// Everything the HTTP layer needs from a storage backend.
#[async_trait]
pub trait HazardStore: Send + Sync {
    async fn create_device(&self, token_hash: String, now: DateTime<Utc>) -> Device;
    async fn device(&self, token_hash: &str, now: DateTime<Utc>) -> Option<Device>;
    async fn remove_device(&self, token_hash: &str) -> usize;
    async fn create(
        &self,
        draft: HazardDraft,
        device_id: &str,
        idempotency_key: Option<&str>,
        now: DateTime<Utc>,
    ) -> CreateOutcome;
    async fn get(&self, id: &str) -> Option<Hazard>;
    async fn list(&self, query: &ListQuery, now: DateTime<Utc>) -> Vec<Hazard>;
    async fn vote(&self, id: &str, vote: Vote, now: DateTime<Utc>) -> Option<Hazard>;
    async fn purge_expired(&self, now: DateTime<Utc>) -> usize;
    async fn health(&self) -> bool;
}

#[derive(Default)]
struct Inner {
    hazards: HashMap<String, Hazard>,
    devices: HashMap<String, Device>,
    idempotency: HashMap<String, String>,
}

#[derive(Default)]
pub struct MemoryStore {
    inner: RwLock<Inner>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().hazards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl HazardStore for MemoryStore {
    async fn create_device(&self, token_hash: String, now: DateTime<Utc>) -> Device {
        let device = Device {
            id: uuid::Uuid::now_v7().to_string(),
            expires_at: now + DEVICE_TTL,
        };
        self.inner
            .write()
            .unwrap()
            .devices
            .insert(token_hash, device.clone());
        device
    }

    async fn device(&self, token_hash: &str, now: DateTime<Utc>) -> Option<Device> {
        self.inner
            .read()
            .unwrap()
            .devices
            .get(token_hash)
            .filter(|d| d.expires_at > now)
            .cloned()
    }

    async fn remove_device(&self, token_hash: &str) -> usize {
        let mut inner = self.inner.write().unwrap();
        let Some(device) = inner.devices.remove(token_hash) else {
            return 0;
        };
        let mut touched = 0;
        for hazard in inner.hazards.values_mut() {
            if hazard.reported_by.as_deref() == Some(device.id.as_str()) {
                hazard.reported_by = None;
                touched += 1;
            }
        }
        touched
    }

    async fn create(
        &self,
        draft: HazardDraft,
        device_id: &str,
        idempotency_key: Option<&str>,
        now: DateTime<Utc>,
    ) -> CreateOutcome {
        let mut inner = self.inner.write().unwrap();

        if let Some(key) = idempotency_key {
            if let Some(existing) = inner.idempotency.get(key) {
                if let Some(hazard) = inner.hazards.get(existing) {
                    return CreateOutcome::Merged(hazard.clone());
                }
            }
        }

        let duplicate = inner
            .hazards
            .values()
            .filter(|h| h.kind == draft.kind && h.is_visible(now))
            .find(|h| distance_m(&h.geom, &draft.geom) <= DEDUPE_RADIUS_M)
            .map(|h| h.id.clone());

        if let Some(id) = duplicate {
            let merged = {
                let hazard = inner.hazards.get_mut(&id).expect("id from map");
                hazard.confirm(now);
                hazard.clone()
            };
            if let Some(key) = idempotency_key {
                inner.idempotency.insert(key.to_string(), id);
            }
            return CreateOutcome::Merged(merged);
        }

        let hazard = Hazard::new(draft, device_id.to_string(), now);
        let id = hazard.id.clone();
        inner.hazards.insert(id.clone(), hazard.clone());
        if let Some(key) = idempotency_key {
            inner.idempotency.insert(key.to_string(), id);
        }
        CreateOutcome::Created(hazard)
    }

    async fn get(&self, id: &str) -> Option<Hazard> {
        self.inner.read().unwrap().hazards.get(id).cloned()
    }

    async fn list(&self, query: &ListQuery, now: DateTime<Utc>) -> Vec<Hazard> {
        let inner = self.inner.read().unwrap();
        let limit = query.limit.clamp(1, 500);
        let mut result: Vec<Hazard> = inner
            .hazards
            .values()
            .filter(|h| match query.since_ms {
                Some(since) => h.updated_at.timestamp_millis() > since,
                None => h.is_visible(now),
            })
            .filter(|h| query.types.is_empty() || query.types.contains(&h.kind))
            .filter(|h| match query.bbox {
                Some((min_lon, min_lat, max_lon, max_lat)) => {
                    (min_lon..=max_lon).contains(&h.geom.lon())
                        && (min_lat..=max_lat).contains(&h.geom.lat())
                }
                None => true,
            })
            .cloned()
            .collect();
        result.sort_by_key(|h| (h.updated_at, h.id.clone()));
        result.truncate(limit);
        result
    }

    async fn vote(&self, id: &str, vote: Vote, now: DateTime<Utc>) -> Option<Hazard> {
        let mut inner = self.inner.write().unwrap();
        let hazard = inner.hazards.get_mut(id)?;
        match vote {
            Vote::Confirm => hazard.confirm(now),
            Vote::Deny => hazard.deny(now),
            Vote::Resolved => hazard.resolve(now),
        }
        Some(hazard.clone())
    }

    async fn purge_expired(&self, now: DateTime<Utc>) -> usize {
        let mut inner = self.inner.write().unwrap();
        let before = inner.hazards.len();
        inner.hazards.retain(|_, h| h.expires_at > now || h.deleted);
        before - inner.hazards.len()
    }

    async fn health(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Point;

    fn draft(lon: f64, lat: f64) -> HazardDraft {
        HazardDraft {
            kind: HazardType::Pothole,
            subtype: None,
            geom: Point::new(lon, lat),
            road: None,
            ttl_seconds: None,
        }
    }

    #[tokio::test]
    async fn dedupes_within_radius() {
        let store = MemoryStore::new();
        let now = Utc::now();
        let device = store.create_device("hash".into(), now).await;
        let first = store
            .create(draft(-46.6, -23.5), &device.id, None, now)
            .await;
        let second = store
            .create(draft(-46.6001, -23.5001), &device.id, None, now)
            .await;
        assert!(matches!(first, CreateOutcome::Created(_)));
        match second {
            CreateOutcome::Merged(h) => assert_eq!(h.confirmations, 1),
            CreateOutcome::Created(_) => panic!("expected merge"),
        }
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn idempotency_returns_same_hazard() {
        let store = MemoryStore::new();
        let now = Utc::now();
        let device = store.create_device("hash".into(), now).await;
        let a = store
            .create(draft(-46.6, -23.5), &device.id, Some("k1"), now)
            .await;
        let b = store
            .create(draft(-10.0, 10.0), &device.id, Some("k1"), now)
            .await;
        let id = |o: CreateOutcome| match o {
            CreateOutcome::Created(h) | CreateOutcome::Merged(h) => h.id,
        };
        assert_eq!(id(a), id(b));
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn bbox_filters() {
        let store = MemoryStore::new();
        let now = Utc::now();
        let device = store.create_device("hash".into(), now).await;
        store
            .create(draft(-46.6, -23.5), &device.id, None, now)
            .await;
        store
            .create(draft(-43.2, -22.9), &device.id, None, now)
            .await;
        let query = ListQuery {
            bbox: Some((-47.0, -24.0, -46.0, -23.0)),
            ..Default::default()
        };
        assert_eq!(store.list(&query, now).await.len(), 1);
    }

    #[tokio::test]
    async fn erasing_a_device_anonymizes_reports() {
        let store = MemoryStore::new();
        let now = Utc::now();
        let device = store.create_device("hash".into(), now).await;
        store
            .create(draft(-46.6, -23.5), &device.id, None, now)
            .await;
        assert_eq!(store.remove_device("hash").await, 1);
        assert_eq!(store.len(), 1);
        let hazard = store.list(&ListQuery::default(), now).await.remove(0);
        assert!(hazard.reported_by.is_none());
    }
}
