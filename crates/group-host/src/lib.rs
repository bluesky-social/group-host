//! The group host: the PDS for every group it runs, the
//! authority for their spaces, their OAuth server, and the
//! `fyi.opensocial.*` methods that run a group. `docs/design/README.md`
//! says how it is built and what is still to come.

pub mod api;
pub mod config;
pub mod plc;
pub mod repo;
pub mod secrets;
pub mod store;
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
    pub store: store::Store,
    pub secrets: secrets::Secrets,
    pub plc: plc::Directory,
    /// Each commit's revision, later than the last.
    pub clock: atproto::tid::TidClock,
    /// Held while a commit is made, so each builds on the one before.
    writing: std::sync::Mutex<()>,
}

impl Host {
    /// The host `config` describes, its data directory made if need be.
    pub fn open(config: Config) -> anyhow::Result<Host> {
        std::fs::create_dir_all(&config.data_dir)?;
        let store = store::Store::open(&config.data_dir.join("host.sqlite"))?;
        let secrets = secrets::Secrets::load(&config.data_dir)?;
        let plc = plc::Directory::new(&config.plc_url);
        // A clock id of its own, so two hosts' revisions never collide.
        let clock = atproto::tid::TidClock::new(u64::from(secrets.signing.public().to_sec1()[1]));
        Ok(Host {
            config,
            store,
            secrets,
            plc,
            clock,
            writing: Default::default(),
        })
    }
}

/// The host's routes.
pub fn router(host: Arc<Host>) -> Router {
    Router::new()
        .route("/xrpc/_health", get(health))
        .route(
            "/xrpc/com.atproto.repo.describeRepo",
            get(api::describe_repo),
        )
        .route("/xrpc/com.atproto.repo.getRecord", get(api::get_record))
        .route("/xrpc/com.atproto.repo.listRecords", get(api::list_records))
        .route("/xrpc/com.atproto.sync.getRepo", get(api::get_repo))
        .route(
            "/xrpc/com.atproto.sync.getLatestCommit",
            get(api::get_latest_commit),
        )
        .route("/xrpc/com.atproto.sync.listRepos", get(api::list_repos))
        .route(
            "/xrpc/com.atproto.identity.resolveHandle",
            get(api::resolve_handle),
        )
        .route("/.well-known/did.json", get(api::did_json))
        .route("/.well-known/atproto-did", get(api::atproto_did))
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
    let app = router(Arc::new(Host::open(config)?));
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
