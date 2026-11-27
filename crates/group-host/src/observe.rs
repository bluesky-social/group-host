//! What the host logs and counts of each request: a line, `request`,
//! with its method, route, path (never its query), status and duration,
//! and for an XRPC failure, its `error` and `error_message`, in a trace of its own or the one its `traceparent` names, which every
//! line logged while answering it carries and the answer gives back as
//! `x-trace-id`; and a count in [`crate::stats`]. A trace is the
//! caller's to name, so one may name another's and have its lines read
//! with them. The health checks, asked every few seconds, are logged at
//! debug and not counted.
//!
//! The host's own calls to other servers through `atproto-client` go
//! through [`Calls`]: each attempt sent with a `traceparent` naming the
//! trace of the request it is made for, if any, and logged in it as a
//! line, `call`.

use crate::Host;
use atproto_client::sender::{self, BoxFuture, SendError, Sender};
use atproto_client::xrpc::XrpcError;
use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderValue, Method};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use std::time::Instant;
use tracing::Instrument;

tokio::task_local! {
    /// The trace of the request being answered.
    static TRACE: String;
}

/// `N` random bytes, in lower-case hex.
fn random_hex<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sends each of the host's calls through `atproto-client` in the trace of
/// the request it is made for, and logs each attempt: a line, `call`, with
/// the server's host, the path, its status or error, `resend` (how many
/// times it was sent before) and duration. A refusal an XRPC call is
/// answered with is logged too, as `refused`.
#[derive(Debug)]
pub struct Calls;

impl Sender for Calls {
    fn send<'a>(
        &'a self,
        mut req: reqwest::Request,
        next: sender::Next<'a>,
    ) -> BoxFuture<'a, Result<reqwest::Response, SendError>> {
        Box::pin(async move {
            if let Ok(trace) = TRACE.try_with(String::clone) {
                let parent = format!("00-{trace}-{}-01", random_hex::<8>());
                let parent = HeaderValue::try_from(parent).expect("hex is a header's value");
                req.headers_mut().insert("traceparent", parent);
            }
            let host = req.url().host_str().unwrap_or_default().to_string();
            let path = req.url().path().to_string();
            let (resend, started) = (next.resend(), Instant::now());
            let res = next.run(req).await;
            let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let status = res.as_ref().map(|r| r.status().as_u16()).ok();
            let error = res.as_ref().err().map(ToString::to_string);
            tracing::info!(host, path, status, error, resend, duration_ms, "call");
            res
        })
    }

    fn refused(&self, url: &reqwest::Url, refusal: &XrpcError) {
        let origin = url.origin().ascii_serialization();
        let (status, error) = (refusal.status, refusal.error.as_str());
        tracing::info!(origin, status, error, message = refusal.message, "refused");
    }
}

/// The health checks' route.
const HEALTH: &str = "/xrpc/_health";

/// The routes that stream for as long as their reader stays.
const STREAMS: [&str; 1] = ["/xrpc/com.atproto.sync.subscribeRepos"];

/// What a request no route matched is counted as.
pub const UNMATCHED: &str = "(unmatched)";

/// A request's method as counted: one of HTTP's own, or any other as
/// `OTHER`, so a client sending methods of its making makes no more
/// counts.
pub fn method_of(method: &Method) -> &str {
    const KNOWN: [Method; 7] = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::HEAD,
        Method::OPTIONS,
    ];
    if KNOWN.contains(method) {
        method.as_str()
    } else {
        "OTHER"
    }
}

/// The trace `traceparent` names, if it is one (W3C Trace Context:
/// `00-<32 hex>-<16 hex>-<2 hex>`, its trace not all zeros); else a new
/// one.
pub fn trace_from(traceparent: Option<&str>) -> String {
    let named = traceparent.and_then(|t| {
        let parts: Vec<&str> = t.trim().split('-').collect();
        let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
        let ok = matches!(parts.as_slice(), [v, trace, span, flags]
            if hex(v, 2) && hex(trace, 32) && hex(span, 16) && hex(flags, 2)
                && trace.bytes().any(|b| b != b'0'));
        ok.then(|| parts[1].to_ascii_lowercase())
    });
    named.unwrap_or_else(random_hex::<16>)
}

/// Logs and counts the request, answered in its trace.
pub async fn observe(State(host): State<Arc<Host>>, req: Request, next: Next) -> Response {
    let started = Instant::now();
    let matched = req.extensions().get::<MatchedPath>();
    let route = matched.map_or(UNMATCHED, MatchedPath::as_str).to_string();
    let method = method_of(req.method()).to_string();
    let path = req.uri().path().to_string();
    let parent = req.headers().get("traceparent");
    let trace = trace_from(parent.and_then(|v| v.to_str().ok()));
    let span = tracing::info_span!("request", trace = %trace);
    let answered = next.run(req).instrument(span.clone());
    let mut res = TRACE.scope(trace.clone(), answered).await;
    let took = started.elapsed();
    let status = res.status().as_u16();
    let id = HeaderValue::try_from(trace.as_str()).expect("hex is a header's value");
    res.headers_mut().insert("x-trace-id", id);
    let duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX);
    // Why it was refused or failed, as its answer said.
    let failure = res.extensions().get::<crate::xrpc::XrpcError>();
    let error = failure.map(|f| f.error);
    let error_message = failure.map(|f| f.message.as_str());
    span.in_scope(|| {
        if route == HEALTH {
            tracing::debug!(
                method,
                route,
                path,
                status,
                duration_ms,
                error,
                error_message,
                "request"
            );
            return;
        }
        tracing::info!(
            method,
            route,
            path,
            status,
            duration_ms,
            error,
            error_message,
            "request"
        );
        let streams = STREAMS.contains(&route.as_str());
        host.stats.served(&route, &method, status, took, streams);
    });
    res
}
