-- v1 schema. No PostGIS yet: a btree on (lon, lat) plus a Rust-side haversine
-- check is enough at community scale and keeps the deployment simple.

CREATE TABLE device (
    id          uuid PRIMARY KEY,
    token_hash  text NOT NULL UNIQUE,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL,
    deleted_at  timestamptz
);

CREATE TABLE hazard (
    id            uuid PRIMARY KEY,
    type          text NOT NULL,
    subtype       text,
    lon           double precision NOT NULL,
    lat           double precision NOT NULL,
    status        text NOT NULL,
    confidence    real NOT NULL,
    confirmations integer NOT NULL DEFAULT 0,
    denials       integer NOT NULL DEFAULT 0,
    source        text NOT NULL,
    license       text NOT NULL,
    reported_at   timestamptz NOT NULL,
    expires_at    timestamptz NOT NULL,
    updated_at    timestamptz NOT NULL,
    deleted       boolean NOT NULL DEFAULT false,
    reported_by   uuid REFERENCES device(id) ON DELETE SET NULL
);

CREATE INDEX hazard_bbox_idx ON hazard (lon, lat);
CREATE INDEX hazard_cursor_idx ON hazard (updated_at, id);
CREATE INDEX hazard_expires_idx ON hazard (expires_at);

CREATE TABLE idempotency (
    key        text PRIMARY KEY,
    hazard_id  uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
