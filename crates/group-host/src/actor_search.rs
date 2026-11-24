//! Finding an account by what is typed of its handle, for the console's
//! handle fields (`fyi.opensocial.internal.searchActors`), anyone may ask, as the
//! sign-in field does before anyone has.
//!
//! Where it looks is `ACTOR_SEARCH`. On the network everyone is on, a
//! typeahead service that knows all of it (typeahead.waow.tech), asked by
//! the host so that what is typed goes from one server, not each browser.
//! On a network of its own, whose directory is not plc.directory, the
//! handles its directory names: every DID's `alsoKnownAs` in the
//! directory's `/export`, read a page at a time and kept for a minute.

use crate::Host;
use crate::api::Params;
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the directory's handles are kept before it is read again.
pub const KEPT_FOR: Duration = Duration::from_secs(60);

/// The most pages of the directory's export read at once, each as many
/// operations as it sends: a network of its own, not the whole one.
const PAGES_AT_MOST: usize = 50;

/// Each DID a directory names a handle for, and the handle.
type Named = Arc<Vec<(String, String)>>;

/// The handles a directory names, as last read, and when. Held while the
/// directory is read again, so callers at once share one read of it.
#[derive(Default)]
pub struct Handles(tokio::sync::Mutex<Option<(Instant, Named)>>);

/// The handle an operation of the directory's export names, if it is one
/// that names one: a tombstone, or an update naming none, names none.
fn handle_of(op: &Value) -> Option<String> {
    let aka = op["alsoKnownAs"].as_array().into_iter().flatten();
    let named = aka
        .filter_map(Value::as_str)
        .find_map(|a| a.strip_prefix("at://"));
    named.map(str::to_ascii_lowercase)
}

impl Host {
    /// Every DID the directory names a handle for, and the handle: from the
    /// last read, if it is less than [`KEPT_FOR`] old. A read that never
    /// reached the directory is not kept, so the next caller tries again.
    async fn directory_handles(&self) -> Named {
        let mut kept = self.handles.0.lock().await;
        if let Some((_, handles)) = kept.as_ref().filter(|(at, _)| at.elapsed() < KEPT_FOR) {
            return handles.clone();
        }
        let mut reached = false;
        let mut last: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut after = String::new();
        for _ in 0..PAGES_AT_MOST {
            let url = format!("{}/export", self.config.plc_url);
            let asked = self
                .http
                .get(url)
                .query(&[("count", "1000"), ("after", &after)]);
            let Ok(res) = asked.send().await else {
                break;
            };
            reached = true;
            let text = res.text().await.unwrap_or_default();
            let lines: Vec<Value> = text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            let Some(end) = lines.last() else { break };
            after = end["createdAt"].as_str().unwrap_or_default().to_string();
            let live = lines.iter().filter(|e| e["nullified"] != true);
            for entry in live {
                let did = entry["did"].as_str().unwrap_or_default().to_string();
                last.insert(did, handle_of(&entry["operation"]));
            }
        }
        let handles: Vec<(String, String)> = last
            .into_iter()
            .filter_map(|(d, h)| Some((d, h?)))
            .collect();
        let handles = Arc::new(handles);
        if reached {
            *kept = Some((Instant::now(), handles.clone()));
        }
        handles
    }

    /// Who the directory names whose handle begins with `q`, then whose
    /// handle holds it, shortest first: at most `limit`.
    async fn in_directory(&self, q: &str, limit: usize) -> Vec<Value> {
        let handles = self.directory_handles().await;
        let mut found: Vec<(bool, &String, &String)> = handles
            .iter()
            .filter(|(_, h)| h.contains(q))
            .map(|(d, h)| (!h.starts_with(q), h, d))
            .collect();
        found.sort_by_key(|(later, h, _)| (*later, h.len(), *h));
        let found = found.into_iter().take(limit);
        found
            .map(|(_, h, d)| json!({"did": d, "handle": h}))
            .collect()
    }

    /// Who the typeahead service at `url` says matches `q`: none, when it
    /// cannot say.
    async fn asking_service(&self, url: &str, q: &str, limit: usize) -> Vec<Value> {
        let limit = limit.to_string();
        let asked = self.http.get(url).query(&[("q", q), ("limit", &limit)]);
        let said: Value = match asked.send().await {
            Ok(res) => res.json().await.unwrap_or_default(),
            Err(_) => Value::Null,
        };
        let actors = said["actors"].as_array().into_iter().flatten();
        let shown = actors.filter(|a| {
            a["did"].is_string() && a["handle"].as_str().is_some_and(|h| h != "handle.invalid")
        });
        let kept = |a: &Value| {
            let mut out = json!({"did": a["did"], "handle": a["handle"]});
            for key in ["displayName", "avatar"] {
                if a[key].is_string() {
                    out[key] = a[key].clone();
                }
            }
            out
        };
        shown.map(kept).collect()
    }
}

pub async fn search_actors(
    State(host): State<Arc<Host>>,
    Query(p): Query<Params>,
) -> Result<Json<Value>, XrpcError> {
    let q = p
        .get("q")
        .map_or("", |q| q.trim().trim_start_matches('@'))
        .to_ascii_lowercase();
    // Its lexicon has checked it is a number, from 1 to 25.
    let limit = p.get("limit").and_then(|l| l.parse().ok()).unwrap_or(6);
    let actors = match (q.chars().count() >= 2, host.config.actor_search.as_str()) {
        (false, _) => Vec::new(),
        (true, "plc") => host.in_directory(&q, limit).await,
        (true, url) => host.asking_service(url, &q, limit).await,
    };
    Ok(Json(json!({ "actors": actors })))
}
