//! The group host: the PDS for every group it runs, the
//! authority for their spaces, their OAuth server, and the
//! `org.atgrp.*` methods that run a group. `docs/design/README.md`
//! says how it is built and what is still to come.

pub mod account_move;
pub mod actor_search;
pub mod actors;
pub mod admin;
pub mod api;
pub mod app_passwords;
pub mod apps;
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
pub mod group_domain;
pub mod group_handle;
pub mod group_invites;
pub mod group_join;
pub mod group_lifecycle;
pub mod group_members;
pub mod group_spaces;
pub mod group_tokens;
pub mod import;
pub mod import_space;
pub mod index;
pub mod index_api;
pub mod lexicons;
pub mod lifecycle;
pub mod limits;
pub mod logs;
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
pub mod observe;
pub mod permission_sets;
pub mod plc;
pub mod provision;
pub mod proxy;
pub mod rbac;
pub mod recovery;
pub mod repo;
pub mod scopes;
pub mod secrets;
pub mod space;
pub mod space_api;
pub mod stats;
pub mod store;
pub mod takedown;
pub mod time;
pub mod token_writes;
pub mod xrpc;

use atproto_client::guard::{Guard, Guarded};
use atproto_client::identity::{HandleResolver, Resolver};
use axum::routing::{get, post};
use axum::{Json, Router};
use config::Config;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;

/// What every request is answered with.
pub struct Host {
    pub config: Config,
    pub store: store::Store,
    pub secrets: secrets::Secrets,
    pub plc: plc::Directory,
    /// Other accounts' documents and handles, as read.
    pub identity: Resolver,
    /// Each commit's revision, later than the last.
    pub clock: atproto::tid::TidClock,
    /// Held while a commit is made, one lock for each group's commits.
    writing: repo::Writing,
    /// Told of each new event, which wakes the firehose's streams.
    pub events: tokio::sync::broadcast::Sender<()>,
    pub(crate) http: reqwest::Client,
    /// For the hosts others' data names: a DID document's PDS and
    /// services, a syncer's endpoint, a client's metadata, a group's own
    /// domain. Held to https at public addresses, but for a host on http
    /// itself (`PUBLIC_URL`), as on a network of test servers.
    pub(crate) guarded: Guarded,
    /// The console's OAuth client, signing people in and signing in to
    /// accounts being moved here (`console_login::client`).
    pub(crate) console: atproto_client::oauth::Client,
    /// The spaces the index is to read, and its wake.
    indexing: std::sync::Mutex<std::collections::BTreeSet<String>>,
    index_wake: tokio::sync::Notify,
    /// The OAuth clients' metadata, as read.
    pub(crate) clients: oauth_clients::Clients,
    /// The handles the directory names, as last read for a search.
    pub(crate) handles: actor_search::Handles,
    /// Other apps' lexicons, as read.
    pub(crate) lexicons: permission_sets::Read,
    /// Apps' logos, as read.
    pub(crate) logos: apps::Logos,
    /// The lines it logged, kept for its operators.
    pub logs: Arc<logs::Logs>,
    /// How it is used, counted for its operators.
    pub stats: stats::Stats,
    /// How much each caller has asked, against the limits.
    pub limits: limits::Limiter,
}

