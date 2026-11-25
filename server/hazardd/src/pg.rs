//! Postgres backend. Same `HazardStore` contract as the in-memory store.
//!
//! No PostGIS yet: a bbox prefilter on (lon, lat) plus a Rust-side haversine
//! check keeps dedupe honest without a spatial extension.

use crate::domain::{distance_m, Hazard, HazardDraft, Point, Source, Status};
use crate::store::{CreateOutcome, Device, HazardStore, ListQuery, Vote};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::Row;
use uuid::Uuid;

const DEDUPE_RADIUS_M: f64 = 100.0;
const DEDUPE_BOX_DEG: f64 = 0.01;
const DEVICE_TTL: Duration = Duration::days(90);

pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn hazard_from_row(row: &PgRow) -> Hazard {
    let id: Uuid = row.get("id");
    let kind: String = row.get("type");
    let status: String = row.get("status");
    let source: String = row.get("source");
    let lon: f64 = row.get("lon");
    let lat: f64 = row.get("lat");
    let reported_by: Option<Uuid> = row.get("reported_by");

    Hazard {
        id: id.to_string(),
        kind: crate::domain::HazardType::parse(&kind).unwrap_or(crate::domain::HazardType::Hazard),
        subtype: row.get("subtype"),
        geom: Point::new(lon, lat),
        road: None,
        reported_at: row.get("reported_at"),
        expires_at: row.get("expires_at"),
        status: Status::parse(&status).unwrap_or(Status::Active),
        confidence: row.get("confidence"),
        confirmations: row.get::<i32, _>("confirmations").max(0) as u32,
        denials: row.get::<i32, _>("denials").max(0) as u32,
        source: Source::parse(&source).unwrap_or(Source::User),
        license: row.get("license"),
        deleted: row.get("deleted"),
        reported_by: reported_by.map(|id| id.to_string()),
        updated_at: row.get("updated_at"),
    }
}

async fn fetch_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Hazard>, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM hazard WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(hazard_from_row))
}

