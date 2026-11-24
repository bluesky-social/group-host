//! A host to test against, on a free port over a data directory of its
//! own, and the PLC directory it publishes to, faked in this process.

#![allow(dead_code)]

pub mod groups;
pub mod members;
pub mod net;
pub mod oauth;
pub mod spaces;

use atproto::plc::Signed;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use group_host::Host;
use group_host::config::Config;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A PLC directory: each DID's operations, each checked as the real one
/// checks them, and its document as it serves one. `refuse` turns every
/// operation away, and `refuse_ends` every tombstone; `slow` holds each
/// read of a last operation a while, so two changes at once both read
/// before either writes.
#[derive(Default)]
pub struct Plc {
    pub ops: Mutex<HashMap<String, Vec<Signed>>>,
    /// Every operation taken, in order, as `/export` lists them.
    pub exported: Mutex<Vec<Value>>,
    /// How many times `/export` was asked.
    pub exports: std::sync::atomic::AtomicUsize,
    pub refuse: AtomicBool,
    pub refuse_ends: AtomicBool,
    pub slow: AtomicBool,
}

/// Whether `switch` is on.
pub fn on(switch: &AtomicBool) -> bool {
    switch.load(Ordering::SeqCst)
}

/// Turns `switch` to `to`.
pub fn turn(switch: &AtomicBool, to: bool) {
    switch.store(to, Ordering::SeqCst);
}

/// Takes a database back to its schema before migration `to`, as one
/// made then would be, so opening it runs `to` and those after again:
/// each later migration undone, the last first, statement by statement.
/// A table or index it made is dropped, a rename is reversed, a column it
/// added is dropped, and a table it dropped is made again as an earlier
/// one made it. What only moves rows is left as it is.
#[allow(dead_code)]
pub fn rewind(conn: &rusqlite::Connection, to: usize) {
    let migrations = group_host::store::MIGRATIONS;
    let statements = |m: &str| -> Vec<String> {
        m.split(';')
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
            .collect()
    };
    let name = |rest: &str| {
        let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
        let rest = rest.strip_prefix("IF EXISTS ").unwrap_or(rest);
        rest.split([' ', '(']).next().unwrap().to_string()
    };
    for (at, migration) in migrations.iter().enumerate().skip(to).rev() {
        for statement in statements(migration).iter().rev() {
            let undo = if let Some(rest) = statement.strip_prefix("CREATE TABLE ") {
                format!("DROP TABLE IF EXISTS {}", name(rest))
            } else if let Some(rest) = statement
                .strip_prefix("CREATE INDEX ")
                .or_else(|| statement.strip_prefix("CREATE UNIQUE INDEX "))
            {
                format!("DROP INDEX IF EXISTS {}", name(rest))
            } else if let Some(rest) = statement.strip_prefix("ALTER TABLE ") {
                let words: Vec<&str> = rest.split(' ').collect();
                match words[..] {
                    [from, "RENAME", "TO", to] => format!("ALTER TABLE {to} RENAME TO {from}"),
                    [table, "ADD", "COLUMN", column, ..] => {
                        format!("ALTER TABLE {table} DROP COLUMN {column}")
                    }
                    _ => panic!("no undoing {statement}"),
                }
            } else if let Some(rest) = statement.strip_prefix("DROP TABLE ") {
                let table = name(rest);
                let made = |s: &String| {
                    s.strip_prefix("CREATE TABLE ")
                        .is_some_and(|r| name(r) == table)
                };
                let mut earlier = migrations[..at].iter().flat_map(|m| statements(m));
                earlier.rfind(made).expect("a table made before")
            } else {
                continue;
            };
            conn.execute(&undo, []).unwrap();
        }
    }
    conn.pragma_update(None, "user_version", to as i64).unwrap();
}

