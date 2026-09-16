mod access;
mod auth;
mod config;
mod error;
mod gateway;
mod ids;
mod livekit;
mod models;
mod perms;
mod routes;
mod state;
mod validate;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "minichat_server=info,tower_http=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    tokio::fs::create_dir_all(&config.data_dir).await?;
    tokio::fs::create_dir_all(config.uploads_dir()).await?;

    let db = connect_database(&config).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    // A process restart means nobody is connected; clear any presence left
    // behind by an unclean shutdown.
    sqlx::query("UPDATE users SET presence = 'offline' WHERE presence != 'offline'")
        .execute(&db)
        .await?;

    let bind_addr = config.bind_addr.clone();
    let web_dir = config.web_dir.clone();
    let uploads_dir = config.uploads_dir();
    let livekit_ready = config.livekit_ready();
    let public_url = config.public_url.clone();
    let state = AppState::new(db, config);

    // Uploads are user-controlled bytes served from the instance's own origin,
    // so pin the sniffing rules down hard.
    let uploads = ServeDir::new(&uploads_dir).precompressed_gzip();
    let uploads = Router::new().fallback_service(uploads).layer((
        SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; sandbox; frame-ancestors 'none'"),
        ),
        SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ),
    ));

    // Unknown paths fall back to index.html so client-side routes such as
    // /invite/abc survive a refresh. `not_found_service` would preserve the
    // 404 status, so serve the shell from a handler that returns 200 instead.
    let index_html = tokio::fs::read_to_string(format!("{web_dir}/index.html"))
        .await
        .unwrap_or_else(|_| {
            tracing::warn!("no web build found in {web_dir} — serving the API only");
            "<!doctype html><title>MiniChat</title><p>Web build not found.".to_string()
        });
    let spa = ServeDir::new(&web_dir)
        .precompressed_gzip()
        .fallback(axum::routing::get(move || {
            let body = index_html.clone();
            async move {
                (
                    [
                        // The shell names content-hashed assets, so it must never
                        // be cached or a deploy would serve stale script tags.
                        (header::CACHE_CONTROL, "no-cache"),
                        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    ],
                    body,
                )
            }
        }));

    // Vite fingerprints everything under /assets, so it is safe to cache hard.
    let assets = Router::new()
        .fallback_service(ServeDir::new(format!("{web_dir}/assets")).precompressed_gzip())
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    let app = Router::new()
        .nest("/api", routes::api_router())
        .nest("/webhooks", routes::webhook_router())
        .nest_service("/uploads", uploads)
        .nest_service("/assets", assets)
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .fallback_service(spa)
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .layer(CorsLayer::very_permissive())
        .layer(TraceLayer::new_for_http())
        .method_not_allowed_fallback(|| async {
            (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed").into_response()
        })
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("minichat listening on {bind_addr}");
    tracing::info!("public url: {public_url}");
    if livekit_ready {
        tracing::info!("voice & video: enabled");
    } else {
        tracing::warn!(
            "voice & video: disabled (set LIVEKIT_URL, LIVEKIT_API_KEY and LIVEKIT_API_SECRET)"
        );
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn connect_database(config: &Config) -> Result<sqlx::SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(&config.database_url)?
        .create_if_missing(true)
        // WAL keeps readers from blocking the writer, which matters because
        // every gateway event fans out into reads.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10))
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