#[async_trait]
impl HazardStore for PgStore {
    async fn create_device(&self, token_hash: String, now: DateTime<Utc>) -> Device {
        let id = Uuid::now_v7();
        let expires_at = now + DEVICE_TTL;
        sqlx::query("INSERT INTO device (id, token_hash, expires_at) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(&token_hash)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .expect("insert device");
        Device {
            id: id.to_string(),
            expires_at,
        }
    }

    async fn device(&self, token_hash: &str, now: DateTime<Utc>) -> Option<Device> {
        let row = sqlx::query(
            "SELECT id, expires_at FROM device WHERE token_hash = $1 AND expires_at > $2",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .ok()??;
        Some(Device {
            id: row.get::<Uuid, _>("id").to_string(),
            expires_at: row.get("expires_at"),
        })
    }

    async fn remove_device(&self, token_hash: &str) -> usize {
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(_) => return 0,
        };
        let device: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM device WHERE token_hash = $1")
                .bind(token_hash)
                .fetch_optional(&mut *tx)
                .await
                .ok()
                .flatten();
        let Some(device_id) = device else { return 0 };

        let touched = sqlx::query("UPDATE hazard SET reported_by = NULL WHERE reported_by = $1")
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map(|r| r.rows_affected() as usize)
            .unwrap_or(0);
        let _ = sqlx::query("DELETE FROM device WHERE id = $1")
            .bind(device_id)
            .execute(&mut *tx)
            .await;
        let _ = tx.commit().await;
        touched
    }

    async fn create(
        &self,
        draft: HazardDraft,
        device_id: &str,
        idempotency_key: Option<&str>,
        now: DateTime<Utc>,
    ) -> CreateOutcome {
        let mut tx = self.pool.begin().await.expect("tx");

        if let Some(key) = idempotency_key {
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT hazard_id FROM idempotency WHERE key = $1")
                    .bind(key)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()
                    .flatten();
            if let Some(id) = existing {
                if let Some(hazard) = sqlx::query("SELECT * FROM hazard WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await
                    .ok()
                    .flatten()
                    .as_ref()
                    .map(hazard_from_row)
                {
                    return CreateOutcome::Merged(hazard);
                }
            }
        }

        let candidates = sqlx::query(
            "SELECT * FROM hazard
             WHERE \"type\" = $1 AND deleted = false AND status = 'active' AND expires_at > now()
               AND lon BETWEEN $2 AND $3 AND lat BETWEEN $4 AND $5",
        )
        .bind(draft.kind.as_str())
        .bind(draft.geom.lon() - DEDUPE_BOX_DEG)
        .bind(draft.geom.lon() + DEDUPE_BOX_DEG)
        .bind(draft.geom.lat() - DEDUPE_BOX_DEG)
        .bind(draft.geom.lat() + DEDUPE_BOX_DEG)
        .fetch_all(&mut *tx)
        .await
        .expect("dedupe query");

        if let Some(row) = candidates
            .iter()
            .map(hazard_from_row)
            .find(|h| distance_m(&h.geom, &draft.geom) <= DEDUPE_RADIUS_M)
        {
            let updated = sqlx::query(
                "UPDATE hazard SET confirmations = confirmations + 1,
                    confidence = LEAST(1.0, confidence + 0.1), updated_at = $2
                 WHERE id = $1 RETURNING *",
            )
            .bind(Uuid::parse_str(&row.id).expect("uuid"))
            .bind(now)
            .fetch_one(&mut *tx)
            .await
            .expect("confirm")
            .pipe(|row| hazard_from_row(&row));

            if let Some(key) = idempotency_key {
                let _ = sqlx::query(
                    "INSERT INTO idempotency (key, hazard_id) VALUES ($1, $2)
                     ON CONFLICT (key) DO NOTHING",
                )
                .bind(key)
                .bind(Uuid::parse_str(&updated.id).expect("uuid"))
                .execute(&mut *tx)
                .await;
            }
            let _ = tx.commit().await;
            return CreateOutcome::Merged(updated);
        }

        let id = Uuid::now_v7();
        let reported_by = Uuid::parse_str(device_id).ok();
        let row = sqlx::query(
            "INSERT INTO hazard
               (id, \"type\", subtype, lon, lat, status, confidence, confirmations, denials,
                source, license, reported_at, expires_at, updated_at, deleted, reported_by)
             VALUES ($1,$2,$3,$4,$5,'active',0.6,0,0,'user','ODbL-1.0',$6,$7,$6,false,$8)
             RETURNING *",
        )
        .bind(id)
        .bind(draft.kind.as_str())
        .bind(&draft.subtype)
        .bind(draft.geom.lon())
        .bind(draft.geom.lat())
        .bind(now)
        .bind(now + draft.kind.ttl())
        .bind(reported_by)
        .fetch_one(&mut *tx)
        .await
        .expect("insert hazard");

        if let Some(key) = idempotency_key {
            let _ = sqlx::query("INSERT INTO idempotency (key, hazard_id) VALUES ($1, $2)")
                .bind(key)
                .bind(id)
                .execute(&mut *tx)
                .await;
        }
        let _ = tx.commit().await;
        CreateOutcome::Created(hazard_from_row(&row))
    }

    async fn get(&self, id: &str) -> Option<Hazard> {
        let uuid = Uuid::parse_str(id).ok()?;
        fetch_by_id(&self.pool, uuid).await.ok().flatten()
    }

    async fn list(&self, query: &ListQuery, now: DateTime<Utc>) -> Vec<Hazard> {
        let (min_lon, min_lat, max_lon, max_lat) = match query.bbox {
            Some(bbox) => (Some(bbox.0), Some(bbox.1), Some(bbox.2), Some(bbox.3)),
            None => (None, None, None, None),
        };
        let types: Option<Vec<String>> = if query.types.is_empty() {
            None
        } else {
            Some(query.types.iter().map(|t| t.as_str().to_string()).collect())
        };
        let limit = query.limit.clamp(1, 500) as i64;

        let rows = sqlx::query(
            "SELECT * FROM hazard
             WHERE ($1::float8 IS NULL OR (lon >= $1 AND lon <= $3 AND lat >= $2 AND lat <= $4))
               AND ($5::bigint IS NULL OR updated_at > to_timestamp($5::bigint / 1000.0))
               AND ($5::bigint IS NOT NULL OR (deleted = false AND status = 'active' AND expires_at > $6))
               AND ($7::text[] IS NULL OR \"type\" = ANY($7))
             ORDER BY updated_at, id
             LIMIT $8",
        )
        .bind(min_lon)
        .bind(min_lat)
        .bind(max_lon)
        .bind(max_lat)
        .bind(query.since_ms)
        .bind(now)
        .bind(types)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .expect("list query");

        rows.iter().map(hazard_from_row).collect()
    }

    async fn vote(&self, id: &str, vote: Vote, now: DateTime<Utc>) -> Option<Hazard> {
        let uuid = Uuid::parse_str(id).ok()?;
        let current = fetch_by_id(&self.pool, uuid).await.ok().flatten()?;
        let (confirmations, denials, confidence, status, deleted) = match vote {
            Vote::Confirm => (
                current.confirmations + 1,
                current.denials,
                (current.confidence + 0.1).min(1.0),
                current.status,
                current.deleted,
            ),
            Vote::Deny => {
                let confidence = (current.confidence - 0.4).max(0.0);
                let denials = current.denials + 1;
                let hide = confidence < 0.25 && denials >= 3;
                (
                    current.confirmations,
                    denials,
                    confidence,
                    if hide { Status::Hidden } else { current.status },
                    current.deleted || hide,
                )
            }
            Vote::Resolved => (
                current.confirmations,
                current.denials,
                current.confidence,
                Status::Hidden,
                true,
            ),
        };

        let row = sqlx::query(
            "UPDATE hazard SET confirmations=$2, denials=$3, confidence=$4, status=$5,
                deleted=$6, updated_at=$7 WHERE id=$1 RETURNING *",
        )
        .bind(uuid)
        .bind(confirmations as i32)
        .bind(denials as i32)
        .bind(confidence)
        .bind(status.as_str())
        .bind(deleted)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .ok()??;
        Some(hazard_from_row(&row))
    }

    async fn purge_expired(&self, now: DateTime<Utc>) -> usize {
        let deleted = sqlx::query(
            "DELETE FROM hazard
             WHERE (deleted = true AND updated_at < $1 - interval '7 days')
                OR (deleted = false AND expires_at < $1 - interval '1 day')",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map(|r| r.rows_affected() as usize)
        .unwrap_or(0);
        let _ = sqlx::query("DELETE FROM idempotency WHERE created_at < $1 - interval '1 day'")
            .bind(now)
            .execute(&self.pool)
            .await;
        deleted
    }

    async fn health(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }
}

/// Tiny pipe helper so query chains stay readable.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}
