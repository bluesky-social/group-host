//! A host to test against, on a free port over a data directory of its
//! own, and the PLC directory it publishes to, faked in this process.

#![allow(dead_code)]

pub mod net;

use atproto::plc::Signed;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use group_host::Host;
use group_host::config::Config;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A PLC directory: each DID's operations, each checked as the real one
/// checks them, and its document as it serves one. `refuse` turns every
/// operation away.
#[derive(Default)]
pub struct Plc {
    pub ops: Mutex<HashMap<String, Vec<Signed>>>,
    pub refuse: bool,
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
    if plc.refuse || !signed {
        return StatusCode::BAD_REQUEST;
    }
    log.push(op);
    StatusCode::OK
}

async fn document(
    State(plc): State<Arc<Plc>>,
    Path(did): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let ops = plc.ops.lock().unwrap();
    let Some(Signed::Update { op, .. }) = ops.get(&did).and_then(|l| l.last()) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let key = op.verification_methods["atproto"]
        .strip_prefix("did:key:")
        .unwrap();
    Ok(Json(json!({
        "@context": ["https://www.w3.org/ns/did/v1", "https://w3id.org/security/multikey/v1"],
        "id": did,
        "alsoKnownAs": op.also_known_as,
        "verificationMethod": [{"id": format!("{did}#atproto"), "type": "Multikey", "controller": did, "publicKeyMultibase": key}],
        "service": [{"id": "#atproto_pds", "type": "AtprotoPersonalDataServer", "serviceEndpoint": op.services["atproto_pds"].endpoint}],
    })))
}

/// Serves `router` on a free port, answering its address.
async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

pub async fn start_plc(refuse: bool) -> (Arc<Plc>, String) {
    let plc = Arc::new(Plc {
        refuse,
        ..Plc::default()
    });
    let router = Router::new()
        .route("/{did}", get(document).post(publish))
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
    let url = serve(group_host::router(host.clone())).await;
    Running {
        host,
        url,
        plc,
        http: reqwest::Client::new(),
        data,
    }
}