impl Host {
    /// The host `config` describes, its data directory made if need be.
    pub fn open(config: Config) -> anyhow::Result<Host> {
        std::fs::create_dir_all(&config.data_dir)?;
        let store = store::Store::open(&config.data_dir.join("host.sqlite"))?;
        let secrets = secrets::Secrets::load(&config.data_dir)?;
        let logs = logs::Logs::open(&config.data_dir.join("logs.sqlite"), config.logs_keep)?;
        let stats = stats::Stats::open(&config.data_dir.join("stats.sqlite"))?;
        let plc = plc::Directory::new(&config.plc_url);
        let guard = match config.public_url.starts_with("http://") {
            true => Guard::allowing_private(),
            false => Guard::strict(),
        };
        let guarded = Guarded::new(guard, client_builder()).expect("a client builds");
        // Each call to a host others' data names traced and logged.
        let guarded = guarded.sending(Arc::new(observe::Calls));
        let identity = resolver(&config, guarded.clone());
        let console = console_login::client(&config, &secrets, &store, &guarded, &identity);
        // A clock id of its own, so two hosts' revisions never collide.
        let clock = atproto::tid::TidClock::new(u64::from(secrets.signing.public().to_sec1()[1]));
        let limits = limits::Limiter::new(config.rate_limits);
        Ok(Host {
            config,
            store,
            secrets,
            plc,
            identity,
            clock,
            writing: Default::default(),
            events: tokio::sync::broadcast::channel(16).0,
            http: client(),
            guarded,
            console,
            indexing: Default::default(),
            index_wake: Default::default(),
            clients: Default::default(),
            handles: Default::default(),
            lexicons: Default::default(),
            logos: Default::default(),
            logs: Arc::new(logs),
            stats,
            limits,
        })
    }
}

/// How long the host waits on another server before giving up on it: a
/// server that takes a request and never answers holds nothing up longer.
pub const OUTGOING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// How long the host keeps a connection to another server open with no
/// request on it: under the 5 s after which Node's HTTP server, which the
/// reference PDS and PLC run on, closes an idle one. A request sent on a
/// connection as its server closes it fails ("connection closed before
/// message completed"), so the host drops one before that can happen.
pub const IDLE_AT_MOST: std::time::Duration = std::time::Duration::from_secs(4);

/// What every client of the host's for other servers is built from.
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(OUTGOING_TIMEOUT)
        .pool_idle_timeout(IDLE_AT_MOST)
}

/// A client for the host's requests to other servers.
pub fn client() -> reqwest::Client {
    let built = client_builder().build();
    built.expect("a client with a timeout builds")
}

/// The resolver of other accounts' identities `config` sets: the PLC
/// directory, the handle resolvers in their order (`HANDLE_RESOLVERS`),
/// the `did:web`s read elsewhere (`DID_WEB_HOSTS`) and on http
/// (`DID_WEB_SCHEME`); the hosts others' data names asked through
/// `guarded`, the services it is set with through a client following no
/// redirect.
pub fn resolver(config: &Config, guarded: Guarded) -> Resolver {
    let set = client_builder().redirect(reqwest::redirect::Policy::none());
    let set = set.build().expect("a client builds");
    let resolvers = config.handle_resolvers.iter().map(|r| match r.as_str() {
        config::WELL_KNOWN => HandleResolver::WellKnown,
        service => HandleResolver::Xrpc(service.to_string()),
    });
    let scheme = &config.did_web_scheme;
    let hosts = config.did_web_hosts.iter();
    let hosts = hosts.map(|(host, at)| (host.clone(), format!("{scheme}://{at}")));
    let resolver = Resolver::new(set, guarded, &config.plc_url, resolvers.collect());
    let resolver = resolver.reading_at(hosts.collect());
    let resolver = resolver.sending(Arc::new(observe::Calls));
    match scheme.as_str() {
        "http" => resolver.on_http(),
        _ => resolver,
    }
}

/// The host's routes, each request logged and counted
/// ([`observe`]), and spent from its client's limits ([`limits`]).
pub fn router(host: Arc<Host>) -> Router {
    let observing = axum::middleware::from_fn_with_state(host.clone(), observe::observe);
    let limiting = axum::middleware::from_fn_with_state(host.clone(), limits::by_address);
    routes(host).layer(limiting).layer(observing)
}