async fn publish(
    State(plc): State<Arc<Plc>>,
    Path(did): Path<String>,
    Json(op): Json<Value>,
) -> StatusCode {
    let Ok(op) = Signed::from_json(&op) else {
        return StatusCode::BAD_REQUEST;
    };
    let mut ops = plc.ops.lock().unwrap();
    let log = ops.entry(did.clone()).or_default();
    let signed = match (log.last(), &op) {
        (
            None,
            Signed::Update {
                op: o, prev: None, ..
            },
        ) => op.did() == did && op.signed_by(&o.rotation_keys).is_some(),
        (Some(last @ Signed::Update { op: o, .. }), _) => {
            op.prev() == Some(last.cid()) && op.signed_by(&o.rotation_keys).is_some()
        }
        _ => false,
    };
    // At most five rotation keys, each one it can check a signature by.
    let keys_taken = match &op {
        Signed::Update { op: o, .. } => {
            o.rotation_keys.len() <= 5
                && o.rotation_keys
                    .iter()
                    .all(|k| atproto::PublicKey::from_did_key(k).is_ok())
        }
        Signed::Tombstone { .. } => true,
    };
    let ending = matches!(op, Signed::Tombstone { .. });
    if on(&plc.refuse) || (ending && on(&plc.refuse_ends)) || !signed || !keys_taken {
        return StatusCode::BAD_REQUEST;
    }
    let mut exported = plc.exported.lock().unwrap();
    let at = exported.len();
    exported.push(json!({
        "did": did,
        "operation": op.to_json(),
        "cid": op.cid().to_string(),
        "nullified": false,
        "createdAt": format!("2026-01-01T{:02}:{:02}:{:02}.000Z", at / 3600, at / 60 % 60, at % 60),
    }));
    log.push(op);
    StatusCode::OK
}

/// The operations taken after `after`, as JSON lines, two at most: a page
/// shorter than any a reader asks for, so it reads on to an empty one.
async fn export(State(plc): State<Arc<Plc>>, Query(q): Query<HashMap<String, String>>) -> String {
    plc.exports.fetch_add(1, Ordering::SeqCst);
    let after = q.get("after").cloned().unwrap_or_default();
    let exported = plc.exported.lock().unwrap();
    let later = exported
        .iter()
        .filter(|e| e["createdAt"].as_str().unwrap() > after.as_str());
    later.take(2).map(|e| format!("{e}\n")).collect()
}

async fn document(
    State(plc): State<Arc<Plc>>,
    Path(did): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let ops = plc.ops.lock().unwrap();
    let Some(Signed::Update { op, .. }) = ops.get(&did).and_then(|l| l.last()) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let methods: Vec<Value> = op
        .verification_methods
        .iter()
        .map(|(name, key)| {
            json!({
                "id": format!("{did}#{name}"),
                "type": "Multikey",
                "controller": did,
                "publicKeyMultibase": key.strip_prefix("did:key:").unwrap(),
            })
        })
        .collect();
    let services: Vec<Value> = op
        .services
        .iter()
        .map(|(name, s)| json!({"id": format!("#{name}"), "type": s.kind, "serviceEndpoint": s.endpoint}))
        .collect();
    Ok(Json(json!({
        "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
        "id": did,
        "alsoKnownAs": op.also_known_as,
        "verificationMethod": methods,
        "service": services,
    })))
}

/// `did`'s last operation, as the directory's `/log/last` serves it.
async fn last(
    State(plc): State<Arc<Plc>>,
    Path(did): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    if plc.slow.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let ops = plc.ops.lock().unwrap();
    let last = ops.get(&did).and_then(|l| l.last());
    last.map(|op| Json(op.to_json()))
        .ok_or(StatusCode::NOT_FOUND)
}

/// The bytes the favicon service answers for a site whose name starts
/// `chat.`.
pub const FAVICON: &[u8] = b"\x89PNG a green dot";

