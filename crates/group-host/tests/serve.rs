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
    let mut child = Command::new(env!("CARGO_BIN_EXE_group-host"))
        .env("PORT", "0")
        .env("DATA_DIR", data)
        .env("RUST_LOG", "off")
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

    for req in [
        http.get(format!("{}/xrpc/fyi.opensocial.listSubjects", host.1)),
        http.post(format!("{}/xrpc/fyi.opensocial.applyLabel", host.1)),
    ] {
        let res = req.send().await.unwrap();
        assert_eq!(res.status(), 501);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["error"], "MethodNotImplemented");
    }

    assert!(host.stop().success());
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
