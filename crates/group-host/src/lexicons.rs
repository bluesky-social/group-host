//! The lexicons the host answers by, read once: its own methods' and
//! records' (`fyi.opensocial.*`), and the others' they refer to. Every
//! `fyi.opensocial.*` method's parameters and JSON input are checked
//! against its lexicon before its handler runs ([`check`]), and a record
//! of one of our own collections an app writes as a group before it is
//! kept ([`check_record`]).

use crate::api::invalid;
use crate::xrpc::XrpcError;
use atproto::lexicon::Lexicons;
use axum::body::Body;
use axum::extract::{Query, Request};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::sync::OnceLock;

macro_rules! lexicon {
    ($path:literal) => {
        include_str!(concat!("../../../lexicons/", $path, ".json"))
    };
}

/// Every lexicon under `lexicons/`.
pub const FILES: &[&str] = &[
    lexicon!("fyi/opensocial/acceptance"),
    lexicon!("fyi/opensocial/authManageGroup"),
    lexicon!("fyi/opensocial/authManageProfile"),
    lexicon!("fyi/opensocial/authModerate"),
    lexicon!("fyi/opensocial/authParticipate"),
    lexicon!("fyi/opensocial/cancelJoinRequest"),
    lexicon!("fyi/opensocial/createReport"),
    lexicon!("fyi/opensocial/declaration"),
    lexicon!("fyi/opensocial/defs"),
    lexicon!("fyi/opensocial/denyJoinRequest"),
    lexicon!("fyi/opensocial/getModSubjectHistory"),
    lexicon!("fyi/opensocial/invite"),
    lexicon!("fyi/opensocial/invites"),
    lexicon!("fyi/opensocial/labeler"),
    lexicon!("fyi/opensocial/leaveGroup"),
    lexicon!("fyi/opensocial/listJoinRequests"),
    lexicon!("fyi/opensocial/listModSubjects"),
    lexicon!("fyi/opensocial/members"),
    lexicon!("fyi/opensocial/membership"),
    lexicon!("fyi/opensocial/meta"),
    lexicon!("fyi/opensocial/policy"),
    lexicon!("fyi/opensocial/profile"),
    lexicon!("fyi/opensocial/requestJoin"),
    lexicon!("fyi/opensocial/resolveModSubject"),
    lexicon!("fyi/opensocial/role"),
    lexicon!("fyi/opensocial/rule"),
    lexicon!("fyi/opensocial/space"),
    lexicon!("fyi/opensocial/internal/activateGroup"),
    lexicon!("fyi/opensocial/internal/addRecoveryKey"),
    lexicon!("fyi/opensocial/internal/approveAuthorization"),
    lexicon!("fyi/opensocial/internal/cancelAccountMove"),
    lexicon!("fyi/opensocial/internal/checkAddress"),
    lexicon!("fyi/opensocial/internal/checkGroupHandle"),
    lexicon!("fyi/opensocial/internal/checkImport"),
    lexicon!("fyi/opensocial/internal/completeImport"),
    lexicon!("fyi/opensocial/internal/createAppPassword"),
    lexicon!("fyi/opensocial/internal/createGroup"),
    lexicon!("fyi/opensocial/internal/deactivateGroup"),
    lexicon!("fyi/opensocial/internal/declineAuthorization"),
    lexicon!("fyi/opensocial/internal/declineInvite"),
    lexicon!("fyi/opensocial/internal/defs"),
    lexicon!("fyi/opensocial/internal/deleteGroup"),
    lexicon!("fyi/opensocial/internal/deleteGroupDomain"),
    lexicon!("fyi/opensocial/internal/deleteLabelDefinition"),
    lexicon!("fyi/opensocial/internal/finishAccountMove"),
    lexicon!("fyi/opensocial/internal/getAccountMove"),
    lexicon!("fyi/opensocial/internal/getActor"),
    lexicon!("fyi/opensocial/internal/getActorGroups"),
    lexicon!("fyi/opensocial/internal/getActorInvites"),
    lexicon!("fyi/opensocial/internal/getAuthorization"),
    lexicon!("fyi/opensocial/internal/getGroup"),
    lexicon!("fyi/opensocial/internal/getGroupAuth"),
    lexicon!("fyi/opensocial/internal/getJoinRequest"),
    lexicon!("fyi/opensocial/internal/getLogs"),
    lexicon!("fyi/opensocial/internal/getModSubjectBlob"),
    lexicon!("fyi/opensocial/internal/getSpaceBlob"),
    lexicon!("fyi/opensocial/internal/getStats"),
    lexicon!("fyi/opensocial/internal/getViewer"),
    lexicon!("fyi/opensocial/internal/importAccount"),
    lexicon!("fyi/opensocial/internal/importAccountSpace"),
    lexicon!("fyi/opensocial/internal/listAppPasswords"),
    lexicon!("fyi/opensocial/internal/listConnectedApps"),
    lexicon!("fyi/opensocial/internal/listGroupTokens"),
    lexicon!("fyi/opensocial/internal/listInvites"),
    lexicon!("fyi/opensocial/internal/listMembers"),
    lexicon!("fyi/opensocial/internal/listRecoveryKeys"),
    lexicon!("fyi/opensocial/internal/listSpaceRecords"),
    lexicon!("fyi/opensocial/internal/migrateActivate"),
    lexicon!("fyi/opensocial/internal/migrateComplete"),
    lexicon!("fyi/opensocial/internal/migrateExport"),
    lexicon!("fyi/opensocial/internal/migratePrepare"),
    lexicon!("fyi/opensocial/internal/migrateReceive"),
    lexicon!("fyi/opensocial/internal/provisionGroup"),
    lexicon!("fyi/opensocial/internal/putGroupDomain"),
    lexicon!("fyi/opensocial/internal/putLabelDefinition"),
    lexicon!("fyi/opensocial/internal/removeRecoveryKey"),
    lexicon!("fyi/opensocial/internal/revokeAppPassword"),
    lexicon!("fyi/opensocial/internal/revokeConnectedApp"),
    lexicon!("fyi/opensocial/internal/revokeGroupToken"),
    lexicon!("fyi/opensocial/internal/searchActors"),
    lexicon!("fyi/opensocial/internal/startAccountMove"),
    lexicon!("fyi/opensocial/internal/updateGroupHandle"),
    lexicon!("upstream/com/atproto/admin/defs"),
    lexicon!("upstream/com/atproto/identity/resolveHandle"),
    lexicon!("upstream/com/atproto/label/defs"),
    lexicon!("upstream/com/atproto/label/record"),
    lexicon!("upstream/com/atproto/managedspace/policy"),
    lexicon!("upstream/com/atproto/memberspace/policy"),
    lexicon!("upstream/com/atproto/personalspace/policy"),
    lexicon!("upstream/com/atproto/repo/strongRef"),
    lexicon!("upstream/com/atproto/server/defs"),
    lexicon!("upstream/com/atproto/server/describeServer"),
    lexicon!("upstream/com/atproto/space/createSpace"),
    lexicon!("upstream/com/atproto/space/deleteSpace"),
    lexicon!("upstream/com/atproto/space/getSpace"),
    lexicon!("upstream/com/atproto/space/policy"),
    lexicon!("upstream/com/atproto/space/updateSpace"),
];

