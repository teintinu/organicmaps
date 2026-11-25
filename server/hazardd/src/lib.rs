//! Self-hostable road hazard service for Organic Maps.
//!
//! Storage is a trait: `store::MemoryStore` for tests/demos and `pg::PgStore`
//! for a real deployment (selected by `DATABASE_URL`).

pub mod api;
pub mod domain;
pub mod pg;
pub mod store;

pub use api::{app, AppState};
