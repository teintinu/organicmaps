//! Postgres round-trip, only when TEST_DATABASE_URL points at a scratch database.
//!
//!   docker run -d --rm --name hazardd-pg -e POSTGRES_PASSWORD=hazard \
//!     -e POSTGRES_DB=hazard -p 15432:5432 postgres:16-alpine
//!   TEST_DATABASE_URL=postgres://postgres:hazard@127.0.0.1:15432/hazard \
//!     cargo test --test postgres -- --ignored

use chrono::Utc;
use hazardd::domain::{HazardDraft, HazardType, Point};
use hazardd::pg::PgStore;
use hazardd::store::{HazardStore, ListQuery, Vote};

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
#[ignore = "needs TEST_DATABASE_URL"]
async fn postgres_roundtrip() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let store = PgStore::connect(&url).await.expect("connect");

    let now = Utc::now();
    let hash = format!("test-{}", uuid::Uuid::new_v4());
    let device = store.create_device(hash.clone(), now).await;
    assert!(store.device("missing", now).await.is_none());

    let created = store
        .create(
            draft(-46.6333, -23.5505),
            &device.id,
            Some("pg-test-1"),
            now,
        )
        .await;
    let hazard = match created {
        hazardd::store::CreateOutcome::Created(h) => h,
        hazardd::store::CreateOutcome::Merged(_) => panic!("first insert must create"),
    };

    // Idempotency replay.
    let replay = store
        .create(draft(-10.0, 10.0), &device.id, Some("pg-test-1"), now)
        .await;
    match replay {
        hazardd::store::CreateOutcome::Merged(h) => assert_eq!(h.id, hazard.id),
        hazardd::store::CreateOutcome::Created(_) => panic!("replay must not create"),
    }

    // Dedupe within 100 m.
    let merged = store
        .create(draft(-46.6334, -23.5506), &device.id, None, now)
        .await;
    match merged {
        hazardd::store::CreateOutcome::Merged(h) => assert_eq!(h.confirmations, 1),
        hazardd::store::CreateOutcome::Created(_) => panic!("expected dedupe"),
    }

    let q = ListQuery {
        bbox: Some((-46.7, -23.6, -46.5, -23.4)),
        ..Default::default()
    };
    assert_eq!(store.list(&q, now).await.len(), 1);

    let voted = store
        .vote(&hazard.id, Vote::Confirm, now)
        .await
        .expect("vote");
    assert_eq!(voted.confirmations, 2);

    assert_eq!(store.remove_device(&hash).await, 1);
    assert!(store.health().await);
}