/// The host's routes, as the MCP server calls them for a tool, within
/// the request that called it.
pub(crate) fn routes(host: Arc<Host>) -> Router {
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
        .route(
            "/xrpc/org.atgrp.internal.checkAddress",
            get(api::check_address),
        )
        .route(
            "/xrpc/org.atgrp.internal.searchActors",
            get(actor_search::search_actors),
        )
        .route("/xrpc/com.atproto.repo.getRecord", get(api::get_record))
        .route("/xrpc/com.atproto.repo.listRecords", get(api::list_records))
        .route("/xrpc/com.atproto.sync.getRepo", get(api::get_repo))
        .route(
            "/xrpc/com.atproto.sync.getRecord",
            get(api::get_sync_record),
        )
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
            "/xrpc/com.atproto.space.notifyCredentialsRevoked",
            post(space_api::notify_credentials_revoked),
        )
        .route(
            "/xrpc/com.atproto.space.notifyRepoUpdate",
            post(authority_api::notify_repo_update),
        )
        .route(
            "/xrpc/org.atgrp.internal.createGroup",
            post(group_api::create_group),
        )
        .route(
            "/xrpc/org.atgrp.internal.getGroup",
            get(group_api::get_group),
        )
        .route(
            "/xrpc/org.atgrp.internal.listMembers",
            get(group_api::list_members),
        )
        .route(
            "/xrpc/org.atgrp.internal.deactivateGroup",
            post(group_lifecycle::deactivate_group),
        )
        .route(
            "/xrpc/org.atgrp.internal.activateGroup",
            post(group_lifecycle::activate_group),
        )
        .route(
            "/xrpc/org.atgrp.internal.deleteGroup",
            post(group_lifecycle::delete_group),
        )
        .route(
            "/xrpc/org.atgrp.internal.checkGroupHandle",
            get(group_handle::check_group_handle),
        )
        .route(
            "/xrpc/org.atgrp.internal.updateGroupHandle",
            post(group_handle::update_group_handle),
        )
        .route(
            "/xrpc/org.atgrp.internal.listSpaceRecords",
            get(index_api::list_space_records),
        )
        .route(
            "/xrpc/org.atgrp.internal.getSpaceBlob",
            get(index_api::get_space_blob),
        )
        .route(
            "/xrpc/org.atgrp.leaveGroup",
            post(group_members::leave_group),
        )
        .route(
            "/xrpc/org.atgrp.requestJoin",
            post(group_join::request_join),
        )
        .route(
            "/xrpc/org.atgrp.internal.getJoinRequest",
            get(group_join::get_join_request),
        )
        .route(
            "/xrpc/org.atgrp.listJoinRequests",
            get(group_join::list_join_requests),
        )
        .route(
            "/xrpc/org.atgrp.denyJoinRequest",
            post(group_join::deny_join_request),
        )
        .route(
            "/xrpc/org.atgrp.cancelJoinRequest",
            post(group_join::cancel_join_request),
        )
        .route(
            "/xrpc/org.atgrp.internal.listInvites",
            get(group_invites::list_invites),
        )
        .route(
            "/xrpc/org.atgrp.internal.getActorInvites",
            get(group_invites::get_actor_invites),
        )
        .route(
            "/xrpc/org.atgrp.internal.declineInvite",
            post(group_invites::decline_invite),
        )
        .route(
            "/xrpc/org.atgrp.internal.getAuthorization",
            get(authorization::get_authorization),
        )
        .route(
            "/xrpc/org.atgrp.internal.approveAuthorization",
            post(authorization::approve_authorization),
        )
        .route(
            "/xrpc/org.atgrp.internal.declineAuthorization",
            post(authorization::decline_authorization),
        )
        .route(
            "/xrpc/org.atgrp.internal.createAppPassword",
            post(app_passwords::create_app_password),
        )
        .route(
            "/xrpc/org.atgrp.internal.listAppPasswords",
            get(app_passwords::list_app_passwords),
        )
        .route(
            "/xrpc/org.atgrp.internal.revokeAppPassword",
            post(app_passwords::revoke_app_password),
        )
        .route(
            "/xrpc/org.atgrp.internal.listGroupTokens",
            get(group_tokens::list_group_tokens),
        )
        .route(
            "/xrpc/org.atgrp.internal.revokeGroupToken",
            post(group_tokens::revoke_group_token),
        )
        .route(
            "/xrpc/org.atgrp.internal.getViewer",
            get(actors::get_viewer),
        )
        .route("/xrpc/org.atgrp.internal.getActor", get(actors::get_actor))
        .route(
            "/xrpc/org.atgrp.internal.getActorGroups",
            get(actors::get_actor_groups),
        )
        .route(
            "/xrpc/org.atgrp.internal.putLabelDefinition",
            post(moderation::put_label_definition),
        )
        .route(
            "/xrpc/org.atgrp.internal.deleteLabelDefinition",
            post(moderation::delete_label_definition),
        )
        .route(
            "/xrpc/org.atgrp.mod.querySubjects",
            get(moderation::query_subjects),
        )
        .route(
            "/xrpc/org.atgrp.mod.querySubjectHistory",
            get(moderation::query_subject_history),
        )
        .route(
            "/xrpc/org.atgrp.internal.getModSubjectBlob",
            get(moderation::get_mod_subject_blob),
        )
        .route(
            "/xrpc/org.atgrp.mod.updateSubject",
            post(moderation::update_subject),
        )
        .route(
            "/xrpc/org.atgrp.mod.submitReport",
            post(moderation::submit_report),
        )
        .route(
            "/xrpc/org.atgrp.mod.submitAppeal",
            post(moderation::submit_appeal),
        )
        .route(
            "/xrpc/org.atgrp.mod.querySelfHistory",
            get(moderation::query_self_history),
        )
        .route(
            "/xrpc/org.atgrp.internal.listRecoveryKeys",
            get(recovery::list_recovery_keys),
        )
        .route(
            "/xrpc/org.atgrp.internal.addRecoveryKey",
            post(recovery::add_recovery_key),
        )
        .route(
            "/xrpc/org.atgrp.internal.removeRecoveryKey",
            post(recovery::remove_recovery_key),
        )
        .route(
            "/xrpc/org.atgrp.internal.migratePrepare",
            post(migration::migrate_prepare),
        )
        .route(
            "/xrpc/org.atgrp.internal.migrateExport",
            get(migration::migrate_export),
        )
        .route(
            "/xrpc/org.atgrp.internal.migrateReceive",
            post(migration::migrate_receive),
        )
        .route(
            "/xrpc/org.atgrp.internal.migrateComplete",
            post(migration::migrate_complete),
        )
        .route(
            "/xrpc/org.atgrp.internal.migrateActivate",
            post(migration::migrate_activate),
        )
        .route(
            "/xrpc/org.atgrp.internal.checkImport",
            get(import::check_import),
        )
        .route(
            "/xrpc/org.atgrp.internal.importAccount",
            post(import::import_account),
        )
        .route(
            "/xrpc/org.atgrp.internal.completeImport",
            post(import::complete_import),
        )
        .route("/xrpc/org.atgrp.internal.getLogs", get(admin::get_logs))
        .route("/xrpc/org.atgrp.internal.getStats", get(admin::get_stats))
        .route(
            "/xrpc/org.atgrp.internal.importAccountSpace",
            post(import_space::import_account_space),
        )
        .route("/oauth/authorize", get(oauth::authorize))
        .route(
            "/console/client-metadata.json",
            get(console_login::client_metadata),
        )
        .route("/img/app", get(apps::logo))
        .route("/img/{did}/{cid}", get(blobs::image))
        .route("/console/login", get(console_login::login))
        .route("/console/move/login", get(account_move::login))
        .route(
            "/xrpc/org.atgrp.internal.getAccountMove",
            get(account_move::get_account_move),
        )
        .route(
            "/xrpc/org.atgrp.internal.startAccountMove",
            post(account_move::start_account_move),
        )
        .route(
            "/xrpc/org.atgrp.internal.finishAccountMove",
            post(account_move::finish_account_move),
        )
        .route(
            "/xrpc/org.atgrp.internal.cancelAccountMove",
            post(account_move::cancel_account_move),
        )
        .route("/console/callback", get(console_login::callback))
        .route("/console/move/callback", get(account_move::callback))
        .route("/console/switch", post(console_login::switch))
        .route("/console/logout", post(console_login::logout))
        .merge(token_routes())
        .merge(oauth_routes())
        .merge(space_routes())
        .route(
            "/xrpc/org.atgrp.internal.putGroupDomain",
            post(group_domain::put_group_domain),
        )
        .route(
            "/xrpc/org.atgrp.internal.deleteGroupDomain",
            post(group_domain::delete_group_domain),
        )
        .route("/tls/ask", get(group_domain::tls_ask))
        .route("/.well-known/did.json", get(api::did_json))
        .route("/.well-known/atproto-did", get(api::atproto_did))
        .layer(axum::middleware::from_fn(lexicons::check))
        .layer(axum::middleware::from_fn_with_state(
            host.clone(),
            group_domain::serve,
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
            "/xrpc/org.atgrp.internal.listConnectedApps",
            get(mcp::list_connected_apps),
        )
        .route(
            "/xrpc/org.atgrp.internal.revokeConnectedApp",
            post(mcp::revoke_connected_app),
        )
        .layer(axum::middleware::from_fn(oauth::cors))
}

