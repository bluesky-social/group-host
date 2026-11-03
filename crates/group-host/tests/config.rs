//! The config the environment gives, and what it refuses.

use group_host::config::Config;
use std::collections::HashMap;

fn config(vars: &[(&str, &str)]) -> anyhow::Result<Config> {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Config::from(|name| vars.get(name).cloned())
}

#[test]
fn defaults_serve_localhost() {
    let c = config(&[]).unwrap();
    assert_eq!(c.port, 2600);
    assert_eq!(c.public_url, "http://localhost:2600");
    assert_eq!(c.service_did, "did:web:localhost%3A2600");
    assert_eq!(c.handle_domain, ".test");
    assert_eq!(c.data_dir, std::path::PathBuf::from("data"));
}

#[test]
fn the_environment_names_the_host() {
    let c = config(&[
        ("PORT", "8080"),
        ("PUBLIC_URL", "https://groups.example.com/"),
        ("HANDLE_DOMAIN", ".groups.example.com"),
        ("DATA_DIR", "/var/lib/group-host"),
    ])
    .unwrap();
    assert_eq!(c.port, 8080);
    assert_eq!(c.public_url, "https://groups.example.com");
    assert_eq!(c.service_did, "did:web:groups.example.com");
    assert_eq!(c.handle_domain, ".groups.example.com");

    let c = config(&[("SERVICE_DID", "did:web:other.example")]).unwrap();
    assert_eq!(c.service_did, "did:web:other.example");
}

#[test]
fn refuses_what_does_not_read() {
    let err = |vars| config(vars).unwrap_err().to_string();
    assert_eq!(err(&[("PORT", "eighty")]), "PORT is no port: eighty");
    assert_eq!(
        err(&[("PUBLIC_URL", "ftp://example.com")]),
        "PUBLIC_URL is no origin: ftp://example.com"
    );
    assert_eq!(
        err(&[("PUBLIC_URL", "https://example.com/path")]),
        "PUBLIC_URL is no origin: https://example.com/path"
    );
    assert_eq!(
        err(&[("BLOB_UPLOAD_LIMIT", "lots")]),
        "BLOB_UPLOAD_LIMIT is no number of bytes: lots"
    );
    assert_eq!(
        err(&[("HANDLE_DOMAIN", "example.com")]),
        "HANDLE_DOMAIN starts with a dot: .example.com"
    );
}
