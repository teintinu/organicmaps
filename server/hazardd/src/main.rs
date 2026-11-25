use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080);
    let pepper = std::env::var("TOKEN_PEPPER").unwrap_or_else(|_| "dev-only-pepper".to_string());
    if std::env::var("TOKEN_PEPPER").is_err() {
        eprintln!("warning: TOKEN_PEPPER not set, using the development default");
    }

    let store: std::sync::Arc<dyn hazardd::store::HazardStore> = match std::env::var("DATABASE_URL")
    {
        Ok(url) => {
            let pg = hazardd::pg::PgStore::connect(&url)
                .await
                .unwrap_or_else(|e| panic!("cannot connect to Postgres: {e}"));
            eprintln!("storage: postgres");
            std::sync::Arc::new(pg)
        }
        Err(_) => {
            eprintln!("storage: memory (set DATABASE_URL for Postgres)");
            std::sync::Arc::new(hazardd::store::MemoryStore::new())
        }
    };
    let state = hazardd::AppState::with_store(store, pepper);

    // TTL sweeper: expired hazards stop being served immediately; this frees memory.
    let store = Arc::clone(&state.store);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            let removed = store.purge_expired(chrono::Utc::now()).await;
            if removed > 0 {
                eprintln!("purged {removed} expired hazards");
            }
        }
    });

    let app = hazardd::app(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("cannot bind port {port}: {e}"));
    eprintln!("hazardd listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("server error");
}