/// What a space's member reads it by, from its authority and from each
/// writer's repo here: open to any origin, as lex-space reads a space in
/// a browser.
fn space_routes() -> Router<Arc<Host>> {
    Router::new()
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
            "/xrpc/com.atproto.space.authorizeSpaceSync",
            post(authority_api::authorize_space_sync),
        )
        .route(
            "/xrpc/com.atproto.space.listWriters",
            get(authority_api::list_writers),
        )
        .layer(axum::middleware::from_fn(space_api::cors))
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
            &x("com.atproto.space.createSpace"),
            post(group_spaces::create_space),
        )
        .route(
            &x("com.atproto.space.updateSpace"),
            post(group_spaces::update_space),
        )
        .route(
            &x("com.atproto.space.deleteSpace"),
            post(group_spaces::delete_space),
        )
        .route(
            &x("com.atproto.space.getSpace"),
            get(group_spaces::get_space),
        )
        .route(
            &x("com.atproto.space.getDelegationToken"),
            get(group_spaces::get_delegation_token),
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
            &x("org.atgrp.internal.provisionGroup"),
            post(provision::provision_group),
        )
        .route(
            &x("org.atgrp.internal.getGroupAuth"),
            post(group_tokens::get_group_auth),
        )
        .route(
            &x("com.atproto.server.getServiceAuth"),
            get(proxy::get_service_auth),
        )
        // Any other method, proxied where `atproto-proxy` says.
        .route("/xrpc/{nsid}", get(proxy::proxy).post(proxy::proxy))
        .layer(axum::middleware::from_fn(oauth::cors))
}

