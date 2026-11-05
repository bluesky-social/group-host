//! The group host: the PDS for every group it runs, the
//! authority for their spaces, their OAuth server, and the
//! `fyi.opensocial.*` methods that run a group. `docs/design/README.md`
//! says how it is built and what is still to come.

pub mod account_move;
pub mod actors;
pub mod api;
pub mod app_passwords;
pub mod authority;
pub mod authority_api;
pub mod authorization;
pub mod blobs;
pub mod config;
pub mod console_login;
pub mod firehose;
pub mod group;
pub mod group_api;
pub mod group_config;
pub mod group_handle;
pub mod group_invites;
pub mod group_join;
pub mod group_lifecycle;
pub mod group_members;
pub mod group_spaces;
pub mod group_tokens;
pub mod identity;
pub mod import;
pub mod import_space;
pub mod index;
pub mod index_api;
pub mod lexicons;
pub mod lifecycle;
pub mod mcp;
pub mod member_writes;
pub mod migration;
pub mod moderation;
pub mod move_host;
pub mod oauth;
pub mod oauth_clients;
pub mod oauth_dpop;
pub mod oauth_resource;
pub mod oauth_scope;
pub mod oauth_store;
pub mod plc;
pub mod provision;
pub mod rbac;
pub mod recovery;
pub mod repo;
pub mod scopes;
pub mod secrets;
pub mod sites;
pub mod space;
pub mod space_api;
pub mod store;
pub mod time;
pub mod token_writes;
pub mod xrpc;

use axum::extract::Path;
use axum::routing::{get, post};
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
    /// Other accounts' documents.
    pub identity: identity::Resolver,
    /// Each commit's revision, later than the last.
    pub clock: atproto::tid::TidClock,
    /// Held while a commit is made, so each builds on the one before.
    writing: std::sync::Mutex<()>,
    /// Told of each new event, which wakes the firehose's streams.
    pub events: tokio::sync::broadcast::Sender<()>,
    /// When the relays were last told of news.
    crawled: std::sync::Mutex<Option<std::time::Instant>>,
    pub(crate) http: reqwest::Client,
    /// The spaces the index is to read, and its wake.
    indexing: std::sync::Mutex<std::collections::BTreeSet<String>>,
    index_wake: tokio::sync::Notify,
    /// The OAuth clients' metadata, as read.
    pub(crate) clients: oauth_clients::Clients,
}

impl Host {
    /// The host `config` describes, its data directory made if need be.
    pub fn open(config: Config) -> anyhow::Result<Host> {
        std::fs::create_dir_all(&config.data_dir)?;
        let store = store::Store::open(&config.data_dir.join("host.sqlite"))?;
        let secrets = secrets::Secrets::load(&config.data_dir)?;
        let plc = plc::Directory::new(&config.plc_url);
        let identity = identity::Resolver::new(&config.plc_url, &config.did_web_scheme);
        // A clock id of its own, so two hosts' revisions never collide.
        let clock = atproto::tid::TidClock::new(u64::from(secrets.signing.public().to_sec1()[1]));
        Ok(Host {
            config,
            store,
            secrets,
            plc,
            identity,
            clock,
            writing: Default::default(),
            events: tokio::sync::broadcast::channel(16).0,
            crawled: Default::default(),
            http: client(),
            indexing: Default::default(),
            index_wake: Default::default(),
            clients: Default::default(),
        })
    }
}

/// How long the host waits on another server before giving up on it: a
/// server that takes a request and never answers holds nothing up longer.
pub const OUTGOING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// A client for the host's requests to other servers.
pub fn client() -> reqwest::Client {
    let built = reqwest::Client::builder().timeout(OUTGOING_TIMEOUT).build();
    built.expect("a client with a timeout builds")
}

