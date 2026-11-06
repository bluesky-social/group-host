//! Blobs: uploaded, made permanent by a record that references them,
//! served and listed, and gone once nothing references them.

mod common;

use atproto::{Cid, Value, drisl};
use common::{Running, start};
use group_host::blobs::{TEMPORARY_FOR, blob_refs};
use group_host::repo::Write;
use group_host::time::now_millis;
use serde_json::{Value as Json, json};

const RIDERS: &str = "riders.groups.test";

fn profile(rkey: &str, avatar: &Json) -> Write {
    Write::Create {
        collection: "fyi.opensocial.profile".into(),
        rkey: rkey.into(),
        value: json!({"$type": "fyi.opensocial.profile", "displayName": "Riders", "avatar": avatar}),
    }
}

async fn blob(h: &Running, did: &str, cid: &str) -> (u16, reqwest::header::HeaderMap, Vec<u8>) {
    let url = format!(
        "{}/xrpc/com.atproto.sync.getBlob?did={did}&cid={cid}",
        h.url
    );
    let res = h.http.get(url).send().await.unwrap();
    (
        res.status().as_u16(),
        res.headers().clone(),
        res.bytes().await.unwrap().to_vec(),
    )
}

fn link(r: &Json) -> String {
    r["ref"]["$link"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn serves_a_blob_once_a_record_references_it() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let picture = b"\x89PNG not really".to_vec();
    let r = h.host.upload_blob(&did, &picture, "image/png").unwrap();
    assert_eq!(r["mimeType"], "image/png");
    assert_eq!(r["size"], picture.len());
    let cid = link(&r);
    assert_eq!(cid, Cid::of_raw(&picture).to_string());

    // Uploaded and not yet referenced: not served, not listed.
    let (status, _, body) = blob(&h, &did, &cid).await;
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_slice::<Json>(&body).unwrap()["error"],
        "BlobNotFound"
    );

    h.host.write(&did, &[profile("self", &r)]).unwrap();
    let (status, headers, body) = blob(&h, &did, &cid).await;
    assert_eq!(status, 200);
    assert_eq!(body, picture);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; sandbox"
    );
    let (_, listed) = h
        .get("com.atproto.sync.listBlobs", &format!("did={did}"))
        .await;
    assert_eq!(listed, json!({"cids": [cid], "cursor": null}));

    // The commit's event names the blob.
    let events = h.host.store.events_after(0, 100).unwrap();
    let body = drisl::decode(&events.last().unwrap().body)
        .unwrap()
        .into_map()
        .unwrap();
    assert_eq!(
        body["blobs"],
        Value::Array(vec![Value::Link(Cid::parse(&cid).unwrap())])
    );
}

#[tokio::test]
async fn keeps_a_blob_while_any_record_references_it() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let r = h.host.upload_blob(&did, b"shared", "image/png").unwrap();
    let cid = link(&r);
    h.host
        .write(&did, &[profile("a", &r), profile("b", &r)])
        .unwrap();
    h.host
        .write(
            &did,
            &[Write::Delete {
                collection: "fyi.opensocial.profile".into(),
                rkey: "a".into(),
            }],
        )
        .unwrap();
    assert_eq!(blob(&h, &did, &cid).await.0, 200);
    h.host
        .write(
            &did,
            &[Write::Delete {
                collection: "fyi.opensocial.profile".into(),
                rkey: "b".into(),
            }],
        )
        .unwrap();
    assert_eq!(blob(&h, &did, &cid).await.0, 400);
    let file = h
        .data
        .path()
        .join("blobs")
        .join(did.replace(':', "_"))
        .join(&cid);
    assert!(!file.exists());
}

