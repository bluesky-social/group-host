//! The host as it runs: the built binary, answering over HTTP.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

/// The binary, listening on a free port, and its address.
struct Running(Child, String);

impl Running {
    /// Tells it to stop, as a deploy does, and waits for it to: it then
    /// exits as it should, and writes what coverage it counted.
    fn stop(mut self) -> std::process::ExitStatus {
        let pid = self.0.id().to_string();
        Command::new("kill").args(["-TERM", &pid]).status().unwrap();
        self.0.wait().unwrap()
    }
}

fn start(data: &std::path::Path) -> Running {
    start_logging(data, "off")
}

/// The binary, logging what `filter` lets through.
fn start_logging(data: &std::path::Path, filter: &str) -> Running {
    let mut child = Command::new(env!("CARGO_BIN_EXE_group-host"))
        .env("PORT", "0")
        .env("DATA_DIR", data)
        .env("RUST_LOG", filter)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line.trim().strip_prefix("listening on ").unwrap();
    let port = addr.rsplit(':').next().unwrap();
    Running(child, format!("http://127.0.0.1:{port}"))
}

#[tokio::test]
async fn answers_health_and_refuses_unknown_methods() {
    let data = tempfile::tempdir().unwrap();
    let host = start(data.path());
    let http = reqwest::Client::new();

    let health: serde_json::Value = http
        .get(format!("{}/xrpc/_health", host.1))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["version"], env!("CARGO_PKG_VERSION"));
    // The commit the image was built from (Dockerfile); none otherwise.
    assert_eq!(
        health["commit"],
        serde_json::json!(option_env!("GROUP_HOST_COMMIT"))
    );

    for req in [
        http.get(format!("{}/xrpc/fyi.opensocial.noSuchQuery", host.1)),
        http.post(format!("{}/xrpc/fyi.opensocial.noSuchProcedure", host.1)),
    ] {
        let res = req.send().await.unwrap();
        assert_eq!(res.status(), 501);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["error"], "MethodNotImplemented");
    }

    assert!(host.stop().success());
}

/// What it logs is kept, beside what it counts, for its operators to
/// read; what was logged and counted last is kept as it stops.
#[tokio::test]
async fn keeps_the_lines_it_logs_and_counts_its_requests() {
    let data = tempfile::tempdir().unwrap();
    let host = start_logging(data.path(), "info");
    let http = reqwest::Client::new();
    let asked = http.get(format!("{}/xrpc/fyi.opensocial.noSuchQuery", host.1));
    let res = asked.send().await.unwrap();
    let trace = res.headers()["x-trace-id"].to_str().unwrap().to_string();
    assert_eq!(trace.len(), 32);
    let health = http.get(format!("{}/xrpc/_health", host.1)).send().await;
    let health = health.unwrap().headers()["x-trace-id"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(host.stop().success());

    let logs = group_host::logs::Logs::open(&data.path().join("logs.sqlite"), 100).unwrap();
    let read = |trace: Option<&str>, message: Option<&str>| {
        let asked = group_host::logs::Asked {
            trace: trace.map(String::from),
            message: message.map(String::from),
            ..Default::default()
        };
        logs.read(&asked).unwrap()
    };
    let lines = read(Some(&trace), None);
    assert_eq!(lines.len(), 1, "{:?}", read(None, None));
    let fields = &lines[0]["fields"];
    assert_eq!(fields["message"], "request");
    assert_eq!(fields["route"], "/xrpc/{nsid}");
    assert_eq!(fields["path"], "/xrpc/fyi.opensocial.noSuchQuery");
    assert_eq!(fields["status"], 501);
    // The health checks are logged at debug, below what it was told.
    assert!(read(Some(&health), None).is_empty());
    for said in ["serving", "stopped"] {
        assert_eq!(read(None, Some(said)).len(), 1, "{said}");
    }

    let stats = group_host::stats::Stats::open(&data.path().join("stats.sqlite")).unwrap();
    let store = group_host::store::Store::open(&data.path().join("host.sqlite")).unwrap();
    let now = group_host::time::now_millis();
    let report = stats.report(&store, 1, now).unwrap();
    assert_eq!(report["days"][0]["requests"], 1, "{report}");
    assert_eq!(report["routes"][0]["route"], "/xrpc/{nsid}");
}

#[test]
fn refuses_a_bad_environment() {
    let out = Command::new(env!("CARGO_BIN_EXE_group-host"))
        .env("PORT", "eighty")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("PORT is no port: eighty"));
}

#[test]
fn moves_the_host_from_an_old_address() {
    let data = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_group-host"))
            .args(args)
            .env("DATA_DIR", data.path())
            .env("PUBLIC_URL", "https://new.test")
            .env("RUST_LOG", "off")
            .output()
            .unwrap()
    };
    let out = run(&["move-host", "--from", "https://old.test", "--dry-run"]);
    assert!(out.status.success());
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("0 of 0 DID documents to repoint"), "{said}");
    for wrong in [
        &["move-host"][..],
        &["move-host", "--from", "x", "--wet"],
        &["nonsense"],
    ] {
        let out = run(wrong);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage: group-host"));
    }
}