/// The host's routes.
pub fn router(host: Arc<Host>) -> Router {
    // A site is published whole, its files in one body: as large as one
    // upload may be, as the lexicons' check reads it.
    let site_limit = usize::try_from(host.config.blob_upload_limit).unwrap_or(usize::MAX);
    Router::new()
        .route("/xrpc/_health", get(health))
        .route(
            "/xrpc/com.atproto.repo.describeRepo",
            get(api::describe_repo),
        )
        .route(
            "/xrpc/com.atproto.server.describeServer",
            get(api::describe_server),
        )
        .route("/xrpc/fyi.opensocial.checkAddress", get(api::check_address))
        .route("/xrpc/com.atproto.repo.getRecord", get(api::get_record))
        .route("/xrpc/com.atproto.repo.listRecords", get(api::list_records))
        .route("/xrpc/com.atproto.sync.getRepo", get(api::get_repo))
        .route(
            "/xrpc/com.atproto.sync.getLatestCommit",
            get(api::get_latest_commit),
        )
        .route("/xrpc/com.atproto.sync.listRepos", get(api::list_repos))
        .route("/xrpc/com.atproto.sync.getBlob", get(api::get_blob))
        .route("/xrpc/com.atproto.sync.listBlobs", get(api::list_blobs))
        .route(
            "/xrpc/com.atproto.sync.subscribeRepos",
            get(firehose::subscribe_repos),
        )
        .route(
            "/xrpc/com.atproto.identity.resolveHandle",
            get(api::resolve_handle),
        )
        .route(
            "/xrpc/com.atproto.space.getRecord",
            get(space_api::get_record),
        )
        .route(
            "/xrpc/com.atproto.space.listRecords",
            get(space_api::list_records),
        )
        .route("/xrpc/com.atproto.space.getRepo", get(space_api::get_repo))
        .route(
            "/xrpc/com.atproto.space.getLatestCommit",
            get(space_api::get_latest_commit),
        )
        .route(
            "/xrpc/com.atproto.space.listRepoOps",
            get(space_api::list_repo_ops),
        )
        .route("/xrpc/com.atproto.space.getBlob", get(space_api::get_blob))
        .route(
            "/xrpc/com.atproto.space.listBlobs",
            get(space_api::list_blobs),
        )
        .route(
            "/xrpc/com.atproto.space.notifyCredentialRevoked",
            post(space_api::notify_credential_revoked),
        )
        .route(
            "/xrpc/com.atproto.space.getSpaceCredential",
            post(authority_api::get_space_credential),
        )
        .route(
            "/xrpc/com.atproto.space.notifyWrite",
            post(authority_api::notify_write),
        )
        .route(
            "/xrpc/com.atproto.space.listRepos",
            get(authority_api::list_repos),
        )
        .route(
            "/xrpc/com.atproto.space.registerNotify",
            post(authority_api::register_notify),
        )
        .route(
            "/xrpc/com.atproto.space.unregisterNotify",
            post(authority_api::unregister_notify),
        )
        .route(
            "/xrpc/fyi.opensocial.createGroup",
            post(group_api::create_group),
        )
        .route("/xrpc/fyi.opensocial.getGroup", get(group_api::get_group))
        .route(
            "/xrpc/fyi.opensocial.listMembers",
            get(group_api::list_members),
        )
        .route(
            "/xrpc/fyi.opensocial.deactivateGroup",
            post(group_lifecycle::deactivate_group),
        )
        .route(
            "/xrpc/fyi.opensocial.activateGroup",
            post(group_lifecycle::activate_group),
        )
        .route(
            "/xrpc/fyi.opensocial.deleteGroup",
            post(group_lifecycle::delete_group),
        )
        .route(
            "/xrpc/fyi.opensocial.checkGroupHandle",
            get(group_handle::check_group_handle),
        )
        .route(
            "/xrpc/fyi.opensocial.updateGroupHandle",
            post(group_handle::update_group_handle),
        )
        .route(
            "/xrpc/fyi.opensocial.createSpace",
            post(group_spaces::create_space),
        )
        .route(
            "/xrpc/fyi.opensocial.updateSpace",
            post(group_spaces::update_space),
        )
        .route(
            "/xrpc/fyi.opensocial.deleteSpace",
            post(group_spaces::delete_space),
        )
        .route(
            "/xrpc/fyi.opensocial.listSpaceRecords",
            get(index_api::list_space_records),
        )
        .route(
            "/xrpc/fyi.opensocial.getSpaceBlob",
            get(index_api::get_space_blob),
        )
        .route(
            "/xrpc/fyi.opensocial.leaveGroup",
            post(group_members::leave_group),
        )
        .route(
            "/xrpc/fyi.opensocial.requestJoin",
            post(group_join::request_join),
        )
        .route(
            "/xrpc/fyi.opensocial.getJoinRequest",
            get(group_join::get_join_request),
        )
        .route(
            "/xrpc/fyi.opensocial.listJoinRequests",
            get(group_join::list_join_requests),
        )
        .route(
            "/xrpc/fyi.opensocial.denyJoinRequest",
            post(group_join::deny_join_request),
        )
        .route(
            "/xrpc/fyi.opensocial.cancelJoinRequest",
            post(group_join::cancel_join_request),
        )
        .route(
            "/xrpc/fyi.opensocial.listInvites",
            get(group_invites::list_invites),
        )
        .route(
            "/xrpc/fyi.opensocial.getActorInvites",
            get(group_invites::get_actor_invites),
        )
        .route(
            "/xrpc/fyi.opensocial.declineInvite",
            post(group_invites::decline_invite),
        )
        .route(
            "/xrpc/com.atproto.managedspace.checkUserAccess",
            get(group_invites::check_user_access),
        )
        .route(
            "/xrpc/fyi.opensocial.getAuthorization",
            get(authorization::get_authorization),
        )
        .route(
            "/xrpc/fyi.opensocial.approveAuthorization",
            post(authorization::approve_authorization),
        )
        .route(
            "/xrpc/fyi.opensocial.declineAuthorization",
            post(authorization::decline_authorization),
        )
        .route(
            "/xrpc/fyi.opensocial.createAppPassword",
            post(app_passwords::create_app_password),
        )
        .route(
            "/xrpc/fyi.opensocial.listAppPasswords",
            get(app_passwords::list_app_passwords),
        )
        .route(
            "/xrpc/fyi.opensocial.revokeAppPassword",
            post(app_passwords::revoke_app_password),
        )
        .route(
            "/xrpc/fyi.opensocial.listGroupTokens",
            get(group_tokens::list_group_tokens),
        )
        .route(
            "/xrpc/fyi.opensocial.revokeGroupToken",
            post(group_tokens::revoke_group_token),
        )
        .route("/xrpc/fyi.opensocial.getViewer", get(actors::get_viewer))
        .route("/xrpc/fyi.opensocial.getActor", get(actors::get_actor))
        .route(
            "/xrpc/fyi.opensocial.getActorGroups",
            get(actors::get_actor_groups),
        )
        .route(
            "/xrpc/fyi.opensocial.applyLabel",
            post(moderation::apply_label),
        )
        .route(
            "/xrpc/fyi.opensocial.negateLabel",
            post(moderation::negate_label),
        )
        .route(
            "/xrpc/fyi.opensocial.listSubjects",
            get(moderation::list_subjects),
        )
        .route(
            "/xrpc/fyi.opensocial.getSubjectHistory",
            get(moderation::get_subject_history),
        )
        .route(
            "/xrpc/fyi.opensocial.resolveSubject",
            post(moderation::resolve_subject),
        )
        .route(
            "/xrpc/fyi.opensocial.createReport",
            post(moderation::create_report),
        )
        .route(
            "/xrpc/fyi.opensocial.listRecoveryKeys",
            get(recovery::list_recovery_keys),
        )
        .route(
            "/xrpc/fyi.opensocial.addRecoveryKey",
            post(recovery::add_recovery_key),
        )
        .route(
            "/xrpc/fyi.opensocial.removeRecoveryKey",
            post(recovery::remove_recovery_key),
        )
        .route(
            "/xrpc/fyi.opensocial.migratePrepare",
            post(migration::migrate_prepare),
        )
        .route(
            "/xrpc/fyi.opensocial.migrateExport",
            get(migration::migrate_export),
        )
        .route(
            "/xrpc/fyi.opensocial.migrateReceive",
            post(migration::migrate_receive),
        )
        .route(
            "/xrpc/fyi.opensocial.migrateComplete",
            post(migration::migrate_complete),
        )
        .route(
            "/xrpc/fyi.opensocial.migrateActivate",
            post(migration::migrate_activate),
        )
        .route(
            "/xrpc/fyi.opensocial.checkImport",
            get(import::check_import),
        )
        .route(
            "/xrpc/fyi.opensocial.importAccount",
            post(import::import_account),
        )
        .route(
            "/xrpc/fyi.opensocial.completeImport",
            post(import::complete_import),
        )
        .route(
            "/xrpc/fyi.opensocial.importAccountSpace",
            post(import_space::import_account_space),
        )
        .route("/oauth/authorize", get(oauth::authorize))
        .route(
            "/console/client-metadata.json",
            get(console_login::client_metadata),
        )
        .route("/img/{did}/{cid}", get(blobs::image))
        .route("/console/login", get(console_login::login))
        .route("/console/move/login", get(account_move::login))
        .route(
            "/xrpc/fyi.opensocial.getAccountMove",
            get(account_move::get_account_move),
        )
        .route(
            "/xrpc/fyi.opensocial.startAccountMove",
            post(account_move::start_account_move),
        )
        .route(
            "/xrpc/fyi.opensocial.finishAccountMove",
            post(account_move::finish_account_move),
        )
        .route(
            "/xrpc/fyi.opensocial.cancelAccountMove",
            post(account_move::cancel_account_move),
        )
        .route("/console/callback", get(console_login::callback))
        .route("/console/logout", post(console_login::logout))
        .merge(token_routes())
        .merge(oauth_routes())
        .route(
            "/xrpc/fyi.opensocial.putSite",
            post(sites::put_site).layer(axum::extract::DefaultBodyLimit::max(site_limit)),
        )
        .route("/xrpc/fyi.opensocial.deleteSite", post(sites::delete_site))
        .route("/.well-known/did.json", get(api::did_json))
        .route("/.well-known/atproto-did", get(api::atproto_did))
        .route("/xrpc/{nsid}", get(not_implemented).post(not_implemented))
        .layer(axum::middleware::from_fn_with_state(
            host.clone(),
            lexicons::check,
        ))
        .layer(axum::middleware::from_fn_with_state(
            host.clone(),
            sites::serve,
        ))
        .with_state(host)
}