#[tokio::test]
async fn lists_blobs_a_page_at_a_time_and_since_a_rev() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let a = h.host.upload_blob(&did, b"one", "").unwrap();
    assert_eq!(a["mimeType"], "application/octet-stream");
    let first = h.host.write(&did, &[profile("a", &a)]).unwrap();
    let b = h.host.upload_blob(&did, b"two", "image/png").unwrap();
    h.host.write(&did, &[profile("b", &b)]).unwrap();
    let mut both = [link(&a), link(&b)];
    both.sort();

    let (_, page) = h
        .get("com.atproto.sync.listBlobs", &format!("did={did}&limit=1"))
        .await;
    assert_eq!(page["cids"], json!([both[0]]));
    let cursor = page["cursor"].as_str().unwrap();
    let (_, page) = h
        .get(
            "com.atproto.sync.listBlobs",
            &format!("did={did}&limit=1&cursor={cursor}"),
        )
        .await;
    assert_eq!(page["cids"], json!([both[1]]));
    // A page is up to 1000, as atproto's lexicon has it.
    let (_, page) = h
        .get(
            "com.atproto.sync.listBlobs",
            &format!("did={did}&limit=1000"),
        )
        .await;
    assert_eq!(page["cids"].as_array().unwrap().len(), 2);
    let (_, refused) = h
        .get(
            "com.atproto.sync.listBlobs",
            &format!("did={did}&limit=1001"),
        )
        .await;
    assert_eq!(refused["message"], "limit is 1 to 1000");
    let (_, since) = h
        .get(
            "com.atproto.sync.listBlobs",
            &format!("did={did}&since={}", first.rev),
        )
        .await;
    assert_eq!(since["cids"], json!([link(&b)]));
}

#[tokio::test]
async fn refuses_what_it_cannot_keep_or_find() {
    let h = start(&[("BLOB_UPLOAD_LIMIT", "4")], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    assert_eq!(
        h.host
            .upload_blob(&did, b"too big", "image/png")
            .unwrap_err()
            .error,
        "BlobTooLarge"
    );
    assert_eq!(
        h.host
            .upload_blob("did:plc:nobody", b"x", "image/png")
            .unwrap_err()
            .error,
        "RepoNotFound"
    );
    let stranger = json!({"$type": "blob", "ref": {"$link": Cid::of_raw(b"elsewhere").to_string()}, "mimeType": "image/png", "size": 9});
    let refused = h
        .host
        .write(&did, &[profile("self", &stranger)])
        .unwrap_err();
    assert!(
        refused.message.contains("was not uploaded to"),
        "{}",
        refused.message
    );

    for (nsid, query, error) in [
        (
            "com.atproto.sync.getBlob",
            "did=did:plc:nobody&cid=x",
            "RepoNotFound",
        ),
        (
            "com.atproto.sync.getBlob",
            &format!("did={did}&cid=x") as &str,
            "InvalidRequest",
        ),
        (
            "com.atproto.sync.listBlobs",
            "did=did:plc:nobody",
            "RepoNotFound",
        ),
    ] {
        let (_, body) = h.get(nsid, query).await;
        assert_eq!(body["error"], error, "{nsid}?{query}");
    }
}

#[tokio::test]
async fn sweeps_uploads_no_record_took() {
    let h = start(&[], false).await;
    let did = h.host.create_account(RIDERS).await.unwrap();
    let kept = h.host.upload_blob(&did, b"kept", "image/png").unwrap();
    h.host.write(&did, &[profile("self", &kept)]).unwrap();
    let left = h.host.upload_blob(&did, b"left", "image/png").unwrap();
    assert_eq!(h.host.sweep_blobs(now_millis()).unwrap(), 0);
    assert_eq!(
        h.host
            .sweep_blobs(now_millis() + TEMPORARY_FOR + 1000)
            .unwrap(),
        1
    );
    let dir = h.data.path().join("blobs").join(did.replace(':', "_"));
    assert!(!dir.join(link(&left)).exists());
    assert!(dir.join(link(&kept)).exists());
}

#[test]
fn finds_every_blob_a_record_references() {
    let a = Cid::of_raw(b"a");
    let b = Cid::of_raw(b"b");
    let blob = |c: &Cid| json!({"$type": "blob", "ref": {"$link": c.to_string()}});
    let record = json!({
        "avatar": blob(&a),
        "gallery": [blob(&b), blob(&a), {"$type": "blob", "ref": {"$link": "not a cid"}}],
        "other": {"$type": "fyi.opensocial.rule", "ref": {"$link": b.to_string()}},
        "n": 1,
    });
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(blob_refs(&record), want);
}
