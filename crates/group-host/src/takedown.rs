//! Takedowns: a `!takedown` label the group writes takes what it labels
//! out of the group's spaces as the host serves them, from its index
//! (`listSpaceRecords`) and the files there (`getSpaceBlob`), as the
//! spaces convention has a space's authority take content down.
//!
//! A takedown of a record, written in the space the record is in, takes
//! that record out (only the version it names, if it names one) and the
//! files it holds. A takedown of an account takes out all the account
//! wrote in the space it is written in; written in the members space,
//! where an account's labels go, in every space of the group. Deleting
//! the label, or its lapsing (`exp`), puts it all back. Only the group's
//! own labels count: one anyone else writes takes nothing down. What is
//! taken down stays in its writer's repo; the host only stops serving it.

use crate::Host;
use crate::blobs::blob_refs;
use crate::group::members_space;
use crate::group_invites::millis_of;
use crate::rbac::LABEL;
use crate::time::now_millis;
use anyhow::Result;
use atproto::{Cid, drisl, syntax};
use std::collections::HashSet;

/// The label that takes content down.
pub const TAKEDOWN: &str = "!takedown";

/// What the group's takedowns take out of one of its spaces.
#[derive(Debug, Default)]
pub struct TakenDown {
    /// Records, by URI, each with the version taken down if only one is.
    records: Vec<(String, Option<String>)>,
    /// Accounts everything of whose is.
    accounts: HashSet<String>,
}

impl TakenDown {
    /// Whether the record at `uri`, `repo`'s, as it is at `cid`, is taken
    /// down.
    pub fn hides(&self, uri: &str, repo: &str, cid: &str) -> bool {
        let named = |(at, version): &(String, Option<String>)| {
            at == uri && version.as_deref().is_none_or(|v| v == cid)
        };
        self.accounts.contains(repo) || self.records.iter().any(named)
    }
}

impl Host {
    /// What the group's takedowns take out of `space`, one of `group`'s:
    /// its `!takedown` labels there, and those on accounts in the members
    /// space, each still in force.
    pub fn taken_down(&self, group: &str, space: &str) -> Result<TakenDown> {
        let here = self.group_records(group, space, LABEL)?;
        let members = members_space(group);
        let everywhere = match space == members {
            true => Vec::new(),
            false => self.group_records(group, &members, LABEL)?,
        };
        let now = now_millis();
        let lapsed = |exp: Option<&str>| exp.and_then(millis_of).is_some_and(|at| at <= now);
        let labels = here.iter().chain(&everywhere).map(|(_, label)| label);
        let mut down = TakenDown::default();
        for label in labels.filter(|l| l["val"] == TAKEDOWN && !lapsed(l["exp"].as_str())) {
            let uri = label["uri"].as_str().unwrap_or_default().to_string();
            // An account's label in the members space covers every space;
            // a record's covers only records in its own.
            if syntax::is_did(&uri) {
                down.accounts.insert(uri);
            } else {
                down.records
                    .push((uri, label["cid"].as_str().map(String::from)));
            }
        }
        Ok(down)
    }

    /// Whether `down` takes `repo`'s file `cid` out of `space`: everything
    /// of theirs is taken down there, or a record of theirs taken down, as
    /// the index holds it, holds the file.
    pub fn file_taken_down(
        &self,
        down: &TakenDown,
        space: &str,
        repo: &str,
        cid: &Cid,
    ) -> Result<bool> {
        if down.accounts.contains(repo) {
            return Ok(true);
        }
        let theirs = format!("{space}/{repo}/");
        let paths = down
            .records
            .iter()
            .filter_map(|(uri, _)| Some((uri, uri.strip_prefix(&theirs)?)));
        for (uri, path) in paths {
            let found = self.store.indexed_one(space, repo, path)?;
            let holds = found.is_some_and(|r| {
                let value = drisl::decode(&r.value).expect("a record the host indexed");
                down.hides(uri, repo, &r.cid.to_string())
                    && blob_refs(&value.to_json()).contains(cid)
            });
            if holds {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
