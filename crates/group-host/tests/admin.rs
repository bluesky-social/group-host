//! What the host's operators read of it: the lines it logged, and how it
//! is used.

mod common;

use common::groups::*;
use common::{Running, start};
use serde_json::{Value as Json, json};

const LOGS: &str = "fyi.opensocial.getLogs";
const STATS: &str = "fyi.opensocial.getStats";

/// A host whose operator is the first of three people, who sign in at a
/// directory the host reads: the people, and the host.
async fn operated() -> ([common::spaces::Authority; 3], Running) {
    let directory = start(&[], false).await;
    let people = people(&directory).await;
    let plc = directory.host.config.plc_url.clone();
    let h = start(&[("PLC_URL", &plc), ("HOST_ADMINS", &people[0].did)], false).await;
    (people, h)
}

/// A line as the host logs one, in `trace`.
fn line(message: &str, trace: &str) -> Vec<u8> {
    let v = json!({"level": "INFO", "fields": {"message": message},
        "spans": [{"name": "request", "trace": trace}]});
    format!("{v}\n").into_bytes()
}

#[tokio::test]
async fn only_its_operators_read_its_logs_and_stats() {
    let ([alice, bob, _], h) = operated().await;
    for lxm in [LOGS, STATS] {
        let (status, said) = get(&h, lxm, "", &[]).await;
        assert_eq!((status, said["error"].as_str()), (401, Some("AuthMissing")));
        let (status, said) = get(&h, lxm, "", &[auth(&h, &bob, lxm)]).await;
        assert_eq!((status, said["error"].as_str()), (403, Some("Forbidden")));
        let (status, said) = get(&h, lxm, "", &[auth(&h, &alice, lxm)]).await;
        assert_eq!(status, 200, "{said}");
    }
}

#[tokio::test]
async fn reads_one_requests_lines_by_its_trace() {
    let ([alice, _, _], h) = operated().await;
    h.host.logs.keep(&line("request", "aaaa"));
    h.host.logs.keep(&line("indexed", "aaaa"));
    h.host.logs.keep(&line("request", "bbbb"));
    let (status, said) = get(&h, LOGS, "trace=aaaa", &[auth(&h, &alice, LOGS)]).await;
    assert_eq!(status, 200, "{said}");
    let messages: Vec<&Json> = said["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| &l["line"]["fields"]["message"])
        .collect();
    assert_eq!(messages, ["request", "indexed"]);
    let asked = "message=request&level=info&limit=1";
    let (_, said) = get(&h, LOGS, asked, &[auth(&h, &alice, LOGS)]).await;
    assert_eq!(said["lines"].as_array().unwrap().len(), 1);
    // A read asking what the lexicon does not take is refused before it.
    let (status, said) = get(&h, LOGS, "limit=501", &[auth(&h, &alice, LOGS)]).await;
    assert_eq!(
        (status, said["error"].as_str()),
        (400, Some("InvalidRequest"))
    );
}

#[tokio::test]
async fn counts_each_request_by_its_route_and_what_the_host_holds() {
    let ([alice, _, _], h) = operated().await;
    start_group(
        &h,
        &alice,
        json!({"handle": "riders.groups.test", "displayName": "Riders"}),
    )
    .await;
    // The health checks are not counted; a path no route answers is.
    let traced = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
    let res = h
        .http
        .get(format!("{}/xrpc/_health", h.url))
        .header("traceparent", traced)
        .send()
        .await
        .unwrap();
    // The trace a request names is the one it is answered in.
    assert_eq!(
        res.headers()["x-trace-id"],
        "0af7651916cd43dd8448eb211c80319c"
    );
    let (status, _) = h.get_path("/nowhere").await;
    assert_eq!(status, 404);
    // A method of the client's making is counted as one, whatever it is.
    for method in ["BREW", "WHEN"] {
        let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let asked = h
            .http
            .request(method, format!("{}/xrpc/fyi.opensocial.getGroup", h.url));
        asked.send().await.unwrap();
    }

    let (status, said) = get(&h, STATS, "days=2", &[auth(&h, &alice, STATS)]).await;
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["host"]["groups"], 1);
    assert_eq!(said["host"]["spaces"], 2);
    let today = &said["days"][1];
    assert_eq!(today["groupsMade"], 1);
    assert_eq!(today["spacesMade"], 2);
    let routes = said["routes"].as_array().unwrap();
    let route = |r: &str| routes.iter().find(|x| x["route"] == r).cloned();
    let made = route("/xrpc/fyi.opensocial.createGroup").unwrap();
    assert_eq!(
        (made["method"].as_str(), made["requests"].as_i64()),
        (Some("POST"), Some(1))
    );
    let unmatched = route("(unmatched)").unwrap();
    assert_eq!(unmatched["refused"], 1);
    let others: Vec<&Json> = routes.iter().filter(|x| x["method"] == "OTHER").collect();
    assert_eq!(others.len(), 1, "{routes:?}");
    assert_eq!(others[0]["requests"], 2);
    assert_eq!(others[0]["route"], "/xrpc/fyi.opensocial.getGroup");
    assert_eq!(route("/xrpc/_health"), None);
    // A week unless asked.
    let (_, said) = get(&h, STATS, "", &[auth(&h, &alice, STATS)]).await;
    assert_eq!(said["days"].as_array().unwrap().len(), 7);
}

/// The lines and counts are written as the host serves, and the oldest
/// lines let go.
#[tokio::test]
async fn keeps_what_it_logs_and_counts_as_it_goes() {
    let h = start(&[("LOGS_KEEP", "1")], false).await;
    h.host.logs.keep(&line("first", "cccc"));
    h.host.logs.keep(&line("second", "cccc"));
    h.host.stats.tool("group_show", true);
    let every = std::time::Duration::from_millis(10);
    let going = tokio::spawn(group_host::admin::keeping(h.host.clone(), every));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    going.abort();
    let asked = group_host::logs::Asked::default();
    let kept = h.host.logs.read(&asked).unwrap();
    assert_eq!(kept.len(), 1, "{kept:?}");
    let now = group_host::time::now_millis();
    let report = h.host.stats.report(&h.host.store, 1, now).unwrap();
    assert_eq!(report["days"][0]["toolCalls"], 1);
}
