# hazardd

Self-hostable road-hazard service for Organic Maps, implementing
[`plans/ev/api/hazard-api.v1.yaml`](../../../plans/ev/api/hazard-api.v1.yaml).

Status: **v1 deployable** — full HTTP contract, Postgres persistence (migrations run at startup),
in-memory backend for tests/demos, per-token rate limits, batch flush for the offline queue.

## Run locally

```bash
# memory backend (no database)
TOKEN_PEPPER=dev cargo run

# Postgres backend
export DATABASE_URL=postgres://hazard:hazard@127.0.0.1:5432/hazard
TOKEN_PEPPER=dev cargo run
```

Migrations in `migrations/` are embedded and applied on startup.

## Deploy

```bash
POSTGRES_PASSWORD=$(openssl rand -hex 16) \
TOKEN_PEPPER=$(openssl rand -hex 32) \
  docker compose up -d --build

curl -s localhost:8080/v1/meta
```

Put Caddy/nginx in front for TLS:

```caddyfile
gps.teintinu.com {
    encode zstd gzip
    reverse_proxy 127.0.0.1:8080
}
```

`compose.yaml` publishes hazardd only on `127.0.0.1`, keeps Postgres on a named volume, and waits for
`pg_isready` before starting the app.

## Endpoints

| Method | Path | Notes |
|---|---|---|
| GET | `/health` | liveness + database check (`degraded` → 503) |
| GET | `/v1/meta` | instance, licence, coverage, capabilities, TTLs |
| POST | `/v1/devices` | anonymous token, 90 days |
| DELETE | `/v1/me` | erases the device; its reports become anonymous |
| GET | `/v1/hazards?bbox=&since=&types=&limit=` | GeoJSON + `nextSince` cursor |
| POST | `/v1/hazards` | Bearer + `Idempotency-Key`; 201 new, 409 merged |
| POST | `/v1/hazards:batch` | ≤ 50 reports, per-item `created`/`merged`/`rejected` |
| GET | `/v1/hazards/{id}` | |
| POST | `/v1/hazards/{id}/votes` | `confirm` / `deny` / `resolved` |
| POST | `/v1/hazards/{id}/abuse` | 204 |

Policy: TTL per type (server-enforced), dedupe within 100 m, `police` absent (decision D8), bbox
limited to 0.5°×0.5°, 10 reports/hour and 60 votes/hour per token (fixed window, per process).

## Verify

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test          # 15 tests: 8 unit + 7 HTTP contract

# end-to-end against Postgres
docker run -d --rm --name hazardd-pg -e POSTGRES_PASSWORD=hazard -e POSTGRES_DB=hazard \
  -p 15432:5432 postgres:16-alpine
DATABASE_URL=postgres://postgres:hazard@127.0.0.1:15432/hazard PORT=18081 TOKEN_PEPPER=t cargo run
curl -s localhost:18081/health
```

## Configuration

| Variable | Required | Meaning |
|---|---|---|
| `DATABASE_URL` | for persistence | Postgres URL; omit to use the in-memory store |
| `TOKEN_PEPPER` | in production | secret mixed into device-token hashes; rotating invalidates tokens |
| `PORT` | no (8080) | listen port |

## Known gaps (next milestone)

* Shared rate limiting across replicas (Redis) — today it is per process.
* `410 Gone` for cursors older than the tombstone window (currently the client just gets a delta).
* PostGIS-backed dedupe at larger scale (today: bbox prefilter + haversine in Rust).

## Licence

Apache-2.0 (decision D11).