/// The OAuth endpoints an app reaches from a browser, open to any origin.
fn oauth_routes() -> Router<Arc<Host>> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth::resource_metadata),
        )
        .route("/oauth/par", post(oauth::par))
        .route("/oauth/token", post(oauth::token))
        .route("/oauth/revoke", post(oauth::revoke))
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(mcp::resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/mcp",
            get(mcp::server_metadata),
        )
        .route("/mcp", post(mcp::mcp))
        .route("/mcp/register", post(mcp::register))
        .route("/mcp/authorize", get(mcp::authorize))
        .route("/mcp/token", post(mcp::token))
        .route(
            "/xrpc/fyi.opensocial.listConnectedApps",
            get(mcp::list_connected_apps),
        )
        .route(
            "/xrpc/fyi.opensocial.revokeConnectedApp",
            post(mcp::revoke_connected_app),
        )
        .layer(axum::middleware::from_fn(oauth::cors))
}

/// The methods an app calls with a token, as the group: open to any
/// origin, as the OAuth endpoints are.
fn token_routes() -> Router<Arc<Host>> {
    let x = |nsid: &str| format!("/xrpc/{nsid}");
    Router::new()
        .route(
            &x("com.atproto.repo.createRecord"),
            post(token_writes::create_record),
        )
        .route(
            &x("com.atproto.repo.putRecord"),
            post(token_writes::put_record),
        )
        .route(
            &x("com.atproto.repo.deleteRecord"),
            post(token_writes::delete_record),
        )
        .route(
            &x("com.atproto.repo.applyWrites"),
            post(token_writes::apply_writes),
        )
        .route(
            &x("com.atproto.repo.uploadBlob"),
            post(token_writes::upload_blob),
        )
        .route(
            &x("com.atproto.space.createRecord"),
            post(token_writes::space_create_record),
        )
        .route(
            &x("com.atproto.space.putRecord"),
            post(token_writes::space_put_record),
        )
        .route(
            &x("com.atproto.space.deleteRecord"),
            post(token_writes::space_delete_record),
        )
        .route(
            &x("com.atproto.space.applyWrites"),
            post(token_writes::space_apply_writes),
        )
        .route(
            &x("com.atproto.server.getSession"),
            get(token_writes::get_session),
        )
        .route(
            &x("com.atproto.server.createSession"),
            post(app_passwords::create_session),
        )
        .route(
            &x("com.atproto.server.refreshSession"),
            post(app_passwords::refresh_session),
        )
        .route(
            &x("com.atproto.server.deleteSession"),
            post(app_passwords::delete_session),
        )
        .route(
            &x("fyi.opensocial.provisionGroup"),
            post(provision::provision_group),
        )
        .route(
            &x("fyi.opensocial.getGroupAuth"),
            post(group_tokens::get_group_auth),
        )
        .layer(axum::middleware::from_fn(oauth::cors))
}

