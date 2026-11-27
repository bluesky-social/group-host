//! The firehose, `com.atproto.sync.subscribeRepos`: every event the host
//! keeps, after a cursor if one is given, then each as it happens, over a
//! WebSocket. A frame is a header (`op` 1 and the event's `t`) then its
//! body, both DRISL; a refusal, a header of `op` -1 and an error body.
//! And the relays, asked to read it.

use crate::Host;
use crate::store::Sequenced;
use crate::xrpc::XrpcError;
use atproto::{Value, drisl};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

/// How many events are read from the database at a time.
const PAGE: u32 = 500;

/// How long between asking the relays to read the firehose.
pub const CRAWL_EVERY: Duration = Duration::from_secs(20 * 60);

/// Asks each relay `PDS_CRAWLERS` names to read the firehose
/// (`requestCrawl`), now and then every `every`, whether or not anything
/// was written. A relay that starts reading a host takes only the events
/// after it connects, so it must be reading before a group's first events
/// go out. One that stopped reading a host, as after failing to reach it a
/// while, reads it again only when asked; asking one that reads already
/// changes nothing.
pub async fn crawling(host: Arc<Host>, every: Duration) {
    let hostname = host.config.public_url.split("://").nth(1);
    let body = serde_json::json!({ "hostname": hostname.unwrap_or_default() });
    loop {
        for crawler in &host.config.crawlers {
            let url = format!("{crawler}/xrpc/com.atproto.sync.requestCrawl");
            let asked = host.http.post(url).json(&body).send().await;
            match asked.and_then(|r| r.error_for_status()) {
                Ok(_) => tracing::info!(crawler, "asked the relay to crawl"),
                Err(e) => tracing::warn!(crawler, error = %e, "requestCrawl failed"),
            }
        }
        tokio::time::sleep(every).await;
    }
}

fn map<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Map(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// An event's frame: its header, then its body with its `seq`.
fn frame(event: &Sequenced) -> Vec<u8> {
    let header = map([
        ("op", Value::Integer(1)),
        ("t", Value::String(event.kind.clone())),
    ]);
    let body = drisl::decode(&event.body).expect("an event the host wrote");
    let mut body = body.into_map().unwrap_or_default();
    body.insert("seq".into(), Value::Integer(event.seq));
    [drisl::encode(&header), drisl::encode(&Value::Map(body))].concat()
}

/// A refusal's frame.
fn refusal(error: &str, message: &str) -> Vec<u8> {
    let header = map([("op", Value::Integer(-1))]);
    let body = Value::Map(BTreeMap::from([
        ("error".into(), Value::String(error.into())),
        ("message".into(), Value::String(message.into())),
    ]));
    [drisl::encode(&header), drisl::encode(&body)].concat()
}

pub async fn subscribe_repos(
    ws: WebSocketUpgrade,
    State(host): State<Arc<Host>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, XrpcError> {
    let cursor = match params.get("cursor") {
        None => None,
        Some(c) => Some(c.parse::<i64>().map_err(|_| {
            XrpcError::new(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "cursor is a sequence number",
            )
        })?),
    };
    Ok(ws.on_upgrade(move |socket| stream(host, socket, cursor)))
}

/// Sends `socket` every event after `cursor`, or from now with none, then
/// each as it is made, until it goes away.
async fn stream(host: Arc<Host>, socket: WebSocket, cursor: Option<i64>) {
    let _ = follow(host, socket, cursor).await;
}

/// The stream is over: its socket went away, or its cursor was refused.
struct Gone;

async fn follow(host: Arc<Host>, mut socket: WebSocket, cursor: Option<i64>) -> Result<(), Gone> {
    // Told of new events from before the first read, so none goes unseen
    // between reading and waiting.
    let mut told = host.events.subscribe();
    let last = host.store.last_seq().unwrap_or_default();
    if cursor.is_some_and(|c| c > last) {
        let refused = refusal("FutureCursor", "Cursor in the future.");
        let _ = socket.send(Message::Binary(refused.into())).await;
        return Err(Gone);
    }
    let mut from = cursor.unwrap_or(last);
    loop {
        let events = host.store.events_after(from, PAGE).unwrap_or_default();
        for event in &events {
            let sent = socket.send(Message::Binary(frame(event).into())).await;
            sent.ok().ok_or(Gone)?;
            from = event.seq;
        }
        if events.len() == PAGE as usize {
            continue;
        }
        tokio::select! {
            // A lagged receiver missed only wake-ups: the database has them.
            _ = told.recv() => {}
            said = socket.recv() => {
                said.and_then(Result::ok).ok_or(Gone)?;
            }
        }
    }
}
