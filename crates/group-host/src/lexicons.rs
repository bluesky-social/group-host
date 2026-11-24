//! The lexicons the host answers by, read once: its own methods' and
//! records' (`org.atgrp.*`), and the others' they refer to. Every
//! `org.atgrp.*` method's parameters and JSON input are checked
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
    lexicon!("org/atgrp/acceptance"),
    lexicon!("org/atgrp/authManageGroup"),
    lexicon!("org/atgrp/authManageProfile"),
    lexicon!("org/atgrp/authModerate"),
    lexicon!("org/atgrp/authParticipate"),
    lexicon!("org/atgrp/cancelJoinRequest"),
    lexicon!("org/atgrp/createReport"),
    lexicon!("org/atgrp/declaration"),
    lexicon!("org/atgrp/defs"),
    lexicon!("org/atgrp/denyJoinRequest"),
    lexicon!("org/atgrp/getModSubjectHistory"),
    lexicon!("org/atgrp/invite"),
    lexicon!("org/atgrp/invites"),
    lexicon!("org/atgrp/labeler"),
    lexicon!("org/atgrp/leaveGroup"),
    lexicon!("org/atgrp/listJoinRequests"),
    lexicon!("org/atgrp/listModSubjects"),
    lexicon!("org/atgrp/members"),
    lexicon!("org/atgrp/membership"),
    lexicon!("org/atgrp/meta"),
    lexicon!("org/atgrp/policy"),
    lexicon!("org/atgrp/profile"),
    lexicon!("org/atgrp/requestJoin"),
    lexicon!("org/atgrp/resolveModSubject"),
    lexicon!("org/atgrp/role"),
    lexicon!("org/atgrp/rule"),
    lexicon!("org/atgrp/space"),
    lexicon!("org/atgrp/internal/activateGroup"),
    lexicon!("org/atgrp/internal/addRecoveryKey"),
    lexicon!("org/atgrp/internal/approveAuthorization"),
    lexicon!("org/atgrp/internal/cancelAccountMove"),
    lexicon!("org/atgrp/internal/checkAddress"),
    lexicon!("org/atgrp/internal/checkGroupHandle"),
    lexicon!("org/atgrp/internal/checkImport"),
    lexicon!("org/atgrp/internal/completeImport"),
    lexicon!("org/atgrp/internal/createAppPassword"),
    lexicon!("org/atgrp/internal/createGroup"),
    lexicon!("org/atgrp/internal/deactivateGroup"),
    lexicon!("org/atgrp/internal/declineAuthorization"),
    lexicon!("org/atgrp/internal/declineInvite"),
    lexicon!("org/atgrp/internal/defs"),
    lexicon!("org/atgrp/internal/deleteGroup"),
    lexicon!("org/atgrp/internal/deleteGroupDomain"),
    lexicon!("org/atgrp/internal/deleteLabelDefinition"),
    lexicon!("org/atgrp/internal/finishAccountMove"),
    lexicon!("org/atgrp/internal/getAccountMove"),
    lexicon!("org/atgrp/internal/getActor"),
    lexicon!("org/atgrp/internal/getActorGroups"),
    lexicon!("org/atgrp/internal/getActorInvites"),
    lexicon!("org/atgrp/internal/getAuthorization"),
    lexicon!("org/atgrp/internal/getGroup"),
    lexicon!("org/atgrp/internal/getGroupAuth"),
    lexicon!("org/atgrp/internal/getJoinRequest"),
    lexicon!("org/atgrp/internal/getLogs"),
    lexicon!("org/atgrp/internal/getModSubjectBlob"),
    lexicon!("org/atgrp/internal/getSpaceBlob"),
    lexicon!("org/atgrp/internal/getStats"),
    lexicon!("org/atgrp/internal/getViewer"),
    lexicon!("org/atgrp/internal/importAccount"),
    lexicon!("org/atgrp/internal/importAccountSpace"),
    lexicon!("org/atgrp/internal/listAppPasswords"),
    lexicon!("org/atgrp/internal/listConnectedApps"),
    lexicon!("org/atgrp/internal/listGroupTokens"),
    lexicon!("org/atgrp/internal/listInvites"),
    lexicon!("org/atgrp/internal/listMembers"),
    lexicon!("org/atgrp/internal/listRecoveryKeys"),
    lexicon!("org/atgrp/internal/listSpaceRecords"),
    lexicon!("org/atgrp/internal/migrateActivate"),
    lexicon!("org/atgrp/internal/migrateComplete"),
    lexicon!("org/atgrp/internal/migrateExport"),
    lexicon!("org/atgrp/internal/migratePrepare"),
    lexicon!("org/atgrp/internal/migrateReceive"),
    lexicon!("org/atgrp/internal/provisionGroup"),
    lexicon!("org/atgrp/internal/putGroupDomain"),
    lexicon!("org/atgrp/internal/putLabelDefinition"),
    lexicon!("org/atgrp/internal/removeRecoveryKey"),
    lexicon!("org/atgrp/internal/revokeAppPassword"),
    lexicon!("org/atgrp/internal/revokeConnectedApp"),
    lexicon!("org/atgrp/internal/revokeGroupToken"),
    lexicon!("org/atgrp/internal/searchActors"),
    lexicon!("org/atgrp/internal/startAccountMove"),
    lexicon!("org/atgrp/internal/updateGroupHandle"),
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

/// Refuses a call to a `org.atgrp.*` method whose parameters or JSON
/// input are not what its lexicon describes, as `InvalidRequest` saying
/// where and what, or whose input is larger than it takes, as
/// `PayloadTooLarge`: [`INPUT_AT_MOST`]. Any other request passes as it
/// came.
pub async fn check(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let ours = path
        .strip_prefix("/xrpc/")
        .filter(|n| n.starts_with("org.atgrp."));
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
    let ours = collection.starts_with("org.atgrp.") || collection == crate::rbac::LABEL;
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