/// The lexicons, read once.
pub fn lexicons() -> &'static Lexicons {
    static LEXICONS: OnceLock<Lexicons> = OnceLock::new();
    LEXICONS.get_or_init(|| {
        let docs = FILES.iter().map(|text| {
            let doc: Value = serde_json::from_str(text).expect("a lexicon is JSON");
            doc
        });
        Lexicons::new(docs)
    })
}

/// The most JSON input a method reads, as a handler's `Json` reads it.
pub const INPUT_AT_MOST: usize = 2 * 1024 * 1024;

fn refuse(err: XrpcError) -> Response {
    err.into_response()
}

/// Refuses a call to a `fyi.opensocial.*` method whose parameters or JSON
/// input are not what its lexicon describes, as `InvalidRequest` saying
/// where and what, or whose input is larger than it takes, as
/// `PayloadTooLarge`: [`INPUT_AT_MOST`]. Any other request passes as it
/// came.
pub async fn check(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let ours = path
        .strip_prefix("/xrpc/")
        .filter(|n| n.starts_with("fyi.opensocial."));
    let Some((nsid, def)) = ours.and_then(|n| Some((n.to_string(), lexicons().def(n)?))) else {
        return next.run(request).await;
    };
    // A method asked the wrong way is the router's to refuse.
    let method = if def["type"] == "query" {
        Method::GET
    } else {
        Method::POST
    };
    if request.method() != method {
        return next.run(request).await;
    }
    let pairs = Query::<Vec<(String, String)>>::try_from_uri(request.uri());
    let pairs = pairs.map(|Query(p)| p).unwrap_or_default();
    let params = lexicons().query_params(&nsid, &pairs);
    if let Err(e) = lexicons().validate_params(&nsid, &params) {
        return refuse(invalid(e.to_string()));
    }
    if def["input"]["encoding"] != "application/json" {
        return next.run(request).await;
    }
    let (parts, body) = request.into_parts();
    let most = INPUT_AT_MOST;
    let Ok(bytes) = axum::body::to_bytes(body, most).await else {
        let said = format!("an input of {most} bytes at most, read whole");
        return refuse(XrpcError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PayloadTooLarge",
            said,
        ));
    };
    let Ok(input) = serde_json::from_slice::<Value>(&bytes) else {
        return refuse(invalid("the input is not JSON"));
    };
    if let Err(e) = lexicons().validate_input(&nsid, &input) {
        return refuse(invalid(e.to_string()));
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

/// What `validationStatus` says of a record of `collection`: `valid` for
/// one of our own collections, or a label, which [`check_record`] checks
/// against its lexicon before it is kept, `unknown` for any other.
pub fn validation(collection: &str) -> &'static str {
    let ours = collection.starts_with("fyi.opensocial.") || collection == crate::rbac::LABEL;
    match lexicons().def(collection) {
        Some(def) if ours && def["type"] == "record" => "valid",
        _ => "unknown",
    }
}

/// Refuses a record of one of our own collections, or a label, that is
/// not what its lexicon describes; a record of any other collection
/// passes unread.
pub fn check_record(collection: &str, record: &Value) -> Result<(), XrpcError> {
    if validation(collection) == "unknown" {
        return Ok(());
    }
    let checked = lexicons().validate(collection, record);
    checked.map_err(|e| invalid(e.to_string()))
}