/// The commit the host was built from: `GROUP_HOST_COMMIT` as it was
/// built, which the Dockerfile sets; none in a build of a checkout.
const COMMIT: Option<&str> = option_env!("GROUP_HOST_COMMIT");

/// That the host is up, and which build it is.
async fn health() -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION"), "commit": COMMIT }))
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
    let host = Arc::new(Host::open(config)?);
    // Every line from here kept for the operators too, written as it goes.
    logs::sink(&host.logs);
    tokio::spawn(admin::keeping(host.clone(), admin::FLUSH));
    // The relays asked to read the firehose now, before anything is
    // written, and again every twenty minutes, so one is reading when a
    // group's first events go out.
    tokio::spawn(firehose::crawling(host.clone(), firehose::CRAWL_EVERY));
    let config = &host.config;
    tracing::info!(
        public_url = %config.public_url,
        did = %config.service_did,
        version = env!("CARGO_PKG_VERSION"),
        commit = COMMIT,
        "serving"
    );
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
    // Word of writes owed to authorities elsewhere, and word of lapses and
    // deletions owed to syncers, tried as it falls due.
    let delivering = host.clone();
    tokio::spawn(async move {
        loop {
            let tried = delivering.deliver_due().await;
            tracing::debug!(?tried, "delivered the notifyRepoUpdates due");
            let retried = delivering.retry_syncers_due().await;
            tracing::debug!(?retried, "retried the word owed to syncers");
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
    // Served knowing each request's address, which the limits count by.
    let app = router(host.clone()).into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(stopped())
        .await?;
    tracing::info!("stopped");
    // What was logged and counted since the last write, kept before it
    // goes.
    admin::wrote("logs", host.logs.flush());
    admin::wrote("stats", host.stats.flush());
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
