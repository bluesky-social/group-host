//! Apps as the console shows them: each by what its OAuth client metadata
//! says of it, its name, its home page and its logo, and the logo served
//! from here, at `/img/app?id=<client_id>`, so that a reader's browser
//! never asks an app's own server for anything.
//!
//! A client's `client_uri` and `logo_uri` are shown only on the client
//! id's own origin, over https (or http, while the host is on http
//! itself), with no credentials or fragment; the loopback client names
//! neither. A logo is fetched from a public address, following no
//! redirect, at most 256 KiB, and served only when its bytes are a PNG,
//! JPEG, GIF or WebP image by their first bytes, whatever type its server
//! says: never SVG. A logo read, or found missing, is kept ten minutes.

use crate::Host;
use crate::api::Params;
use crate::index_api::capped;
use crate::time::now_millis;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use reqwest::Url;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// How long a logo read, or found missing, is kept.
const KEEP_FOR: i64 = 10 * 60_000;
/// How many apps' logos are kept at once.
const KEEP_AT_MOST: usize = 100;
/// The most bytes a logo may be.
const LOGO_AT_MOST: usize = 256 * 1024;

/// An app's logo: its type, as its bytes say, and its bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Logo {
    pub kind: &'static str,
    pub bytes: Bytes,
}

/// The logos read, by client id, and when: `None` for one not served.
#[derive(Default)]
pub struct Logos(Mutex<HashMap<String, (i64, Option<Logo>)>>);

impl Logos {
    fn get(&self, id: &str, now: i64) -> Option<Option<Logo>> {
        let kept = self.0.lock().expect("not poisoned");
        let (at, logo) = kept.get(id).filter(|(at, _)| now - at < KEEP_FOR)?;
        tracing::debug!(client = id, at, "kept");
        Some(logo.clone())
    }

    fn keep(&self, id: &str, now: i64, logo: Option<Logo>) {
        let mut kept = self.0.lock().expect("not poisoned");
        if kept.len() >= KEEP_AT_MOST {
            kept.clear();
        }
        kept.insert(id.into(), (now, logo));
    }
}

/// Whether a page may show `url`, which the metadata of the client `id`
/// names as its home page or logo: an https URL (http too, `http_here`,
/// for a host on http itself) on the client id's origin, with no
/// credentials or fragment.
pub fn shown(id: &str, url: &str, http_here: bool) -> bool {
    let (Ok(at), Ok(client)) = (Url::parse(url), Url::parse(id)) else {
        return false;
    };
    let scheme = at.scheme() == "https" || (http_here && at.scheme() == "http");
    let bare = at.username().is_empty() && at.password().is_none() && at.fragment().is_none();
    scheme && bare && at.origin() == client.origin()
}

/// The type of image `bytes` are, by their first bytes: PNG, JPEG, GIF or
/// WebP, or none.
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some("image/png"),
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

impl Host {
    /// The app `id` as a page shows it, by what `metadata`, its client
    /// metadata, says of it: its name, and its home page and logo where
    /// a page may show them.
    pub(crate) fn app_view(&self, id: &str, metadata: &Value) -> Value {
        let http_here = self.config.public_url.starts_with("http://");
        let url = |from: &str| metadata[from].as_str().filter(|u| shown(id, u, http_here));
        let fields = [
            ("name", metadata["client_name"].as_str()),
            ("uri", url("client_uri")),
            ("logo", url("logo_uri")),
        ];
        let mut view = json!({"clientId": id});
        for (k, v) in fields.into_iter().filter_map(|(k, v)| Some((k, v?))) {
            view[k] = json!(v);
        }
        view
    }

    /// The app `id` names, as a page shows it, if its client metadata can
    /// be read: an OAuth client's, named by an https URL, or the loopback
    /// client's.
    pub(crate) async fn client_view(&self, id: &str, now: i64) -> Option<Value> {
        let url = Some(id).filter(|i| i.starts_with("https://") || i.starts_with("http://"));
        let client = self.client(url?, now).await.ok()?;
        Some(self.app_view(id, &client.metadata))
    }

    /// The logo of the app `id` names, kept or read now: none for one
    /// whose metadata names no logo a page may show, or whose logo is no
    /// image.
    pub async fn logo(&self, id: &str, now: i64) -> Option<Logo> {
        if let Some(kept) = self.logos.get(id, now) {
            return kept;
        }
        let logo = self.read_logo(id, now).await;
        tracing::info!(client = id, found = logo.is_some(), "read a logo");
        self.logos.keep(id, now, logo.clone());
        logo
    }

    async fn read_logo(&self, id: &str, now: i64) -> Option<Logo> {
        let view = self.client_view(id, now).await?;
        let asked = self.guarded.get(view["logo"].as_str()?).ok()?;
        let res = asked.send().await.ok()?;
        let res = res.error_for_status().ok()?;
        let bytes = capped(res, LOGO_AT_MOST).await?;
        let kind = image_kind(&bytes)?;
        Some(Logo {
            kind,
            bytes: bytes.into(),
        })
    }
}

/// The logo of the app `?id=` names, for a page to draw: nothing for one
/// the host does not serve.
pub async fn logo(State(host): State<Arc<Host>>, Query(p): Query<Params>) -> Response {
    let id = p.get("id").map(String::as_str).unwrap_or_default();
    let Some(logo) = host.logo(id, now_millis()).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let headers = [
        (header::CONTENT_TYPE, logo.kind),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        ),
        (header::CACHE_CONTROL, "private, max-age=600"),
    ];
    (headers, logo.bytes).into_response()
}