/// That the host is up, and which build it is.
async fn health() -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION") }))
}

async fn not_implemented(Path(nsid): Path<String>) -> XrpcError {
    XrpcError::not_implemented(&nsid)
}

/// What `group-host` prints for a command it does not know.
pub const USAGE: &str = "usage: group-host                 serve, as the environment says
       group-host move-host --from <old address> [--dry-run]
                                  repoint each account's DID document at the
                                  address the environment names now";

/// What `group-host` does with `args` (without its own name): serves,
/// or moves the host, printing what it did.
pub async fn run(args: &[String], config: Config) -> anyhow::Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => serve(config).await,
        ["move-host", "--from", from, rest @ ..] if rest.iter().all(|a| *a == "--dry-run") => {
            let host = Host::open(config)?;
            let dry_run = !rest.is_empty();
            let report = move_host::move_host(&host, from, dry_run).await?;
            for line in &report.said {
                println!("{line}");
            }
            if report.failed > 0 {
                anyhow::bail!(
                    "{} DID documents not repointed: run it again once the directory answers for them",
                    report.failed
                );
            }
            Ok(())
        }
        _ => anyhow::bail!("{USAGE}"),
    }
}

/// Serves `config` until the process is told to stop (SIGTERM, or ctrl-c),
/// saying where once it listens: `listening on <address>`. Requests under
/// way when it is told are answered first.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
    println!("listening on {}", listener.local_addr()?);
    tracing::info!(public_url = %config.public_url, did = %config.service_did, "serving");
    let host = Arc::new(Host::open(config)?);
    // Uploads no record took, moves kept a day, and invites lapsed, swept
    // now and each hour after.
    let sweeping = host.clone();
    tokio::spawn(async move {
        loop {
            let swept = sweeping.sweep_blobs(time::now_millis());
            tracing::info!(?swept, "swept the uploads no record took");
            let moves = sweeping.sweep_moves(time::now_millis()).await;
            tracing::info!(?moves, "gave back the moves kept a day");
            let invites = sweeping.sweep_invites(time::now_millis()).await;
            tracing::info!(?invites, "took back the invites lapsed");
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    });
    // The index reads the spaces whose writers moved, as they move, and
    // each minute, so a wake missed waits no longer.
    let indexing = host.clone();
    tokio::spawn(async move {
        loop {
            let woken = indexing.index_wake.notified();
            indexing.index_pending().await;
            let minute = std::time::Duration::from_secs(60);
            let _ = tokio::time::timeout(minute, woken).await;
        }
    });
    // Word of writes owed to authorities elsewhere, tried as it falls due.
    let delivering = host.clone();
    tokio::spawn(async move {
        loop {
            let tried = delivering.deliver_due().await;
            tracing::debug!(?tried, "delivered the notifyWrites due");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
    let app = router(host);
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