/// A favicon service, as the host asks one: where it is, the link's
/// address to follow. A site whose name starts `chat.` has `FAVICON`, one
/// whose starts `big.` an icon larger than a link's may be, `gif.` one
/// that is no PNG; any other none.
pub async fn start_favicons() -> String {
    async fn icon(
        axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        let site = q.get("url").map(String::as_str).unwrap_or_default();
        let site = site.trim_start_matches("https://");
        let png = [(axum::http::header::CONTENT_TYPE, "image/png")];
        match site.split('.').next() {
            Some("chat") => (png, FAVICON).into_response(),
            Some("big") => (png, vec![0; 100_001]).into_response(),
            Some("gif") => {
                ([(axum::http::header::CONTENT_TYPE, "image/gif")], "GIF89a").into_response()
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        }
    }
    let at = serve(Router::new().route("/favicon", get(icon))).await;
    format!("{at}/favicon?url=")
}

/// Serves `router` on a free port, answering its address.
pub async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

pub async fn start_plc(refuse: bool) -> (Arc<Plc>, String) {
    let plc = Arc::new(Plc {
        refuse: refuse.into(),
        ..Plc::default()
    });
    let router = Router::new()
        .route("/{did}", get(document).post(publish))
        .route("/{did}/log/last", get(last))
        .route("/export", get(export))
        .with_state(plc.clone());
    (plc, serve(router).await)
}

/// A running host and what it stands on.
pub struct Running {
    pub host: Arc<Host>,
    pub url: String,
    pub plc: Arc<Plc>,
    pub http: reqwest::Client,
    pub data: tempfile::TempDir,
}

impl Running {
    /// What `GET <path>` answers: its status and JSON.
    pub async fn get_path(&self, path: &str) -> (u16, Value) {
        let res = self
            .http
            .get(format!("{}{path}", self.url))
            .send()
            .await
            .unwrap();
        (
            res.status().as_u16(),
            res.json().await.unwrap_or(Value::Null),
        )
    }

    /// What `GET /xrpc/<nsid>?<query>` answers: its status and JSON.
    pub async fn get(&self, nsid: &str, query: &str) -> (u16, Value) {
        let res = self
            .http
            .get(format!("{}/xrpc/{nsid}?{query}", self.url))
            .send()
            .await
            .unwrap();
        (
            res.status().as_u16(),
            res.json().await.unwrap_or(Value::Null),
        )
    }
}

/// A host whose groups' handles are under `.groups.test`, with `vars`
/// besides, publishing to a fake PLC directory.
pub async fn start(vars: &[(&str, &str)], refuse: bool) -> Running {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    start_on(listener, vars, refuse).await
}

/// A host whose `PUBLIC_URL` is where it listens, for servers elsewhere
/// to reach it.
pub async fn start_reachable(vars: &[(&str, &str)]) -> Running {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut vars = vars.to_vec();
    vars.push(("PUBLIC_URL", &url));
    start_on(listener, &vars, false).await
}

/// A host given where it listens as `var` too, as for a domain that
/// reaches it (`SITE_CHECK_AT`).
pub async fn start_naming_itself(var: &str, vars: &[(&str, &str)]) -> Running {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut vars = vars.to_vec();
    vars.push((var, &url));
    start_on(listener, &vars, false).await
}

/// A host reachable by name, at `http://localhost:<port>`, its `did:web`
/// of that name: for servers elsewhere that resolve its DID, as one
/// resolves `localhost` over http. It listens on every address, IPv6 and
/// IPv4, whichever `localhost` is.
pub async fn start_reachable_by_name(vars: &[(&str, &str)]) -> Running {
    let listener = tokio::net::TcpListener::bind("[::]:0").await.unwrap();
    let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let mut vars = vars.to_vec();
    vars.push(("PUBLIC_URL", &url));
    start_on(listener, &vars, false).await
}

/// Every event the host logs written out, into nothing, as one is when it
/// serves: with no subscriber, an event's fields are never evaluated, and
/// coverage counts their lines as never run.
fn logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter("group_host=trace")
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
    });
}

async fn start_on(
    listener: tokio::net::TcpListener,
    vars: &[(&str, &str)],
    refuse: bool,
) -> Running {
    logging();
    let (plc, plc_url) = start_plc(refuse).await;
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().to_str().unwrap().to_string();
    let mut env: HashMap<String, String> = [
        ("DATA_DIR", dir.as_str()),
        ("PLC_URL", plc_url.as_str()),
        ("HANDLE_DOMAIN", ".groups.test"),
        ("PUBLIC_URL", "https://host.test"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    env.extend(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let config = Config::from(|k| env.get(k).cloned()).unwrap();
    let host = Arc::new(Host::open(config).unwrap());
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = group_host::router(host.clone());
    // As the host serves itself: knowing each request's address.
    let app = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Running {
        host,
        url,
        plc,
        http: reqwest::Client::new(),
        data,
    }
}
