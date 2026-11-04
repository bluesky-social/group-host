//! An app signing in to the host's OAuth server, as atproto's clients do:
//! a DPoP key, the nonce the server last gave, PKCE, and a pushed request.

#![allow(dead_code)]

use super::Running;
use super::groups::{NS, auth, post};
use super::spaces::{Authority, now};
use atproto::jose;
use atproto::{Curve, Keypair};
use reqwest::header::HeaderMap;
use serde_json::{Value as Json, json};
use std::sync::Mutex;

pub const REDIRECT: &str = "http://127.0.0.1/callback";

/// A loopback client asking for `scope`, redirecting to [`REDIRECT`].
pub fn loopback_id(scope: &str) -> String {
    let q = reqwest::Url::parse_with_params(
        "http://localhost",
        &[("redirect_uri", REDIRECT), ("scope", scope)],
    )
    .unwrap();
    q.to_string()
}

/// Fields a confidential client signs each request to the server with.
pub type Assert = Box<dyn Fn(&Running) -> Vec<(String, String)> + Send + Sync>;

pub struct App {
    pub id: String,
    pub key: Keypair,
    pub nonce: Mutex<Option<String>>,
    pub verifier: String,
    /// Fields every request to the server carries: a confidential
    /// client's assertion, made fresh each time.
    pub assert: Option<Assert>,
}

/// What the server answered: its status, body and headers.
pub struct Said {
    pub status: u16,
    pub body: Json,
    pub headers: HeaderMap,
}

impl Said {
    pub fn error(&self) -> (u16, &str) {
        (self.status, self.body["error"].as_str().unwrap_or_default())
    }
}

impl App {
    pub fn new(id: &str) -> App {
        App {
            id: id.into(),
            key: Keypair::generate(Curve::P256),
            nonce: Mutex::new(None),
            verifier: jose::random(32),
            assert: None,
        }
    }

    pub fn loopback(scope: &str) -> App {
        App::new(&loopback_id(scope))
    }

    /// A proof of `method` to `path`, as `claims` change it.
    pub fn proof_with(
        &self,
        h: &Running,
        method: &str,
        path: &str,
        change: impl Fn(&mut Json),
    ) -> String {
        let mut claims = json!({
            "jti": jose::random(12),
            "htm": method,
            "htu": format!("{}{path}", h.host.config.public_url),
            "iat": now(),
        });
        if let Some(nonce) = self.nonce.lock().unwrap().clone() {
            claims["nonce"] = json!(nonce);
        }
        change(&mut claims);
        let header = json!({"typ": "dpop+jwt", "jwk": jose::jwk(&self.key.public())});
        jose::sign(&self.key, &header, &claims)
    }

    /// POSTs `form` to `path` once, with `proof` if any.
    pub async fn send(
        &self,
        h: &Running,
        path: &str,
        form: &[(&str, &str)],
        proof: Option<String>,
    ) -> Said {
        let mut fields: Vec<(String, String)> = vec![("client_id".into(), self.id.clone())];
        if let Some(assert) = &self.assert {
            fields.extend(assert(h));
        }
        for (k, v) in form {
            fields.retain(|(f, _)| f != k);
            fields.push((k.to_string(), v.to_string()));
        }
        fields.retain(|(_, v)| v != "\u{0}");
        let mut req = h.http.post(format!("{}{path}", h.url)).form(&fields);
        if let Some(proof) = proof {
            req = req.header("dpop", proof);
        }
        let res = req.send().await.unwrap();
        let (status, headers) = (res.status().as_u16(), res.headers().clone());
        if let Some(nonce) = headers.get("dpop-nonce") {
            *self.nonce.lock().unwrap() = Some(nonce.to_str().unwrap().into());
        }
        let body = res.json().await.unwrap_or(Json::Null);
        Said {
            status,
            body,
            headers,
        }
    }

    /// POSTs `form` to `path` with a fresh proof, once more with the
    /// server's nonce if it asks for one. A field given as "\0" is left out.
    pub async fn post(&self, h: &Running, path: &str, form: &[(&str, &str)]) -> Said {
        let said = self
            .send(
                h,
                path,
                form,
                Some(self.proof_with(h, "POST", path, |_| {})),
            )
            .await;
        if said.error() != (400, "use_dpop_nonce") {
            return said;
        }
        self.send(
            h,
            path,
            form,
            Some(self.proof_with(h, "POST", path, |_| {})),
        )
        .await
    }

    /// The form of a pushed request, as `form` changes it.
    pub fn request<'a>(
        &'a self,
        challenge: &'a str,
        form: &[(&'a str, &'a str)],
    ) -> Vec<(&'a str, &'a str)> {
        let mut fields = vec![
            ("response_type", "code"),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", "st4te"),
        ];
        for (k, v) in form {
            fields.retain(|(f, _)| f != k);
            fields.push((k, v));
        }
        fields
    }

    /// Pushes a plain request: what the server said.
    pub async fn push_said(&self, h: &Running) -> Said {
        let challenge = jose::sha256(&self.verifier);
        self.post(h, "/oauth/par", &self.request(&challenge, &[]))
            .await
    }

    /// Pushes a request, as `form` changes it: its request_uri.
    pub async fn push(&self, h: &Running, form: &[(&str, &str)]) -> String {
        let challenge = jose::sha256(&self.verifier);
        let said = self
            .post(h, "/oauth/par", &self.request(&challenge, form))
            .await;
        assert_eq!(said.status, 201, "{}", said.body);
        said.body["request_uri"].as_str().unwrap().to_string()
    }

    /// Exchanges the code a redirect carries.
    pub async fn exchange(&self, h: &Running, redirect: &str) -> Said {
        let code = param(redirect, "code").unwrap();
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", REDIRECT),
            ("code_verifier", self.verifier.as_str()),
        ];
        self.post(h, "/oauth/token", &form).await
    }

    pub async fn refresh(&self, h: &Running, refresh_token: &str) -> Said {
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ];
        self.post(h, "/oauth/token", &form).await
    }

    /// Signs in as `group`, `member` approving: the token response.
    pub async fn sign_in(&self, h: &Running, member: &Authority, group: &str) -> Json {
        let request_uri = self.push(h, &[]).await;
        let (status, approved) = approve(h, member, &request_uri, group).await;
        assert_eq!(status, 200, "{approved}");
        let said = self
            .exchange(h, approved["redirectUrl"].as_str().unwrap())
            .await;
        assert_eq!(said.status, 200, "{}", said.body);
        said.body
    }
}

/// `member` approves `request_uri` for `group`.
pub async fn approve(
    h: &Running,
    member: &Authority,
    request_uri: &str,
    group: &str,
) -> (u16, Json) {
    let lxm = format!("{NS}.approveAuthorization");
    let body = json!({"requestUri": request_uri, "group": group});
    post(h, &lxm, &body, &[auth(h, member, &lxm)]).await
}

/// The value of `name` in a URL's query, or its fragment's.
pub fn param(url: &str, name: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).unwrap();
    let fragment = url.fragment().unwrap_or_default();
    let from_fragment = reqwest::Url::parse(&format!("http://x/?{fragment}")).unwrap();
    url.query_pairs()
        .chain(from_fragment.query_pairs().collect::<Vec<_>>())
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.to_string())
}

/// Runs `sql` on the host's database, as time passing would.
pub fn age(h: &Running, sql: &str) {
    let conn = rusqlite::Connection::open(h.data.path().join("host.sqlite")).unwrap();
    conn.execute(sql, []).unwrap();
}
