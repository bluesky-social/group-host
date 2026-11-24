//! What the host's operators, the accounts `HOST_ADMINS` names, may read
//! of it: the lines it logged (`org.atgrp.internal.getLogs`) and how it is
//! used (`org.atgrp.internal.getStats`); and the writing of both, as it
//! serves.

use crate::Host;
use crate::group_api::signed_in;
use crate::logs::Asked;
use crate::rbac::INTERNAL;
use crate::space_api::forbidden;
use crate::xrpc::XrpcError;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

type Answer = Result<Json<Value>, XrpcError>;

/// How often the lines logged and the counts are written.
pub const FLUSH: Duration = Duration::from_secs(1);

/// How often the oldest lines are let go.
const PRUNE: Duration = Duration::from_secs(60);

/// The caller of `lxm`, who must be one of the host's operators.
async fn operator(host: &Host, headers: &HeaderMap, lxm: &str) -> Result<String, XrpcError> {
    let who = signed_in(host, headers, lxm).await?;
    if !host.config.host_admins.contains(&who) {
        return Err(forbidden("only this host's operators may read this"));
    }
    Ok(who)
}

/// The lines `asked` names, oldest first. The read is itself logged,
/// `logs read`, with who asked and for what.
pub async fn get_logs(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Query(asked): Query<Asked>,
) -> Answer {
    let did = operator(&host, &headers, &format!("{INTERNAL}.getLogs")).await?;
    tracing::info!(did, asked = asked.said(), "logs read");
    // What was logged up to now, this read's own line with it.
    let read = host.logs.flush().and_then(|()| host.logs.read(&asked));
    let lines = read.map_err(XrpcError::internal)?;
    let lines: Vec<Value> = lines
        .into_iter()
        .map(|line| json!({"line": line}))
        .collect();
    Ok(Json(json!({ "lines": lines })))
}

#[derive(Deserialize)]
pub struct StatsAsked {
    days: Option<u32>,
}

/// How the host was used over the last `days` days (7 unless asked),
/// and what it holds.
pub async fn get_stats(
    State(host): State<Arc<Host>>,
    headers: HeaderMap,
    Query(asked): Query<StatsAsked>,
) -> Answer {
    operator(&host, &headers, &format!("{INTERNAL}.getStats")).await?;
    let counted = host.stats.flush().map_err(anyhow::Error::from);
    let days = asked.days.unwrap_or(7);
    let now = crate::time::now_millis();
    let report = counted.and_then(|()| host.stats.report(&host.store, days, now));
    Ok(Json(report.map_err(XrpcError::internal)?))
}

/// Writes the lines logged and the counts every `every`, and lets go of
/// the oldest lines every [`PRUNE`], at once the first time.
pub async fn keeping(host: Arc<Host>, every: Duration) {
    let mut pruned: Option<Instant> = None;
    loop {
        wrote("logs", host.logs.flush());
        wrote("stats", host.stats.flush());
        if pruned.is_none_or(|t| t.elapsed() >= PRUNE) {
            wrote("logs", host.logs.prune());
            pruned = Some(Instant::now());
        }
        tokio::time::sleep(every).await;
    }
}

/// Says, to stderr alone, why `what` was not written: it is written
/// with the next.
pub fn wrote(what: &str, done: rusqlite::Result<()>) {
    if let Err(err) = done {
        eprintln!("{what} not kept: {err:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_was_not_kept() {
        wrote("logs", Ok(()));
        wrote("logs", Err(rusqlite::Error::InvalidQuery));
    }
}
