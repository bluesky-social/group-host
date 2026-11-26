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
    assert_eq!(c.did_web_scheme, "https");
    assert!(c.group_founders.is_empty());
    assert_eq!(
        c.handle_resolvers,
        ["well-known:https", "https://public.api.bsky.app"]
    );
    assert_eq!(c.console_url, "http://localhost:2600");
    assert!(c.provisioning_clients.is_empty());
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

    let c = config(&[
        ("SERVICE_DID", "did:web:other.example"),
        ("DID_WEB_SCHEME", "http"),
        ("GROUP_FOUNDERS", "did:plc:a, did:plc:b,"),
        ("HANDLE_RESOLVERS", "https://resolver.example/, /"),
        ("PDS_CRAWLERS", "https://relay.example/"),
        ("DID_WEB_HOSTS", "grain.social=127.0.0.1:2584, "),
    ])
    .unwrap();
    // A service's URL is taken without its trailing slash.
    assert_eq!(c.handle_resolvers, ["https://resolver.example"]);
    assert_eq!(c.crawlers, ["https://relay.example"]);
    assert_eq!(c.group_founders, ["did:plc:a", "did:plc:b"]);
    assert_eq!(c.service_did, "did:web:other.example");
    assert_eq!(c.did_web_scheme, "http");
    assert_eq!(
        c.did_web_hosts,
        [("grain.social".to_string(), "127.0.0.1:2584".to_string())].into()
    );
    let c = config(&[
        ("CONSOLE_URL", "https://console.example.com/"),
        (
            "PROVISIONING_CLIENTS",
            "https://app.example/client.json, https://app.example/, *",
        ),
    ])
    .unwrap();
    assert_eq!(c.console_url, "https://console.example.com");
    // A client id is as written: one may end in a slash.
    assert_eq!(
        c.provisioning_clients,
        [
            "https://app.example/client.json",
            "https://app.example/",
            "*"
        ]
    );
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
        err(&[("DID_WEB_SCHEME", "ftp")]),
        "DID_WEB_SCHEME is https or http, not ftp"
    );
    assert_eq!(
        err(&[("DID_WEB_HOSTS", "grain.social")]),
        "DID_WEB_HOSTS is host=address pairs, not grain.social"
    );
    assert_eq!(
        err(&[("HANDLE_DOMAIN", "example.com")]),
        "HANDLE_DOMAIN starts with a dot: .example.com"
    );
    assert_eq!(
        err(&[("CONSOLE_URL", "console")]),
        "CONSOLE_URL is no URL: console"
    );
}

#[test]
fn a_handle_is_looked_for_where_the_directory_is() {
    let typeahead = "https://typeahead.waow.tech/xrpc/tech.waow.typeahead.searchActors";
    assert_eq!(config(&[]).unwrap().actor_search, typeahead);
    let own = config(&[("PLC_URL", "http://plc.next.test")]).unwrap();
    assert_eq!(own.actor_search, "plc");
    let told = [
        ("PLC_URL", "http://plc.next.test"),
        ("ACTOR_SEARCH", "https://find.test/x"),
    ];
    assert_eq!(config(&told).unwrap().actor_search, "https://find.test/x");
    assert_eq!(
        config(&[("ACTOR_SEARCH", "plc")]).unwrap().actor_search,
        "plc"
    );
    let refused = config(&[("ACTOR_SEARCH", "somewhere")]).unwrap_err();
    assert!(refused.to_string().contains("ACTOR_SEARCH"), "{refused}");
}

#[test]
fn someone_signs_up_at_bluesky_unless_told() {
    assert_eq!(config(&[]).unwrap().signup_pds, "https://bsky.social");
    let told = config(&[("SIGNUP_PDS", "https://pds.next.test/")]).unwrap();
    assert_eq!(told.signup_pds, "https://pds.next.test");
    let refused = config(&[("SIGNUP_PDS", "not a url")]).unwrap_err();
    assert!(refused.to_string().contains("SIGNUP_PDS"), "{refused}");
}

#[test]
fn its_admins_and_how_many_lines_it_keeps() {
    let c = config(&[]).unwrap();
    assert!(c.host_admins.is_empty());
    assert_eq!(c.logs_keep, 500_000);
    let c = config(&[
        ("HOST_ADMINS", "did:plc:a, did:plc:b,"),
        ("LOGS_KEEP", "10"),
    ])
    .unwrap();
    assert_eq!(c.host_admins, ["did:plc:a", "did:plc:b"]);
    assert_eq!(c.logs_keep, 10);
    let refused = config(&[("LOGS_KEEP", "many")]).unwrap_err();
    assert_eq!(refused.to_string(), "LOGS_KEEP is no number of lines: many");
}

#[test]
fn rate_limits_are_on_unless_turned_off() {
    assert!(config(&[]).unwrap().rate_limits);
    assert!(config(&[("RATE_LIMITS", "on")]).unwrap().rate_limits);
    assert!(!config(&[("RATE_LIMITS", "off")]).unwrap().rate_limits);
    let refused = config(&[("RATE_LIMITS", "maybe")]).unwrap_err();
    assert_eq!(refused.to_string(), "RATE_LIMITS is on or off, not maybe");
}
