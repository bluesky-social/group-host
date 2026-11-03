//! The opensocial group host: the PDS for every group it runs, the
//! authority for their spaces, their OAuth server, and the
//! `fyi.opensocial.*` methods that run a group. `docs/design/README.md`
//! says how it is built and what is still to come from the TypeScript host.

pub mod config;
pub mod xrpc;

use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use config::Config;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use xrpc::XrpcError;

/// What every request is answered with.
pub struct Host {
    pub config: Config,
}

/// The host's routes.
pub fn router(host: Arc<Host>) -> Router {
    Router::new()
        .route("/xrpc/_health", get(health))
        .route("/xrpc/{nsid}", get(not_implemented).post(not_implemented))
        .with_state(host)
}

/// That the host is up, and which build it is.
async fn health() -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION") }))
}

async fn not_implemented(Path(nsid): Path<String>) -> XrpcError {
    XrpcError::not_implemented(&nsid)
}

/// Serves `config` until the process is told to stop (SIGTERM, or ctrl-c),
/// saying where once it listens: `listening on <address>`. Requests under
/// way when it is told are answered first.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
    println!("listening on {}", listener.local_addr()?);
    tracing::info!(public_url = %config.public_url, did = %config.service_did, "serving");
    let app = router(Arc::new(Host { config }));
    axum::serve(listener, app)
        .with_graceful_shutdown(stopped())
        .await?;
    tracing::info!("stopped");
    Ok(())
}

/// Once the process is told to stop.
async fn stopped() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM can be listened for");
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
