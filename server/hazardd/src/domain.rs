//! Domain types and policy: hazard kinds, TTL, confidence, distance.
//!
//! No I/O here, so the rules are unit-testable without a server.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Hazard kinds accepted by API v1.
///
/// `police` is deliberately absent (decision D8): highest abuse rate and
/// legal exposure in several countries, with no navigation value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HazardType {
    Accident,
    Hazard,
    Pothole,
    RoadClosure,
    Construction,
    StoppedVehicle,
    Flooding,
    Debris,
    Animal,
}

impl HazardType {
    pub fn as_str(self) -> &'static str {
        match self {
            HazardType::Accident => "accident",
            HazardType::Hazard => "hazard",
            HazardType::Pothole => "pothole",
            HazardType::RoadClosure => "road_closure",
            HazardType::Construction => "construction",
            HazardType::StoppedVehicle => "stopped_vehicle",
            HazardType::Flooding => "flooding",
            HazardType::Debris => "debris",
            HazardType::Animal => "animal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "accident" => HazardType::Accident,
            "hazard" => HazardType::Hazard,
            "pothole" => HazardType::Pothole,
            "road_closure" => HazardType::RoadClosure,
            "construction" => HazardType::Construction,
            "stopped_vehicle" => HazardType::StoppedVehicle,
            "flooding" => HazardType::Flooding,
            "debris" => HazardType::Debris,
            "animal" => HazardType::Animal,
            _ => return None,
        })
    }

    /// Server-enforced lifetime. The client may hint, never decide.
    pub fn ttl(self) -> Duration {
        match self {
            HazardType::Accident => Duration::minutes(60),
            HazardType::Hazard => Duration::hours(6),
            HazardType::Pothole => Duration::days(90),
            HazardType::RoadClosure => Duration::days(7),
            HazardType::Construction => Duration::days(7),
            HazardType::StoppedVehicle => Duration::minutes(30),
            HazardType::Flooding => Duration::hours(12),
            HazardType::Debris => Duration::hours(3),
            HazardType::Animal => Duration::hours(3),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    UnderReview,
    Hidden,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::UnderReview => "under_review",
            Status::Hidden => "hidden",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "active" => Status::Active,
            "under_review" => Status::UnderReview,
            "hidden" => Status::Hidden,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    User,
    Instance,
    Partner,
    Vendor,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::User => "user",
            Source::Instance => "instance",
            Source::Partner => "partner",
            Source::Vendor => "vendor",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "user" => Source::User,
            "instance" => Source::Instance,
            "partner" => Source::Partner,
            "vendor" => Source::Vendor,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    #[serde(rename = "type")]
    pub kind: String,
    pub coordinates: [f64; 2],
}

impl Point {
    pub fn new(lon: f64, lat: f64) -> Self {
        Self {
            kind: "Point".into(),
            coordinates: [lon, lat],
        }
    }

    pub fn lon(&self) -> f64 {
        self.coordinates[0]
    }

    pub fn lat(&self) -> f64 {
        self.coordinates[1]
    }

    pub fn is_valid(&self) -> bool {
        (-180.0..=180.0).contains(&self.lon()) && (-90.0..=90.0).contains(&self.lat())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Road {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osm_way_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearing: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HazardDraft {
    #[serde(rename = "type")]
    pub kind: HazardType,
    #[serde(default)]
    pub subtype: Option<String>,
    pub geom: Point,
    #[serde(default)]
    pub road: Option<Road>,
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hazard {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: HazardType,
    pub subtype: Option<String>,
    pub geom: Point,
    pub road: Option<Road>,
    pub reported_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: Status,
    pub confidence: f32,
    pub confirmations: u32,
    pub denials: u32,
    pub source: Source,
    pub license: String,
    pub deleted: bool,
    #[serde(skip)]
    pub reported_by: Option<String>,
    #[serde(skip)]
    pub updated_at: DateTime<Utc>,
}

impl Hazard {
    pub fn new(draft: HazardDraft, reported_by: String, now: DateTime<Utc>) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            kind: draft.kind,
            subtype: draft.subtype,
            expires_at: now + draft.kind.ttl(),
            geom: draft.geom,
            road: draft.road,
            reported_at: now,
            status: Status::Active,
            confidence: 0.6,
            confirmations: 0,
            denials: 0,
            source: Source::User,
            license: "ODbL-1.0".into(),
            deleted: false,
            reported_by: Some(reported_by),
            updated_at: now,
        }
    }

    pub fn confirm(&mut self, now: DateTime<Utc>) {
        self.confirmations += 1;
        self.confidence = (self.confidence + 0.1).min(1.0);
        self.updated_at = now;
    }

    pub fn deny(&mut self, now: DateTime<Utc>) {
        self.denials += 1;
        self.confidence = (self.confidence - 0.4).max(0.0);
        if self.confidence < 0.25 && self.denials >= 3 {
            self.status = Status::Hidden;
            self.deleted = true;
        }
        self.updated_at = now;
    }

    pub fn resolve(&mut self, now: DateTime<Utc>) {
        self.status = Status::Hidden;
        self.deleted = true;
        self.updated_at = now;
    }

    pub fn is_visible(&self, now: DateTime<Utc>) -> bool {
        !self.deleted && self.status == Status::Active && self.expires_at > now
    }
}

/// Great-circle distance in metres.
pub fn distance_m(a: &Point, b: &Point) -> f64 {
    let (lat1, lat2) = (a.lat().to_radians(), b.lat().to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon() - a.lon()).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lon: f64, lat: f64) -> Point {
        Point::new(lon, lat)
    }

    #[test]
    fn ttl_is_per_kind() {
        assert!(HazardType::Pothole.ttl() > HazardType::Accident.ttl());
        assert_eq!(HazardType::Accident.ttl(), Duration::minutes(60));
    }

    #[test]
    fn distance_is_sane() {
        // ~111 km per degree of latitude.
        let d = distance_m(&at(0.0, 0.0), &at(0.0, 1.0));
        assert!((110_000.0..112_000.0).contains(&d), "got {d}");
    }

    #[test]
    fn denials_hide_a_hazard() {
        let draft = HazardDraft {
            kind: HazardType::Hazard,
            subtype: None,
            geom: at(-46.6, -23.5),
            road: None,
            ttl_seconds: None,
        };
        let now = Utc::now();
        let mut hazard = Hazard::new(draft, "device".into(), now);
        for _ in 0..3 {
            hazard.deny(now);
        }
        assert!(hazard.deleted);
        assert_eq!(hazard.status, Status::Hidden);
    }

    #[test]
    fn rejects_out_of_range_coordinates() {
        assert!(at(-46.6, -23.5).is_valid());
        assert!(!at(200.0, 0.0).is_valid());
    }
}
